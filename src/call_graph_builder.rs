use crate::call_graph::meta_resolver;
use crate::call_graph::symbol_locator;
use crate::call_graph::{CallGraph, CallGraphEdge, CallGraphNode};
use crate::lsp;
use crate::lsp::types::Notification;
use lsp_types::CallHierarchyItem;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

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

    pub async fn generate_call_graph(&mut self, entry: &str) -> anyhow::Result<CallGraph> {
        self.collect_call_graph_from(entry).await
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

        let function_symbols = symbol_locator::workspace_function_symbols(&mut self.client).await?;

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
        let mut stack: Vec<CallHierarchyItem> = roots;

        while let Some(item) = stack.pop() {
            if !self.client.is_uri_in_workspace(&item.uri) {
                continue;
            }

            let from_id = Self::call_item_key(&item);
            let from_meta = meta_resolver::resolve_function_meta(
                &item,
                &function_symbols,
                self.client.workspace_root_path(),
                self.client.crate_name(),
            );
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
                let to_meta = meta_resolver::resolve_function_meta(
                    &child,
                    &function_symbols,
                    self.client.workspace_root_path(),
                    self.client.crate_name(),
                );
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

    fn call_item_key(item: &CallHierarchyItem) -> String {
        format!(
            "{}:{}:{}:{}",
            item.uri,
            item.selection_range.start.line,
            item.selection_range.start.character,
            item.name
        )
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
