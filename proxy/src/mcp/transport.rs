use super::parser::{self, McpMessage, ParseError, ToolListResponse};
use crate::circuit_breaker::CircuitBreaker;
use crate::config::ProxyConfig;
use crate::enforcement::{self, DecisionOutcome, ToolCallOwned};
use crate::grpc::GrpcClients;
use crate::policy_cache::PolicyCache;
use crate::telemetry;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

#[allow(dead_code)]
#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    Parse(ParseError),
    Config(String),
}

impl From<io::Error> for TransportError {
    fn from(err: io::Error) -> Self {
        TransportError::Io(err)
    }
}

impl From<ParseError> for TransportError {
    fn from(err: ParseError) -> Self {
        TransportError::Parse(err)
    }
}

pub async fn run_stdio(
    config: ProxyConfig,
    clients: Arc<GrpcClients>,
) -> Result<(), TransportError> {
    if config.mcp.child_command.is_empty() {
        return Err(TransportError::Config(
            "missing mcp child command".to_string(),
        ));
    }

    let mut child = Command::new(&config.mcp.child_command)
        .args(&config.mcp.child_args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    let child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| TransportError::Config("child stdin unavailable".to_string()))?;
    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| TransportError::Config("child stdout unavailable".to_string()))?;

    let stdout = Arc::new(Mutex::new(io::stdout()));
    let child_stdin = Arc::new(Mutex::new(child_stdin));
    let policy_cache = Arc::new(Mutex::new(PolicyCache::new(config.policy_cache_ttl_ms)));
    let circuit_breaker = Arc::new(CircuitBreaker::new());

    let agent_task = {
        let stdout = stdout.clone();
        let child_stdin = child_stdin.clone();
        let policy_cache = policy_cache.clone();
        let circuit_breaker = circuit_breaker.clone();
        let config = config.clone();
        let clients = clients.clone();
        tokio::spawn(async move {
            let stdin = io::stdin();
            let reader = BufReader::new(stdin);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let start = Instant::now();
                let message = match parser::parse_message(line.as_bytes()) {
                    Ok(message) => message,
                    Err(err) => {
                        let _ = write_line(&stdout, &line).await;
                        telemetry::record_request(
                            "parse_failed",
                            start.elapsed().as_secs_f64() * 1000.0,
                        );
                        return Err(TransportError::Parse(err));
                    }
                };

                let outcome =
                    handle_message(message, &config, &clients, &policy_cache, &circuit_breaker)
                        .await;

                match outcome {
                    HandleOutcome::Forward => {
                        let _ = write_line(&child_stdin, &line).await;
                    }
                    HandleOutcome::Respond(response) => {
                        let _ = write_line(&stdout, &response).await;
                    }
                }

                telemetry::record_request("mcp_stdio", start.elapsed().as_secs_f64() * 1000.0);
            }
            Ok::<(), TransportError>(())
        })
    };

    let child_task = {
        let stdout = stdout.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(child_stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(McpMessage::ToolListResponse(response)) =
                    parser::parse_message(line.as_bytes())
                {
                    scan_tools_list(&response);
                }
                let _ = write_line(&stdout, &line).await;
            }
            Ok::<(), TransportError>(())
        })
    };

    let _ = tokio::join!(agent_task, child_task);
    Ok(())
}

enum HandleOutcome {
    Forward,
    Respond(String),
}

async fn handle_message(
    message: McpMessage,
    config: &ProxyConfig,
    clients: &GrpcClients,
    policy_cache: &Mutex<PolicyCache>,
    circuit_breaker: &CircuitBreaker,
) -> HandleOutcome {
    match message {
        McpMessage::ToolCall(call) => {
            let owned = ToolCallOwned {
                id: call.id,
                name: call.name,
                arguments: call.arguments,
            };
            match enforcement::evaluate_tool_call(
                owned,
                config,
                clients,
                policy_cache,
                circuit_breaker,
            )
            .await
            {
                Ok(DecisionOutcome::Allow) => HandleOutcome::Forward,
                Ok(DecisionOutcome::Block(response)) => HandleOutcome::Respond(response),
                Ok(DecisionOutcome::Queue(response)) => HandleOutcome::Respond(response),
                Err(response) => HandleOutcome::Respond(response),
            }
        }
        McpMessage::ToolListResponse(response) => {
            scan_tools_list(&response);
            HandleOutcome::Forward
        }
        McpMessage::SamplingRequest => {
            telemetry::record_request("mcp_sampling_request", 0.0);
            HandleOutcome::Forward
        }
        _ => HandleOutcome::Forward,
    }
}

fn scan_tools_list(response: &ToolListResponse) {
    for tool in &response.tools {
        if let Some(description) = &tool.description {
            if is_poisoning_suspected(description) {
                telemetry::record_request("tool_poisoning_suspected", 0.0);
                eprintln!(
                    "MCP tool poisoning suspicion: tool={} description={}",
                    tool.name, description
                );
            }
        }
    }
}

fn is_poisoning_suspected(description: &str) -> bool {
    let normalized = description.to_ascii_lowercase();
    let markers = [
        "ignore previous instructions",
        "system prompt",
        "override policy",
        "exfiltrate",
        "disable guardrail",
        "bypass safety",
    ];
    markers.iter().any(|marker| normalized.contains(marker))
}

async fn write_line<W>(writer: &Mutex<W>, line: &str) -> Result<(), io::Error>
where
    W: AsyncWriteExt + Unpin,
{
    let mut writer = writer.lock().await;
    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
