use crate::lsp;

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
        self.client.get_all_function_list().await?;
        Ok(())
    }

    pub async fn generate_call_graph_dot(&mut self, entry: &str) -> anyhow::Result<String> {
        let graph = self.client.collect_call_graph_from(entry).await?;
        Ok(crate::dot::to_dot(&graph))
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.client.shutdown().await?;
        Ok(())
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
