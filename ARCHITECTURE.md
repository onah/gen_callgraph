# Architecture

## Module Responsibilities

| Module | Responsibility |
|---|---|
| `src/cli.rs` | CLI argument parsing. Produces `Config`. |
| `src/lsp_client.rs` + `src/lsp/` | LSP communication. Sends requests and receives responses. No domain logic. |
| `src/call_graph_builder.rs` | Builds `CallGraph` from LSP results. No output format knowledge. |
| `src/dot_renderer.rs` | Renders `CallGraph` into DOT format string. No LSP/analysis knowledge. |
| `src/app.rs` | Orchestration only. Wires CLI → LSP → Builder → Renderer → file write. |
| `src/main.rs` | Entry point. Parses config and calls `app::run`. |

## Dependency Direction

```
CLI -> App -> CallGraphBuilder -> LspClient
                    |
               DotRenderer (depends only on CallGraph)
```

## Key Rule

- `call_graph_builder` returns `CallGraph`. It does not know about output formats.
- `dot_renderer` takes `CallGraph`. It does not know about LSP or analysis.
- `app` is the only place that connects these layers.
