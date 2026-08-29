use axum::{routing::{get, post}, Router, extract::State, response::IntoResponse, http::{HeaderMap, StatusCode}};
use std::sync::Arc;
use serde_json::Value;

const ZEN_BASE: &str = "https://opencode.ai/zen/v1";
const USER_AGENT: &str = "opencode/1.18.18";

#[derive(Clone)]
struct AppState { client: reqwest::Client }

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let state = Arc::new(AppState { client: reqwest::Client::new() });
    let app = Router::new()
        .route("/api/status", get(status))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(proxy))
        .route("/v1/messages", post(proxy))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:4096").await.unwrap();
    println!("zen-proxy http://0.0.0.0:4096");
    axum::serve(listener, app).await.unwrap();
}

async fn status(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let mem = 0;
    axum::Json(serde_json::json!({"status":"ok","mode":"zen-rust","port":4096}))
}
async fn models(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let r = s.client.get(format!("{}/models", ZEN_BASE)).header("User-Agent", USER_AGENT).send().await.unwrap();
    let j: Value = r.json().await.unwrap_or(serde_json::json!({"data":[]}));
    axum::Json(j)
}
async fn proxy(State(s): State<Arc<AppState>>, headers: HeaderMap, body: axum::Json<Value>) -> impl IntoResponse {
    let mut v = body.0;
    if let Some(m) = v.get("model").and_then(|x| x.as_str()) {
        let m2 = m.replace("[1m]", "").replace("[128k]", "");
        v["model"] = Value::String(m2);
    }
    let path = if v.get("model").and_then(|x| x.as_str()).map(|x| x.starts_with("muse-spark")).unwrap_or(false) { "/responses" } else { "/chat/completions" };
    let r = s.client.post(format!("{}{}", ZEN_BASE, path)).header("User-Agent", USER_AGENT).header("Content-Type", "application/json").json(&v).send().await.unwrap();
    let status = r.status();
    let bytes = r.bytes().await.unwrap();
    (status, headers, bytes)
}
