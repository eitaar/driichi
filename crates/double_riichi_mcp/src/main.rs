#[tokio::main]
async fn main() -> std::process::ExitCode {
    double_riichi_mcp::process_entrypoint().await
}
