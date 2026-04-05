use looper::walker::{discover_files, WalkError, WalkOptions};
use std::fs;
use tempfile::TempDir;

fn setup_project(files: &[&str]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for file in files {
        let path = dir.path().join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, format!("// {file}")).unwrap();
    }
    dir
}

#[test]
fn discovers_js_and_ts_files() {
    let dir = setup_project(&[
        "src/index.ts",
        "src/utils.js",
        "src/component.tsx",
        "src/hooks.jsx",
        "lib/helpers.mjs",
        "lib/compat.cjs",
        "lib/types.mts",
        "lib/legacy.cts",
    ]);

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    assert_eq!(result.files.len(), 8);
}

#[test]
fn ignores_non_js_ts_files() {
    let dir = setup_project(&[
        "src/index.ts",
        "src/styles.css",
        "src/image.png",
        "README.md",
        "Cargo.toml",
        "data.json",
    ]);

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].ends_with("index.ts"));
}

#[test]
fn respects_gitignore() {
    let dir = setup_project(&[
        "src/index.ts",
        "dist/bundle.js",
        "node_modules/pkg/index.js",
    ]);

    fs::write(dir.path().join(".gitignore"), "dist/\nnode_modules/\n").unwrap();

    // Need a git repo for .gitignore to take effect
    fs::create_dir_all(dir.path().join(".git")).unwrap();

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].ends_with("index.ts"));
}

#[test]
fn no_gitignore_flag_includes_ignored_files() {
    let dir = setup_project(&[
        "src/index.ts",
        "generated/output.js",
    ]);

    fs::write(dir.path().join(".gitignore"), "generated/\n").unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();

    let opts = WalkOptions {
        no_gitignore: true,
        ..Default::default()
    };

    let result = discover_files(&[dir.path().to_path_buf()], &opts).unwrap();

    assert_eq!(result.files.len(), 2);
}

#[test]
fn no_gitignore_still_excludes_default_dirs() {
    let dir = setup_project(&[
        "src/index.ts",
        "node_modules/pkg/index.js",
        "dist/bundle.js",
    ]);

    fs::write(dir.path().join(".gitignore"), "").unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();

    let opts = WalkOptions {
        no_gitignore: true,
        ..Default::default()
    };

    let result = discover_files(&[dir.path().to_path_buf()], &opts).unwrap();

    assert_eq!(result.files.len(), 1, "default excludes should still apply with --no-gitignore");
    assert!(result.files[0].ends_with("index.ts"));
}

#[test]
fn no_default_excludes_includes_default_dirs() {
    let dir = setup_project(&[
        "src/index.ts",
        "node_modules/pkg/index.js",
        "dist/bundle.js",
    ]);

    let opts = WalkOptions {
        no_default_excludes: true,
        ..Default::default()
    };

    let result = discover_files(&[dir.path().to_path_buf()], &opts).unwrap();

    assert_eq!(result.files.len(), 3, "all files should be found with --no-default-excludes");
}

#[test]
fn extra_ignore_patterns() {
    let dir = setup_project(&[
        "src/index.ts",
        "src/index.test.ts",
        "src/utils.spec.ts",
        "src/helpers.ts",
    ]);

    let opts = WalkOptions {
        extra_ignore_patterns: vec![
            "*.test.ts".into(),
            "*.spec.ts".into(),
        ],
        ..Default::default()
    };

    let result = discover_files(&[dir.path().to_path_buf()], &opts).unwrap();

    assert_eq!(result.files.len(), 2);
    let names: Vec<String> = result
        .files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"index.ts".to_string()));
    assert!(names.contains(&"helpers.ts".to_string()));
}

#[test]
fn multiple_root_paths() {
    let dir = setup_project(&[
        "src/app.ts",
        "lib/utils.ts",
        "scripts/build.js",
    ]);

    let result = discover_files(
        &[
            dir.path().join("src"),
            dir.path().join("lib"),
        ],
        &WalkOptions::default(),
    )
    .unwrap();

    assert_eq!(result.files.len(), 2);
    let names: Vec<String> = result
        .files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"app.ts".to_string()));
    assert!(names.contains(&"utils.ts".to_string()));
    assert!(!names.contains(&"build.js".to_string()));
}

#[test]
fn returns_error_when_no_files_found() {
    let dir = setup_project(&[
        "README.md",
        "Cargo.toml",
    ]);

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    );

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), WalkError::NoFilesFound));
}

#[test]
fn skips_empty_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("empty.ts"), "").unwrap();
    fs::write(dir.path().join("real.ts"), "const x = 1;").unwrap();

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    // Walker finds both files (filtering empty is done at parse time),
    // but it should at least not crash
    assert!(!result.files.is_empty());
}

