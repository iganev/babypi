use std::fs::OpenOptions;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use actix_cors::Cors;
use actix_files::Files;
use actix_web::dev::ServerHandle;
use actix_web::http::header::ContentType;
use actix_web::http::header::ACCEPT;
use actix_web::http::header::AUTHORIZATION;
use actix_web::http::header::CONTENT_TYPE;
use actix_web::http::header::RANGE;
use actix_web::mime;
use actix_web::web;
use actix_web::App;
use actix_web::HttpResponse;
use actix_web::HttpServer;
use anyhow::Result;

// use audio_monitor::AudioMonitor;
use config::TomlConfig;
use ffmpeg::audio::FfmpegAudio;
use ffmpeg::audio::FFMPEG_DEFAULT_AUDIO_DEVICE;
use ffmpeg::Ffmpeg;
use ffmpeg::FfmpegExtraArgs;
use ffmpeg::FFMPEG_DEFAULT_STREAM_DIR;
use image::codecs::webp::WebPEncoder;
use image::ExtendedColorType;
use live_stream::LiveStream;
use rpicam::Rpicam;
use rpicam::RpicamDeviceMode;
use tracing::debug;
use tracing::error;
use tracing::info;

use crate::audio_monitor::AudioMonitor;
use crate::audio_monitor::AudioMonitorContext;
use crate::ffmpeg::audio::FfmpegAudioSampleFormat;
use crate::ffmpeg::audio::FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT;
use crate::ffmpeg::audio::FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE;
use crate::server::middleware::auth::AuthMiddleware;
use crate::server::middleware::headers::HlsHeadersMiddleware;
use crate::server::websocket::ws_handler_telemetry;
use crate::server::DEFAULT_MICRO_UI;
use crate::telemetry::events::EventDispatcher;

pub mod audio_monitor;
pub mod config;
pub mod ffmpeg;
pub mod gpio;
pub mod live_stream;
pub mod mlx90640;
pub mod mmwave;
pub mod process_control;
pub mod rpicam;
pub mod serde_stuff;
pub mod server;
pub mod telemetry;

/// Check if file exists
pub async fn file_exists(file: impl AsRef<Path>) -> bool {
    tokio::fs::try_exists(file).await.is_ok_and(|res| res)
}

#[derive(Debug)]
pub struct BabyPi {
    config: TomlConfig,
    verbose: bool,
    events: EventDispatcher,

    live_stream: Option<LiveStream>,
    web_server: Option<ServerHandle>,
    audio_monitor: Option<AudioMonitor>,
}

