use std::{path::PathBuf, process::Stdio, str::FromStr, time::Duration};

use babypi::rpicam::{Rpicam, RpicamCodec, RpicamDeviceMode, RPICAM_BIN};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
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

    // let mut stdout = cam
    //     .stdout
    //     .take()
    //     .ok_or_else(|| anyhow!("Failed to capture child process output for {}", RPICAM_BIN))?;

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
        "live.h264", //pipe:
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
        .spawn()?;

    // if let Some(mut ffmpeg_stdin) = ffmpeg.stdin.take() {
    //     tokio::spawn(async move {
    //         tokio::io::copy(&mut stdout, &mut ffmpeg_stdin).await.ok();
    //     });
    // }

    tokio::spawn(async move {
        match ffmpeg.wait().await {
            Ok(code) => {
                info!(
                    "Child process {} exit code: {}",
                    "ffmpeg",
                    code.code().unwrap_or(-1)
                );
            }
            Err(e) => {
                error!("Child process {} error: {}", "ffmpeg", e);
            }
        }
    });

    Ok(())
}
