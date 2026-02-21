# Application Architecture

This document defines the target architecture for `gen_callgraph`.

## Goals

- Keep responsibilities clearly separated.
- Make LSP-dependent code isolated from domain logic.
- Make output format generation replaceable (e.g., DOT, JSON, Mermaid).
- Keep CLI and orchestration thin and easy to maintain.

## Layered Structure

### 1) LSP Communication Layer

**Responsibility**
- Start and manage the language server process.
- Send/receive LSP requests and notifications.
- Handle protocol framing, parsing, timeouts, and transport details.

**Current modules (mainly)**
- `src/lsp/` (transport, framed protocol, parser, message builder)
- `src/lsp.rs` (`LspClient` request orchestration)

### 2) Call-Graph Analysis Layer

**Responsibility**
- Convert raw LSP results into a normalized call-graph structure.
- Resolve entry function and build graph nodes/edges.
- Apply filtering, deduplication, and traversal rules.

**Current modules (mainly)**
- `src/lsp.rs` (`collect_call_graph_from` and related symbol/call-hierarchy logic)
- `src/code_analysis.rs` (`CodeAnalyzer` as analysis-facing facade)

**Recommended next split**
- Symbol collection/adapters (LSP-shaped data handling)
- Pure graph construction (domain logic, minimal I/O)

### 3) Output Generation Layer

**Responsibility**
- Convert call-graph domain data into output formats.
- Keep rendering logic independent from LSP and CLI.

**Current modules**
- `src/dot.rs`

### 4) Command-Line Interface Layer

**Responsibility**
- Parse command-line arguments.
- Validate user input and produce runtime config.

**Current modules**
- `src/cli.rs`

### 5) Application Orchestration / Utilities Layer

**Responsibility**
- Wire together CLI, LSP client, analysis, and output.
- Control retries and readiness checks.
- Provide shared utility behavior where needed.

**Current modules**
- `src/main.rs` (thin entrypoint)
- `src/app.rs` (runtime flow orchestration)

## Dependency Direction

Preferred dependency direction:

`CLI -> App -> Analysis -> LSP`

`Output` should depend on domain graph structures, not on LSP transport details.

## Refactoring Principles

- Keep `main.rs` minimal (parse config + call app runner).
- Avoid putting domain logic inside transport/client internals when possible.
- Return meaningful errors for retryable states (e.g., indexing not ready).
- Keep each layer testable in isolation.

## Near-Term Refactoring Plan

1. Keep retry/readiness policy centralized (prefer app-level policy or a dedicated policy module).
2. Move graph-building logic from `LspClient` into a dedicated analysis component.
3. Keep `LspClient` focused on protocol-facing operations and typed data retrieval.
4. Add small integration tests around entry resolution and call-graph generation stability.
