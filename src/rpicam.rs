use std::fmt::Display;
use std::path::PathBuf;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::LazyLock;

use anyhow::anyhow;
use anyhow::Result;
use regex::Regex;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::Command;
use tracing::error;
use tracing::info;

pub const RPICAM_BIN: &str = "rpicam-vid";

pub const RPICAM_LIST_REGEX_DEVICE: &str =
    r#"^(\d+)\s:\s(.*)\s\[(\d+)x(\d+)\s(\d+)-bit\]\s\((.*)\)"#;
pub static RPICAM_LIST_REGEX_DEVICE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(RPICAM_LIST_REGEX_DEVICE).expect("Failed to compile device regex"));

pub const RPICAM_LIST_REGEX_MODE_FORMAT_START: &str =
    r#"'([^']+)'\s*:\s*(\d+)x(\d+)\s+\[(\d+)\..*?\s+fps"#;
pub static RPICAM_LIST_REGEX_MODE_FORMAT_START_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(RPICAM_LIST_REGEX_MODE_FORMAT_START)
        .expect("Failed to compile mode format start regex")
});

pub const RPICAM_LIST_REGEX_MODE_FORMAT_CONTINUE: &str = r#"^\s+(\d+)x(\d+)\s+\[(\d+)\.\d+\s+fps"#;
pub static RPICAM_LIST_REGEX_MODE_FORMAT_CONTINUE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(RPICAM_LIST_REGEX_MODE_FORMAT_CONTINUE)
        .expect("Failed to compile mode format continue regex")
});

#[derive(Clone, Debug)]
pub struct Rpicam {
    pub camera: Option<RpicamDevice>,
    pub codec: Option<RpicamCodec>,
    pub mode: Option<RpicamDeviceMode>,
    pub tuning_file: Option<PathBuf>,
    pub output_file: Option<PathBuf>,
    pub extra_args: Option<Vec<String>>,
    // pub psips_pipe: bool,
}

impl Default for Rpicam {
    fn default() -> Self {
        Self {
            camera: None,
            codec: Default::default(),
            mode: Default::default(),
            tuning_file: None,
            output_file: None,
            extra_args: None,
            // psips_pipe: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RpicamCodec {
    #[default]
    H264,
    MJPEG,
    YUV420,
}

impl Display for RpicamCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpicamCodec::H264 => write!(f, "h264"),
            RpicamCodec::MJPEG => write!(f, "mjpeg"),
            RpicamCodec::YUV420 => write!(f, "yuv420"),
        }
    }
}

impl FromStr for RpicamCodec {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "h264" => Ok(Self::H264),
            "mjpeg" => Ok(Self::MJPEG),
            "yuv420" => Ok(Self::YUV420),
            _ => Err(anyhow!("Unknown codec: {}", s)),
        }
    }
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

#[derive(Clone, Debug)]
pub struct RpicamDeviceMode {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for RpicamDeviceMode {
    fn default() -> Self {
        Self {
            format: "default".to_string(),
            width: 1920,
            height: 1080,
            fps: 30,
        }
    }
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
                // info!("Unparsed Line: {}", line);
            }
        }

        // last one
        if let Some(current_device) = current_device {
            results.push(current_device);
        }

        Ok(results)
    }

    pub fn new(
        camera: Option<RpicamDevice>,
        codec: Option<RpicamCodec>,
        mode: Option<RpicamDeviceMode>,
        tuning_file: Option<PathBuf>,
        output_file: Option<PathBuf>,
        extra_args: Option<Vec<String>>,
        // psips: bool,
    ) -> Self {
        Self {
            camera,
            codec,
            mode,
            tuning_file,
            output_file,
            extra_args,
            // psips_pipe: psips,
        }
    }

    //
    // rpicam-vid -t 0 -n --tuning-file /usr/share/libcamera/ipa/rpi/vc4/imx219_noir.json --codec h264 --framerate 30 --width 1920 --height 1080 --inline --listen -o - | psips > live.h264
    //
    fn build_rpicam_cmd_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        args.push("-t 0".to_string());
        args.push("-n".to_string());

        if let Some(tuning_file) = self.tuning_file.as_deref() {
            args.push(format!("--tuning-file {}", tuning_file.to_string_lossy()));
        }

        if let Some(codec) = &self.codec {
            args.push(format!("--codec {}", codec));

            if *codec == RpicamCodec::H264 {
                args.push("--inline".to_string());
            }
        }

        let (w, h, fps) = if let Some(mode) = self.mode.as_ref() {
            (mode.width, mode.height, mode.fps)
        } else {
            let mode = RpicamDeviceMode::default();
            (mode.width, mode.height, mode.fps)
        };

        args.push(format!("--framerate {}", fps));
        args.push(format!("--width {}", w));
        args.push(format!("--height {}", h));

        let output = self
            .output_file
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("-");

        args.push(format!("-o {}", output));

        args
    }

    pub async fn spawn(&self) -> Result<Child> {
        let args = self.build_rpicam_cmd_args();

        info!("CMD ARGS: {:?}", args);

        let child = Command::new(RPICAM_BIN)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn child process {}: {}", RPICAM_BIN, e))?;

        let Some(_pid) = child.id() else {
            return Err(anyhow!(
                "Failed to resolve child process PID for {}",
                RPICAM_BIN
            ));
        };

        // let stdout = child
        //     .stdout
        //     .take()
        //     .ok_or_else(|| anyhow!("Failed to capture child process output for {}", RPICAM_BIN))?;

        // let stderr = child.stderr.take().ok_or_else(|| {
        //     anyhow!(
        //         "Failed to capture child process err output for {}",
        //         RPICAM_BIN
        //     )
        // })?;

        // let mut reader = BufReader::new(stderr).lines();

        // tokio::spawn(async move {
        //     match child.wait().await {
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

        Ok(child)
    }
}
