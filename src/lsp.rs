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
use lsp_types::{CallHierarchyItem, CallHierarchyOutgoingCall, SymbolKind, SymbolInformation};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

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
        LspClient {
            communicator: Box::new(framed),
            message_builder,
            workspace_root,
            workspace_root_path,
        }
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
        let request = self
            .message_builder
            .create_request("workspace/symbol", Some(serde_json::json!({"query": ""})))?;

        // send request and wait for response
        let id = self.communicator.send_request(request).await?;

        let response = self
            .communicator
            .receive_response_with_timeout(id, Some(Duration::from_secs(10)))
            .await?;

        match response {
            Message::Response(response) => {
                let symbols: Vec<lsp_types::SymbolInformation> =
                    serde_json::from_value(response.result.unwrap()).unwrap();

                for symbol in symbols {
                    match symbol.kind {
                        SymbolKind::FUNCTION => println!("Function: {}", symbol.name),
                        SymbolKind::STRUCT => println!("Struct: {}", symbol.name),
                        _ => {}
                    }
                }
            }
            Message::Error(_response) => {
                // handle error
            }
            Message::Notification(_notification) => {
                // ignore notifications here
            }
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
                        s.kind == SymbolKind::FUNCTION && self.is_uri_in_workspace(&s.location.uri)
                    })
                    .collect())
            }
            Message::Error(_) => Ok(vec![]),
            Message::Notification(_) => Ok(vec![]),
        }
    }

    async fn find_function_symbol(&mut self, query: &str) -> anyhow::Result<Option<SymbolInformation>> {
        let request = self
            .message_builder
            .create_request("workspace/symbol", Some(serde_json::json!({"query": query})))?;

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
                        s.kind == SymbolKind::FUNCTION
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
                        s.kind == SymbolKind::FUNCTION && self.is_uri_in_workspace(&s.location.uri)
                    })
                    .cloned();
                Ok(partial)
            }
            Message::Error(_) => Ok(None),
            Message::Notification(_) => Ok(None),
        }
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
                let result = response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("prepareCallHierarchy response has no result"))?;
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
        let request = self
            .message_builder
            .create_request("callHierarchy/outgoingCalls", Some(serde_json::json!({"item": item})))?;

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
        item: &CallHierarchyItem,
        function_symbols: &[SymbolInformation],
    ) -> FunctionMeta {
        let matched = function_symbols.iter().find(|s| {
            s.name == item.name
                && s.location.uri == item.uri
                && s.location.range.start.line == item.selection_range.start.line
        });

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

        FunctionMeta {
            qualified_label: item.name.clone(),
            group: String::from("global"),
        }
    }

    pub async fn collect_call_graph_from(&mut self, entry: &str) -> anyhow::Result<CallGraph> {
        let function_symbols = self.get_workspace_function_symbols().await?;

        let Some(symbol) = self.find_function_symbol(entry).await? else {
            return Err(anyhow::anyhow!("entry function not found: {}", entry));
        };

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
            let from_meta = Self::resolve_function_meta(&item, &function_symbols);
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
                let to_meta = Self::resolve_function_meta(&child, &function_symbols);
                node_info.insert(
                    to_id.clone(),
                    (to_meta.qualified_label, to_meta.group),
                );
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
