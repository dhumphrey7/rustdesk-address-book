use axum::{
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
};
use rust_embed::Embed;
use serde_json::json;

#[derive(Embed)]
#[folder = "web/dist/"]
struct FrontendAssets;

pub async fn serve_frontend(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // An unmatched /api/* route is a missing endpoint, not a client-side route.
    // Answering with index.html makes the RustDesk client report
    // `FormatException: Unexpected character (at character 1) <!doctype html>`
    // instead of a clear 404, so fail loudly with JSON.
    if uri.path().starts_with("/api/") {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": format!("No such API endpoint: {}", uri.path()) })),
        )
            .into_response();
    }

    // Try to serve the exact file
    if let Some(file) = FrontendAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref())],
            file.data.into_owned(),
        )
            .into_response()
    } else {
        // SPA fallback: serve index.html for any non-file route
        match FrontendAssets::get("index.html") {
            Some(file) => Html(String::from_utf8_lossy(&file.data).to_string()).into_response(),
            None => (StatusCode::NOT_FOUND, "Frontend not built").into_response(),
        }
    }
}
