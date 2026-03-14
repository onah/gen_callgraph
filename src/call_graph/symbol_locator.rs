use crate::lsp;
use lsp_types::{SymbolInformation, SymbolKind};
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

    Ok(None)
}

pub(crate) async fn workspace_function_symbols(
    client: &mut lsp::LspClient,
) -> anyhow::Result<Vec<SymbolInformation>> {
    let symbols = client.workspace_symbol("").await?;
    Ok(symbols
        .into_iter()
        .filter(|s| {
            (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                && client.is_uri_in_workspace(&s.location.uri)
        })
        .collect())
}

async fn find_function_symbol(
    client: &mut lsp::LspClient,
    query: &str,
) -> anyhow::Result<Option<SymbolInformation>> {
    let symbols = client.workspace_symbol(query).await?;

    let exact = symbols
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

    let partial = symbols
        .iter()
        .find(|s| {
            (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                && client.is_uri_in_workspace(&s.location.uri)
        })
        .cloned();
    Ok(partial)
}
