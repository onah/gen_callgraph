use crate::call_graph::meta_resolver;
use crate::call_graph::symbol_locator;
use crate::call_graph::{CallGraph, CallGraphEdge, CallGraphNode};
use crate::lsp;
use crate::lsp::types::Notification;
use lsp_types::{CallHierarchyItem, SymbolInformation, SymbolKind};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub struct CodeAnalyzer {
    client: lsp::LspClient,
}

impl CodeAnalyzer {
    pub fn new(client: lsp::LspClient) -> Self {
        CodeAnalyzer { client }
    }

    pub async fn initialize(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> anyhow::Result<lsp_types::InitializeResult> {
        self.client.initialize(timeout).await
    }

    pub async fn generate_call_graph(&mut self, entry: &str) -> anyhow::Result<CallGraph> {
        self.collect_call_graph_from(entry).await
    }

    pub async fn generate_call_graph_all(&mut self) -> anyhow::Result<CallGraph> {
        self.collect_call_graph_all_symbols().await
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.client.shutdown().await?;
        Ok(())
    }

    pub async fn wait_notification(
        &mut self,
        timeout: Option<Duration>,
    ) -> anyhow::Result<Notification> {
        self.client.wait_notification(timeout).await
    }

    async fn collect_call_graph_from(&mut self, entry: &str) -> anyhow::Result<CallGraph> {
        let function_symbols = self.client.workspace_symbol("").await?;
        let workspace_root_path = self.client.workspace_root_path().to_path_buf();
        let crate_name = self.client.crate_name().to_string();

        let Some(symbol) = symbol_locator::find_function_symbol_with_retry(
            &mut self.client,
            entry,
            20,
            Duration::from_millis(500),
        )
        .await?
        else {
            return Err(anyhow::anyhow!("entry function not found: {}", entry));
        };

        let roots = self
            .client
            .text_document_prepare_call_hierarchy(&symbol)
            .await?;
        if roots.is_empty() {
            return Err(anyhow::anyhow!(
                "no call hierarchy root found for: {}",
                entry
            ));
        }

        let mut visited_nodes: HashSet<String> = HashSet::new();
        let mut visited_edges: HashSet<(String, String)> = HashSet::new();
        let mut node_info: HashMap<String, (String, String)> = HashMap::new();

        traverse_items(
            &mut self.client,
            roots,
            &function_symbols,
            &workspace_root_path,
            &crate_name,
            &mut visited_nodes,
            &mut visited_edges,
            &mut node_info,
        )
        .await?;

        Ok(build_call_graph(node_info, visited_edges))
    }

    async fn collect_call_graph_all_symbols(&mut self) -> anyhow::Result<CallGraph> {
        let function_symbols = self.client.workspace_symbol("").await?;
        let workspace_root_path = self.client.workspace_root_path().to_path_buf();
        let crate_name = self.client.crate_name().to_string();

        // Filter workspace/symbol results to functions/methods within this workspace.
        let mut workspace_functions: Vec<SymbolInformation> = function_symbols
            .iter()
            .filter(|s| {
                (s.kind == SymbolKind::FUNCTION || s.kind == SymbolKind::METHOD)
                    && self.client.is_uri_in_workspace(&s.location.uri)
            })
            .cloned()
            .collect();

        // Fallback: when workspace/symbol returns nothing, scan source files directly.
        if workspace_functions.is_empty() {
            println!("  workspace/symbol returned no results, falling back to source file scan.");
            workspace_functions =
                symbol_locator::find_all_workspace_functions(&mut self.client).await?;
        }

        if workspace_functions.is_empty() {
            return Err(anyhow::anyhow!("no function symbols found in workspace"));
        }
        println!(
            "Found {} function symbols in workspace",
            workspace_functions.len()
        );

        let mut visited_nodes: HashSet<String> = HashSet::new();
        let mut visited_edges: HashSet<(String, String)> = HashSet::new();
        let mut node_info: HashMap<String, (String, String)> = HashMap::new();

        for symbol in &workspace_functions {
            let roots = self
                .client
                .text_document_prepare_call_hierarchy(symbol)
                .await?;

            traverse_items(
                &mut self.client,
                roots,
                &function_symbols,
                &workspace_root_path,
                &crate_name,
                &mut visited_nodes,
                &mut visited_edges,
                &mut node_info,
            )
            .await?;
        }

        Ok(build_call_graph(node_info, visited_edges))
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

fn call_item_key(item: &CallHierarchyItem) -> String {
    format!(
        "{}:{}:{}:{}",
        item.uri,
        item.selection_range.start.line,
        item.selection_range.start.character,
        item.name
    )
}

async fn traverse_items(
    client: &mut lsp::LspClient,
    initial_items: Vec<CallHierarchyItem>,
    function_symbols: &[SymbolInformation],
    workspace_root_path: &std::path::Path,
    crate_name: &str,
    visited_nodes: &mut HashSet<String>,
    visited_edges: &mut HashSet<(String, String)>,
    node_info: &mut HashMap<String, (String, String)>,
) -> anyhow::Result<()> {
    let mut stack = initial_items;

    while let Some(item) = stack.pop() {
        if !client.is_uri_in_workspace(&item.uri) {
            continue;
        }

        let from_id = call_item_key(&item);
        let from_meta = meta_resolver::resolve_function_meta(
            &item,
            function_symbols,
            workspace_root_path,
            crate_name,
        );
        node_info.insert(from_id.clone(), (from_meta.qualified_label, from_meta.group));

        if !visited_nodes.insert(from_id.clone()) {
            continue;
        }

        let outgoing = client.call_hierarchy_outgoing_calls(&item).await?;

        for call in outgoing {
            let child = call.to;
            if !client.is_uri_in_workspace(&child.uri) {
                continue;
            }

            let to_id = call_item_key(&child);
            let to_meta = meta_resolver::resolve_function_meta(
                &child,
                function_symbols,
                workspace_root_path,
                crate_name,
            );
            node_info.insert(to_id.clone(), (to_meta.qualified_label, to_meta.group));
            visited_edges.insert((from_id.clone(), to_id.clone()));

            if !visited_nodes.contains(&to_id) {
                stack.push(child);
            }
        }
    }

    Ok(())
}

fn build_call_graph(
    node_info: HashMap<String, (String, String)>,
    visited_edges: HashSet<(String, String)>,
) -> CallGraph {
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

    CallGraph { nodes, edges }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range, SymbolKind, Url};

    fn make_call_hierarchy_item(
        name: &str,
        uri: &str,
        line: u32,
        character: u32,
    ) -> CallHierarchyItem {
        let url = Url::parse(uri).unwrap();
        let pos = Position { line, character };
        let range = Range { start: pos, end: pos };
        CallHierarchyItem {
            name: name.to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: url,
            range,
            selection_range: range,
            data: None,
        }
    }

    // --- call_item_key ---

    #[test]
    fn call_item_key_format_contains_uri_position_name() {
        let item = make_call_hierarchy_item("foo", "file:///src/main.rs", 10, 4);
        let key = call_item_key(&item);
        assert!(key.contains("file:///src/main.rs"), "key should contain URI");
        assert!(key.contains("10"), "key should contain line number");
        assert!(key.contains("4"), "key should contain character offset");
        assert!(key.contains("foo"), "key should contain function name");
    }

    #[test]
    fn call_item_key_differs_by_line() {
        let a = make_call_hierarchy_item("foo", "file:///src/main.rs", 10, 0);
        let b = make_call_hierarchy_item("foo", "file:///src/main.rs", 20, 0);
        assert_ne!(call_item_key(&a), call_item_key(&b));
    }

    #[test]
    fn call_item_key_differs_by_name() {
        let a = make_call_hierarchy_item("foo", "file:///src/main.rs", 10, 0);
        let b = make_call_hierarchy_item("bar", "file:///src/main.rs", 10, 0);
        assert_ne!(call_item_key(&a), call_item_key(&b));
    }

    #[test]
    fn call_item_key_differs_by_uri() {
        let a = make_call_hierarchy_item("foo", "file:///src/main.rs", 10, 0);
        let b = make_call_hierarchy_item("foo", "file:///src/lib.rs", 10, 0);
        assert_ne!(call_item_key(&a), call_item_key(&b));
    }

    // --- build_call_graph ---

    #[test]
    fn build_call_graph_empty_input_produces_empty_graph() {
        let graph = build_call_graph(HashMap::new(), HashSet::new());
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn build_call_graph_nodes_are_sorted_by_id() {
        let mut node_info = HashMap::new();
        node_info.insert("c::foo".to_string(), ("foo".to_string(), "c".to_string()));
        node_info.insert("a::bar".to_string(), ("bar".to_string(), "a".to_string()));
        node_info.insert("b::baz".to_string(), ("baz".to_string(), "b".to_string()));
        let graph = build_call_graph(node_info, HashSet::new());
        let ids: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec!["a::bar", "b::baz", "c::foo"]);
    }

    #[test]
    fn build_call_graph_edges_are_sorted_by_from_then_to() {
        let edges = HashSet::from([
            ("a".to_string(), "z".to_string()),
            ("b".to_string(), "c".to_string()),
            ("a".to_string(), "b".to_string()),
        ]);
        let graph = build_call_graph(HashMap::new(), edges);
        let pairs: Vec<(&str, &str)> = graph
            .edges
            .iter()
            .map(|e| (e.from.as_str(), e.to.as_str()))
            .collect();
        assert_eq!(pairs, vec![("a", "b"), ("a", "z"), ("b", "c")]);
    }

    #[test]
    fn build_call_graph_node_label_and_group_are_preserved() {
        let mut node_info = HashMap::new();
        node_info.insert(
            "id1".to_string(),
            ("my::label".to_string(), "my_group".to_string()),
        );
        let graph = build_call_graph(node_info, HashSet::new());
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id, "id1");
        assert_eq!(graph.nodes[0].label, "my::label");
        assert_eq!(graph.nodes[0].group, "my_group");
    }
}