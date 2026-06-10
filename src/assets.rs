use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};

use crate::error::ApiError;

#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist/"]
struct Assets;

/// index.html, with the remote API base injected for --ui-only mode so the
/// SPA calls it instead of its own origin.
fn index_html(api_base: Option<&str>) -> Option<Response> {
    let content = Assets::get("index.html")?;
    let Some(base) = api_base else {
        return Some(Html(content.data).into_response());
    };
    // serde_json string-escapes the URL so it's safe inside the script.
    let script = format!(
        "<script>window.SLATEDB_API_BASE={};</script>",
        serde_json::to_string(base).expect("strings always serialize")
    );
    let html = String::from_utf8_lossy(&content.data);
    let html = match html.find("</head>") {
        Some(i) => format!("{}{}{}", &html[..i], script, &html[i..]),
        None => format!("{script}{html}"),
    };
    Some(Html(html).into_response())
}

/// Serves the embedded frontend build; unknown non-API paths fall back to
/// index.html so client-side routing works.
pub async fn static_handler(uri: Uri, api_base: Option<String>) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Unknown API paths must not fall back to index.html: a 200 with HTML
    // turns a route typo into a JSON parse error on the client.
    if path == "api" || path.starts_with("api/") {
        return ApiError::NotFound(format!("no such API endpoint: {}", uri.path()))
            .into_response();
    }

    let path = if path.is_empty() { "index.html" } else { path };

    if path != "index.html" {
        if let Some(content) = Assets::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            return ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response();
        }
    }

    match index_html(api_base.as_deref()) {
        Some(resp) => resp,
        None => (
            StatusCode::NOT_FOUND,
            "frontend not built — run `npm run build --prefix web` (or use the Vite dev server)",
        )
            .into_response(),
    }
}
