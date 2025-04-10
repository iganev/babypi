use std::{path::PathBuf, process::Stdio, str::FromStr, time::Duration};

use actix_web::{web, App, HttpResponse, HttpServer};
use babypi::{
    ffmpeg::{
        audio::FfmpegAudio, audio::FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE,
        audio::FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT, audio::FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE, Ffmpeg,
        FFMPEG_BIN,
    },
    process_control::ProcessControl,
    rpicam::{Rpicam, RpicamCodec, RPICAM_BIN},
};
use bytes::{BufMut, BytesMut};
use clap::Parser;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    runtime, select,
    sync::mpsc,
    time::sleep,
};
use tracing::{error, info};
use tracing_subscriber::{util::SubscriberInitExt, FmtSubscriber};

use babypi::config::CliArgs;

use anyhow::{anyhow, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();

    // Logging
    FmtSubscriber::builder()
        .with_max_level(tracing::Level::from_str("DEBUG")?)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .finish()
        .init();

    // let res = Rpicam::list_cameras().await?;

    // let dev = res
    //     .get(0)
    //     .ok_or_else(|| anyhow!("No devices found"))?
    //     .clone();

    let mut cam = Rpicam::new(
        None,
        Some(RpicamCodec::default()),
        None,
        PathBuf::from_str("/usr/share/libcamera/ipa/rpi/vc4/imx219_noir.json").ok(),
        Some(vec!["--hflip".to_string(), "--vflip".to_string()]),
    )
    .spawn()?;

    let mut rpicam_stdout = cam
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture child process output for {}", RPICAM_BIN))?;

    // let rpicam_stderr = cam
    //     .stderr
    //     .take()
    //     .ok_or_else(|| anyhow!("Failed to capture child process output for {}", RPICAM_BIN))?;

    // // waiter
    // tokio::spawn(async move {
    //     match cam.wait().await {
    //         Ok(code) => {
    //             info!(
    //                 "Child process {} exit code: {}",
    //                 RPICAM_BIN,
    //                 code.code().unwrap_or(-1)
    //             );
    //         }
    //         Err(e) => {
    //             error!("Child process {} error: {}", RPICAM_BIN, e);
    //         }
    //     }
    // });

    let mut process_control_cam = ProcessControl::new("camera", cam)?;

    //

    // let (tx, mut rx) = mpsc::channel::<BytesMut>(100);

    // Spawn a task to read from first process and send to channel
    // let read_task = tokio::spawn(async move {
    //     let mut buffer = BytesMut::with_capacity(33554432);

    //     loop {
    //         // Reserve more space if needed
    //         if buffer.remaining_mut() < 8192 {
    //             buffer.reserve(33554432);
    //         }

    //         match stdout.read_buf(&mut buffer).await {
    //             Ok(0) => {
    //                 error!("Reached the end of the camera output buffer!");
    //                 break; // EOF
    //             }
    //             Ok(_) => {
    //                 let data = buffer.split();
    //                 if tx.send(data).await.is_err() {
    //                     error!("Rx appears to have dropped...");
    //                     break; // Receiver dropped
    //                 }
    //             }
    //             Err(e) => {
    //                 error!("Error reading camera output into buffer: {}", e);
    //                 break;
    //             }
    //         }
    //     }
    // });

    //

    let ffmpeg_audio = FfmpegAudio::new(
        babypi::ffmpeg::audio::FfmpegAudioDeviceType::Pulse,
        "alsa_input.usb-DCMT_Technology_USB_Lavalier_Microphone_214b206000000178-00.mono-fallback", //"hw:3,0",
        Some(FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE),
        Some(babypi::ffmpeg::audio::FfmpegAudioSampleFormat::S16le),
        Some(1),
        Some(babypi::ffmpeg::audio::FfmpegAudioFormat::Aac),
        Some(FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE.to_string()),
    );

    let mut ffmpeg = Ffmpeg::new("/var/stream", Some(ffmpeg_audio), None).spawn()?;

    let mut ffmpeg_stdin = ffmpeg
        .stdin
        .take()
        .ok_or_else(|| anyhow!("Failed to open ffmpeg stdin"))?;

    // let mut ffmpeg_stdout = ffmpeg
    //     .stdout
    //     .take()
    //     .ok_or_else(|| anyhow!("Failed to capture child process output for {}", FFMPEG_BIN))?;

    // let ffmpeg_stderr = ffmpeg
    //     .stderr
    //     .take()
    //     .ok_or_else(|| anyhow!("Failed to capture child process output for {}", FFMPEG_BIN))?;

    // let write_task = tokio::spawn(async move {
    //     while let Some(data) = rx.recv().await {
    //         if ffmpeg_stdin.write_all(&data).await.is_err() {
    //             error!("Error writing buffer to ffmpeg stdin");
    //             // break;
    //         }
    //     }
    //     error!("Rx channel appears to be closed!");
    // });

    tokio::spawn(async move {
        tokio::io::copy(&mut rpicam_stdout, &mut ffmpeg_stdin)
            .await
            .ok();
        error!("Ran out of buffer to move around");
    });
    // if let Some(mut ffmpeg_stdin) = ffmpeg.stdin.take() {
    //     tokio::spawn(async move {
    //         tokio::io::copy(&mut stdout, &mut ffmpeg_stdin).await.ok();
    //     });
    // }

    // let (cam_res, ffmpeg_res) = tokio::join!(cam.wait(), ffmpeg.wait());

    // let _ = tokio::join!(read_task, write_task);

    // cam_res?;
    // ffmpeg_res?;

    // waiter
    // tokio::spawn(async move {
    //     match ffmpeg.wait().await {
    //         Ok(code) => {
    //             info!(
    //                 "Child process {} exit code: {}",
    //                 FFMPEG_BIN,
    //                 code.code().unwrap_or(-1)
    //             );
    //         }
    //         Err(e) => {
    //             error!("Child process {} error: {}", FFMPEG_BIN, e);
    //         }
    //     }
    // });

    //

    // let (tx, mut rx) = mpsc::channel::<String>(10);

    // let tx_rpicam_stderr = tx.clone();
    // tokio::spawn(async move {
    //     let mut reader = BufReader::new(rpicam_stderr).lines();

    //     while let Some(line) = reader
    //         .next_line()
    //         .await
    //         .expect("Failed to read rpicam stderr")
    //     {
    //         tx_rpicam_stderr
    //             .send(format!("RPICAM STDERR: {}", line))
    //             .await
    //             .expect("Failed to send log, receiver closed");
    //     }
    // });

    // let tx_ffmpeg_stdout = tx.clone();
    // tokio::spawn(async move {
    //     let mut reader = BufReader::new(ffmpeg_stdout).lines();

    //     while let Some(line) = reader
    //         .next_line()
    //         .await
    //         .expect("Failed to read ffmpeg stdout")
    //     {
    //         tx_ffmpeg_stdout
    //             .send(format!("FFMPEG STDOUT: {}", line))
    //             .await
    //             .expect("Failed to send log, receiver closed");
    //     }
    // });

    // let tx_ffmpeg_stderr = tx.clone();
    // tokio::spawn(async move {
    //     let mut reader = BufReader::new(ffmpeg_stderr).lines();

    //     while let Some(line) = reader
    //         .next_line()
    //         .await
    //         .expect("Failed to read ffmpeg stderr")
    //     {
    //         tx_ffmpeg_stderr
    //             .send(format!("FFMPEG STDERR: {}", line))
    //             .await
    //             .expect("Failed to send log, receiver closed");
    //     }
    // });

    // log output
    // tokio::spawn(async move {
    //     while let Some(log) = rx.recv().await {
    //         info!(log);
    //     }
    // });

    let mut process_control_ffmpeg = ProcessControl::new("ffmpeg", ffmpeg)?;

    if let Some(mut watch_cam) = process_control_cam.exit_rx() {
        if let Some(mut watch_ffmpeg) = process_control_ffmpeg.exit_rx() {
            tokio::spawn(async move {
                select! {
                    Ok(p) = &mut watch_cam => {
                        info!("Camera exit: {}", p);
                        let _ = process_control_ffmpeg.stop();
                    }
                    Ok(p) = &mut watch_ffmpeg => {
                        info!("FFmpeg exit: {}", p);
                        let _ = process_control_cam.stop();
                    }
                }
            });
        }
    }

    // server
    serve().await?;

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl+c");

    info!("Shutdown signal received");

    Ok(())
}