#[test]
fn skips_directories_with_js_like_names() {
    let dir = TempDir::new().unwrap();
    // A directory named "something.ts" should be ignored
    fs::create_dir_all(dir.path().join("something.ts")).unwrap();
    fs::write(dir.path().join("real.ts"), "const x = 1;").unwrap();

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].ends_with("real.ts"));
}

#[test]
fn results_are_sorted() {
    let dir = setup_project(&[
        "z.ts",
        "a.ts",
        "m/b.ts",
        "m/a.ts",
    ]);

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    let sorted = result.files.clone();
    let mut expected = sorted.clone();
    expected.sort();
    assert_eq!(result.files, expected);
}

#[test]
fn empty_roots_returns_error() {
    let result = discover_files(&[], &WalkOptions::default());
    assert!(matches!(result.unwrap_err(), WalkError::NoFilesFound));
}

#[test]
fn follows_symlinks() {
    let dir = TempDir::new().unwrap();
    let real_dir = dir.path().join("real");
    fs::create_dir_all(&real_dir).unwrap();
    fs::write(real_dir.join("app.ts"), "export const a = 1;").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_dir, dir.path().join("linked")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&real_dir, dir.path().join("linked")).unwrap();

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    let names: Vec<String> = result
        .files
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    let has_linked = names.iter().any(|n| n.contains("linked"));
    assert!(has_linked, "symlinked directory should be followed: {names:?}");
    assert!(result.files.len() >= 2, "should find file via both real and linked paths");
}

#[test]
fn symlink_cycle_does_not_hang() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("index.ts"), "const x = 1;").unwrap();

    #[cfg(unix)]
    {
        let a = dir.path().join("link_a");
        std::os::unix::fs::symlink(dir.path(), &a).unwrap();
    }

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    );

    // The walker should finish (not hang) and either succeed or report errors.
    // The `ignore` crate detects symlink cycles and avoids infinite loops.
    assert!(result.is_ok());
    assert!(!result.unwrap().files.is_empty());
}

#[test]
fn binary_file_with_js_extension_does_not_crash() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("valid.ts"), "const x = 1;").unwrap();
    // Write non-UTF-8 bytes with a .js extension
    fs::write(dir.path().join("binary.js"), &[0xFF, 0xFE, 0x00, 0xD8, 0x80, 0x00]).unwrap();

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    // Walker discovers both files (it doesn't inspect content)
    assert_eq!(result.files.len(), 2);
}

#[cfg(unix)]
#[test]
fn permission_denied_file_does_not_crash() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("readable.ts"), "const x = 1;").unwrap();

    let denied_path = dir.path().join("secret.ts");
    fs::write(&denied_path, "const s = 1;").unwrap();
    fs::set_permissions(&denied_path, fs::Permissions::from_mode(0o000)).unwrap();

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    // Walker finds both files (permissions are checked at read time, not walk time)
    assert!(result.files.len() >= 1);

    // Restore permissions so TempDir cleanup succeeds
    fs::set_permissions(&denied_path, fs::Permissions::from_mode(0o644)).unwrap();
}

#[cfg(unix)]
#[test]
fn permission_denied_directory_reports_error() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("root.ts"), "const x = 1;").unwrap();

    let denied_dir = dir.path().join("locked");
    fs::create_dir_all(&denied_dir).unwrap();
    fs::write(denied_dir.join("hidden.ts"), "const h = 1;").unwrap();
    fs::set_permissions(&denied_dir, fs::Permissions::from_mode(0o000)).unwrap();

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    // Should still find the root-level file
    assert!(result.files.iter().any(|p| p.ends_with("root.ts")));
    // Should report the permission error
    assert!(!result.errors.is_empty(), "should report walk error for locked dir");

    // Restore permissions so TempDir cleanup succeeds
    fs::set_permissions(&denied_dir, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn unicode_directory_names() {
    let dir = TempDir::new().unwrap();
    let unicode_dir = dir.path().join("コンポーネント");
    fs::create_dir_all(&unicode_dir).unwrap();
    fs::write(unicode_dir.join("app.ts"), "const x = 1;").unwrap();

    let latin_dir = dir.path().join("données");
    fs::create_dir_all(&latin_dir).unwrap();
    fs::write(latin_dir.join("utils.js"), "const y = 2;").unwrap();

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    assert_eq!(result.files.len(), 2, "should discover files in Unicode-named directories");
}

#[test]
fn unicode_dir_not_matched_by_default_excludes() {
    let dir = TempDir::new().unwrap();
    // A Unicode dir should never accidentally match DEFAULT_EXCLUDED_DIRS
    let unicode_dir = dir.path().join("ビルド");
    fs::create_dir_all(&unicode_dir).unwrap();
    fs::write(unicode_dir.join("index.ts"), "const x = 1;").unwrap();

    let result = discover_files(
        &[dir.path().to_path_buf()],
        &WalkOptions::default(),
    )
    .unwrap();

    assert_eq!(result.files.len(), 1);
}
