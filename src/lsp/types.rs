use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: i32,
    pub method: String,
    pub params: serde_json::Value,
}

impl Request {
    pub fn new(id: i32, method: String, params: serde_json::Value) -> Self {
        Request {
            jsonrpc: "2.0".to_string(),
            id,
            method,
            params,
        }
    }
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
    pub params: serde_json::Value,
}

impl Notification {
    pub fn new(method: String, params: serde_json::Value) -> Self {
        Notification {
            jsonrpc: "2.0".to_string(),
            method,
            params,
        }
    }
}

pub enum Message {
    Response(ResponseMessage),
    Error(ResponseError),
    Notification(Notification),
}

pub enum SendMessage {
    Request(Request),
    Notification(Notification),
}
