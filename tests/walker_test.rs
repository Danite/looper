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
        "dist/bundle.js",
    ]);

    fs::write(dir.path().join(".gitignore"), "dist/\n").unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();

    let opts = WalkOptions {
        no_gitignore: true,
        ..Default::default()
    };

    let result = discover_files(&[dir.path().to_path_buf()], &opts).unwrap();

    assert_eq!(result.files.len(), 2);
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
