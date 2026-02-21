use clap::Parser;

#[derive(Debug, Clone)]
pub struct Config {
    pub workspace: String,
    pub entry_function: String,
    pub output_path: String,
}

#[derive(Parser, Debug)]
#[command(name = "gen_callgraph")]
#[command(about = "Generate call graph dot output via rust-analyzer", long_about = None)]
pub struct Cli {
    workspace: Option<String>,
    #[arg(default_value = "main")]
    pub entry_function: String,
    #[arg(default_value = "callgraph.dot")]
    pub output_path: String,
}

impl Cli {
    pub fn from_args() -> Self {
        Self::parse()
    }

    pub fn into_config(self) -> Config {
        Config {
            workspace: self.workspace.unwrap_or_else(|| {
                std::env::current_dir()
                    .ok()
                    .and_then(|p| p.to_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| String::from("."))
            }),
            entry_function: self.entry_function,
            output_path: self.output_path,
        }
    }
}
