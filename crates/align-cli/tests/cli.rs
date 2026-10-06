use std::io::Write;
use std::process::{Command, Stdio};

/// Run the binary with built-in defaults (no config file).
fn run(args: &[&str], stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_align"))
        .args(args)
        .env("ALIGN_CONFIG", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn aligns_stdin() {
    let (code, out, _) = run(&["="], "a = 1\nfoobar = 2\n");
    assert_eq!(code, 0);
    assert_eq!(out, "a      = 1\nfoobar = 2\n");
}

#[test]
fn args_are_joined_like_one_string() {
    // As the shell delivers `align if '=>'` and `align "if '=>'"`.
    let input = "\"a\" if x => y\n\"bb\" => z\n";
    assert_eq!(run(&["if", "=>"], input).1, run(&["if '=>'"], input).1);
}

#[test]
fn preserves_line_endings() {
    assert_eq!(run(&["="], "a = 1\r\nbb = 2\r\n").1, "a  = 1\r\nbb = 2\r\n");
    assert_eq!(run(&["="], "a = 1\nbb = 2").1, "a  = 1\nbb = 2");
}

#[test]
fn errors_exit_2_with_caret() {
    let (code, out, err) = run(&["=", "-x"], "a\n");
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert!(err.contains("unknown flag -x") && err.contains("\n      ^"), "{err}");
}

#[test]
fn json_mode() {
    let (code, out, _) = run(&["--json"], r#"{"pattern":"= -p 0","lines":["a = 1","bb = 2"],"tabstop":4}"#);
    assert_eq!(code, 0);
    assert_eq!(out.trim(), r#"{"output":["a =1","bb=2"],"error":null}"#);

    let (_, out, _) = run(&["--json"], r#"{"pattern":"/[/","lines":[]}"#);
    assert!(out.contains(r#""output":null"#) && out.contains("invalid regex"), "{out}");

    let (_, out, _) = run(&["--json"], r#"{"args":["="],"lines":["a = 1","bb = 2"]}"#);
    assert!(out.contains(r#"["a  = 1","bb = 2"]"#), "{out}");
}

#[test]
fn version_and_help() {
    assert!(run(&["--version"], "").1.starts_with("align "));
    let (code, out, _) = run(&["--help"], "");
    assert_eq!(code, 0);
    assert!(out.contains("PATTERN FLAGS"));
    assert_eq!(run(&[], "").0, 2);
}

#[test]
fn config_file_is_used() {
    let dir = std::env::temp_dir().join(format!("align-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, "pad = 2\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_align"))
        .arg("=")
        .env("ALIGN_CONFIG", &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.take().unwrap().write_all(b"a = 1\nbb = 2\n")?;
            c.wait_with_output()
        })
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "a   =  1\nbb  =  2\n");
}
