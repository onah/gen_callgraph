use crate::lsp::types::{Message, Notification, ResponseError, ResponseMessage};
use anyhow::anyhow;

pub fn parse_notification(json: &serde_json::Value) -> anyhow::Result<Option<Notification>> {
    if json.get("method").is_some() && json.get("id").is_none() {
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
        }

        if json.get("error").is_some() {
            let response: ResponseError = serde_json::from_value(json.clone())?;
            return Ok(Some(Message::Error(response)));
        }
    }
    Ok(None)
}

fn is_server_request(json: &serde_json::Value) -> bool {
    json.get("id").is_some() && json.get("method").is_some()
}

pub fn parse_server_request_from_slice(s: &[u8]) -> anyhow::Result<Option<(i32, String)>> {
    let json: serde_json::Value = serde_json::from_slice(s)?;
    if !is_server_request(&json) {
        return Ok(None);
    }

    let id = json
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow!("invalid server request id"))?;
    let method = json
        .get("method")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("invalid server request method"))?;

    let id = i32::try_from(id).map_err(|_| anyhow!("server request id out of range"))?;
    Ok(Some((id, method.to_string())))
}

/// Parse a full JSON payload (bytes) into a `Message` (Notification/Response/Error).
pub fn parse_message_from_slice(s: &[u8]) -> anyhow::Result<Message> {
    let json: serde_json::Value = serde_json::from_slice(s)?;

    if is_server_request(&json) {
        return Err(anyhow!("server request is not supported yet"));
    }

    if let Some(notification) = parse_notification(&json)? {
        return Ok(Message::Notification(notification));
    }

    if let Some(response) = parse_response(&json)? {
        return Ok(response);
    }

    Err(anyhow!("Other Message"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_request_is_not_parsed_as_notification() {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "client/registerCapability",
            "params": {}
        });

        let bytes = serde_json::to_vec(&payload).unwrap();
        let result = parse_message_from_slice(&bytes);

        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("server request is not supported yet"));
    }

    #[test]
    fn notification_requires_no_id() {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "window/logMessage",
            "params": {"message": "ok"}
        });

        let parsed = parse_notification(&payload).unwrap();
        assert!(parsed.is_some());
    }

    #[test]
    fn parse_server_request_from_slice_returns_id_and_method() {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "window/workDoneProgress/create",
            "params": {}
        });

        let bytes = serde_json::to_vec(&payload).unwrap();
        let parsed = parse_server_request_from_slice(&bytes)
            .unwrap()
            .expect("server request should be detected");

        assert_eq!(parsed.0, 42);
        assert_eq!(parsed.1, "window/workDoneProgress/create");
    }
}
