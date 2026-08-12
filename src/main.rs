mod pdf_ops;

use rust_embed::RustEmbed;
use axum::{
    response::{Response, IntoResponse},
    http::{header, StatusCode, Uri},
    routing::post,
    Router,
    extract::{Multipart, DefaultBodyLimit},
    Json,
};
use serde_json::json;
use tower_http::cors::{CorsLayer, Any};

#[derive(RustEmbed)]
#[folder = "static/"]
struct Asset;

/// Standard API Error helper returning dynamic JSON on failure
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": self.1
        });
        (self.0, Json(body)).into_response()
    }
}

/// Fallback static file handler for embedded assets (index.html, styles, scripts)
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.is_empty() {
        path = "index.html".to_string();
    }
    
    // Support routing "/dist/style.css" -> "style.css", "/dist/app.js" -> "app.js"
    if path.starts_with("dist/") {
        path = path.trim_start_matches("dist/").to_string();
    }

    match Asset::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data,
            ).into_response()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

/// Merges multiple uploaded PDF files
async fn handle_merge(mut multipart: Multipart) -> Result<impl IntoResponse, ApiError> {
    let mut files = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            files.push(data.to_vec());
        }
    }

    let merged_bytes = pdf_ops::merge_pdfs(files).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"merged_document.pdf\""),
        ],
        merged_bytes,
    ))
}

/// Splits a single PDF using page range guidelines
async fn handle_split(mut multipart: Multipart) -> Result<impl IntoResponse, ApiError> {
    let mut file_bytes = None;
    let mut ranges = String::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            file_bytes = Some(data.to_vec());
        } else if name == "ranges" {
            ranges = field.text().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?;
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Missing PDF file to split".to_string()))?;
    let split_bytes = pdf_ops::split_pdf(&file_bytes, &ranges).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"split_document.pdf\""),
        ],
        split_bytes,
    ))
}

/// Rotates all pages in a PDF document
async fn handle_rotate(mut multipart: Multipart) -> Result<impl IntoResponse, ApiError> {
    let mut file_bytes = None;
    let mut angle = 90;

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            file_bytes = Some(data.to_vec());
        } else if name == "angle" {
            let text = field.text().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?;
            angle = text.parse::<i32>().unwrap_or(90);
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Missing PDF file to rotate".to_string()))?;
    let rotated_bytes = pdf_ops::rotate_pdf(&file_bytes, angle).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"rotated_document.pdf\""),
        ],
        rotated_bytes,
    ))
}

/// Removes encryption/password from a PDF document
async fn handle_unlock(mut multipart: Multipart) -> Result<impl IntoResponse, ApiError> {
    let mut file_bytes = None;
    let mut password = String::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            file_bytes = Some(data.to_vec());
        } else if name == "password" {
            password = field.text().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?;
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Missing PDF file to decrypt".to_string()))?;
    let unlocked_bytes = pdf_ops::unlock_pdf(&file_bytes, &password).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"unlocked_document.pdf\""),
        ],
        unlocked_bytes,
    ))
}

/// Converts images to a single PDF
async fn handle_img_to_pdf(mut multipart: Multipart) -> Result<impl IntoResponse, ApiError> {
    let mut images = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            images.push(data.to_vec());
        }
    }

    let pdf_bytes = pdf_ops::images_to_pdf(images).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"images_converted.pdf\""),
        ],
        pdf_bytes,
    ))
}

/// Common Router Initialization
fn init_router() -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/merge", post(handle_merge))
        .route("/api/split", post(handle_split))
        .route("/api/rotate", post(handle_rotate))
        .route("/api/unlock", post(handle_unlock))
        .route("/api/img-to-pdf", post(handle_img_to_pdf))
        // Set maximum request limit to 50MB (matching file sizes)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(cors)
        .fallback(static_handler)
}

/// Shuttle deployment entrypoint
#[cfg(feature = "shuttle")]
#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    let router = init_router();
    Ok(router.into())
}

/// Local standalone server entrypoint (run with standard `cargo run`)
#[cfg(not(feature = "shuttle"))]
#[tokio::main]
async fn main() {
    let router = init_router();
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("🚀 Server running at http://{}", addr);
    axum::serve(listener, router).await.unwrap();
}
