use axum::{
    middleware::Next,
    response::Response,
    http::Request,
    body::Body,
};
use std::time::Instant;
use chrono::Local;

/// Middleware to log HTTP requests (method, route, response status, and duration)
/// without printing any identifiable user data (such as IP or User-Agents).
pub async fn log_middleware(req: Request<Body>, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_owned();

    // Execute the request down the middleware chain / to the handler
    let response = next.run(req).await;

    // Do not log health checks to prevent clogging production logs in Render/cloud
    if path == "/health" {
        return response;
    }

    let duration = start.elapsed();
    let status = response.status();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");

    // Print logs console-friendly (stdout for info/warn, stderr for server errors)
    if status.is_success() {
        println!(
            "[{}] [INFO] {} {} -> {} ({:?})",
            timestamp,
            method,
            path,
            status.as_u16(),
            duration
        );
    } else if status.is_client_error() {
        println!(
            "[{}] [WARN] {} {} -> {} ({:?})",
            timestamp,
            method,
            path,
            status.as_u16(),
            duration
        );
    } else {
        eprintln!(
            "[{}] [ERROR] {} {} -> {} ({:?})",
            timestamp,
            method,
            path,
            status.as_u16(),
            duration
        );
    }

    response
}

/// Log an operational informational message
pub fn log_info(tag: &str, msg: &str) {
    println!(
        "[{}] [INFO] [{}] {}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        tag,
        msg
    );
}

/// Log an operational warning message
pub fn log_warn(tag: &str, msg: &str) {
    println!(
        "[{}] [WARN] [{}] {}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        tag,
        msg
    );
}

/// Log an operational error message to stderr
pub fn log_error(tag: &str, msg: &str) {
    eprintln!(
        "[{}] [ERROR] [{}] {}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        tag,
        msg
    );
}

