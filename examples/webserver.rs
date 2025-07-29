use std::{str::FromStr, time::Duration};

use actix_cors::Cors;
use actix_files::Files;
use actix_web::{
    dev::ServerHandle,
    http::header::{ContentType, ACCEPT, AUTHORIZATION, CONTENT_TYPE, RANGE},
    mime, web, App, HttpResponse, HttpServer,
};
use babypi::{
    ffmpeg::FFMPEG_DEFAULT_STREAM_DIR,
    server::{
        middleware::{auth::AuthMiddleware, headers::HlsHeadersMiddleware},
        websocket::ws_handler_telemetry,
        DEFAULT_MICRO_UI,
    },
    telemetry::events::EventDispatcher,
};
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

    // Events
    let events = EventDispatcher::new();

    let handle = webserver(
        &events,
        "0.0.0.0:8080",
        None,
        Some("/home/ivan/public_html/babypi_stream"),
        Some("admin"),
        Some("123456"),
        Some("token"),
    )
    .await?;

    let sender = events.get_sender();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        let mut counter = 0;

        loop {
            interval.tick().await;
            counter += 1;

            let _ = sender.send(babypi::telemetry::events::Event::Test(format!(
                "Test {}",
                counter
            )));
        }
    });

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl+c");

    info!("Shutdown signal received");

    handle.stop(true).await;

    info!("Done");

    Ok(())
}

async fn webserver(
    events: &EventDispatcher,
    bind: &str,
    static_dir: Option<&str>,
    stream_dir: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
    token: Option<&str>,
) -> Result<ServerHandle> {
    let auth = AuthMiddleware::new(
        username.map(str::to_string),
        password.map(str::to_string),
        token.map(str::to_string),
    );

    let static_dir = static_dir.map(str::to_string);
    let stream_dir = stream_dir.unwrap_or(FFMPEG_DEFAULT_STREAM_DIR).to_string();

    let events = events.clone();

    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST", "HEAD", "OPTIONS"])
            .allowed_headers(vec![AUTHORIZATION, ACCEPT, RANGE])
            .allowed_header(CONTENT_TYPE)
            .max_age(None);

        let mut app = App::new()
            .app_data(web::Data::new(events.clone()))
            .wrap(cors)
            .wrap(auth.clone())
            .wrap(HlsHeadersMiddleware)
            .service(Files::new("/stream", stream_dir.clone()).use_etag(false));

        app = app.route("/telemetry", web::get().to(ws_handler_telemetry));

        if let Some(static_dir) = static_dir.clone() {
            app = app.service(Files::new("/", static_dir).index_file("index.html"));
        } else {
            app = app.route(
                "/",
                web::route().to(|| async {
                    HttpResponse::Ok()
                        .insert_header(ContentType(mime::TEXT_HTML))
                        .body(DEFAULT_MICRO_UI)
                }),
            );
        }

        app
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
