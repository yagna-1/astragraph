use jsonwebtoken::{decode, decode_header, jwk::JwkSet, DecodingKey, Validation};
use reqwest::Client;
use serde::Deserialize;
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AuthState {
    enabled: bool,
    jwks_url: Option<String>,
    audience: Option<String>,
    client: Client,
    cache: Arc<RwLock<JwksCache>>,
}

struct JwksCache {
    jwks: Option<JwkSet>,
    fetched_at: Option<Instant>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Claims {
    #[serde(default)]
    roles: Option<Vec<String>>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    aud: Option<String>,
    #[serde(default)]
    exp: Option<usize>,
}

impl AuthState {
    pub fn new() -> Self {
        let enabled = env::var("ASTRAGRAPH_AUTH_DISABLED")
            .map(|value| value != "true")
            .unwrap_or(true);
        let jwks_url = env::var("ASTRAGRAPH_JWKS_URL").ok();
        let audience = env::var("ASTRAGRAPH_JWT_AUDIENCE").ok();
        Self {
            enabled,
            jwks_url,
            audience,
            client: Client::new(),
            cache: Arc::new(RwLock::new(JwksCache {
                jwks: None,
                fetched_at: None,
            })),
        }
    }

    pub async fn ensure_role(
        &self,
        auth_header: Option<&str>,
        allowed_roles: &[&str],
    ) -> Result<(), AuthError> {
        if !self.enabled {
            return Ok(());
        }

        let token = auth_header
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or(AuthError::Unauthorized)?;

        let claims = self.validate_token(token).await?;
        let mut roles = claims.roles.unwrap_or_default();
        if let Some(role) = claims.role {
            roles.push(role);
        }

        if roles
            .iter()
            .any(|role| allowed_roles.contains(&role.as_str()))
        {
            Ok(())
        } else {
            Err(AuthError::Forbidden)
        }
    }

    async fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        let jwks = self.get_jwks().await?;
        let header = decode_header(token).map_err(|_| AuthError::Unauthorized)?;
        let kid = header.kid.ok_or(AuthError::Unauthorized)?;
        let jwk = jwks
            .keys
            .iter()
            .find(|key| key.common.key_id.as_deref() == Some(&kid))
            .ok_or(AuthError::Unauthorized)?;

        let decoding_key = DecodingKey::from_jwk(jwk).map_err(|_| AuthError::Unauthorized)?;
        let alg = header.alg;
        let mut validation = Validation::new(alg);
        if let Some(audience) = &self.audience {
            validation.set_audience(&[audience]);
        }
        let token_data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|_| AuthError::Unauthorized)?;
        Ok(token_data.claims)
    }

    async fn get_jwks(&self) -> Result<JwkSet, AuthError> {
        let mut cache = self.cache.write().await;
        let expired = cache
            .fetched_at
            .map(|ts| ts.elapsed() > Duration::from_secs(300))
            .unwrap_or(true);

        if cache.jwks.is_none() || expired {
            let url = self.jwks_url.clone().ok_or(AuthError::Unauthorized)?;
            let response = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|_| AuthError::Unauthorized)?;
            let jwks: JwkSet = response.json().await.map_err(|_| AuthError::Unauthorized)?;
            cache.jwks = Some(jwks);
            cache.fetched_at = Some(Instant::now());
        }

        cache.jwks.clone().ok_or(AuthError::Unauthorized)
    }
}

#[derive(Debug)]
pub enum AuthError {
    Unauthorized,
    Forbidden,
}
