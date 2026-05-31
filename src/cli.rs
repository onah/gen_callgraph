//! CLI argument parsing. Parses raw arguments via `clap` and validates them before
//! producing a [`Config`] for the rest of the application.

use clap::Parser;
use std::path::{Path, PathBuf};

/// Runtime configuration produced from validated CLI arguments.
///
/// All paths are canonicalized absolute paths by the time this struct is created.
#[derive(Debug, Clone)]
pub struct Config {
    /// Absolute path to the Rust workspace root (must contain `Cargo.toml`).
    pub workspace: String,
    /// Optional entry function name to start call graph traversal from.
    /// When `None`, the tool traverses all workspace functions.
    pub entry_function: Option<String>,
    /// Path where the generated DOT file will be written.
    pub output_path: String,
}

/// Returns `Ok(())` when `path` is a valid Rust project root,
/// or a descriptive error explaining what is wrong.
///
/// # Why early validation matters
///
/// Before this function existed, a relative or incorrect workspace path (e.g. `.` or a
/// subdirectory) was passed directly to the LSP initializer, which produced an invalid
/// `file://.` URI. rust-analyzer silently ignored it, and `workspace/symbol ""` returned
/// an empty list. The error surfaced much later as a misleading "entry function not found"
/// message. This function catches the mistake at the boundary before the LSP server is
/// ever started.
pub fn validate_rust_workspace(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Err(anyhow::anyhow!("workspace path {:?} does not exist", path));
    }
    if !path.is_dir() {
        return Err(anyhow::anyhow!(
            "workspace path {:?} is not a directory",
            path
        ));
    }
    if !path.join("Cargo.toml").exists() {
        return Err(anyhow::anyhow!(
            "workspace {:?} is not a Rust project root (no Cargo.toml found). \
             Run from the project root or pass the correct workspace path.",
            path
        ));
    }
    Ok(())
}

#[derive(Parser, Debug)]
#[command(name = "gen_callgraph")]
#[command(about = "Generate call graph dot output via rust-analyzer", long_about = None)]
pub struct Cli {
    workspace: Option<String>,
    pub entry_function: Option<String>,
    #[arg(default_value = "tmp/callgraph.dot")]
    pub output_path: String,
}

impl Cli {
    pub fn from_args() -> Self {
        Self::parse()
    }

    /// Converts CLI arguments into a validated [`Config`].
    ///
    /// Canonicalizes the workspace path and verifies it is a Rust project root.
    /// Returns an error early so that downstream code can assume the path is valid.
    pub fn into_config(self) -> anyhow::Result<Config> {
        let raw = self.workspace.unwrap_or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .unwrap_or_else(|| String::from("."))
        });

        let workspace_path = std::fs::canonicalize(&raw).unwrap_or_else(|_| PathBuf::from(&raw));

        validate_rust_workspace(&workspace_path)?;

        Ok(Config {
            workspace: workspace_path.to_string_lossy().to_string(),
            entry_function: self.entry_function,
            output_path: self.output_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_workspace_rejects_nonexistent_path() {
        let path = Path::new("/nonexistent/workspace/path/should_not_exist_abc123");
        let result = validate_rust_workspace(path);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("does not exist"),
            "error should say 'does not exist'"
        );
    }

    #[test]
    fn validate_workspace_rejects_directory_without_cargo_toml() {
        let dir = std::env::temp_dir().join("gen_callgraph_test_no_cargo_toml");
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join("Cargo.toml"));

        let result = validate_rust_workspace(&dir);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Cargo.toml"),
            "error should mention Cargo.toml, got: {}",
            msg
        );
    }

    #[test]
    fn validate_workspace_accepts_directory_with_cargo_toml() {
        let dir = std::env::temp_dir().join("gen_callgraph_test_valid_project");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

        let result = validate_rust_workspace(&dir);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(result.is_ok());
    }

    #[test]
    fn validate_workspace_rejects_file_path() {
        let file = std::env::temp_dir().join("gen_callgraph_test_not_a_dir.txt");
        std::fs::write(&file, "test content").unwrap();

        let result = validate_rust_workspace(&file);
        let _ = std::fs::remove_file(&file);

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("not a directory"),
            "error should say 'not a directory', got: {}",
            msg
        );
    }
}
