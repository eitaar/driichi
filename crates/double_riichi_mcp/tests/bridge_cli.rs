use std::process::Command;

#[test]
fn rejects_missing_environment_token() {
    let output = Command::new(env!("CARGO_BIN_EXE_driichi-mcp"))
        .args(["--server", "http://127.0.0.1:1/mcp"])
        .env_remove("DRIICHI_MCP_TOKEN")
        .output()
        .expect("driichi-mcp process starts");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "DRIICHI_MCP_TOKEN is required"
    );
}
