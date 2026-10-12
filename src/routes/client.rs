//! Endpoints used by the RustDesk client's "Accessible devices" / group tab.
//!
//! These are NOT the same as the address-book endpoints in `peers.rs`:
//!   * `/api/peers`                  — flat list of peers the user can reach
//!   * `/api/device-group/accessible` — device groups the user can see
//!
//! Both paginate with `current` / `pageSize` and must answer
//! `{ "data": [...], "total": N }`. The client GETs them, and if the route is
//! missing the SPA fallback returns index.html, which the client reports as
//! `FormatException: Unexpected character (at character 1) <!doctype html>`.

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::middleware::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/peers", get(get_accessible_peers))
        .route("/api/device-group/accessible", get(get_device_groups))
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct ClientPagination {
    #[serde(default)]
    pub current: Option<i64>,
    #[serde(default)]
    pub pageSize: Option<i64>,
    /// Present (often empty) in the client's query string; unused.
    #[serde(default)]
    pub accessible: Option<String>,
    /// Present in the client's query string; unused.
    #[serde(default)]
    pub status: Option<String>,
}

impl ClientPagination {
    fn limit(&self) -> i64 {
        self.pageSize.unwrap_or(100).clamp(1, 1000)
    }
    fn offset(&self) -> i64 {
        let current = self.current.unwrap_or(1).max(1);
        (current - 1) * self.limit()
    }
}

/// A peer in the shape `PeerPayload.fromJson` expects.
///
/// The client reads `info` as a nested object and pulls `os`, `device_name`
/// and `username` out of it; the flat fields become the other columns.
#[derive(Debug, Serialize)]
struct ClientPeer {
    id: String,
    info: ClientPeerInfo,
    status: i64,
    user: String,
    user_name: String,
    device_group_name: String,
    note: String,
}

#[derive(Debug, Serialize)]
struct ClientPeerInfo {
    os: String,
    device_name: String,
    username: String,
    platform: String,
    version: String,
}

/// GET /api/peers — every peer across the address books this user can access.
async fn get_accessible_peers(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Query(page): Query<ClientPagination>,
) -> Result<Json<Value>, ApiError> {
    // Address books the user owns, plus any shared with them directly or via
    // one of their groups.
    let accessible_abs: Vec<String> = sqlx::query_scalar(
        "SELECT guid FROM address_books
         WHERE owner_id = ?
            OR guid IN (
                 SELECT s.ab_guid FROM ab_shares s
                 WHERE s.user_id = ?
                    OR s.group_id IN (SELECT group_id FROM user_groups WHERE user_id = ?)
               )",
    )
    .bind(claims.user_id)
    .bind(claims.user_id)
    .bind(claims.user_id)
    .fetch_all(&state.db)
    .await?;

    if accessible_abs.is_empty() {
        return Ok(Json(json!({ "data": [], "total": 0 })));
    }

    // Build a parameterised IN (...) list.
    let placeholders = vec!["?"; accessible_abs.len()].join(",");
    let count_sql = format!(
        "SELECT COUNT(*) FROM peers WHERE ab_guid IN ({})",
        placeholders
    );
    let list_sql = format!(
        "SELECT rustdesk_id, hostname, platform, username, note
         FROM peers WHERE ab_guid IN ({})
         ORDER BY rustdesk_id LIMIT ? OFFSET ?",
        placeholders
    );

    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    for guid in &accessible_abs {
        count_q = count_q.bind(guid);
    }
    let total: i64 = count_q.fetch_one(&state.db).await?;

    let mut list_q = sqlx::query_as::<_, (String, String, String, String, String)>(&list_sql);
    for guid in &accessible_abs {
        list_q = list_q.bind(guid);
    }
    let rows = list_q
        .bind(page.limit())
        .bind(page.offset())
        .fetch_all(&state.db)
        .await?;

    let data: Vec<ClientPeer> = rows
        .into_iter()
        .map(|(id, hostname, platform, username, note)| ClientPeer {
            id,
            info: ClientPeerInfo {
                os: platform.clone(),
                device_name: hostname.clone(),
                username: username.clone(),
                platform,
                version: String::new(),
            },
            status: 1,
            user: String::new(),
            user_name: username,
            device_group_name: String::new(),
            note,
        })
        .collect();

    Ok(Json(json!({ "data": data, "total": total })))
}

#[derive(Debug, Serialize)]
struct DeviceGroup {
    name: String,
}

/// GET /api/device-group/accessible — the device groups this user can see.
///
/// `DeviceGroupPayload.fromJson` only reads `name`, so that is all we send.
/// Admins see every group; everyone else sees the groups they belong to.
async fn get_device_groups(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Query(_page): Query<ClientPagination>,
) -> Result<Json<Value>, ApiError> {
    let names: Vec<String> = if claims.is_admin {
        sqlx::query_scalar("SELECT name FROM groups ORDER BY name")
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_scalar(
            "SELECT g.name FROM groups g
             JOIN user_groups ug ON ug.group_id = g.id
             WHERE ug.user_id = ?
             ORDER BY g.name",
        )
        .bind(claims.user_id)
        .fetch_all(&state.db)
        .await?
    };

    let total = names.len() as i64;
    let data: Vec<DeviceGroup> = names.into_iter().map(|name| DeviceGroup { name }).collect();

    Ok(Json(json!({ "data": data, "total": total })))
}
