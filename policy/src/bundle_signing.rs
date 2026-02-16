use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
#[cfg(test)]
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::env;

const SIGNING_KEY_ENV: &str = "ASTRAGRAPH_POLICY_BUNDLE_SIGNING_KEY";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureError {
    MissingSignature,
    InvalidSignature,
    PayloadMismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyBundleClaims {
    yaml: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    exp: Option<usize>,
}

pub fn verify_yaml_signature(
    yaml: &str,
    signature_token: Option<&str>,
) -> Result<(), SignatureError> {
    let signing_key = env::var(SIGNING_KEY_ENV).ok();
    verify_yaml_signature_with_key(yaml, signature_token, signing_key.as_deref())
}

fn verify_yaml_signature_with_key(
    yaml: &str,
    signature_token: Option<&str>,
    signing_key: Option<&str>,
) -> Result<(), SignatureError> {
    let Some(signing_key) = signing_key.filter(|key| !key.trim().is_empty()) else {
        return Ok(());
    };
    let signature_token = signature_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(SignatureError::MissingSignature)?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false;
    validation.required_spec_claims.clear();
    let decoded = decode::<PolicyBundleClaims>(
        signature_token,
        &DecodingKey::from_secret(signing_key.as_bytes()),
        &validation,
    )
    .map_err(|_| SignatureError::InvalidSignature)?;

    if decoded.claims.yaml != yaml {
        return Err(SignatureError::PayloadMismatch);
    }

    Ok(())
}

#[cfg(test)]
pub fn sign_yaml_for_testing(yaml: &str, signing_key: &str) -> String {
    let claims = PolicyBundleClaims {
        yaml: yaml.to_string(),
        exp: None,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(signing_key.as_bytes()),
    )
    .expect("jwt encoding")
}

#[cfg(test)]
mod tests {
    use super::{sign_yaml_for_testing, verify_yaml_signature_with_key, SignatureError};

    #[test]
    fn bypasses_when_signing_key_not_configured() {
        let result = verify_yaml_signature_with_key("kind: AgentPolicy", None, None);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn rejects_when_signature_missing_with_signing_key() {
        let result = verify_yaml_signature_with_key("kind: AgentPolicy", None, Some("secret"));
        assert_eq!(result, Err(SignatureError::MissingSignature));
    }

    #[test]
    fn rejects_invalid_token() {
        let result =
            verify_yaml_signature_with_key("kind: AgentPolicy", Some("not-a-jwt"), Some("secret"));
        assert_eq!(result, Err(SignatureError::InvalidSignature));
    }

    #[test]
    fn rejects_when_token_payload_does_not_match() {
        let signature = sign_yaml_for_testing("kind: DifferentPolicy", "secret");
        let result =
            verify_yaml_signature_with_key("kind: AgentPolicy", Some(&signature), Some("secret"));
        assert_eq!(result, Err(SignatureError::PayloadMismatch));
    }

    #[test]
    fn rejects_when_signature_key_is_wrong() {
        let signature = sign_yaml_for_testing("kind: AgentPolicy", "wrong-secret");
        let result =
            verify_yaml_signature_with_key("kind: AgentPolicy", Some(&signature), Some("secret"));
        assert_eq!(result, Err(SignatureError::InvalidSignature));
    }

    #[test]
    fn accepts_valid_signature() {
        let yaml = "kind: AgentPolicy";
        let signature = sign_yaml_for_testing(yaml, "secret");
        let result = verify_yaml_signature_with_key(yaml, Some(&signature), Some("secret"));
        assert_eq!(result, Ok(()));
    }
}
