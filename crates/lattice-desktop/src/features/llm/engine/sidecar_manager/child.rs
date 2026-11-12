//! A started llama-server and the events it produces.
//!
//! One task owns the tokio `Child`. It waits for the exit, reads both output
//! pipes, and acts on kill requests, so a kill can only ever reach this child
//! and never a pid the OS has since handed to another process. If the task is
//! dropped (runtime shutdown) the `Child` drops and `kill_on_drop` ends the
//! process.

use lattice_core::shared::process::BoundCommand;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Notify;

/// Output is read as fast as the consumer drains it; one slot keeps a stalled
/// consumer from letting the pipes buffer without bound.
const EVENT_CHANNEL_CAPACITY: usize = 1;

/// How a process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features::llm::engine) struct ExitReport {
    pub(in crate::features::llm::engine) code: Option<i32>,
    /// The signal that ended it. Always `None` on Windows.
    pub(in crate::features::llm::engine) signal: Option<i32>,
}

/// What a sidecar did. Lines include their terminator.
#[derive(Debug, Clone)]
pub(in crate::features::llm::engine) enum ServerEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Error(String),
    /// Always the last event, sent after every output line.
    Terminated(ExitReport),
}

/// The handle to a running sidecar: a way to ask for its death.
pub(in crate::features::llm::engine) struct ServerProcess {
    kill: Arc<Notify>,
}

impl ServerProcess {
    /// Asks the owning task to kill the process. Does not block and is
    /// idempotent: the request is a stored permit, and the task ignores it
    /// once the process has been reaped.
    pub(in crate::features::llm::engine) fn kill(&self) -> std::io::Result<()> {
        self.kill.notify_one();
        Ok(())
    }
}

/// Starts `command` with its output piped into the returned events.
pub(super) async fn start(
    command: BoundCommand,
) -> std::io::Result<(Receiver<ServerEvent>, ServerProcess)> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .await?;
    tracing::debug!(pid = child.id(), "sidecar started");
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let kill = Arc::new(Notify::new());
    let (tx, rx) = channel(EVENT_CHANNEL_CAPACITY);
    let requested = Arc::clone(&kill);

    // raw-spawn: owns the child for its life; a runtime shutdown drops it, which kills it.
    tokio::spawn(async move {
        let (report, _, _) = tokio::join!(
            wait_or_kill(&mut child, &requested),
            read_pipe(stdout, &tx, ServerEvent::Stdout),
            read_pipe(stderr, &tx, ServerEvent::Stderr),
        );
        let event = match report {
            Ok(report) => ServerEvent::Terminated(report),
            Err(err) => ServerEvent::Error(err.to_string()),
        };
        let _ = tx.send(event).await;
    });

    Ok((rx, ServerProcess { kill }))
}

async fn wait_or_kill(
    child: &mut tokio::process::Child,
    kill: &Notify,
) -> std::io::Result<ExitReport> {
    let status = tokio::select! {
        status = child.wait() => status?,
        () = kill.notified() => {
            let _ = child.start_kill();
            child.wait().await?
        }
    };
    Ok(ExitReport {
        code: status.code(),
        signal: signal_of(&status),
    })
}

#[cfg(unix)]
fn signal_of(status: &std::process::ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(status)
}

#[cfg(not(unix))]
fn signal_of(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

async fn read_pipe<R: AsyncRead + Unpin>(
    pipe: Option<R>,
    tx: &Sender<ServerEvent>,
    wrap: fn(Vec<u8>) -> ServerEvent,
) {
    let Some(pipe) = pipe else { return };
    let mut reader = BufReader::new(pipe);
    loop {
        let mut line = Vec::new();
        match read_line(&mut reader, &mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let _ = tx.send(wrap(line)).await;
            }
            Err(err) => {
                let _ = tx.send(ServerEvent::Error(err.to_string())).await;
                break;
            }
        }
    }
}

/// Reads up to and including the first `\n` in the buffered data; failing
/// that, up to and including the first `\r`; failing that, everything buffered,
/// continuing until one of those or EOF. The log readers trim the terminator.
async fn read_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    line: &mut Vec<u8>,
) -> std::io::Result<usize> {
    let mut read = 0;
    loop {
        let (done, used) = {
            let available = reader.fill_buf().await?;
            let end = available
                .iter()
                .position(|byte| *byte == b'\n')
                .or_else(|| available.iter().position(|byte| *byte == b'\r'))
                .map(|index| index + 1);
            match end {
                Some(end) => {
                    line.extend_from_slice(available.get(..end).unwrap_or_default());
                    (true, end)
                }
                None => {
                    line.extend_from_slice(available);
                    (false, available.len())
                }
            }
        };
        reader.consume(used);
        read += used;
        if done || used == 0 {
            return Ok(read);
        }
    }
}

#[cfg(test)]
mod tests;
