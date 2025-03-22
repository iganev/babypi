use anyhow::anyhow;
use anyhow::Result;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use tokio::process::Child;
use tokio::sync::mpsc::Sender;

pub enum ProcSignal {
    SigInt,
    SigKill,
}

pub struct ProcessControl {
    child: Child,
    pid: u32,
    tx: Sender<ProcSignal>,
}

impl ProcessControl {
    pub fn new(child: Child) -> Result<Self> {
        let Some(pid) = child.id() else {
            return Err(anyhow!("Failed to resolve child process PID"));
        };

        Ok(Self {
            child,
            pid,
            tx: todo!(),
        })
    }

    pub fn stop(&self) -> Result<()> {
        let nix_pid = Pid::from_raw(self.pid as i32);
        // Send SIGINT to the process
        kill(nix_pid, Signal::SIGINT).map_err(|e| {
            anyhow!(
                "Error sending SIGINT to process with PID {}: {}",
                self.pid,
                e
            )
        })
    }

    pub fn kill(&self) -> Result<()> {
        // let nix_pid = Pid::from_raw(self.pid as i32);
        // // Send SIGINT to the process
        // kill(nix_pid, Signal::SIGTERM).map_err(|e| {
        //     anyhow!(
        //         "Error sending SIGTERM to process with PID {}: {}",
        //         self.pid,
        //         e
        //     )
        // })

        Ok(())
    }
}
