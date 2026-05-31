# Architecture

## Dependency Direction

```
CLI -> App -> CallGraphBuilder -> LspClient
                    |
               DotRenderer (depends only on CallGraph)
```

## Naming Policy

- Keep names consistent by layer. Do not mix transport-level terms into app/domain APIs.

### 1) Low-level transport (`src/lsp/transport.rs`, `src/lsp/stdio_transport.rs`)

- Purpose: frame/binary I/O only.
- Use I/O-oriented verbs such as `read`, `write`, `read_frame`, `write_frame`.
- Do not use domain terms like `symbol`, `call_hierarchy`, `graph`.

### 2) Framed message layer (`src/lsp/lsp_protocol.rs`)

- Purpose: JSON-RPC message routing and correlation.
- Use message-oriented verbs: `send_request`, `send_notification`, `receive_response`, `receive_notification`.
- If timeout is optional argument, prefer one style consistently:
     - either include `_with_timeout` in the name, or
     - use `wait_*` naming with `timeout: Option<Duration>`.
- Internal queue/control enums should be explicitly internal-oriented (e.g. `ClientOutgoing`).

### 3) LSP client API layer (`src/lsp.rs`)

- Purpose: expose operation-level APIs used by analysis/application code.
- Method names should reflect LSP method semantics, e.g. `workspace_symbol`, `prepare_call_hierarchy`, `outgoing_calls`.
- Keep this layer free from framing terms (`channel`, `pending`, `oneshot`, etc.).

### 4) Error naming

- Prefix error messages by category for observability:
     - `transport:*` for read/write/framing failures
     - `protocol:*` for invalid/unexpected JSON-RPC messages
     - `timeout:*` for wait timeouts
- Keep wording stable to make logs searchable.

## Key Rule

- `call_graph_builder` returns `CallGraph`. It does not know about output formats.
- `dot_renderer` takes `CallGraph`. It does not know about LSP or analysis.
- `app` is the only place that connects these layers.
