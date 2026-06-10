use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist/"]
struct Assets;

/// Serves the embedded frontend build; unknown non-API paths fall back to
/// index.html so client-side routing works.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response();
    }

    match Assets::get("index.html") {
        Some(content) => Html(content.data).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "frontend not built — run `just build-web` (or use the Vite dev server)",
        )
            .into_response(),
    }
}
