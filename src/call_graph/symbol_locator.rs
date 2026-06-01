//! Symbol search and discovery within an LSP workspace.
//!
//! # Primary entry points
//!
//! - [`find_function_symbol_with_retry`]: queries a single symbol by name with retry logic.
//!   rust-analyzer may not have finished indexing the workspace when first queried;
//!   this function retries at fixed intervals to tolerate that delay.
//! - [`find_all_workspace_functions`]: full workspace scan used as a fallback when no
//!   specific entry function is given.

use crate::error::{CallGraphError, SymbolError};
use crate::lsp;
use lsp_types::{SymbolInformation, SymbolKind};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

pub(crate) async fn find_function_symbol_with_retry(
    client: &mut lsp::LspClient,
    query: &str,
    max_attempts: usize,
    interval: Duration,
) -> Result<Option<SymbolInformation>, CallGraphError> {
    for attempt in 1..=max_attempts {
        if let Some(symbol) = find_function_symbol(client, query).await? {
            return Ok(Some(symbol));
        }

        if attempt < max_attempts {
            sleep(interval).await;
        }
    }

    // Fallback: Try to find the function using textDocument/documentSymbol
    // This is more reliable for entry functions like 'main'
    if let Some(symbol) = find_function_via_document_symbol(client, query).await? {
        return Ok(Some(symbol));
    }

    Ok(None)
}

// Removed: workspace_function_symbols is no longer needed.
// We now rely on LSP server to provide all necessary metadata in CallHierarchyItem.

async fn find_function_symbol(
    client: &mut lsp::LspClient,
    query: &str,
) -> Result<Option<SymbolInformation>, CallGraphError> {
    // Try exact query first
    let symbols = client.workspace_symbol(query).await?;

    // If exact query returns nothing or very few results, try empty query to get all symbols
    let all_symbols = if symbols.len() < 5 {
        client.workspace_symbol("").await?
    } else {
        symbols.clone()
    };

    // If an exact name match exists in workspace but is not a function/method, report it clearly.
    if let Some(sym) = all_symbols
        .iter()
        .find(|s| s.name == query && client.is_uri_in_workspace(&s.location.uri))
    {
        if sym.kind != SymbolKind::FUNCTION && sym.kind != SymbolKind::METHOD {
            return Err(SymbolError::NotAFunction {
                name: sym.name.clone(),
                kind: sym.kind,
            }
            .into());
        }
    }

    // First try: exact name match with correct kind and in workspace
    let exact = all_symbols
        .iter()
        .find(|s| {
            (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                && s.name == query
                && client.is_uri_in_workspace(&s.location.uri)
        })
        .cloned();
    if exact.is_some() {
        return Ok(exact);
    }

    // Second try: exact name match with correct kind (ignore workspace check)
    let exact_no_ws = all_symbols
        .iter()
        .find(|s| {
            (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD) && s.name == query
        })
        .cloned();
    if exact_no_ws.is_some() {
        return Ok(exact_no_ws);
    }

    let partial = symbols
        .iter()
        .find(|s| {
            (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                && client.is_uri_in_workspace(&s.location.uri)
        })
        .cloned();
    Ok(partial)
}

/// Fallback: Try to find a function by opening common entry point files
/// and using textDocument/documentSymbol.
async fn find_function_via_document_symbol(
    client: &mut lsp::LspClient,
    function_name: &str,
) -> anyhow::Result<Option<SymbolInformation>> {
    // Common entry point files for Rust projects
    let entry_files = vec!["src/main.rs", "src/lib.rs"];

    let workspace_root = PathBuf::from(client.workspace_root_path());

    for entry_file in entry_files {
        let file_path = workspace_root.join(entry_file);

        if !file_path.exists() {
            continue;
        }

        // Read file content
        let Ok(content) = std::fs::read_to_string(&file_path) else {
            continue;
        };

        // Convert file path to URI
        let Ok(uri) = lsp_types::Url::from_file_path(&file_path) else {
            continue;
        };

        // Send didOpen notification
        if let Err(_) = client.text_document_did_open(&uri, "rust", content).await {
            continue;
        }

        // Give server time to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Request document symbols
        let symbols = match client.text_document_document_symbol(&uri).await {
            Ok(syms) => syms,
            Err(_) => {
                continue;
            }
        };

        // Search for the function in document symbols
        if let Some(symbol) = find_function_in_document_symbols(&symbols, function_name, &uri) {
            return Ok(Some(symbol));
        }
    }

    Ok(None)
}

/// Recursively search for a function in DocumentSymbol tree
fn find_function_in_document_symbols(
    symbols: &[lsp_types::DocumentSymbol],
    function_name: &str,
    uri: &lsp_types::Url,
) -> Option<SymbolInformation> {
    for sym in symbols {
        // Check if this symbol is the function we're looking for
        if (sym.kind == SymbolKind::FUNCTION || sym.kind == SymbolKind::METHOD)
            && sym.name == function_name
        {
            // Convert DocumentSymbol to SymbolInformation
            return Some(SymbolInformation {
                name: sym.name.clone(),
                kind: sym.kind,
                tags: sym.tags.clone(),
                #[allow(deprecated)]
                deprecated: sym.deprecated,
                location: lsp_types::Location {
                    uri: uri.clone(),
                    range: sym.selection_range,
                },
                container_name: None,
            });
        }

        // Recursively search in children
        if let Some(children) = &sym.children {
            if let Some(found) = find_function_in_document_symbols(children, function_name, uri) {
                return Some(found);
            }
        }
    }
    None
}

/// Collects all function/method symbols from every Rust source file under `src/`.
/// Used as a fallback when `workspace/symbol ""` returns no results.
pub(crate) async fn find_all_workspace_functions(
    client: &mut lsp::LspClient,
) -> anyhow::Result<Vec<SymbolInformation>> {
    let workspace_root = PathBuf::from(client.workspace_root_path());
    let src_dir = workspace_root.join("src");

    if !src_dir.exists() {
        return Ok(Vec::new());
    }

    let rust_files = collect_rust_files(&src_dir);
    println!(
        "  Scanning {} source files for function symbols...",
        rust_files.len()
    );

    let mut all_symbols: Vec<SymbolInformation> = Vec::new();

    for file_path in &rust_files {
        let Ok(content) = std::fs::read_to_string(file_path) else {
            continue;
        };
        let Ok(uri) = lsp_types::Url::from_file_path(file_path) else {
            continue;
        };

        if let Err(_) = client.text_document_did_open(&uri, "rust", content).await {
            continue;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;

        let doc_symbols = match client.text_document_document_symbol(&uri).await {
            Ok(syms) => syms,
            Err(_) => continue,
        };

        collect_function_symbols_from_doc(&doc_symbols, &uri, &mut all_symbols);
    }

    Ok(all_symbols)
}

/// Recursively walks `dir` and returns all `.rs` file paths.
fn collect_rust_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_rust_files(&path));
        } else if path.extension().map_or(false, |e| e == "rs") {
            files.push(path);
        }
    }
    files
}

