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
use std::collections::HashSet;
use std::time::Duration;

pub struct LspClient {
    communicator: Box<dyn FramedTransport + Send + Sync>,
    message_builder: message_creator::MessageBuilder,
    workspace_root: String,
}

impl LspClient {
    pub fn new(
        transport: Box<dyn crate::lsp::transport::LspTransport + Send + Sync>,
        workspace_root: String,
    ) -> Self {
        let message_builder = message_creator::MessageBuilder::new();
        let framed = crate::lsp::framed_wrapper::FramedBox::new(transport);
        LspClient {
            communicator: Box::new(framed),
            message_builder,
            workspace_root,
        }
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
                    .find(|s| s.kind == SymbolKind::FUNCTION && s.name == query)
                    .cloned();
                if exact.is_some() {
                    return Ok(exact);
                }

                let partial = symbols
                    .iter()
                    .find(|s| s.kind == SymbolKind::FUNCTION)
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

    pub async fn print_call_order_from(&mut self, entry: &str) -> anyhow::Result<()> {
        let Some(symbol) = self.find_function_symbol(entry).await? else {
            println!("Entry function not found: {}", entry);
            return Ok(());
        };

        let roots = self.prepare_call_hierarchy(&symbol).await?;
        if roots.is_empty() {
            println!("No call hierarchy root found for: {}", entry);
            return Ok(());
        }

        let mut visited = HashSet::new();
        let mut stack: Vec<(CallHierarchyItem, usize)> =
            roots.into_iter().rev().map(|item| (item, 0usize)).collect();

        println!("Call order (DFS):");

        while let Some((item, depth)) = stack.pop() {
            let key = Self::call_item_key(&item);
            if !visited.insert(key) {
                continue;
            }

            let indent = "  ".repeat(depth);
            println!("{}- {}", indent, item.name);

            let outgoing = self.get_outgoing_calls(&item).await?;
            let mut children: Vec<CallHierarchyItem> = outgoing.into_iter().map(|c| c.to).collect();
            children.reverse();
            for child in children {
                stack.push((child, depth + 1));
            }
        }

        Ok(())
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
