use crate::lsp::types::{Message, Notification, ResponseError, ResponseMessage};
use crate::lsp::DynError;

pub fn parse_notification(json: &serde_json::Value) -> Result<Option<Notification>, DynError> {
    if json.get("method").is_some() {
        let notification: Notification = serde_json::from_value(json.clone())?;
        return Ok(Some(notification));
    }
    Ok(None)
}

pub fn parse_response(json: &serde_json::Value) -> Result<Option<Message>, DynError> {
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

/// Parse a full JSON string (payload) into a `Message` (Notification/Response/Error).
pub fn parse_message_from_str(s: &str) -> Result<Message, DynError> {
    let json: serde_json::Value = serde_json::from_str(s)?;
    if let Some(notification) = parse_notification(&json)? {
        return Ok(Message::Notification(notification));
    }
    if let Some(response) = parse_response(&json)? {
        return Ok(response);
    }
    Err("Other Message".into())
}
