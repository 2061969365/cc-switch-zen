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
    proxy_config.auto_start = true;

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
        let existing = db.get_provider_by_id("opencode-zen", app_type)?;
        if existing.is_none() {
            let mut custom_headers = HashMap::new();
            custom_headers.insert("User-Agent".to_string(), "opencode/1.18.18".to_string());

            let mut meta = ProviderMeta::default();
            meta.api_format = Some("openai_chat".to_string());
            meta.custom_headers = custom_headers;

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
                "OpenCode Zen (Auto Seeded)".to_string(),
                settings_config,
                Some("https://opencode.ai".to_string()),
            );
            provider.meta = Some(meta);

            db.save_provider(app_type, &provider)?;
            log::info!("已自动初始化 [{app_type}] 供应商: opencode-zen");
        }

        // Ensure active provider is set
        if db.get_current_provider(app_type)?.is_none() {
            let _ = db.switch_provider(app_type, "opencode-zen");
            log::info!("已自动激活 [{app_type}] 默认供应商: opencode-zen");
        }
    }

    Ok(())
}
