use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Instant, Duration};
use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::{Response, IntoResponse},
    http::StatusCode,
    Json,
};
use serde_json::json;

// --- RATE LIMITING ---

struct RateLimitInfo {
    tokens: f64,
    last_update: Instant,
}

pub struct RateLimiter {
    clients: Mutex<HashMap<IpAddr, RateLimitInfo>>,
    max_tokens: f64,
    refill_rate: f64,
}

impl RateLimiter {
    pub fn new(max_requests: f64, per_seconds: f64) -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            max_tokens: max_requests,
            refill_rate: max_requests / per_seconds,
        }
    }

    pub async fn check_limit(&self, ip: IpAddr) -> bool {
        let mut clients = self.clients.lock().await;
        let now = Instant::now();

        // Prevent memory leak by cleaning up inactive entries
        if clients.len() > 1000 {
            let ten_mins = Duration::from_secs(600);
            clients.retain(|_, info| {
                now.checked_duration_since(info.last_update)
                    .map(|d| d < ten_mins)
                    .unwrap_or(true)
            });
        }

        let client_info = clients.entry(ip).or_insert_with(|| RateLimitInfo {
            tokens: self.max_tokens,
            last_update: now,
        });

        let elapsed = now.checked_duration_since(client_info.last_update)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        client_info.tokens = (client_info.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        client_info.last_update = now;

        if client_info.tokens >= 1.0 {
            client_info.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Helper function to extract IP address from headers (reverse proxy) or ConnectInfo
pub fn get_client_ip(req: &Request) -> IpAddr {
    // 1. Check X-Forwarded-For header (useful when behind Render, Cloudflare, etc.)
    if let Some(forwarded_for) = req.headers().get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded_for.to_str() {
            if let Some(first_ip) = forwarded_str.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }

    // 2. Check X-Real-IP header
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(real_ip_str) = real_ip.to_str() {
            if let Ok(ip) = real_ip_str.trim().parse::<IpAddr>() {
                return ip;
            }
        }
    }

    // 3. Fallback to Axum ConnectInfo (local standalone TcpListener)
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip();
    }

    // 4. Default fallback to loopback IP
    IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
}

/// Axum middleware function for Rate Limiting
pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    req: Request,
    next: Next,
) -> Response {
    let client_ip = get_client_ip(&req);

    if limiter.check_limit(client_ip).await {
        next.run(req).await
    } else {
        let body = json!({
            "error": "Limite de requisições excedido. Por favor, aguarde um momento antes de tentar novamente."
        });
        (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response()
    }
}


