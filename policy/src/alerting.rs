use serde_json::json;
use std::env;

pub async fn emit_rollout_event(
    event: &str,
    policy: &str,
    severity: &str,
    details: serde_json::Value,
) {
    let webhook = match env::var("ASTRAGRAPH_POLICY_ALERT_WEBHOOK_URL") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return,
    };

    let payload = json!({
        "source": "astragraph-policy",
        "event": event,
        "policy": policy,
        "severity": severity,
        "timestamp_unix": unix_now(),
        "details": details,
    });

    let mut request = reqwest::Client::new().post(&webhook).json(&payload);
    if let Ok(token) = env::var("ASTRAGRAPH_POLICY_ALERT_WEBHOOK_TOKEN") {
        if !token.trim().is_empty() {
            request = request.bearer_auth(token);
        }
    }

    if let Err(err) = request.send().await {
        tracing::warn!("failed to emit policy alert webhook: {err}");
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}
