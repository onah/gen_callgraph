pub mod framed;
pub mod framed_wrapper;
pub mod message_creator;
pub mod message_parser;
pub mod stdio_transport;
pub mod transport;
pub mod types;

/// Common boxed error type for LSP module boundaries.
// Using `anyhow::Error` directly across the codebase; removed `DynError alias.
use crate::lsp::framed::FramedTransport;
use crate::lsp::types::Message;
use lsp_types::{CallHierarchyItem, CallHierarchyOutgoingCall, SymbolInformation, SymbolKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub struct CallGraphNode {
    pub id: String,
    pub label: String,
    pub group: String,
}

#[derive(Debug, Clone)]
pub struct CallGraphEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct CallGraph {
    pub nodes: Vec<CallGraphNode>,
    pub edges: Vec<CallGraphEdge>,
}

#[derive(Debug, Clone)]
struct FunctionMeta {
    qualified_label: String,
    group: String,
}

pub struct LspClient {
    communicator: Box<dyn FramedTransport + Send + Sync>,
    message_builder: message_creator::MessageBuilder,
    workspace_root: String,
    workspace_root_path: PathBuf,
    crate_name: String,
}

impl LspClient {
    pub fn new(
        transport: Box<dyn crate::lsp::transport::LspTransport + Send + Sync>,
        workspace_root: String,
    ) -> Self {
        let message_builder = message_creator::MessageBuilder::new();
        let framed = crate::lsp::framed_wrapper::FramedBox::new(transport);
        let workspace_root_path = std::fs::canonicalize(&workspace_root)
            .unwrap_or_else(|_| PathBuf::from(workspace_root.clone()));
        let crate_name =
            Self::read_crate_name(&workspace_root_path).unwrap_or_else(|| String::from("crate"));
        LspClient {
            communicator: Box::new(framed),
            message_builder,
            workspace_root,
            workspace_root_path,
            crate_name,
        }
    }

    fn read_crate_name(workspace_root_path: &Path) -> Option<String> {
        let cargo = workspace_root_path.join("Cargo.toml");
        let text = std::fs::read_to_string(cargo).ok()?;
        let mut in_package = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_package = trimmed == "[package]";
                continue;
            }
            if in_package && trimmed.starts_with("name") {
                let (_, rhs) = trimmed.split_once('=')?;
                let value = rhs.trim().trim_matches('"').trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        None
    }

    fn is_uri_in_workspace(&self, uri: &lsp_types::Url) -> bool {
        let Ok(path) = uri.to_file_path() else {
            return false;
        };
        let normalized = std::fs::canonicalize(&path).unwrap_or(path);
        normalized.starts_with(&self.workspace_root_path)
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let request = self.message_builder.initialize(&self.workspace_root)?;
        // send request via framed transport and wait for response
        let id = self.communicator.send_request(request).await?;
        let _resp = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        let initialized_notification = self.message_builder.initialized_notification()?;
        // send initialized notification
        self.communicator
            .send_notification(initialized_notification)
            .await?;

        Ok(())
    }

    pub async fn get_all_function_list(&mut self) -> anyhow::Result<()> {
        let symbols = self.get_workspace_function_symbols().await?;
        if symbols.is_empty() {
            return Err(anyhow::anyhow!(
                "workspace function symbols are not ready yet"
            ));
        }

        for symbol in symbols {
            println!("Function: {}", symbol.name);
        }

        Ok(())
    }

    async fn get_workspace_function_symbols(&mut self) -> anyhow::Result<Vec<SymbolInformation>> {
        let request = self
            .message_builder
            .create_request("workspace/symbol", Some(serde_json::json!({"query": ""})))?;

        let id = self.communicator.send_request(request).await?;
        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                let result = response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("workspace/symbol response has no result"))?;
                let symbols: Vec<SymbolInformation> = serde_json::from_value(result)?;
                Ok(symbols
                    .into_iter()
                    .filter(|s| {
                        (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                            && self.is_uri_in_workspace(&s.location.uri)
                    })
                    .collect())
            }
            Message::Error(_) => Ok(vec![]),
            Message::Notification(_) => Ok(vec![]),
        }
    }

    async fn find_function_symbol(
        &mut self,
        query: &str,
    ) -> anyhow::Result<Option<SymbolInformation>> {
        let request = self.message_builder.create_request(
            "workspace/symbol",
            Some(serde_json::json!({"query": query})),
        )?;

        let id = self.communicator.send_request(request).await?;
        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                let result = response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("workspace/symbol response has no result"))?;
                let symbols: Vec<SymbolInformation> = serde_json::from_value(result)?;

                let exact = symbols
                    .iter()
                    .find(|s| {
                        (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                            && s.name == query
                            && self.is_uri_in_workspace(&s.location.uri)
                    })
                    .cloned();
                if exact.is_some() {
                    return Ok(exact);
                }

                let partial = symbols
                    .iter()
                    .find(|s| {
                        (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                            && self.is_uri_in_workspace(&s.location.uri)
                    })
                    .cloned();
                Ok(partial)
            }
            Message::Error(_) => Ok(None),
            Message::Notification(_) => Ok(None),
        }
    }

    async fn find_function_symbol_with_retry(
        &mut self,
        query: &str,
        max_attempts: usize,
        interval: Duration,
    ) -> anyhow::Result<Option<SymbolInformation>> {
        for attempt in 1..=max_attempts {
            if let Some(symbol) = self.find_function_symbol(query).await? {
                return Ok(Some(symbol));
            }

            if attempt < max_attempts {
                sleep(interval).await;
            }
        }

        Ok(None)
    }

    async fn prepare_call_hierarchy(
        &mut self,
        symbol: &SymbolInformation,
    ) -> anyhow::Result<Vec<CallHierarchyItem>> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": symbol.location.uri
            },
            "position": symbol.location.range.start
        });

        let request = self
            .message_builder
            .create_request("textDocument/prepareCallHierarchy", Some(params))?;

        let id = self.communicator.send_request(request).await?;
        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                let result = response.result.ok_or_else(|| {
                    anyhow::anyhow!("prepareCallHierarchy response has no result")
                })?;
                let items: Vec<CallHierarchyItem> = serde_json::from_value(result)?;
                Ok(items)
            }
            Message::Error(_) => Ok(vec![]),
            Message::Notification(_) => Ok(vec![]),
        }
    }

    async fn get_outgoing_calls(
        &mut self,
        item: &CallHierarchyItem,
    ) -> anyhow::Result<Vec<CallHierarchyOutgoingCall>> {
        let request = self.message_builder.create_request(
            "callHierarchy/outgoingCalls",
            Some(serde_json::json!({"item": item})),
        )?;

        let id = self.communicator.send_request(request).await?;
        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                if let Some(result) = response.result {
                    let calls: Vec<CallHierarchyOutgoingCall> = serde_json::from_value(result)?;
                    Ok(calls)
                } else {
                    Ok(vec![])
                }
            }
            Message::Error(_) => Ok(vec![]),
            Message::Notification(_) => Ok(vec![]),
        }
    }

    fn call_item_key(item: &CallHierarchyItem) -> String {
        format!(
            "{}:{}:{}:{}",
            item.uri,
            item.selection_range.start.line,
            item.selection_range.start.character,
            item.name
        )
    }

    fn resolve_function_meta(
        &self,
        item: &CallHierarchyItem,
        function_symbols: &[SymbolInformation],
    ) -> FunctionMeta {
        let same_file_and_name: Vec<&SymbolInformation> = function_symbols
            .iter()
            .filter(|s| s.name == item.name && s.location.uri == item.uri)
            .collect();

        let matched = same_file_and_name
            .iter()
            .min_by_key(|s| {
                let a = s.location.range.start.line as i64;
                let b = item.selection_range.start.line as i64;
                (a - b).abs()
            })
            .copied();

        if let Some(symbol) = matched {
            if let Some(container) = &symbol.container_name {
                let group = container.trim().to_string();
                if !group.is_empty() {
                    return FunctionMeta {
                        qualified_label: format!("{}::{}", group, item.name),
                        group,
                    };
                }
            }
        }

        if let Some(detail) = &item.detail {
            let detail = detail.trim();
            if let Some(after_impl) = detail.strip_prefix("impl ") {
                let candidate = if let Some(for_pos) = after_impl.find(" for ") {
                    after_impl[for_pos + 5..].trim()
                } else if after_impl.starts_with('<') {
                    if let Some(end) = after_impl.find('>') {
                        after_impl[end + 1..].trim()
                    } else {
                        after_impl
                    }
                } else {
                    after_impl
                };

                let base = candidate
                    .split_whitespace()
                    .next()
                    .unwrap_or(candidate)
                    .split('<')
                    .next()
                    .unwrap_or(candidate)
                    .trim()
                    .to_string();

                if !base.is_empty() {
                    return FunctionMeta {
                        qualified_label: format!("{}::{}", base, item.name),
                        group: base,
                    };
                }
            }
        }

        if let Some(owner) = Self::infer_impl_owner_from_source(item) {
            return FunctionMeta {
                qualified_label: format!("{}::{}", owner, item.name),
                group: owner,
            };
        }

        if let Some(module) = self.infer_module_owner_from_uri(item) {
            return FunctionMeta {
                qualified_label: format!("{}::{}", module, item.name),
                group: module,
            };
        }

        FunctionMeta {
            qualified_label: item.name.clone(),
            group: String::from("global"),
        }
    }

    fn infer_impl_owner_from_source(item: &CallHierarchyItem) -> Option<String> {
        let path = item.uri.to_file_path().ok()?;
        let text = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return None;
        }

        let target_line = (item.selection_range.start.line as usize).min(lines.len() - 1);
        let fn_line = Self::find_nearest_fn_line(&lines, target_line, &item.name)?;

        for start in (0..=fn_line).rev() {
            if !Self::looks_like_impl_header_start(lines[start]) {
                continue;
            }

            let header = Self::collect_header_until_brace(&lines, start);
            if !header.contains("impl") {
                continue;
            }

            if !Self::header_block_contains_line(&lines, start, fn_line) {
                continue;
            }

            if let Some(owner) = Self::parse_impl_owner(&header) {
                return Some(owner);
            }
        }

        None
    }

    fn looks_like_impl_header_start(line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with("impl ")
            || trimmed.starts_with("impl<")
            || trimmed.starts_with("unsafe impl ")
            || trimmed.starts_with("unsafe impl<")
    }

    fn find_nearest_fn_line(lines: &[&str], target_line: usize, fn_name: &str) -> Option<usize> {
        let marker = format!("fn {}", fn_name);
        let mut i = target_line;
        loop {
            if lines[i].contains(&marker) {
                return Some(i);
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
        None
    }

    fn collect_header_until_brace(lines: &[&str], start: usize) -> String {
        let mut joined = String::new();
        let end = (start + 8).min(lines.len());
        for line in lines.iter().take(end).skip(start) {
            if !joined.is_empty() {
                joined.push(' ');
            }
            joined.push_str(line.trim());
            if line.contains('{') {
                break;
            }
        }
        joined
    }

    fn header_block_contains_line(lines: &[&str], start: usize, target: usize) -> bool {
        let mut depth: i32 = 0;
        for line in lines.iter().take(target + 1).skip(start) {
            for ch in line.chars() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                }
            }
        }
        depth > 0
    }

    fn parse_impl_owner(header: &str) -> Option<String> {
        let after_impl = header.split_once("impl")?.1.trim();

        let candidate = if let Some(pos) = after_impl.find(" for ") {
            after_impl[pos + 5..].trim()
        } else if after_impl.starts_with('<') {
            if let Some(end) = after_impl.find('>') {
                after_impl[end + 1..].trim()
            } else {
                after_impl
            }
        } else {
            after_impl
        };

        let token = candidate
            .split_whitespace()
            .next()
            .unwrap_or(candidate)
            .trim_matches('{')
            .trim_matches('(')
            .trim_matches(')')
            .trim_matches('&')
            .trim_start_matches("mut ")
            .split('<')
            .next()
            .unwrap_or(candidate)
            .trim();

        if token.is_empty() {
            None
        } else {
            Some(token.to_string())
        }
    }

    fn infer_module_owner_from_uri(&self, item: &CallHierarchyItem) -> Option<String> {
        let path = item.uri.to_file_path().ok()?;
        let rel = path.strip_prefix(&self.workspace_root_path).ok()?;

        // src/main.rs and src/lib.rs are treated as project root module (crate name).
        if rel == Path::new("src/main.rs") || rel == Path::new("src/lib.rs") {
            return Some(self.crate_name.clone());
        }

        if !rel.starts_with("src") {
            return None;
        }

        let no_src = rel.strip_prefix("src").ok()?;
        let mut parts: Vec<String> = no_src
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        if parts.is_empty() {
            return None;
        }

        if let Some(last) = parts.last_mut() {
            if last == "mod.rs" {
                parts.pop();
            } else if let Some(stem) = last.strip_suffix(".rs") {
                *last = stem.to_string();
            }
        }

        if parts.is_empty() {
            return Some(self.crate_name.clone());
        }

        Some(parts.join("::"))
    }

    pub async fn collect_call_graph_from(&mut self, entry: &str) -> anyhow::Result<CallGraph> {
        let Some(symbol) = self
            .find_function_symbol_with_retry(entry, 20, Duration::from_millis(500))
            .await?
        else {
            return Err(anyhow::anyhow!("entry function not found: {}", entry));
        };

        let function_symbols = self.get_workspace_function_symbols().await?;

        let roots = self.prepare_call_hierarchy(&symbol).await?;
        if roots.is_empty() {
            return Err(anyhow::anyhow!(
                "no call hierarchy root found for: {}",
                entry
            ));
        }

        let mut visited_nodes: HashSet<String> = HashSet::new();
        let mut visited_edges: HashSet<(String, String)> = HashSet::new();
        let mut node_info: HashMap<String, (String, String)> = HashMap::new();
        let mut stack: Vec<CallHierarchyItem> = roots;

        while let Some(item) = stack.pop() {
            if !self.is_uri_in_workspace(&item.uri) {
                continue;
            }

            let from_id = Self::call_item_key(&item);
            let from_meta = self.resolve_function_meta(&item, &function_symbols);
            node_info.insert(
                from_id.clone(),
                (from_meta.qualified_label, from_meta.group),
            );

            if !visited_nodes.insert(from_id.clone()) {
                continue;
            }

            let outgoing = self.get_outgoing_calls(&item).await?;

            for call in outgoing {
                let child = call.to;
                if !self.is_uri_in_workspace(&child.uri) {
                    continue;
                }

                let to_id = Self::call_item_key(&child);
                let to_meta = self.resolve_function_meta(&child, &function_symbols);
                node_info.insert(to_id.clone(), (to_meta.qualified_label, to_meta.group));
                visited_edges.insert((from_id.clone(), to_id.clone()));

                if !visited_nodes.contains(&to_id) {
                    stack.push(child);
                }
            }
        }

        let mut nodes: Vec<CallGraphNode> = node_info
            .into_iter()
            .map(|(id, (label, group))| CallGraphNode { id, label, group })
            .collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));

        let mut edges: Vec<CallGraphEdge> = visited_edges
            .into_iter()
            .map(|(from, to)| CallGraphEdge { from, to })
            .collect();
        edges.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));

        Ok(CallGraph { nodes, edges })
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        let request = self.message_builder.create_request("shutdown", Some(""))?;

        // send shutdown request and wait for response
        let id = self.communicator.send_request(request).await?;

        let _response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        let notification = self.message_builder.create_notification("exit", Some(""))?;
        self.communicator.send_notification(notification).await?;

        Ok(())
    }
    /*
    pub async fn did_open_notification(
        &mut self,
        file_path: &str,
        file_contents: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let notification = self
            .message_creator
            .did_open_notification(file_path, file_contents)?;
        let message = serde_json::to_string(&notification)?;
        self.communicator.send_message2(&message).await?;

        Ok(())
    }
    */
}