async fn serve() -> Result<()> {
    let server = HttpServer::new(|| {
        App::new()
            // Serve .m3u8 playlist files with correct MIME type
            .route("/stream/{filename:.*\\.m3u8}", web::get().to(serve_m3u8))
            // Serve .ts segment files with correct MIME type
            .route("/stream/{filename:.*\\.ts}", web::get().to(serve_ts))
            // Serve other static files
            .service(actix_files::Files::new("/stream", "/var/stream"))
    })
    .bind("0.0.0.0:8080")?
    .run();

    // Store handle if you need to stop the server gracefully
    let _server_handle = server.handle();

    // Spawn the server onto the current runtime
    tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("Server error: {}", e);
        }
    });

    Ok(())
}

async fn serve_m3u8(path: web::Path<String>) -> HttpResponse {
    let file_path = PathBuf::from("/var/stream").join(path.into_inner());
    // Return the playlist with appropriate headers
    HttpResponse::Ok()
        .content_type("application/vnd.apple.mpegurl")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Access-Control-Allow-Origin", "*"))
        .body(std::fs::read(file_path).unwrap_or_default())
}

async fn serve_ts(path: web::Path<String>) -> HttpResponse {
    let file_path = PathBuf::from("/var/stream").join(path.into_inner());
    // Return the segment with appropriate headers
    HttpResponse::Ok()
        .content_type("video/MP2T")
        .insert_header(("Cache-Control", "no-cache"))
        .body(std::fs::read(file_path).unwrap_or_default())
}
