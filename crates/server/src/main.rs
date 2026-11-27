use std::process::ExitCode;

use anyq_server::api::app;
use anyq_server::config::Config;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("review") {
        return anyq_server::cli::main(&args[1..]);
    }
    // Same defaults as `review`: the local config if there is one, the key from `.env`.
    let path = args.first().cloned().unwrap_or_else(|| {
        if std::path::Path::new("anyq.toml").exists() {
            "anyq.toml".into()
        } else {
            "anyq.example.toml".into()
        }
    });
    match serve(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("anyq-server: {error}");
            ExitCode::FAILURE
        }
    }
}

#[tokio::main]
async fn serve(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let config = Config::load(path)?;
    let listener = tokio::net::TcpListener::bind(&config.listen).await?;
    tracing::info!(listen = %config.listen, config = %path, "anyq-server is up");
    let dotenv = std::fs::read_to_string(".env")
        .map(|t| anyq_server::cli::dotenv(&t))
        .unwrap_or_default();
    let state = config.into_state(&|name| {
        std::env::var(name)
            .ok()
            .or_else(|| dotenv.get(name).cloned())
    })?;
    axum::serve(listener, app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
