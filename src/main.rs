mod pdf_ops;
mod security;
mod logging;

use axum::{
    response::{Response, IntoResponse},
    http::{header, StatusCode},
    routing::{post, get},
    Router,
    extract::{Multipart, DefaultBodyLimit, State},
    Json,
    middleware,
};
use serde_json::json;
use tower_http::cors::{CorsLayer, Any};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
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
    pw_semaphore: Arc<Semaphore>,
}

/// Fallback 404 handler for unmatched API routes
async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(json!({"error": "Resource not found"})))
}

static START_TIME: OnceLock<Instant> = OnceLock::new();

/// Returns the elapsed uptime of the server in seconds (initialized on first access)
fn get_uptime() -> u64 {
    START_TIME.get_or_init(Instant::now).elapsed().as_secs()
}

/// Endpoint /health for monitoring status and performance uptime metrics
async fn health_check() -> impl IntoResponse {
    let uptime = get_uptime();
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "uptime_seconds": uptime,
            "uptime_formatted": format!("{}h {}m {}s", 
                uptime / 3600,
                (uptime % 3600) / 60,
                uptime % 60
            ),
            "version": env!("CARGO_PKG_VERSION")
        }))
    )
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

    let total_size: usize = files.iter().map(|f| f.len()).sum();
    logging::log_info("Merge", &format!("Starting merge operation for {} files (total {} bytes)", files.len(), total_size));

    // Acquire semaphore permit for memory-intensive PDF operations
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let merged_bytes = pdf_ops::merge_pdfs(files).map_err(|e| {
        logging::log_error("Merge", &format!("Failed to merge PDFs: {}", e));
        ApiError(StatusCode::BAD_REQUEST, e)
    })?;

    logging::log_info("Merge", &format!("Merge completed successfully. Output size: {} bytes", merged_bytes.len()));

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
    
    let file_size = file_bytes.len();
    logging::log_info("Split", &format!("Starting split operation. Ranges: '{}', PDF size: {} bytes", ranges, file_size));

    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let split_bytes = pdf_ops::split_pdf(&file_bytes, &ranges).map_err(|e| {
        logging::log_error("Split", &format!("Failed to split PDF: {}", e));
        ApiError(StatusCode::BAD_REQUEST, e)
    })?;

    logging::log_info("Split", &format!("Split completed successfully. Output size: {} bytes", split_bytes.len()));

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

    let file_size = file_bytes.len();
    logging::log_info("Rotate", &format!("Starting rotate operation. Angle: {}, PDF size: {} bytes", angle, file_size));

    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rotated_bytes = pdf_ops::rotate_pdf(&file_bytes, angle).map_err(|e| {
        logging::log_error("Rotate", &format!("Failed to rotate PDF: {}", e));
        ApiError(StatusCode::BAD_REQUEST, e)
    })?;

    logging::log_info("Rotate", &format!("Rotate completed successfully. Output size: {} bytes", rotated_bytes.len()));

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

    let total_size: usize = images.iter().map(|img| img.len()).sum();
    logging::log_info("ImageToPdf", &format!("Starting conversion for {} images (total {} bytes)", images.len(), total_size));

    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let pdf_bytes = pdf_ops::images_to_pdf(images).map_err(|e| {
        logging::log_error("ImageToPdf", &format!("Failed to convert images: {}", e));
        ApiError(StatusCode::BAD_REQUEST, e)
    })?;

    logging::log_info("ImageToPdf", &format!("Conversion completed successfully. Output size: {} bytes", pdf_bytes.len()));

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

    let file_size = file_bytes.len();
    logging::log_info("Compress", &format!("Starting compression. PDF size: {} bytes", file_size));

    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let compressed_bytes = pdf_ops::compress_pdf(&file_bytes).map_err(|e| {
        logging::log_error("Compress", &format!("Failed to compress PDF: {}", e));
        ApiError(StatusCode::BAD_REQUEST, e)
    })?;

    logging::log_info(
        "Compress",
        &format!(
            "Compression completed. Input: {} bytes, Output: {} bytes ({:.2}% reduction)",
            file_size,
            compressed_bytes.len(),
            (1.0 - (compressed_bytes.len() as f64 / file_size as f64)) * 100.0
        )
    );

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

    let file_size = file_bytes.len();
    logging::log_info("DocxToPdf", &format!("Starting DOCX to PDF conversion. Input size: {} bytes", file_size));

    // Acquire semaphore permit
    let _permit = state.semaphore.acquire().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let pdf_bytes = pdf_ops::docx_to_pdf(&file_bytes).map_err(|e| {
        logging::log_error("DocxToPdf", &format!("Failed to convert DOCX to PDF: {}", e));
        ApiError(StatusCode::BAD_REQUEST, e)
    })?;

    logging::log_info("DocxToPdf", &format!("DOCX to PDF conversion completed. Output size: {} bytes", pdf_bytes.len()));

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"documento_convertido.pdf\""),
        ],
        pdf_bytes,
    ))
}

