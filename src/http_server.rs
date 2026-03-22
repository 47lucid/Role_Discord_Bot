use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct ServerState {
    pub last_ping: Arc<AtomicU64>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            last_ping: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn update_ping(&self) {
        self.last_ping
            .store(chrono::Utc::now().timestamp() as u64, Ordering::SeqCst);
    }
}

async fn health_check(State(state): State<ServerState>) -> impl IntoResponse {
    state.update_ping();
    Json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "uptime_check": "v1"
    }))
}

async fn ready(State(state): State<ServerState>) -> impl IntoResponse {
    state.update_ping();
    Json(json!({
        "ready": true,
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

pub async fn start_http_server(state: ServerState) {
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/ready", get(ready))
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind("0.0.0.0:8080").await {
        Ok(listener) => {
            println!("✅ HTTP server listening on 0.0.0.0:8080");
            listener
        }
        Err(e) => {
            eprintln!("❌ Failed to bind HTTP server to port 8080: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("❌ HTTP server error: {}", e);
        std::process::exit(1);
    }
}
