# gen_callgraph overview
- Purpose: Generate a call graph from Rust code via LSP and output it (currently DOT).
- Language/stack: Rust (Cargo), async runtime with tokio, LSP types/protocol handling.
- Architecture target (from ARCHITECTURE.md): layered structure
  - CLI (`src/cli.rs`)
  - App orchestration (`src/app.rs`, thin `src/main.rs`)
  - Analysis (`src/code_analysis.rs`, parts of `src/lsp.rs` currently)
  - LSP communication (`src/lsp/` and `src/lsp.rs` client orchestration)
  - Output generation (`src/dot.rs`)
- Dependency direction preference: CLI -> App -> Analysis -> LSP. Output should depend on domain graph structures, not transport details.
