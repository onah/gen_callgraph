use crate::call_graph::{CallGraph, CallGraphEdge, CallGraphNode};
use crate::lsp;
use lsp_types::{CallHierarchyItem, SymbolInformation, SymbolKind};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone)]
struct FunctionMeta {
    qualified_label: String,
    group: String,
}

pub struct CodeAnalyzer {
    client: lsp::LspClient,
}

impl CodeAnalyzer {
    pub fn new(client: lsp::LspClient) -> Self {
        CodeAnalyzer { client }
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        self.client.initialize().await?;
        Ok(())
    }

    pub async fn get_all_function_list(&mut self) -> anyhow::Result<()> {
        let symbols = self.workspace_function_symbols().await?;
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

    pub async fn generate_call_graph_dot(&mut self, entry: &str) -> anyhow::Result<String> {
        let graph = self.collect_call_graph_from(entry).await?;
        Ok(crate::dot::to_dot(&graph))
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.client.shutdown().await?;
        Ok(())
    }

    async fn collect_call_graph_from(&mut self, entry: &str) -> anyhow::Result<CallGraph> {
        let Some(symbol) = self
            .find_function_symbol_with_retry(entry, 20, Duration::from_millis(500))
            .await?
        else {
            return Err(anyhow::anyhow!("entry function not found: {}", entry));
        };

        let function_symbols = self.workspace_function_symbols().await?;

        let roots = self.client.text_document_prepare_call_hierarchy(&symbol).await?;
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
            if !self.client.is_uri_in_workspace(&item.uri) {
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

            let outgoing = self.client.call_hierarchy_outgoing_calls(&item).await?;

            for call in outgoing {
                let child = call.to;
                if !self.client.is_uri_in_workspace(&child.uri) {
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

    async fn workspace_function_symbols(&mut self) -> anyhow::Result<Vec<SymbolInformation>> {
        let symbols = self.client.workspace_symbol("").await?;
        Ok(symbols
            .into_iter()
            .filter(|s| {
                (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                    && self.client.is_uri_in_workspace(&s.location.uri)
            })
            .collect())
    }

    async fn find_function_symbol(&mut self, query: &str) -> anyhow::Result<Option<SymbolInformation>> {
        let symbols = self.client.workspace_symbol(query).await?;

        let exact = symbols
            .iter()
            .find(|s| {
                (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                    && s.name == query
                    && self.client.is_uri_in_workspace(&s.location.uri)
            })
            .cloned();
        if exact.is_some() {
            return Ok(exact);
        }

        let partial = symbols
            .iter()
            .find(|s| {
                (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                    && self.client.is_uri_in_workspace(&s.location.uri)
            })
            .cloned();
        Ok(partial)
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
        let rel = path.strip_prefix(self.client.workspace_root_path()).ok()?;

        if rel == Path::new("src/main.rs") || rel == Path::new("src/lib.rs") {
            return Some(self.client.crate_name().to_string());
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
            return Some(self.client.crate_name().to_string());
        }

        Some(parts.join("::"))
    }

    /*
        pub async fn collect_outgoing_calls(
            &mut self,
            file_path: &str,
        ) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
    */

    /*

    pub async fn _get_main_function_location(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // send textDocumetn/didOpen notification

        let file_path = "c:/Users/PCuser/Work/rust/gen_callgraph/src/communicate_lsp.rs";
        let file_contents = fs::read_to_string(file_path).unwrap();

        let did_open_notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", file_path),
                    "languageId": "rust",
                    "version": 1,
                    "text": file_contents
                }
            }
        });

        self.client
            .send_message(&did_open_notification)
            .await
            .unwrap();

        // send textDocument/documentSymbol request

        let request = Request {
            jsonrpc: "2.0".to_string(),
            id: 3,
            method: "textDocument/documentSymbol".to_string(),
            params: Some(serde_json::json!({
                "textDocument": {
                    "uri": "file:///c:/Users/PCuser/Work/rust/gen_callgraph/src/communicate_lsp.rs"
                }
            })),
        };

        self.client.send_message(&request).await.unwrap();
        let response = self.client.receive_message().await?;

        match response {
            Message::Response(response) => {
                let symbols: Vec<DocumentSymbol> =
                    serde_json::from_value(response.result.unwrap()).unwrap();

                for symbol in symbols {
                    println!("{:#?}", symbol);
                }

                //println!("{:#?}", response.result.unwrap());
            }
            Message::Error(response) => {
                println!("{:#?}", response.error.unwrap());
            }
            Message::Notification(notification) => {
                println!("{:#?}", notification);
            }
        }

        let request = Request {
            jsonrpc: "2.0".to_string(),
            id: 4,
            method: "textDocument/prepareCallHierarchy".to_string(),
            params: Some(serde_json::json!({
                "textDocument": {
                    "uri": "file:///c:/Users/PCuser/Work/rust/gen_callgraph/src/communicate_lsp.rs"
                },
                "position": {
                    "line": 0,
                    "character": 0
                }
            })),
        };

        self.client.send_message(&request).await?;
        let response = self.client.receive_message().await?;
        match response {
            Message::Response(response) => {
                println!("{:#?}", response.result.unwrap());
            }
            Message::Error(response) => {
                println!("{:#?}", response.error.unwrap());
            }
            Message::Notification(notification) => {
                println!("{:#?}", notification);
            }
        }
        Ok(())
    }
    */
}
