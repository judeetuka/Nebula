use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use super::handlers::AppState;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub role: String,
    pub exp: u64,
    pub iat: u64,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: u64,
    pub role: String,
}

#[derive(Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub expiry_hours: u64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: "nebula-default-secret-change-in-production".to_string(),
            expiry_hours: 24,
        }
    }
}

pub fn generate_token(
    user_id: &str, email: &str, role: &str, config: &JwtConfig,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let claims = Claims {
        sub: user_id.to_string(), email: email.to_string(), role: role.to_string(),
        iat: now, exp: now + config.expiry_hours * 3600,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(config.secret.as_bytes()))
}

pub fn validate_token(token: &str, config: &JwtConfig) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(token, &DecodingKey::from_secret(config.secret.as_bytes()), &Validation::default())?;
    Ok(data.claims)
}

/// Axum middleware: require valid JWT in Authorization header.
pub async fn require_auth(
    State(state): State<AppState>, mut req: Request<axum::body::Body>, next: Next,
) -> Result<Response, StatusCode> {
    let header = req.headers().get("authorization").and_then(|v| v.to_str().ok()).ok_or(StatusCode::UNAUTHORIZED)?;
    let token = header.strip_prefix("Bearer ").ok_or(StatusCode::UNAUTHORIZED)?;
    let claims = validate_token(token, &state.jwt_config).map_err(|_| StatusCode::UNAUTHORIZED)?;
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

/// Axum middleware: require super_admin role.
pub async fn require_super_admin(
    req: Request<axum::body::Body>, next: Next,
) -> Result<Response, StatusCode> {
    let claims = req.extensions().get::<Claims>().ok_or(StatusCode::UNAUTHORIZED)?;
    if claims.role != "super_admin" { return Err(StatusCode::FORBIDDEN); }
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JwtConfig {
        JwtConfig { secret: "test-secret".to_string(), expiry_hours: 1 }
    }

    #[test]
    fn test_generate_and_validate_token() {
        let config = test_config();
        let token = generate_token("user-123", "test@nebula.dev", "admin", &config).unwrap();
        let claims = validate_token(&token, &config).unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.email, "test@nebula.dev");
        assert_eq!(claims.role, "admin");
    }

    #[test]
    fn test_validate_expired_token_fails() {
        let config = test_config();
        let claims = Claims { sub: "u".into(), email: "a@b".into(), role: "admin".into(), iat: 1000000, exp: 1000001 };
        let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(config.secret.as_bytes())).unwrap();
        assert!(validate_token(&token, &config).is_err());
    }

    #[test]
    fn test_validate_invalid_token_fails() {
        assert!(validate_token("not.a.jwt", &test_config()).is_err());
    }
}
