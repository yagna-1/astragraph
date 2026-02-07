use serde::Deserialize;
#[allow(dead_code)]
#[derive(Debug)]
pub enum McpMessage {
    ToolCall(ToolCall),
    ToolListResponse(ToolListResponse),
    SamplingRequest,
    Other,
}

#[derive(Debug)]
pub struct ToolCall {
    pub id: Option<serde_json::Value>,
    pub name: String,
    pub arguments: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ToolListResponse {
    pub id: Option<serde_json::Value>,
    pub tools: Vec<ToolDescriptor>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum ParseError {
    Json(serde_json::Error),
    InvalidJsonRpcVersion,
    MissingParams,
    InvalidParams(serde_json::Error),
    InvalidResult(serde_json::Error),
}

impl From<serde_json::Error> for ParseError {
    fn from(err: serde_json::Error) -> Self {
        ParseError::Json(err)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Envelope {
    Request(Request),
    Response(Response),
}

#[derive(Deserialize)]
struct Request {
    jsonrpc: String,
    #[serde(default)]
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Response {
    jsonrpc: String,
    #[serde(default)]
    id: Option<serde_json::Value>,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ToolsListResult {
    tools: Vec<ToolDescriptor>,
}

pub fn parse_message(raw: &[u8]) -> Result<McpMessage, ParseError> {
    let envelope: Envelope = serde_json::from_slice(raw)?;
    match envelope {
        Envelope::Request(request) => parse_request(request),
        Envelope::Response(response) => parse_response(response),
    }
}

fn parse_request(request: Request) -> Result<McpMessage, ParseError> {
    if request.jsonrpc != "2.0" {
        return Err(ParseError::InvalidJsonRpcVersion);
    }

    if request.method == "tools/call" {
        let params = request.params.ok_or(ParseError::MissingParams)?;
        let parsed: ToolCallParams =
            serde_json::from_value(params).map_err(ParseError::InvalidParams)?;
        return Ok(McpMessage::ToolCall(ToolCall {
            id: request.id,
            name: parsed.name,
            arguments: parsed.arguments,
        }));
    }

    if request.method.starts_with("sampling") {
        return Ok(McpMessage::SamplingRequest);
    }

    Ok(McpMessage::Other)
}

fn parse_response(response: Response) -> Result<McpMessage, ParseError> {
    if response.jsonrpc != "2.0" {
        return Err(ParseError::InvalidJsonRpcVersion);
    }

    if let Some(result) = response.result {
        let tools_result: Result<ToolsListResult, serde_json::Error> =
            serde_json::from_value(result);
        if let Ok(parsed) = tools_result {
            return Ok(McpMessage::ToolListResponse(ToolListResponse {
                id: response.id,
                tools: parsed.tools,
            }));
        }
    }

    if response.error.is_some() {
        return Ok(McpMessage::Other);
    }

    Ok(McpMessage::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_call() {
        let payload = r#"{
          "jsonrpc": "2.0",
          "id": "req-1234",
          "method": "tools/call",
          "params": {
            "name": "export_data",
            "arguments": { "table": "customers", "format": "csv" }
          }
        }"#;

        let message = parse_message(payload.as_bytes()).expect("parse ok");
        match message {
            McpMessage::ToolCall(call) => {
                assert_eq!(call.name, "export_data");
                assert!(call.arguments.is_some());
            }
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn parses_tool_list_response() {
        let payload = r#"{
          "jsonrpc": "2.0",
          "id": "req-0001",
          "result": {
            "tools": [
              { "name": "export_data", "description": "Exports table data..." }
            ]
          }
        }"#;

        let message = parse_message(payload.as_bytes()).expect("parse ok");
        match message {
            McpMessage::ToolListResponse(response) => {
                assert_eq!(response.tools.len(), 1);
                assert_eq!(response.tools[0].name, "export_data");
            }
            _ => panic!("expected tool list response"),
        }
    }
}
