#![allow(dead_code)]

#[derive(Debug, Clone, Copy)]
pub enum TraceMode {
    Explicit,
    Streaming,
    Absent,
}

#[derive(Debug, Default)]
pub struct TraceExtraction {
    pub trace: Option<String>,
    pub action: Option<String>,
}

pub fn extract(mode: TraceMode, input: &str) -> TraceExtraction {
    match mode {
        TraceMode::Explicit => extract_explicit(input),
        TraceMode::Streaming => extract_streaming(input),
        TraceMode::Absent => TraceExtraction::default(),
    }
}

fn extract_explicit(input: &str) -> TraceExtraction {
    let start_tag = "<think>";
    let end_tag = "</think>";

    let start = match input.find(start_tag) {
        Some(idx) => idx + start_tag.len(),
        None => return TraceExtraction::default(),
    };
    let end = match input[start..].find(end_tag) {
        Some(idx) => start + idx,
        None => return TraceExtraction::default(),
    };

    let trace = input[start..end].trim().to_string();
    let action = input[end + end_tag.len()..].trim().to_string();

    TraceExtraction {
        trace: if trace.is_empty() { None } else { Some(trace) },
        action: if action.is_empty() {
            None
        } else {
            Some(action)
        },
    }
}

fn extract_streaming(input: &str) -> TraceExtraction {
    // Streaming mode treats the content before </think> as trace when present.
    let end_tag = "</think>";
    if let Some(end) = input.find(end_tag) {
        let trace = input[..end].trim().to_string();
        let action = input[end + end_tag.len()..].trim().to_string();
        return TraceExtraction {
            trace: if trace.is_empty() { None } else { Some(trace) },
            action: if action.is_empty() {
                None
            } else {
                Some(action)
            },
        };
    }

    TraceExtraction::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_explicit_trace() {
        let input = "<think>Reasoning</think> tool_call";
        let result = extract(TraceMode::Explicit, input);
        assert_eq!(result.trace.as_deref(), Some("Reasoning"));
        assert_eq!(result.action.as_deref(), Some("tool_call"));
    }

    #[test]
    fn extracts_streaming_trace() {
        let input = "Reasoning tokens</think> final_action";
        let result = extract(TraceMode::Streaming, input);
        assert_eq!(result.trace.as_deref(), Some("Reasoning tokens"));
        assert_eq!(result.action.as_deref(), Some("final_action"));
    }

    #[test]
    fn absent_mode_returns_empty() {
        let result = extract(TraceMode::Absent, "anything");
        assert!(result.trace.is_none());
        assert!(result.action.is_none());
    }
}
