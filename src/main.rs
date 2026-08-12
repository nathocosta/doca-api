mod pdf_ops;
mod security;

use axum::{
    response::{Response, IntoResponse},
    http::{header, StatusCode},
    routing::post,
    Router,
    extract::{Multipart, DefaultBodyLimit, State},
    Json,
    middleware,
};
use serde_json::json;
use tower_http::cors::{CorsLayer, Any};
use std::sync::Arc;
use tokio::sync::Semaphore;

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

#[derive(Clone)]
struct AppState {
    semaphore: Arc<Semaphore>,
}

/// Fallback 404 handler for unmatched API routes
async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(json!({"error": "Resource not found"})))
}

/// Merges multiple uploaded PDF files
async fn handle_merge(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let mut files = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            if files.len() >= 15 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Limite máximo de 15 arquivos excedido.".to_string()));
            }
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if data.len() > 30 * 1024 * 1024 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Cada arquivo PDF individual não deve exceder 30MB.".to_string()));
            }
            files.push(data.to_vec());
        }
    }

    if files.len() < 2 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "Selecione pelo menos 2 arquivos para juntar.".to_string()));
    }

    // Acquire semaphore permit for memory-intensive PDF operations
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
async fn handle_split(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let mut file_bytes = None;
    let mut ranges = String::new();
    let mut files_count = 0;

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            files_count += 1;
            if files_count > 1 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Selecione apenas 1 arquivo PDF para dividir.".to_string()));
            }
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if data.len() > 30 * 1024 * 1024 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "O arquivo PDF não deve exceder 30MB.".to_string()));
            }
            file_bytes = Some(data.to_vec());
        } else if name == "ranges" {
            let val = field.text().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?;
            if val.len() > 200 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Parâmetro de páginas inválido ou muito longo.".to_string()));
            }
            if !val.chars().all(|c| c.is_ascii_digit() || c == ' ' || c == ',' || c == '-') {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Parâmetro de páginas contém caracteres inválidos. Use apenas números, vírgulas e hífen.".to_string()));
            }
            ranges = val;
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Arquivo PDF não fornecido para divisão.".to_string()))?;
    
    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
async fn handle_rotate(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let mut file_bytes = None;
    let mut angle = None;
    let mut files_count = 0;

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            files_count += 1;
            if files_count > 1 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Selecione apenas 1 arquivo PDF para rotacionar.".to_string()));
            }
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if data.len() > 30 * 1024 * 1024 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "O arquivo PDF não deve exceder 30MB.".to_string()));
            }
            file_bytes = Some(data.to_vec());
        } else if name == "angle" {
            let text = field.text().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?;
            let val = text.parse::<i32>().map_err(|_| ApiError(StatusCode::BAD_REQUEST, "Ângulo de rotação deve ser um inteiro válido.".to_string()))?;
            if val != 90 && val != 180 && val != 270 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Ângulo inválido. Escolha 90, 180 ou 270.".to_string()));
            }
            angle = Some(val);
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Arquivo PDF não fornecido para rotação.".to_string()))?;
    let angle = angle.unwrap_or(90);

    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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


/// Converts images to a single PDF
async fn handle_img_to_pdf(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let mut images = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            if images.len() >= 30 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Limite máximo de 30 imagens excedido.".to_string()));
            }
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if data.len() > 30 * 1024 * 1024 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Cada imagem individual não deve exceder 30MB.".to_string()));
            }
            images.push(data.to_vec());
        }
    }

    if images.is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "Selecione pelo menos 1 imagem para converter.".to_string()));
    }

    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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

/// Compresses a PDF document
async fn handle_compress(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let mut file_bytes = None;
    let mut files_count = 0;

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            files_count += 1;
            if files_count > 1 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Selecione apenas 1 arquivo PDF para compactar.".to_string()));
            }
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if data.len() > 30 * 1024 * 1024 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "O arquivo PDF não deve exceder 30MB.".to_string()));
            }
            file_bytes = Some(data.to_vec());
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Arquivo PDF não fornecido para compactação.".to_string()))?;

    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let compressed_bytes = pdf_ops::compress_pdf(&file_bytes).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"documento_compactado.pdf\""),
        ],
        compressed_bytes,
    ))
}

/// Converts DOCX to PDF
async fn handle_docx_to_pdf(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let mut file_bytes = None;
    let mut files_count = 0;

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            files_count += 1;
            if files_count > 1 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Selecione apenas 1 arquivo DOCX para converter.".to_string()));
            }
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if data.len() > 30 * 1024 * 1024 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "O arquivo DOCX não deve exceder 30MB.".to_string()));
            }
            file_bytes = Some(data.to_vec());
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Arquivo DOCX não fornecido.".to_string()))?;

    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let pdf_bytes = pdf_ops::docx_to_pdf(&file_bytes).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"documento_convertido.pdf\""),
        ],
        pdf_bytes,
    ))
}

/// Common Router Initialization
fn init_router() -> Router {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(|origin, _| {
            let origin_bytes = origin.as_bytes();
            origin_bytes == b"https://nathocosta.github.io" ||
            origin_bytes.starts_with(b"http://localhost:") ||
            origin_bytes.starts_with(b"http://127.0.0.1:")
        }))
        .allow_methods(Any)
        .allow_headers(Any);

    let limiter = Arc::new(security::RateLimiter::new(30.0, 60.0)); // 30 requests per minute
    let semaphore = Arc::new(Semaphore::new(3)); // max 3 parallel heavy operations

    let state = AppState {
        semaphore,
    };

    Router::new()
        .route("/api/merge", post(handle_merge))
        .route("/api/split", post(handle_split))
        .route("/api/rotate", post(handle_rotate))
        .route("/api/compress", post(handle_compress))
        .route("/api/docx-to-pdf", post(handle_docx_to_pdf))

        .route("/api/img-to-pdf", post(handle_img_to_pdf))
        .with_state(state)
        // Order of middleware execution: last added is run first.
        // We want Rate Limiter to run first (outermost layer).
        .layer(middleware::from_fn_with_state(limiter, security::rate_limit_middleware))
        // Set maximum request limit to 50MB (matching total upload limits)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(cors)
        .fallback(handler_404)
}
/// Standalone server entrypoint (run with standard `cargo run`)
#[tokio::main]
async fn main() {
    let router = init_router();
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);
    let addr = format!("0.0.0.0:{}", port);
    
    // Set up TCP Listener supporting connection info for rate limiter fallback
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("🚀 Server running at http://{}", addr);
    
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();
}

