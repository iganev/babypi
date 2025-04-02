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

#[derive(Debug)]
pub struct LiveStream {
    rpicam: Rpicam,
    ffmpeg: Ffmpeg,

    rpicam_process: Option<ProcessControl>,
    ffmpeg_process: Option<ProcessControl>,

    handle_pipe: Option<JoinHandle<()>>,
    handle_watch: Option<JoinHandle<()>>,

    running: Arc<RwLock<bool>>,
    retry_count: u8,
}

impl LiveStream {
    pub fn new(rpicam: Rpicam, ffmpeg: Ffmpeg) -> Self {
        Self {
            rpicam,
            ffmpeg,
            rpicam_process: None,
            ffmpeg_process: None,
            handle_pipe: None,
            handle_watch: None,
            running: Arc::new(RwLock::new(false)),
            retry_count: 0,
        }
    }

    /// Start streaming
    pub async fn start(&mut self) -> Result<()> {
        loop {
            if let Err(e) = self.start_inner().await {
                error!(
                    target = "live_stream",
                    "Error while starting live stream: {}", e
                );
                self.retry_count += 1;

                if self.retry_count < LIVE_STREAM_BOOTSTRAP_RETRY {
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

    async fn start_inner(&mut self) -> Result<()> {
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

        let running_ref = self.running.clone();

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

                    let mut running_write_lock = running_ref.write().await;
                    *running_write_lock = false;
                    drop(running_write_lock);
                }))
            } else {
                Err(anyhow!("Failed to get watch receiver for `{}`", FFMPEG_BIN))
            }
        } else {
            Err(anyhow!("Failed to get watch receiver for `{}`", RPICAM_BIN))
        }?;

        info!(target = "live_stream", "Setup watch task");

        self.rpicam_process = Some(rpicam_process);
        self.ffmpeg_process = Some(ffmpeg_process);
        self.handle_pipe = Some(handle_pipe);
        self.handle_watch = Some(handle_watch);

        let mut running_write_lock = self.running.write().await;
        *running_write_lock = true;
        drop(running_write_lock);

        Ok(())
    }

    /// Stop streaming and reset state
    pub async fn stop(&mut self) {
        self.retry_count = 0;
        let mut running_write_lock = self.running.write().await;
        *running_write_lock = false;
        drop(running_write_lock);

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

    /// Are we live?
    pub async fn is_running(&self) -> bool {
        let lock = self.running.read().await;
        let val = *lock;
        drop(lock);
        val
    }
}
