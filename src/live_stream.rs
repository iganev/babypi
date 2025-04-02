use std::sync::Arc;
use std::time::Duration;

use crate::ffmpeg::FFMPEG_BIN;
use crate::rpicam::RPICAM_BIN;
use crate::{ffmpeg::Ffmpeg, process_control::ProcessControl, rpicam::Rpicam};
use anyhow::anyhow;
use anyhow::Result;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

pub const LIVE_STREAM_BOOTSTRAP_RETRY: u8 = 10;

#[derive(Debug, Default)]
struct LiveStreamState {
    rpicam_process: Option<ProcessControl>,
    ffmpeg_process: Option<ProcessControl>,

    handle_pipe: Option<JoinHandle<()>>,
    handle_watch: Option<JoinHandle<()>>,

    running: bool,
    retry_count: u8,
}

impl LiveStreamState {
    pub fn update(
        &mut self,
        rpicam_process: Option<ProcessControl>,
        ffmpeg_process: Option<ProcessControl>,
        handle_pipe: Option<JoinHandle<()>>,
        handle_watch: Option<JoinHandle<()>>,
        running: bool,
        // retry_count: u8,
    ) {
        self.rpicam_process = rpicam_process;
        self.ffmpeg_process = ffmpeg_process;
        self.handle_pipe = handle_pipe;
        self.handle_watch = handle_watch;
        self.running = running;
        // self.retry_count = retry_count;
    }

    pub async fn reset(&mut self) {
        self.stop().await;
        self.retry_count = 0;
    }

    pub async fn stop(&mut self) {
        self.running = false;

        if let Some(handle_pipe) = self.handle_pipe.take() {
            handle_pipe.abort();
        }

        if let Some(handle_watch) = self.handle_watch.take() {
            handle_watch.abort();
        }

        if let Some(ffmpeg_process) = self.ffmpeg_process.take() {
            if let Err(e) = ffmpeg_process.stop() {
                error!(
                    target = "live_stream",
                    "Error while stopping `{}`: {}", FFMPEG_BIN, e
                );
            }
        }

        if let Some(rpicam_process) = self.rpicam_process.take() {
            if let Err(e) = rpicam_process.stop() {
                error!(
                    target = "live_stream",
                    "Error while stopping `{}`: {}", RPICAM_BIN, e
                );
            }
        }
    }

    pub fn retry_increment(&mut self) -> u8 {
        self.retry_count += 1;

        self.retry_count
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}

#[derive(Debug)]
pub struct LiveStream {
    rpicam: Rpicam,
    ffmpeg: Ffmpeg,

    state: Arc<RwLock<LiveStreamState>>,
    // rpicam_process: Option<ProcessControl>,
    // ffmpeg_process: Option<ProcessControl>,

    // handle_pipe: Option<JoinHandle<()>>,
    // handle_watch: Option<JoinHandle<()>>,

    // running: Arc<RwLock<bool>>,
    // retry_count: u8,
}

impl LiveStream {
    pub fn new(rpicam: Rpicam, ffmpeg: Ffmpeg) -> Self {
        Self {
            rpicam,
            ffmpeg,
            state: Arc::new(RwLock::new(LiveStreamState::default())),
            // rpicam_process: None,
            // ffmpeg_process: None,
            // handle_pipe: None,
            // handle_watch: None,
            // running: Arc::new(RwLock::new(false)),
            // retry_count: 0,
        }
    }

    /// Start streaming
    pub async fn start(&self) -> Result<()> {
        loop {
            if let Err(e) = self.start_inner().await {
                error!(
                    target = "live_stream",
                    "Error while starting live stream: {}", e
                );

                let current_retry_count = self.state.write().await.retry_increment();

                if current_retry_count < LIVE_STREAM_BOOTSTRAP_RETRY {
                    warn!(target = "live_stream", "Retrying in 3 seconds...");
                    tokio::time::sleep(Duration::from_secs(3)).await
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(())
    }

    async fn start_inner(&self) -> Result<()> {
        let mut rpicam_child = self.rpicam.spawn()?;
        let mut rpicam_stdout = rpicam_child.stdout.take().ok_or_else(|| {
            anyhow!(
                "Failed to capture child process output for `{}`",
                RPICAM_BIN
            )
        })?;
        let mut rpicam_process = ProcessControl::new(RPICAM_BIN, rpicam_child)?;

        info!(
            target = "live_stream",
            "Bootstrapped `{}` for live streaming", RPICAM_BIN
        );

        let mut ffmpeg_child = self.ffmpeg.spawn()?;
        let mut ffmpeg_stdin = ffmpeg_child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to open child process input for `{}`", FFMPEG_BIN))?;
        let mut ffmpeg_process = ProcessControl::new(FFMPEG_BIN, ffmpeg_child)?;

        info!(
            target = "live_stream",
            "Bootstrapped `{}` for live streaming", FFMPEG_BIN
        );

        let handle_pipe = tokio::spawn(async move {
            tokio::io::copy(&mut rpicam_stdout, &mut ffmpeg_stdin)
                .await
                .ok();
            error!(target = "live_stream", "Ran out of buffer to move around");
        });

        info!(target = "live_stream", "Connected IO pipe");

        let state_ref = self.state.clone();

        let handle_watch = if let Some(mut watch_cam) = rpicam_process.exit_rx() {
            if let Some(mut watch_ffmpeg) = ffmpeg_process.exit_rx() {
                Ok(tokio::spawn(async move {
                    tokio::select! {
                        Ok(p) = &mut watch_cam => {
                            warn!(target = "live_stream", "Process `{}` exit: {}", RPICAM_BIN, p);
                            // let _ = process_control_ffmpeg.stop();
                        }
                        Ok(p) = &mut watch_ffmpeg => {
                            warn!(target = "live_stream", "Process `{}` exit: {}", FFMPEG_BIN, p);
                            // let _ = process_control_cam.stop();
                        }
                    }

                    state_ref.write().await.stop().await;
                }))
            } else {
                Err(anyhow!("Failed to get watch receiver for `{}`", FFMPEG_BIN))
            }
        } else {
            Err(anyhow!("Failed to get watch receiver for `{}`", RPICAM_BIN))
        }?;

        info!(target = "live_stream", "Setup watch task");

        self.state.write().await.update(
            Some(rpicam_process),
            Some(ffmpeg_process),
            Some(handle_pipe),
            Some(handle_watch),
            true,
            // 0,
        );

        Ok(())
    }

    /// Stop streaming and reset state
    pub async fn stop(&self) {
        self.state.write().await.stop().await;
    }

    /// Are we live?
    pub async fn is_running(&self) -> bool {
        self.state.read().await.is_running()
    }
}