fn zeroize_string(s: &mut String) {
    use zeroize::Zeroize;
    s.zeroize();
}

/// Protects a PDF document with a password
async fn handle_protect(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let mut file_bytes = None;
    let mut password = None;
    let mut files_count = 0;

    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "files" {
            files_count += 1;
            if files_count > 1 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "Selecione apenas 1 arquivo PDF para proteger.".to_string()));
            }
            let data = field.bytes().await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if data.len() > 30 * 1024 * 1024 {
                return Err(ApiError(StatusCode::BAD_REQUEST, "O arquivo PDF não deve exceder 30MB.".to_string()));
            }
            file_bytes = Some(data.to_vec());
        } else if name == "password" {
            let val = field.text().await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?;
            password = Some(val);
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Arquivo PDF não fornecido.".to_string()))?;
    let mut password = password.ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "Senha não fornecida.".to_string()))?;

    let pass_len = password.chars().count();
    if pass_len < 4 || pass_len > 128 {
        zeroize_string(&mut password);
        return Err(ApiError(StatusCode::BAD_REQUEST, "A senha deve ter entre 4 e 128 caracteres.".to_string()));
    }

    let start_time = Instant::now();
    logging::log_info("Protect", &format!("Starting protect operation. PDF size: {} bytes", file_bytes.len()));

    // Acquire semaphore permit (max 2 concurrent operations)
    let _permit = state.pw_semaphore.acquire().await.map_err(|e| {
        zeroize_string(&mut password);
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    // Call PDF operation
    let protected_bytes = pdf_ops::protect_pdf(&file_bytes, &password).map_err(|e| {
        zeroize_string(&mut password);
        logging::log_error("Protect", &format!("Failed to protect PDF: {}", e));
        ApiError(StatusCode::BAD_REQUEST, e)
    })?;

    // Clean up the password
    zeroize_string(&mut password);

    logging::log_info("Protect", &format!("PDF protected successfully. Output size: {} bytes. Took {:?}", protected_bytes.len(), start_time.elapsed()));

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"protected_document.pdf\""),
        ],
        protected_bytes,
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
    let pw_semaphore = Arc::new(Semaphore::new(2)); // max 2 parallel password operations

    let state = AppState {
        semaphore,
        pw_semaphore,
    };

    Router::new()
        .route("/health", get(health_check))
        .route("/api/merge", post(handle_merge))
        .route("/api/split", post(handle_split))
        .route("/api/rotate", post(handle_rotate))
        .route("/api/compress", post(handle_compress))
        .route("/api/docx-to-pdf", post(handle_docx_to_pdf))
        .route("/api/img-to-pdf", post(handle_img_to_pdf))
        .route("/api/protect", post(handle_protect).route_layer(middleware::from_fn_with_state(security::get_password_rate_limiter(), security::rate_limit_middleware)))
        .with_state(state)
        .layer(middleware::from_fn_with_state(limiter, security::rate_limit_middleware))
        .layer(middleware::from_fn(logging::log_middleware))
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

