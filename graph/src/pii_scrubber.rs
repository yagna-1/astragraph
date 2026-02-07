use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use std::env;

pub struct PiiScrubber {
    mode: ScrubberMode,
    regex: RegexScrubber,
}

enum ScrubberMode {
    Regex,
    Model(ModelScrubber),
}

struct RegexScrubber {
    email: Regex,
    phone: Regex,
    ssn: Regex,
    credit_card: Regex,
}

struct ModelScrubber {
    endpoint: String,
    client: reqwest::blocking::Client,
}

impl PiiScrubber {
    pub fn new() -> Self {
        let regex = RegexScrubber::new();
        let mode = match env::var("ASTRAGRAPH_PII_MODEL_URL") {
            Ok(endpoint) if !endpoint.is_empty() => {
                ScrubberMode::Model(ModelScrubber::new(endpoint))
            }
            _ => ScrubberMode::Regex,
        };
        Self { mode, regex }
    }

    pub fn scrub(&self, input: &str) -> String {
        match &self.mode {
            ScrubberMode::Regex => self.regex.scrub(input),
            ScrubberMode::Model(model) => model
                .scrub(input)
                .unwrap_or_else(|| self.regex.scrub(input)),
        }
    }
}

impl RegexScrubber {
    fn new() -> Self {
        Self {
            email: Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
                .expect("email regex"),
            phone: Regex::new(r"\b\d{3}[-.\s]?\d{3}[-.\s]?\d{4}\b").expect("phone regex"),
            ssn: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").expect("ssn regex"),
            credit_card: Regex::new(r"\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b")
                .expect("credit card regex"),
        }
    }

    fn scrub(&self, input: &str) -> String {
        let mut output = input.to_string();
        output = self
            .email
            .replace_all(&output, "[REDACTED_EMAIL]")
            .to_string();
        output = self
            .phone
            .replace_all(&output, "[REDACTED_PHONE]")
            .to_string();
        output = self.ssn.replace_all(&output, "[REDACTED_SSN]").to_string();
        output = self
            .credit_card
            .replace_all(&output, "[REDACTED_CC]")
            .to_string();
        output
    }
}

impl ModelScrubber {
    fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: reqwest::blocking::Client::new(),
        }
    }

    fn scrub(&self, input: &str) -> Option<String> {
        let payload = ScrubRequest {
            text: input.to_string(),
        };
        let response = self
            .client
            .post(&self.endpoint)
            .json(&payload)
            .send()
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let output: ScrubResponse = response.json().ok()?;
        Some(output.scrubbed_text)
    }
}

#[derive(Serialize)]
struct ScrubRequest {
    text: String,
}

#[derive(Deserialize)]
struct ScrubResponse {
    scrubbed_text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_common_pii() {
        let scrubber = PiiScrubber::new();
        let input = "Contact me at jane.doe@example.com or 555-123-4567. SSN 123-45-6789.";
        let output = scrubber.scrub(input);
        assert!(!output.contains("jane.doe@example.com"));
        assert!(!output.contains("555-123-4567"));
        assert!(!output.contains("123-45-6789"));
    }
}
