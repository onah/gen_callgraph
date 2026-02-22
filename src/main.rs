mod app;
mod call_graph;
mod cli;
mod code_analysis;
mod dot;
mod lsp;
use cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Cli::from_args().into_config();
    app::run(config).await
}