/// Flattens a DocumentSymbol tree, collecting all FUNCTION/METHOD symbols as SymbolInformation.
fn collect_function_symbols_from_doc(
    symbols: &[lsp_types::DocumentSymbol],
    uri: &lsp_types::Url,
    out: &mut Vec<SymbolInformation>,
) {
    for sym in symbols {
        if sym.kind == SymbolKind::FUNCTION || sym.kind == SymbolKind::METHOD {
            out.push(SymbolInformation {
                name: sym.name.clone(),
                kind: sym.kind,
                tags: sym.tags.clone(),
                #[allow(deprecated)]
                deprecated: sym.deprecated,
                location: lsp_types::Location {
                    uri: uri.clone(),
                    range: sym.selection_range,
                },
                container_name: None,
            });
        }
        if let Some(children) = &sym.children {
            collect_function_symbols_from_doc(children, uri, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Location, Position, Range, SymbolKind, Url};

    #[test]
    fn test_find_function_in_document_symbols_simple() {
        let uri = Url::parse("file:///test/src/main.rs").unwrap();
        let symbols = vec![lsp_types::DocumentSymbol {
            name: "main".to_string(),
            detail: None,
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 5,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 3,
                },
                end: Position {
                    line: 0,
                    character: 7,
                },
            },
            children: None,
        }];

        let result = find_function_in_document_symbols(&symbols, "main", &uri);
        assert!(result.is_some());
        let symbol = result.unwrap();
        assert_eq!(symbol.name, "main");
        assert_eq!(symbol.kind, SymbolKind::FUNCTION);
    }

    #[test]
    fn test_find_function_in_document_symbols_nested() {
        let uri = Url::parse("file:///test/src/lib.rs").unwrap();
        let symbols = vec![lsp_types::DocumentSymbol {
            name: "MyStruct".to_string(),
            detail: None,
            kind: SymbolKind::STRUCT,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 10,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 7,
                },
                end: Position {
                    line: 0,
                    character: 15,
                },
            },
            children: Some(vec![lsp_types::DocumentSymbol {
                name: "new".to_string(),
                detail: None,
                kind: SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                range: Range {
                    start: Position {
                        line: 5,
                        character: 4,
                    },
                    end: Position {
                        line: 8,
                        character: 4,
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: 5,
                        character: 7,
                    },
                    end: Position {
                        line: 5,
                        character: 10,
                    },
                },
                children: None,
            }]),
        }];

        let result = find_function_in_document_symbols(&symbols, "new", &uri);
        assert!(result.is_some());
        let symbol = result.unwrap();
        assert_eq!(symbol.name, "new");
        assert_eq!(symbol.kind, SymbolKind::FUNCTION);
    }

    #[test]
    fn test_find_function_in_document_symbols_not_found() {
        let uri = Url::parse("file:///test/src/main.rs").unwrap();
        let symbols = vec![lsp_types::DocumentSymbol {
            name: "main".to_string(),
            detail: None,
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 5,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 3,
                },
                end: Position {
                    line: 0,
                    character: 7,
                },
            },
            children: None,
        }];

        let result = find_function_in_document_symbols(&symbols, "nonexistent", &uri);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_function_in_document_symbols_wrong_kind() {
        let uri = Url::parse("file:///test/src/main.rs").unwrap();
        let symbols = vec![lsp_types::DocumentSymbol {
            name: "MyStruct".to_string(),
            detail: None,
            kind: SymbolKind::STRUCT,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 5,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 7,
                },
                end: Position {
                    line: 0,
                    character: 15,
                },
            },
            children: None,
        }];

        // Should not match because it's a struct, not a function
        let result = find_function_in_document_symbols(&symbols, "MyStruct", &uri);
        assert!(result.is_none());
    }

    // --- helpers for collect_function_symbols_from_doc tests ---

    #[allow(deprecated)]
    fn make_doc_symbol(
        name: &str,
        kind: SymbolKind,
        children: Option<Vec<lsp_types::DocumentSymbol>>,
    ) -> lsp_types::DocumentSymbol {
        let pos = Position {
            line: 0,
            character: 0,
        };
        let range = Range {
            start: pos,
            end: pos,
        };
        lsp_types::DocumentSymbol {
            name: name.to_string(),
            detail: None,
            kind,
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children,
        }
    }

    // --- collect_function_symbols_from_doc ---

    #[test]
    fn collect_function_symbols_collects_function_kind() {
        let uri = Url::parse("file:///test/src/main.rs").unwrap();
        let symbols = vec![make_doc_symbol("run", SymbolKind::FUNCTION, None)];
        let mut out = Vec::new();
        collect_function_symbols_from_doc(&symbols, &uri, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "run");
        assert_eq!(out[0].kind, SymbolKind::FUNCTION);
    }

    #[test]
    fn collect_function_symbols_collects_method_kind() {
        let uri = Url::parse("file:///test/src/lib.rs").unwrap();
        let symbols = vec![make_doc_symbol("new", SymbolKind::METHOD, None)];
        let mut out = Vec::new();
        collect_function_symbols_from_doc(&symbols, &uri, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "new");
        assert_eq!(out[0].kind, SymbolKind::METHOD);
    }

    #[test]
    fn collect_function_symbols_skips_non_function_kinds() {
        let uri = Url::parse("file:///test/src/lib.rs").unwrap();
        let symbols = vec![
            make_doc_symbol("MyStruct", SymbolKind::STRUCT, None),
            make_doc_symbol("MY_CONST", SymbolKind::CONSTANT, None),
            make_doc_symbol("my_mod", SymbolKind::MODULE, None),
        ];
        let mut out = Vec::new();
        collect_function_symbols_from_doc(&symbols, &uri, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn collect_function_symbols_recurses_into_children() {
        let uri = Url::parse("file:///test/src/lib.rs").unwrap();
        let method = make_doc_symbol("new", SymbolKind::METHOD, None);
        let parent = make_doc_symbol("MyStruct", SymbolKind::STRUCT, Some(vec![method]));
        let mut out = Vec::new();
        collect_function_symbols_from_doc(&[parent], &uri, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "new");
    }

    #[test]
    fn collect_function_symbols_collects_parent_and_child_functions() {
        let uri = Url::parse("file:///test/src/lib.rs").unwrap();
        let child_fn = make_doc_symbol("helper", SymbolKind::FUNCTION, None);
        let parent_fn = make_doc_symbol("outer", SymbolKind::FUNCTION, Some(vec![child_fn]));
        let mut out = Vec::new();
        collect_function_symbols_from_doc(&[parent_fn], &uri, &mut out);
        let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"outer"), "outer should be collected");
        assert!(names.contains(&"helper"), "helper should be collected");
    }

    #[test]
    fn collect_function_symbols_empty_input_returns_empty() {
        let uri = Url::parse("file:///test/src/main.rs").unwrap();
        let mut out = Vec::new();
        collect_function_symbols_from_doc(&[], &uri, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn collect_function_symbols_uri_is_preserved() {
        let uri = Url::parse("file:///test/src/main.rs").unwrap();
        let symbols = vec![make_doc_symbol("run", SymbolKind::FUNCTION, None)];
        let mut out = Vec::new();
        collect_function_symbols_from_doc(&symbols, &uri, &mut out);
        assert_eq!(out[0].location.uri, uri);
    }
}
