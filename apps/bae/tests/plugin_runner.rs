use std::process::Command;

#[test]
fn runner_exits_on_eof_without_initializing_the_application() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("unused");
    let result = Command::new(env!("CARGO_BIN_EXE_bae"))
        .args(["plugin", "runner"])
        .env("BAE_HOME", &home)
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    assert!(result.stdout.is_empty());
    assert!(!home.exists());
}
