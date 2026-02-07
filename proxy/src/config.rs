use serde::Deserialize;
use std::env;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    pub mcp: McpConfig,
    pub grpc: GrpcConfig,
    pub tls: TlsConfig,
    pub http: HttpConfig,
    pub policy_cache_ttl_ms: u64,
    pub fail_closed: bool,
    pub agent_id: String,
    pub policy_id: String,
    #[serde(default)]
    pub allow_without_trace_tools: Vec<String>,
    #[allow(dead_code)]
    pub trace_mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    pub child_command: String,
    #[serde(default)]
    pub child_args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrpcConfig {
    pub graph_addr: String,
    pub policy_addr: String,
    pub verifier_addr: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub ca_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HttpConfig {
    pub listen_addr: String,
    pub mcp_upstream: String,
    pub a2a_upstream: String,
}

impl ProxyConfig {
    pub fn load() -> Self {
        let file = "astragraph-proxy.toml";
        if let Ok(contents) = fs::read_to_string(file) {
            if let Ok(config) = toml::from_str::<ProxyConfig>(&contents) {
                return Self::apply_env_overrides(config);
            }
        }
        Self::from_env()
    }

    fn apply_env_overrides(mut config: Self) -> Self {
        if let Ok(value) = env::var("ASTRAGRAPH_MCP_CHILD_COMMAND") {
            config.mcp.child_command = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_MCP_CHILD_ARGS") {
            config.mcp.child_args = value.split_whitespace().map(String::from).collect();
        }
        if let Ok(value) = env::var("ASTRAGRAPH_GRAPH_GRPC_ADDR") {
            config.grpc.graph_addr = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_POLICY_GRPC_ADDR") {
            config.grpc.policy_addr = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_VERIFIER_GRPC_ADDR") {
            config.grpc.verifier_addr = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_PROXY_TLS_CERT") {
            config.tls.cert_path = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_PROXY_TLS_KEY") {
            config.tls.key_path = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_PROXY_TLS_CA") {
            config.tls.ca_path = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_PROXY_HTTP_ADDR") {
            config.http.listen_addr = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_MCP_UPSTREAM") {
            config.http.mcp_upstream = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_A2A_UPSTREAM") {
            config.http.a2a_upstream = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_POLICY_CACHE_TTL_MS") {
            if let Ok(parsed) = value.parse() {
                config.policy_cache_ttl_ms = parsed;
            }
        }
        if let Ok(value) = env::var("ASTRAGRAPH_FAIL_CLOSED") {
            config.fail_closed = value == "true";
        }
        if let Ok(value) = env::var("ASTRAGRAPH_AGENT_ID") {
            config.agent_id = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_POLICY_ID") {
            config.policy_id = value;
        }
        if let Ok(value) = env::var("ASTRAGRAPH_ALLOW_WITHOUT_TRACE_TOOLS") {
            config.allow_without_trace_tools = value
                .split(',')
                .map(str::trim)
                .filter(|tool| !tool.is_empty())
                .map(ToString::to_string)
                .collect();
        }
        if let Ok(value) = env::var("ASTRAGRAPH_TRACE_MODE") {
            config.trace_mode = value;
        }
        config
    }

    fn from_env() -> Self {
        Self {
            mcp: McpConfig {
                child_command: env::var("ASTRAGRAPH_MCP_CHILD_COMMAND").unwrap_or_default(),
                child_args: env::var("ASTRAGRAPH_MCP_CHILD_ARGS")
                    .map(|value| value.split_whitespace().map(String::from).collect())
                    .unwrap_or_default(),
            },
            grpc: GrpcConfig {
                graph_addr: env::var("ASTRAGRAPH_GRAPH_GRPC_ADDR")
                    .unwrap_or_else(|_| "https://127.0.0.1:9090".to_string()),
                policy_addr: env::var("ASTRAGRAPH_POLICY_GRPC_ADDR")
                    .unwrap_or_else(|_| "https://127.0.0.1:9091".to_string()),
                verifier_addr: env::var("ASTRAGRAPH_VERIFIER_GRPC_ADDR")
                    .unwrap_or_else(|_| "https://127.0.0.1:8080".to_string()),
            },
            tls: TlsConfig {
                cert_path: env::var("ASTRAGRAPH_PROXY_TLS_CERT").unwrap_or_default(),
                key_path: env::var("ASTRAGRAPH_PROXY_TLS_KEY").unwrap_or_default(),
                ca_path: env::var("ASTRAGRAPH_PROXY_TLS_CA").unwrap_or_default(),
            },
            http: HttpConfig {
                listen_addr: env::var("ASTRAGRAPH_PROXY_HTTP_ADDR")
                    .unwrap_or_else(|_| "0.0.0.0:7070".to_string()),
                mcp_upstream: env::var("ASTRAGRAPH_MCP_UPSTREAM")
                    .unwrap_or_else(|_| "http://127.0.0.1:7071".to_string()),
                a2a_upstream: env::var("ASTRAGRAPH_A2A_UPSTREAM")
                    .unwrap_or_else(|_| "http://127.0.0.1:7072".to_string()),
            },
            policy_cache_ttl_ms: env::var("ASTRAGRAPH_POLICY_CACHE_TTL_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(500),
            fail_closed: env::var("ASTRAGRAPH_FAIL_CLOSED")
                .map(|value| value == "true")
                .unwrap_or(true),
            agent_id: env::var("ASTRAGRAPH_AGENT_ID").unwrap_or_else(|_| "agent-unknown".into()),
            policy_id: env::var("ASTRAGRAPH_POLICY_ID").unwrap_or_default(),
            allow_without_trace_tools: env::var("ASTRAGRAPH_ALLOW_WITHOUT_TRACE_TOOLS")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|tool| !tool.is_empty())
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            trace_mode: env::var("ASTRAGRAPH_TRACE_MODE").unwrap_or_else(|_| "absent".to_string()),
        }
    }
}
