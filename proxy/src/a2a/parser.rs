#![allow(dead_code)]

use serde::Deserialize;

#[derive(Debug)]
pub enum A2aMessage {
    TaskSend(TaskSendRequest),
    TaskStatus(TaskStatusEvent),
    Other,
}

#[derive(Debug, Deserialize)]
pub struct TaskSendRequest {
    pub message: Message,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub parts: Vec<MessagePart>,
}

#[derive(Debug, Deserialize)]
pub struct MessagePart {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub file: Option<FilePart>,
}

#[derive(Debug, Deserialize)]
pub struct FilePart {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TaskStatusEvent {
    pub task: TaskStatus,
}

#[derive(Debug, Deserialize)]
pub struct TaskStatus {
    pub id: String,
    pub status: TaskStatusState,
    #[serde(default)]
    pub artifacts: Option<Vec<Artifact>>,
}

#[derive(Debug, Deserialize)]
pub struct TaskStatusState {
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct Artifact {
    pub parts: Vec<MessagePart>,
}

#[derive(Debug)]
pub enum ParseError {
    Json(serde_json::Error),
}

impl From<serde_json::Error> for ParseError {
    fn from(err: serde_json::Error) -> Self {
        ParseError::Json(err)
    }
}

pub fn parse_message(raw: &[u8]) -> Result<A2aMessage, ParseError> {
    if let Ok(task_send) = serde_json::from_slice::<TaskSendRequest>(raw) {
        return Ok(A2aMessage::TaskSend(task_send));
    }

    if let Ok(task_status) = serde_json::from_slice::<TaskStatusEvent>(raw) {
        return Ok(A2aMessage::TaskStatus(task_status));
    }

    Ok(A2aMessage::Other)
}
