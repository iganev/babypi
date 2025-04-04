use std::fmt::Display;

use anyhow::anyhow;
use anyhow::Result;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::sync::oneshot::channel as oneshot_channel;
use tokio::sync::oneshot::Receiver;
use tokio::task::JoinHandle;
use tracing::error;
use tracing::info;
use tracing::warn;

#[derive(Clone, Debug)]
pub struct ProcessExit {
    code: i32,
    message: Option<String>,
}

impl ProcessExit {
    pub fn new(code: i32, message: Option<String>) -> Self {
        Self { code, message }
    }
}

impl Display for ProcessExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "code = \"{}\"; message = \"{}\"",
            self.code,
            self.message.as_deref().unwrap_or_default()
        )
    }
}

#[derive(Debug)]
pub struct ProcessControl {
    id: String,
    pid: u32,
    logger: JoinHandle<()>,
    waiter: JoinHandle<()>,
    exit_rx: Option<Receiver<ProcessExit>>,
    stopped: bool,
}

impl ProcessControl {
    pub fn new(id: impl ToString, mut child: Child) -> Result<Self> {
        let Some(pid) = child.id() else {
            return Err(anyhow!("Failed to resolve child process PID"));
        };

        let log_id = id.to_string();

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Failed to capture child process output for {}", &log_id))?;

        // logger
        let handle_logger = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();

            while let Ok(Some(line)) = reader.next_line().await.or_else(|e| {
                error!(
                    target = "process_control",
                    "Failed to read stderr for process `{}`: {}", &log_id, e
                );

                Err(None::<String>)
            }) {
                let line = line.replace("\r", "");

                info!(
                    target = "process_control",
                    "PROC[{}] STDERR: {}", &log_id, line
                );
            }
        });

        let log_id = id.to_string();

        let (exit_tx, exit_rx) = oneshot_channel::<ProcessExit>();

        // waiter
        let handle_waiter = tokio::spawn(async move {
            match child.wait().await {
                Ok(code) => {
                    let code = code.code().unwrap_or(-1);
                    info!(
                        target = "process_control",
                        "Child process `{}` exited with code: {}", &log_id, code
                    );

                    if let Err(e) = exit_tx.send(ProcessExit::new(code, None)) {
                        error!(
                            target = "process_control",
                            "Failed to report process `{}` exit: {}", &log_id, e
                        );
                    }
                }
                Err(e) => {
                    error!(
                        target = "process_control",
                        "Child process `{}` await error: {}", &log_id, e
                    );

                    if let Err(e) =
                        exit_tx.send(ProcessExit::new(-1, Some(format!("Error: {}", e))))
                    {
                        error!(
                            target = "process_control",
                            "Failed to report process `{}` exit: {}", &log_id, e
                        );
                    }
                }
            }
        });

        Ok(Self {
            id: id.to_string(),
            pid,
            logger: handle_logger,
            waiter: handle_waiter,
            exit_rx: Some(exit_rx),
            stopped: false,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn exit_rx(&mut self) -> Option<Receiver<ProcessExit>> {
        self.exit_rx.take()
    }

    pub fn stop(&mut self) -> Result<()> {
        self.stopped = true;
        self.send_signal_inner(Signal::SIGINT)
    }

    pub fn kill(&mut self) -> Result<()> {
        self.stopped = true;
        self.send_signal_inner(Signal::SIGTERM)
    }

    fn send_signal_inner(&mut self, sig: Signal) -> Result<()> {
        let nix_pid = Pid::from_raw(self.pid as i32);
        kill(nix_pid, sig).map_err(|e| {
            anyhow!(
                "Error sending {} to process with PID {}: {}",
                sig,
                self.pid,
                e
            )
        })
    }
}

impl Drop for ProcessControl {
    fn drop(&mut self) {
        if !self.stopped {
            warn!(
                target = "process_control",
                "Process control dropped, terminating process `{}`", self.id
            );

            let _ = self.stop();

            self.logger.abort();
            self.waiter.abort();
        }
    }
}
