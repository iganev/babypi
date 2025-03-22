use std::{path::PathBuf, process::Stdio, str::FromStr, time::Duration};

use babypi::{
    ffmpeg::{Ffmpeg, FFMPEG_BIN},
    rpicam::{Rpicam, RpicamCodec, RPICAM_BIN},
};
use bytes::{BufMut, BytesMut};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
    time::sleep,
};
use tracing::{error, info};
use tracing_subscriber::{util::SubscriberInitExt, FmtSubscriber};

use anyhow::{anyhow, Result};

#[tokio::main]
async fn main() -> Result<()> {
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
        None,
    )
    .spawn()?;

    let mut rpicam_stdout = cam
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture child process output for {}", RPICAM_BIN))?;

    let mut rpicam_stderr = cam
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to capture child process output for {}", RPICAM_BIN))?;

    // waiter
    tokio::spawn(async move {
        match cam.wait().await {
            Ok(code) => {
                info!(
                    "Child process {} exit code: {}",
                    RPICAM_BIN,
                    code.code().unwrap_or(-1)
                );
            }
            Err(e) => {
                error!("Child process {} error: {}", RPICAM_BIN, e);
            }
        }
    });

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

    let mut ffmpeg = Ffmpeg::new("/var/stream", None, None).spawn()?;

    let mut ffmpeg_stdin = ffmpeg
        .stdin
        .take()
        .ok_or_else(|| anyhow!("Failed to open ffmpeg stdin"))?;

    let mut ffmpeg_stdout = ffmpeg
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture child process output for {}", FFMPEG_BIN))?;

    let mut ffmpeg_stderr = ffmpeg
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to capture child process output for {}", FFMPEG_BIN))?;

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
    tokio::spawn(async move {
        match ffmpeg.wait().await {
            Ok(code) => {
                info!(
                    "Child process {} exit code: {}",
                    FFMPEG_BIN,
                    code.code().unwrap_or(-1)
                );
            }
            Err(e) => {
                error!("Child process {} error: {}", FFMPEG_BIN, e);
            }
        }
    });

    //

    let (tx, mut rx) = mpsc::channel::<String>(10);

    let tx_rpicam_stdout = tx.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(rpicam_stderr).lines();

        while let Some(line) = reader
            .next_line()
            .await
            .expect("Failed to read rpicam stderr")
        {
            tx_rpicam_stdout
                .send(format!("RPICAM STDERR: {}", line))
                .await
                .expect("Failed to send log, receiver closed");
        }
    });

    let tx_ffmpeg_stdout = tx.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(ffmpeg_stdout).lines();

        while let Some(line) = reader
            .next_line()
            .await
            .expect("Failed to read ffmpeg stdout")
        {
            tx_ffmpeg_stdout
                .send(format!("FFMPEG STDOUT: {}", line))
                .await
                .expect("Failed to send log, receiver closed");
        }
    });

    let tx_ffmpeg_stderr = tx.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(ffmpeg_stderr).lines();

        while let Some(line) = reader
            .next_line()
            .await
            .expect("Failed to read ffmpeg stderr")
        {
            tx_ffmpeg_stderr
                .send(format!("FFMPEG STDERR: {}", line))
                .await
                .expect("Failed to send log, receiver closed");
        }
    });

    while let Some(log) = rx.recv().await {
        info!(log);
    }

    Ok(())
}
