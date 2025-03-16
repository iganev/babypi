use std::path::PathBuf;
use std::process::Stdio;

use anyhow::anyhow;
use anyhow::Result;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio_stream::wrappers::errors;
use tracing::error;
use tracing::info;

pub const RPICAM_BIN: &str = "rpicam-vid";

//
// rpicam-vid -t 0 -n --tuning-file /usr/share/libcamera/ipa/rpi/vc4/imx219_noir.json --codec h264 --framerate 30 --width 1920 --height 1080 --inline --listen -o - | psips > live.h264
//
#[derive(Clone, Debug)]
pub struct Rpicam {
    camera: Option<RpicamDevice>,
    tuning_file: Option<PathBuf>,
    codec: Option<RpicamCodec>,
    framerate: u32,
    width: u32,
    height: u32,
    psips_pipe: bool,
    extra_args: Option<Vec<String>>,
}

impl Default for Rpicam {
    fn default() -> Self {
        Self {
            camera: None,
            tuning_file: None,
            codec: Default::default(),
            framerate: 30,
            width: 1920,
            height: 1080,
            psips_pipe: true,
            extra_args: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum RpicamCodec {
    #[default]
    H264,
    MJPEG,
    YUV420,
}

#[derive(Clone, Debug, Default)]
pub struct RpicamDevice {
    index: u32,
    sensor: String,
    max_width: u32,
    max_height: u32,
    max_bits: u32,
    path: String,
}

impl Rpicam {
    pub async fn list_cameras() -> Result<Vec<RpicamDevice>> {
        let mut child = Command::new(RPICAM_BIN)
            .arg("--list-cameras")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn child process {}: {}", RPICAM_BIN, e))?;

        let Some(pid) = child.id() else {
            return Err(anyhow!(
                "Failed to resolve child process PID for {}",
                RPICAM_BIN
            ));
        };

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture child process output for {}", RPICAM_BIN))?;

        let mut reader = BufReader::new(stdout).lines();

        tokio::spawn(async move {
            match child.wait().await {
                Ok(code) => {
                    info!("Child process {} exit code: {}", RPICAM_BIN, code);
                }
                Err(e) => {
                    error!("Child process {} error: {}", RPICAM_BIN, e);
                }
            }
        });

        while let Some(line) = reader.next_line().await? {
            println!("Line: {}", line);
        }

        Err(anyhow!("asdf"))
    }
}
