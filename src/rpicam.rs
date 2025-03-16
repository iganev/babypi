use std::path::PathBuf;
use std::process::Stdio;
use std::sync::LazyLock;

use anyhow::anyhow;
use anyhow::Result;
use regex::Regex;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tracing::error;
use tracing::info;

pub const RPICAM_BIN: &str = "rpicam-vid";

pub const RPICAM_LIST_REGEX_DEVICE: &str =
    r#"^(\d+)\s:\s(.*)\s\[(\d+)x(\d+)\s(\d+)-bit\]\s\((.*)\)"#;
pub static RPICAM_LIST_REGEX_DEVICE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(RPICAM_LIST_REGEX_DEVICE).expect("Failed to compile device regex"));

pub const RPICAM_LIST_REGEX_MODE_FORMAT_START: &str =
    r#"^\s+'([^']+)'\s*:\s*(\d+)x(\d+)\s+\[(\d+)\..*?\s+fps"#;
pub static RPICAM_LIST_REGEX_MODE_FORMAT_START_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(RPICAM_LIST_REGEX_MODE_FORMAT_START)
        .expect("Failed to compile mode format start regex")
});

pub const RPICAM_LIST_REGEX_MODE_FORMAT_CONTINUE: &str = r#"^\s+(\d+)x(\d+)\s+\[(\d+)\.\d+\s+fps"#;
pub static RPICAM_LIST_REGEX_MODE_FORMAT_CONTINUE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(RPICAM_LIST_REGEX_MODE_FORMAT_CONTINUE)
        .expect("Failed to compile mode format continue regex")
});

//
// rpicam-vid -t 0 -n --tuning-file /usr/share/libcamera/ipa/rpi/vc4/imx219_noir.json --codec h264 --framerate 30 --width 1920 --height 1080 --inline --listen -o - | psips > live.h264
//
#[derive(Clone, Debug)]
pub struct Rpicam {
    pub camera: Option<RpicamDevice>,
    pub tuning_file: Option<PathBuf>,
    pub codec: Option<RpicamCodec>,
    pub framerate: u32,
    pub width: u32,
    pub height: u32,
    pub psips_pipe: bool,
    pub extra_args: Option<Vec<String>>,
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
    pub index: u32,
    pub sensor: String,
    pub max_width: u32,
    pub max_height: u32,
    pub max_bits: u32,
    pub path: String,
    pub modes: Vec<RpicamDeviceMode>,
}

impl RpicamDevice {
    pub fn new(
        index: u32,
        sensor: impl ToString,
        max_width: u32,
        max_height: u32,
        max_bits: u32,
        path: impl ToString,
    ) -> Self {
        Self {
            index,
            sensor: sensor.to_string(),
            max_width,
            max_height,
            max_bits,
            path: path.to_string(),
            modes: Vec::new(),
        }
    }

    pub fn add_mode(&mut self, mode: RpicamDeviceMode) -> &mut Self {
        self.modes.push(mode);

        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct RpicamDeviceMode {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl RpicamDeviceMode {
    pub fn new(format: impl ToString, width: u32, height: u32, fps: u32) -> Self {
        Self {
            format: format.to_string(),
            width,
            height,
            fps,
        }
    }
}

impl Rpicam {
    pub async fn list_cameras() -> Result<Vec<RpicamDevice>> {
        let mut child = Command::new(RPICAM_BIN)
            .arg("--list-cameras")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn child process {}: {}", RPICAM_BIN, e))?;

        let Some(_pid) = child.id() else {
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

        let mut results = Vec::new();
        let mut current_device = None;
        let mut current_format = None;

        while let Some(line) = reader.next_line().await? {
            if let Some((_full, [index, sensor, max_width, max_height, max_bits, path])) =
                RPICAM_LIST_REGEX_DEVICE_REGEX
                    .captures(&line)
                    .map(|caps| caps.extract())
            {
                if let Some(current_device) = current_device {
                    results.push(current_device);
                }

                current_device = Some(RpicamDevice::new(
                    index
                        .parse::<u32>()
                        .map_err(|e| anyhow!("Failed to parse device index: {}", e))?,
                    sensor,
                    max_width
                        .parse::<u32>()
                        .map_err(|e| anyhow!("Failed to parse device max width: {}", e))?,
                    max_height
                        .parse::<u32>()
                        .map_err(|e| anyhow!("Failed to parse device max height: {}", e))?,
                    max_bits
                        .parse::<u32>()
                        .map_err(|e| anyhow!("Failed to parse device max bits: {}", e))?,
                    path,
                ));
            } else if let Some((full, [format, width, height, fps])) =
                RPICAM_LIST_REGEX_MODE_FORMAT_START_REGEX
                    .captures(&line)
                    .map(|caps| caps.extract())
            {
                if let Some(current_device) = current_device.as_mut() {
                    current_format = Some(format.to_string());

                    current_device.add_mode(RpicamDeviceMode::new(
                        format,
                        width
                            .parse::<u32>()
                            .map_err(|e| anyhow!("Failed to parse device mode width: {}", e))?,
                        height
                            .parse::<u32>()
                            .map_err(|e| anyhow!("Failed to parse device mode height: {}", e))?,
                        fps.parse::<u32>()
                            .map_err(|e| anyhow!("Failed to parse device mode fps: {}", e))?,
                    ));
                } else {
                    return Err(anyhow!("Failed to parse device information: {}", full));
                }
            } else if let Some((full, [width, height, fps])) =
                RPICAM_LIST_REGEX_MODE_FORMAT_CONTINUE_REGEX
                    .captures(&line)
                    .map(|caps| caps.extract())
            {
                if let Some(current_device) = current_device.as_mut() {
                    if let Some(current_format) = current_format.as_deref() {
                        current_device.add_mode(RpicamDeviceMode::new(
                            current_format,
                            width
                                .parse::<u32>()
                                .map_err(|e| anyhow!("Failed to parse device mode width: {}", e))?,
                            height.parse::<u32>().map_err(|e| {
                                anyhow!("Failed to parse device mode height: {}", e)
                            })?,
                            fps.parse::<u32>()
                                .map_err(|e| anyhow!("Failed to parse device mode fps: {}", e))?,
                        ));
                    } else {
                        return Err(anyhow!("Failed to parse device information: {}", full));
                    }
                } else {
                    return Err(anyhow!("Failed to parse device information: {}", full));
                }
            } else {
                println!("Unparsed Line: {}", line);
            }
        }

        // last one
        if let Some(current_device) = current_device {
            results.push(current_device);
        }

        Ok(results)
    }
}
