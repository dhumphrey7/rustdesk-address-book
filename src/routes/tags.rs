use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::{json, Value};

use crate::auth::middleware::AuthUser;
use crate::error::ApiError;
use crate::models::tag::*;
use crate::routes::peers::resolve_ab_guid;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // The RustDesk client POSTs this and expects a BARE JSON array
        // ([{name,color},...]); the web console GETs it and expects
        // {data:[...], total}. Same data, different envelope per method.
        .route(
            "/api/ab/tags/{guid}",
            get(get_tags).post(get_tags_client),
        )
        .route("/api/ab/tag/add/{guid}", post(add_tag))
        .route("/api/ab/tag/rename/{guid}", put(rename_tag))
        .route("/api/ab/tag/update/{guid}", put(update_tag_color))
        .route("/api/ab/tag/{guid}", delete(delete_tags))
}

/// Shared tag fetch used by both response shapes.
async fn load_tags(
    db: &sqlx::SqlitePool,
    guid: &str,
) -> Result<Vec<TagPayload>, ApiError> {
    let tags = sqlx::query_as::<_, Tag>(
        "SELECT * FROM tags WHERE ab_guid = ? ORDER BY name",
    )
    .bind(guid)
    .fetch_all(db)
    .await?;

    Ok(tags
        .into_iter()
        .map(|t| TagPayload {
            name: t.name,
            color: t.color,
        })
        .collect())
}

/// GET /api/ab/tags/{guid} — web console shape: `{ data, total }`.
async fn get_tags(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(guid): Path<String>,
) -> Result<Json<TagsResponse>, ApiError> {
    let guid = resolve_ab_guid(&state.db, claims.user_id, &guid).await?;
    let data = load_tags(&state.db, &guid).await?;
    let total = data.len() as i64;
    Ok(Json(TagsResponse { data, total }))
}

/// POST /api/ab/tags/{guid} — RustDesk client shape: a bare JSON array.
async fn get_tags_client(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(guid): Path<String>,
) -> Result<Json<Vec<TagPayload>>, ApiError> {
    let guid = resolve_ab_guid(&state.db, claims.user_id, &guid).await?;
    Ok(Json(load_tags(&state.db, &guid).await?))
}

async fn add_tag(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(guid): Path<String>,
    Json(req): Json<AddTagRequest>,
) -> Result<Json<Value>, ApiError> {
    let guid = resolve_ab_guid(&state.db, claims.user_id, &guid).await?;

    sqlx::query("INSERT OR IGNORE INTO tags (ab_guid, name, color) VALUES (?, ?, ?)")
        .bind(&guid)
        .bind(&req.name)
        .bind(req.color)
        .execute(&state.db)
        .await?;

    Ok(Json(json!({})))
}

async fn rename_tag(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(guid): Path<String>,
    Json(req): Json<RenameTagRequest>,
) -> Result<Json<Value>, ApiError> {
    let guid = resolve_ab_guid(&state.db, claims.user_id, &guid).await?;

    sqlx::query("UPDATE tags SET name = ? WHERE ab_guid = ? AND name = ?")
        .bind(&req.new)
        .bind(&guid)
        .bind(&req.old)
        .execute(&state.db)
        .await?;

    Ok(Json(json!({})))
}

async fn update_tag_color(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(guid): Path<String>,
    Json(req): Json<UpdateTagColorRequest>,
) -> Result<Json<Value>, ApiError> {
    let guid = resolve_ab_guid(&state.db, claims.user_id, &guid).await?;

    sqlx::query("UPDATE tags SET color = ? WHERE ab_guid = ? AND name = ?")
        .bind(req.color)
        .bind(&guid)
        .bind(&req.name)
        .execute(&state.db)
        .await?;

    Ok(Json(json!({})))
}

async fn delete_tags(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(guid): Path<String>,
    Json(req): Json<DeleteTagRequest>,
) -> Result<Json<Value>, ApiError> {
    let guid = resolve_ab_guid(&state.db, claims.user_id, &guid).await?;

    let mut names = req.names;
    if let Some(name) = req.name {
        names.push(name);
    }

    for name in &names {
        // Remove peer_tags associations first
        sqlx::query(
            "DELETE FROM peer_tags WHERE tag_id IN (SELECT id FROM tags WHERE ab_guid = ? AND name = ?)",
        )
        .bind(&guid)
        .bind(name)
        .execute(&state.db)
        .await?;

        sqlx::query("DELETE FROM tags WHERE ab_guid = ? AND name = ?")
            .bind(&guid)
            .bind(name)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(json!({})))
}
