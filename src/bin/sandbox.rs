use std::{path::PathBuf, process::Stdio, str::FromStr, time::Duration};

use babypi::rpicam::{Rpicam, RpicamCodec, RpicamDeviceMode, RPICAM_BIN};
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

    let tuning_file = PathBuf::from_str("/usr/share/libcamera/ipa/rpi/vc4/imx219_noir.json")
        .map_err(|e| anyhow!("Failed to locate tuning file: {}", e))?;

    let mut cam = Rpicam::new(
        None,
        Some(RpicamCodec::default()),
        None,
        Some(tuning_file),
        None,
        None,
    )
    .spawn()
    .await?;

    let mut stdout = cam
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture child process output for {}", RPICAM_BIN))?;

    // sleep(Duration::from_secs(2)).await;

    // let stderr = cam.stderr.take().ok_or_else(|| {
    //     anyhow!(
    //         "Failed to capture child process err output for {}",
    //         RPICAM_BIN
    //     )
    // })?;

    // let mut reader = BufReader::new(stderr).lines();

    // while let Some(line) = reader.next_line().await? {
    //     info!("Process {}: {}", RPICAM_BIN, line);
    // }

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

    //

    let (tx, mut rx) = mpsc::channel::<BytesMut>(100);

    // Spawn a task to read from first process and send to channel
    let read_task = tokio::spawn(async move {
        let mut buffer = BytesMut::with_capacity(33554432);

        loop {
            // Reserve more space if needed
            if buffer.remaining_mut() < 8192 {
                buffer.reserve(33554432);
            }

            match stdout.read_buf(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let data = buffer.split();
                    if tx.send(data).await.is_err() {
                        break; // Receiver dropped
                    }
                }
                Err(_) => break,
            }
        }
    });

    //

    sleep(Duration::from_secs(1)).await;

    //ffmpeg -y \
    //   -probesize 32M \
    //   -thread_queue_size 256 \
    //   -use_wallclock_as_timestamps 1 \
    //   -i live.h264 \
    //   -c:v copy \
    //   -f segment \
    //   -segment_time 4 \
    //   -segment_format mpegts \
    //   -segment_list "/var/stream/live.m3u8" \
    //   -segment_list_size 8 \
    //   -segment_list_flags live \
    //   -segment_list_type m3u8 \
    //   -segment_wrap 10 \
    //   "/var/stream/%08d.ts"

    let ffmpeg_args = [
        "-y",
        "-probesize",
        "32M",
        "-thread_queue_size",
        "256",
        "-use_wallclock_as_timestamps",
        "1",
        "-i",
        "pipe:",
        "-c:v",
        "copy",
        "-f",
        "segment",
        "-segment_time",
        "4",
        "-segment_format",
        "mpegts",
        "-segment_list",
        "/var/stream/live.m3u8",
        "-segment_list_size",
        "8",
        "-segment_list_flags",
        "live",
        "-segment_list_type",
        "m3u8",
        "-segment_wrap",
        "10",
        "\"/var/stream/%08d.ts\"",
    ];

    let mut ffmpeg = Command::new("ffmpeg")
        .args(&ffmpeg_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;

    let mut ffmpeg_stdin = ffmpeg
        .stdin
        .take()
        .ok_or_else(|| anyhow!("Failed to open ffmpeg stdin"))?;

    let write_task = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if ffmpeg_stdin.write_all(&data).await.is_err() {
                break;
            }
        }
        // second_stdin will be closed when dropped at the end of this scope
    });

    // if let Some(mut ffmpeg_stdin) = ffmpeg.stdin.take() {
    //     tokio::spawn(async move {
    //         tokio::io::copy(&mut stdout, &mut ffmpeg_stdin).await.ok();
    //     });
    // }

    let (cam_res, ffmpeg_res) = tokio::join!(cam.wait(), ffmpeg.wait());

    let _ = tokio::join!(read_task, write_task);

    cam_res?;
    ffmpeg_res?;

    // tokio::spawn(async move {
    //     match ffmpeg.wait().await {
    //         Ok(code) => {
    //             info!(
    //                 "Child process {} exit code: {}",
    //                 "ffmpeg",
    //                 code.code().unwrap_or(-1)
    //             );
    //         }
    //         Err(e) => {
    //             error!("Child process {} error: {}", "ffmpeg", e);
    //         }
    //     }
    // });

    Ok(())
}
