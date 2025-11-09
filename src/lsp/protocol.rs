use crate::lsp::message_creator::{Message, Notification, ResponseError, ResponseMessage};

/// Common boxed error type for async boundaries in this crate.
pub type DynError = Box<dyn std::error::Error + Send + Sync>;

pub fn parse_notification(
    json: &serde_json::Value,
) -> Result<Option<Notification>, DynError> {
    if json.get("method").is_some() {
        let notification: Notification = serde_json::from_value(json.clone())?;
        return Ok(Some(notification));
    }
    Ok(None)
}

pub fn parse_response(
    json: &serde_json::Value,
) -> Result<Option<Message>, DynError> {
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
