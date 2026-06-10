use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};

use crate::error::ApiError;

#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist/"]
struct Assets;

/// Serves the embedded frontend build; unknown non-API paths fall back to
/// index.html so client-side routing works.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Unknown API paths must not fall back to index.html: a 200 with HTML
    // turns a route typo into a JSON parse error on the client.
    if path == "api" || path.starts_with("api/") {
        return ApiError::NotFound(format!("no such API endpoint: {}", uri.path()))
            .into_response();
    }

    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response();
    }

    match Assets::get("index.html") {
        Some(content) => Html(content.data).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "frontend not built — run `npm run build --prefix web` (or use the Vite dev server)",
        )
            .into_response(),
    }
}
