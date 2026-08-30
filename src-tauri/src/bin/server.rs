use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use cc_switch_lib::database::Database;
use cc_switch_lib::provider::{Provider, ProviderMeta};
use cc_switch_lib::proxy::{ProxyConfig, ProxyServer};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize logging
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    println!("\n=======================================================");
    println!("  🚀 CC Switch Headless Proxy Server (Native Engine)");
    println!("  - 版本: v{}", env!("CARGO_PKG_VERSION"));
    println!("=======================================================");

    // 2. Initialize database
    let db = Arc::new(Database::init()?);

    // 3. Auto-seed OpenCode Zen provider if empty or missing
    bootstrap_default_providers(&db)?;

    // 4. Configure Proxy Server
    let listen_port = env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(4096);

    let listen_address = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

    let mut proxy_config = ProxyConfig::default();
    proxy_config.listen_port = listen_port;
    proxy_config.listen_address = listen_address.clone();

    // 5a. Initialize global HTTP client (read proxy from env: HTTP_PROXY / HTTPS_PROXY / ALL_PROXY)
    let proxy_env = env::var("HTTP_PROXY")
        .or_else(|_| env::var("HTTPS_PROXY"))
        .or_else(|_| env::var("ALL_PROXY"))
        .ok();
    let proxy_url = proxy_env.as_deref().filter(|s| !s.is_empty());
    if let Err(e) = cc_switch_lib::proxy::http_client::init(proxy_url) {
        log::warn!("[Server] 全局 HTTP 客户端初始化失败: {e}，将使用系统代理回退");
    } else {
        log::info!("[Server] 全局 HTTP 客户端初始化: {}", proxy_url.unwrap_or("直连"));
    }

    let server = ProxyServer::new(proxy_config, db.clone(), None);

    // 5. Start server
    let info = match server.start().await {
        Ok(info) => info,
        Err(e) => {
            eprintln!("❌ 启动失败: {}", e);
            return Err(e.into());
        }
    };

    println!("  ✓ 代理服务器就绪！");
    println!("  - 监听地址: http://{}:{}", info.address, info.port);
    let display_host = if info.address == "0.0.0.0" { "127.0.0.1" } else { &info.address };
    println!("  - Claude Base URL: http://{}:{}/v1", display_host, info.port);
    println!("  - 状态检查: http://{}:{}/health", display_host, info.port);
    println!("  - 原生引擎: 100% CC Switch (Anthropic ↔ OpenAI Chat & Responses, Tool Calling, Reasoning, SSE)");
    println!("=======================================================");
    println!("  [运行中] 按 Ctrl+C 可停止服务...\n");

    // 6. Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    println!("\n🛑 收到退出信号，正在平滑关闭服务...");
    let _ = server.stop().await;
    println!("✓ 服务已正常退出。");

    Ok(())
}

fn bootstrap_default_providers(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    let zen_base = env::var("OPENCODE_ZEN_BASE")
        .unwrap_or_else(|_| "https://opencode.ai/zen/v1".to_string());
    let zen_api_key = env::var("OPENCODE_ZEN_API_KEY")
        .unwrap_or_else(|_| "not-needed".to_string());

    let app_types = ["claude", "opencode", "codex"];

    for app_type in &app_types {
        let mut meta = ProviderMeta::default();
        if *app_type == "codex" {
            meta.api_format = Some("openai_responses".to_string());
        } else {
            meta.api_format = Some("openai_chat".to_string());
        }

        let settings_config = json!({
            "env": {
                "ANTHROPIC_BASE_URL": zen_base,
                "ANTHROPIC_AUTH_TOKEN": zen_api_key,
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "mimo-v2.5-free",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "muse-spark-1.2-contributor-free",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "hy3-free"
            },
            "options": {
                "baseURL": zen_base,
                "apiKey": zen_api_key
            }
        });

        let mut provider = Provider::with_id(
            "opencode-zen".to_string(),
            "OpenCode Zen".to_string(),
            settings_config,
            Some("https://opencode.ai".to_string()),
        );
        provider.meta = Some(meta);

        db.save_provider(app_type, &provider)?;
        let _ = db.set_current_provider(app_type, "opencode-zen");
        log::info!("已锁定 [{app_type}] 供应商: opencode-zen (上游: {zen_base})");
    }

    Ok(())
}
