use crate::lsp::types::{Message, Notification, ResponseError, ResponseMessage};
use anyhow::anyhow;

pub fn parse_notification(json: &serde_json::Value) -> anyhow::Result<Option<Notification>> {
    if json.get("method").is_some() {
        let notification: Notification = serde_json::from_value(json.clone())?;
        return Ok(Some(notification));
    }
    Ok(None)
}

pub fn parse_response(json: &serde_json::Value) -> anyhow::Result<Option<Message>> {
    if json.get("id").is_some() {
        if json.get("result").is_some() {
            let response: ResponseMessage = serde_json::from_value(json.clone())?;
            return Ok(Some(Message::Response(response)));
        } else {
            let response: ResponseError = serde_json::from_value(json.clone())?;
            return Ok(Some(Message::Error(response)));
        }
    }
    Ok(None)
}
/// Parse a full JSON payload (bytes) into a `Message` (Notification/Response/Error).
pub fn parse_message_from_slice(s: &[u8]) -> anyhow::Result<Message> {
    let json: serde_json::Value = serde_json::from_slice(s)?;
    if let Some(notification) = parse_notification(&json)? {
        return Ok(Message::Notification(notification));
    }
    if let Some(response) = parse_response(&json)? {
        return Ok(response);
    }
    Err(anyhow!("Other Message"))
}
