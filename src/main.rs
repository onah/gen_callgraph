mod app;
mod call_graph;
mod call_graph_builder;
mod cli;
mod dot_renderer;
mod lsp;
mod trace;
use cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Cli::from_args().into_config();
    app::run(config).await
}
