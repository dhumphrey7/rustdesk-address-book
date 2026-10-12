use axum::{extract::State, routing::{get, post}, Json, Router};
use serde_json::{json, Value};

use crate::auth::jwt::create_token;
use crate::auth::middleware::AuthUser;
use crate::auth::password::verify_password;
use crate::error::ApiError;
use crate::models::user::{LoginRequest, LoginResponse, UserPayload};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        // The client POSTs currentUser (with the bearer token in a header),
        // so accept both methods.
        .route(
            "/api/currentUser",
            get(current_user).post(current_user),
        )
        // The login window GETs this before showing the form. We support no
        // third-party OIDC providers, so answer with an empty list rather
        // than letting the SPA fallback return HTML.
        .route("/api/login-options", get(login_options))
}

/// GET /api/login-options — third-party login options offered by this server.
/// The client expects a JSON array of strings; empty means "password only".
async fn login_options() -> Json<Value> {
    Json(json!([]))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let user = sqlx::query_as::<_, crate::models::user::User>(
        "SELECT * FROM users WHERE username = ? AND status = 1",
    )
    .bind(&req.username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("Invalid username or password".to_string()))?;

    if !verify_password(&req.password, &user.password_hash) {
        return Err(ApiError::Unauthorized("Invalid username or password".to_string()));
    }

    let token = create_token(
        &user.username,
        user.id,
        user.is_admin,
        &state.config.jwt_secret,
        state.config.token_expiry_hours,
    )?;

    // Log the login in audit
    let ip = req.id.clone();
    sqlx::query("INSERT INTO audit_log (user_id, action, rustdesk_id, ip) VALUES (?, 'login', ?, ?)")
        .bind(user.id)
        .bind(&req.id)
        .bind(&ip)
        .execute(&state.db)
        .await
        .ok();

    Ok(Json(LoginResponse {
        access_token: token,
        token_type: "access_token".to_string(),
        user: UserPayload {
            name: if user.name.is_empty() {
                user.username.clone()
            } else {
                user.name
            },
            email: user.email,
            is_admin: user.is_admin,
            note: String::new(),
        },
    }))
}

async fn logout(AuthUser(_claims): AuthUser) -> Json<Value> {
    Json(json!({}))
}

async fn current_user(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
) -> Result<Json<Value>, ApiError> {
    let user = sqlx::query_as::<_, crate::models::user::User>(
        "SELECT * FROM users WHERE id = ?",
    )
    .bind(claims.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    Ok(Json(json!({
        "name": if user.name.is_empty() { user.username } else { user.name },
        "email": user.email,
        "is_admin": user.is_admin,
        "note": ""
    })))
}
