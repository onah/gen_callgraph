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
) -> anyhow::Result<Option<SymbolInformation>> {
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
) -> anyhow::Result<Option<SymbolInformation>> {
    // Try exact query first
    let symbols = client.workspace_symbol(query).await?;

    // If exact query returns nothing or very few results, try empty query to get all symbols
    let all_symbols = if symbols.len() < 5 {
        client.workspace_symbol("").await?
    } else {
        symbols.clone()
    };

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
}
