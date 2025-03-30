use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: i32,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResponseMessage {
    pub jsonrpc: String,
    pub id: i32,
    pub result: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
pub struct ResponseError {
    pub jsonrpc: String,
    pub id: i32,
    pub error: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
}
pub enum Message {
    Response(ResponseMessage),
    Error(ResponseError),
    Notification(Notification),
}

pub struct MesssageFuctory {
    id: i32,
}

impl MesssageFuctory {
    pub fn new() -> Self {
        MesssageFuctory { id: 0 }
    }

    pub fn get_id(&mut self) -> i32 {
        self.id += 1;
        self.id
    }

    pub fn create_request<T: Serialize>(&mut self, method: &str, params: Option<T>) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: self.get_id(),
            method: method.to_string(),
            params: params.map(|p| serde_json::to_value(p).unwrap()),
        }
    }

    pub fn create_notification<T: Serialize>(
        &mut self,
        method: &str,
        params: Option<T>,
    ) -> Notification {
        Notification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: params.map(|p| serde_json::to_value(p).unwrap()),
        }
    }
}