impl BabyPi {
    pub fn new(config: TomlConfig, verbose: bool) -> Self {
        Self {
            config,
            verbose,
            events: EventDispatcher::new(),
            live_stream: None,
            web_server: None,
            audio_monitor: None,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        self.live_stream = Some(self.run_live_stream().await?);
        self.web_server = Some(self.run_web_server().await?);

        if self.config.monitoring.enabled {
            self.audio_monitor = Some(self.run_audio_monitor().await?);
        }

        // tokio::spawn(SnapshotActor::new(self.events.clone()).run());

        let events = self.events.clone();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_secs(60));
            let mut rx = events.get_receiver();

            loop {
                tokio::select! {
                    _ = timer.tick() => {
                        events.send(telemetry::events::Event::SnapshotRequest);
                        info!("Sent snapshot request");
                    }
                    event = rx.recv() => {
                        if let Ok(telemetry::events::Event::SnapshotData { data }) = event {
                            info!("Received snapshot data");

                            let mut file = OpenOptions::new()
                                        .write(true)
                                        .create(true)
                                        .truncate(true)
                                        .open("/var/stream/snapshot.webp").expect("Failed to open file snapshot.webp");

                            let encoder = WebPEncoder::new_lossless(&mut file);
                            match encoder.encode(&data, data.width(), data.height(), ExtendedColorType::Rgb8) {
                                Ok(_) => {
                                    info!("Saved snapshot.webp");
                                }
                                Err(e) => {
                                    debug!("Failed to encode jpg image: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(web_server) = self.web_server.take() {
            web_server.stop(true).await;
        }

        if let Some(live_stream) = self.live_stream.take() {
            live_stream.stop().await;
        }

        if let Some(mut audio_monitor) = self.audio_monitor.take() {
            audio_monitor.stop().await;
        }

        Ok(())
    }

    async fn run_live_stream(&self) -> Result<LiveStream> {
        let mode = if self.config.hardware.camera.width.is_some()
            && self.config.hardware.camera.height.is_some()
            && self.config.hardware.camera.fps.is_some()
        {
            Some(RpicamDeviceMode::new(
                "selected",
                self.config.hardware.camera.width.unwrap(),
                self.config.hardware.camera.height.unwrap(),
                self.config.hardware.camera.fps.unwrap(),
            ))
        } else {
            None
        };

        let cam = Rpicam::new(
            self.config.hardware.camera.device.clone(),
            self.config.hardware.camera.codec.clone(),
            mode,
            self.config.hardware.camera.tuning_file.clone(),
            self.config.hardware.camera.hflip.unwrap_or(false),
            self.config.hardware.camera.vflip.unwrap_or(false),
            self.config.hardware.camera.extra_args.as_deref().map(|s| {
                s.split(" ")
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<String>>()
            }),
        );

        let ffmpeg_audio =
            if self.config.stream.audio.is_some_and(|v| v) && self.config.hardware.mic.enabled {
                Some(FfmpegAudio::new(
                    self.config
                        .hardware
                        .mic
                        .interface
                        .clone()
                        .unwrap_or_default(),
                    self.config
                        .hardware
                        .mic
                        .device
                        .as_deref()
                        .unwrap_or(FFMPEG_DEFAULT_AUDIO_DEVICE),
                    self.config.hardware.mic.sample_rate,
                    self.config.hardware.mic.sample_format.clone(),
                    self.config.hardware.mic.channels,
                    self.config.hardware.mic.output_format.clone(),
                    self.config.hardware.mic.output_bitrate.clone(),
                ))
            } else {
                None
            };

        let extra_args = if self.config.stream.extra_args_audio_input.is_some()
            || self.config.stream.extra_args_video_input.is_some()
            || self.config.stream.extra_args_setup.is_some()
            || self.config.stream.extra_args_output.is_some()
        {
            Some(FfmpegExtraArgs {
                setup: self.config.stream.extra_args_setup.as_deref().map(|s| {
                    s.split(" ")
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<String>>()
                }),
                video_input: self
                    .config
                    .stream
                    .extra_args_video_input
                    .as_deref()
                    .map(|s| {
                        s.split(" ")
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<String>>()
                    }),
                audio_input: self
                    .config
                    .stream
                    .extra_args_audio_input
                    .as_deref()
                    .map(|s| {
                        s.split(" ")
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<String>>()
                    }),
                output: self.config.stream.extra_args_output.as_deref().map(|s| {
                    s.split(" ")
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<String>>()
                }),
            })
        } else {
            None
        };

        let ffmpeg = Ffmpeg::new(
            self.config
                .stream
                .data_dir
                .clone()
                .unwrap_or(FFMPEG_DEFAULT_STREAM_DIR.into()),
            ffmpeg_audio,
            extra_args,
            self.verbose,
        );

        let live_stream = LiveStream::new(cam, ffmpeg, self.events.clone());

        live_stream.start().await;

        Ok(live_stream)
    }

    async fn run_web_server(&mut self) -> Result<ServerHandle> {
        let auth = AuthMiddleware::new(
            self.config.server.basic_username.clone(),
            self.config.server.basic_password.clone(),
            self.config.server.bearer_token.clone(),
        );

        let static_dir = self.config.server.webroot.clone();
        let stream_dir = self
            .config
            .stream
            .data_dir
            .as_ref()
            .and_then(|p| p.to_str().map(str::to_string))
            .unwrap_or(FFMPEG_DEFAULT_STREAM_DIR.to_string());

        let telemetry_config = self.config.telemetry.clone();

        let events = self.events.clone();

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
                .wrap(HlsHeadersMiddleware);

            app = app.service(Files::new("/stream", stream_dir.clone()).use_etag(false));

            if telemetry_config.enabled {
                app = app.route("/telemetry", web::get().to(ws_handler_telemetry));
            }

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
        .bind(self.config.server.bind.as_deref().unwrap_or("0.0.0.0:8080"))?
        .run();

        let server_handle = server.handle();

        tokio::spawn(async move {
            if let Err(e) = server.await {
                error!(target = "web_server", "Server error: {}", e);
            }
        });

        Ok(server_handle)
    }

    async fn run_audio_monitor(&mut self) -> Result<AudioMonitor> {
        let mut monitor = AudioMonitor::new(
            AudioMonitorContext::new(
                self.config
                    .hardware
                    .mic
                    .sample_format
                    .clone()
                    .unwrap_or(FfmpegAudioSampleFormat::from_str(
                        FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT,
                    )?)
                    .into(),
                self.config
                    .hardware
                    .mic
                    .sample_rate
                    .unwrap_or(FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE),
                self.config.hardware.mic.channels.unwrap_or(1),
                self.config.hardware.mic.device.clone(),
                self.config.monitoring.rms_threshold,
            ),
            Some(self.events.get_sender()),
        );

        monitor.start().await?;

        Ok(monitor)
    }
}

// pub struct SnapshotActor {
//     events: EventDispatcher,
// }

// impl SnapshotActor {
//     fn new(events: EventDispatcher) -> Self {
//         Self { events }
//     }

//     async fn handle_raw_frame_event(&mut self, data: Vec<u8>) -> Result<()> {
//         let mut decoder = Decoder::new()?;
//         let mut img_data = Vec::new();
//         let mut w: u32 = 0;
//         let mut h: u32 = 0;

//         for packet in nal_units(&data) {
//             if let Ok(Some(frame)) = decoder.decode(packet) {
//                 img_data = vec![0; frame.dimensions().0 * frame.dimensions().1 * 3];
//                 w = frame.dimensions().0 as u32;
//                 h = frame.dimensions().1 as u32;
//                 frame.write_rgb8(&mut img_data);
//                 break;
//             }
//         }

//         if !img_data.is_empty() && w > 0 && h > 0 {
//             if let Some(img) = RgbImage::from_raw(w, h, img_data) {
//                 let mut img_enc = Vec::new();
//                 let mut cursor = Cursor::new(&mut img_enc);
//                 let encoder = WebPEncoder::new_lossless(&mut cursor);
//                 match encoder.encode(&img, w, h, ExtendedColorType::Rgb8) {
//                     Ok(_) => {
//                         // TODO TODO TODO
//                         let mut file = OpenOptions::new()
//                             .write(true)
//                             .create(true)
//                             .truncate(true)
//                             .open("/var/stream/snapshot.webp")
//                             .await?;

//                         file.write_all(&img_enc).await?;
//                         file.flush().await?;

//                         self.events
//                             .send(telemetry::events::Event::SnapshotData { data: img_enc });
//                     }
//                     Err(e) => {
//                         debug!("Failed to encode jpg image: {}", e);
//                     }
//                 }
//             } else {
//                 debug!("Failed to parse RGB image data");
//             }
//         } else {
//             debug!("Failed to detect frame in nal units");
//         }

//         Ok(())
//     }

//     async fn run(mut self) {
//         let mut rx = self.events.get_receiver();

//         while let Ok(event) = rx.recv().await {
//             if let telemetry::events::Event::RawFrameData { data } = event {
//                 let _ = self.handle_raw_frame_event(data).await;
//             }
//         }
//     }
// }
