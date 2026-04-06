use std::process::Command;

fn looper_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_looper"))
}

#[test]
fn help_exits_zero() {
    let output = looper_cmd().arg("--help").output().unwrap();
    assert!(output.status.success(), "exit: {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("shadow dependencies"));
}

#[test]
fn version_exits_zero() {
    let output = looper_cmd().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("looper"));
}

#[test]
fn scan_fixtures_json_exits_zero_with_valid_json() {
    let output = looper_cmd()
        .args(["scan", "tests/fixtures/", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "stdout should be valid JSON: {stdout}");
    let val = parsed.unwrap();
    assert!(val["deps"].is_array());
}

#[test]
fn scan_nonexistent_exits_two() {
    let output = looper_cmd()
        .args(["scan", "tests/nonexistent_dir_xyz/"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn scan_invalid_format_exits_one() {
    let output = looper_cmd()
        .args(["scan", "tests/fixtures/", "--format", "josn"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown format"));
}

#[test]
fn implicit_scan_shorthand() {
    let output = looper_cmd()
        .args(["tests/fixtures/", "--quiet"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("shadow deps"),
        "implicit scan should work: {stdout}"
    );
}

#[test]
fn scan_quiet_one_line() {
    let output = looper_cmd()
        .args(["scan", "tests/fixtures/", "--quiet"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "quiet mode should produce one line: {stdout}"
    );
    assert!(stdout.contains("looper v"));
}

#[test]
fn html_without_output_exits_one() {
    let output = looper_cmd()
        .args(["scan", "tests/fixtures/", "--format", "html"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--output"));
}
