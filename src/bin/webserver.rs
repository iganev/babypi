use std::str::FromStr;

use actix_web::{dev::ServerHandle, App, HttpServer};
use babypi::server::middleware::{auth::AuthMiddleware, headers::HlsHeadersMiddleware};
use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::{util::SubscriberInitExt, FmtSubscriber};

use babypi::config::CliArgs;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let _args = CliArgs::parse();

    // Logging
    FmtSubscriber::builder()
        .with_max_level(tracing::Level::from_str("DEBUG")?)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .finish()
        .init();

    let handle = webserver(
        "0.0.0.0:8080",
        "/home/ivan/IdeaProjects/babypi/docs",
        "/home/ivan/public_html/babypi_stream",
        Some("admin"),
        Some("123456"),
        Some("token"),
    )
    .await?;

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl+c");

    info!("Shutdown signal received");

    handle.stop(true).await;

    Ok(())
}

async fn webserver(
    bind: &str,
    static_dir: &str,
    stream_dir: &str,
    username: Option<&str>,
    password: Option<&str>,
    token: Option<&str>,
) -> Result<ServerHandle> {
    let auth = AuthMiddleware::new(
        username.map(str::to_string),
        password.map(str::to_string),
        token.map(str::to_string),
    );

    let static_dir = static_dir.to_string();
    let stream_dir = stream_dir.to_string();

    let server = HttpServer::new(move || {
        App::new()
            .wrap(auth.clone())
            .wrap(HlsHeadersMiddleware)
            .service(actix_files::Files::new("/stream", stream_dir.clone()).use_etag(false))
            .service(actix_files::Files::new("/", static_dir.clone()).index_file("index.html"))
    })
    .bind(bind)?
    .run();

    let server_handle = server.handle();

    tokio::spawn(async move {
        if let Err(e) = server.await {
            error!("Server error: {}", e);
        }
    });

    Ok(server_handle)
}
