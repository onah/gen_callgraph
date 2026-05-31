# gen_callgraph

A Rust call graph generator powered by rust-analyzer (LSP).

> **Note**: This is an experimental, learning-oriented project.
> The primary goal is to explore LSP client implementation in Rust rather than production use.

## Requirements

- [rust-analyzer](https://rust-analyzer.github.io/) in `PATH`
- A Rust project that compiles successfully (`cargo check` passes)

## Usage

```bash
gen_callgraph [WORKSPACE] [ENTRY_FUNCTION] [OUTPUT_PATH]
```

| Argument | Default | Description |
|---|---|---|
| `WORKSPACE` | current directory | Path to the Rust project root (must contain `Cargo.toml`) |
| `ENTRY_FUNCTION` | `main` | Entry function for the call graph traversal |
| `OUTPUT_PATH` | `tmp/callgraph.dot` | Output file path (GraphViz DOT format) |

**Examples:**

```bash
# Analyze main in the current project
gen_callgraph

# Specify workspace and entry function
gen_callgraph /path/to/project my_function output.dot
```

## Visualizing the Output

The generated `.dot` file can be rendered with [GraphViz](https://graphviz.org/):

```bash
dot -Tsvg callgraph.dot -o callgraph.svg
dot -Tpng callgraph.dot -o callgraph.png
```

## Troubleshooting

**"Entry function not found"**
- Verify the function name and that it exists in the workspace.
- rust-analyzer may still be indexing; the tool retries automatically.
- Run `cargo check` to ensure the project compiles.

**LSP timeout**
- Check that rust-analyzer is not blocked by another process.
- Verify the project compiles with `cargo check`.

## Development

```bash
# Run all tests
cargo test

# Run a specific module's tests
cargo test symbol_locator
```

For architecture and design decisions, see [`docs/dev/ARCHITECTURE.md`](docs/dev/ARCHITECTURE.md).

## License

See [LICENSE](LICENSE) for details.
