//! The only way production code starts a child process.
//!
//! A child started here ends when this process ends, however it ends (quit,
//! crash, kill), because the kernel or a watcher outside this process does it.
//! Windows puts every child in one process-wide Job Object that kills its
//! members when the process dies. Linux sets a parent-death signal from a
//! spawner thread that lives as long as the process. macOS forks a watcher
//! that kills the child when this process exits. Dropping the `Child` kills it.

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::{Output, Stdio};

use tokio::process::{Child, Command};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use self::linux as strategy;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as strategy;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as strategy;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("Lattice supports Windows, Linux and macOS; each needs a child process binding");

#[must_use = "a command does nothing until it is spawned"]
pub struct BoundCommand {
    command: Command,
}

impl BoundCommand {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            command: Command::new(program),
        }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.command.arg(arg);
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(args);
        self
    }

    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.command.env(key, value);
        self
    }

    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.envs(vars);
        self
    }

    pub fn env_remove(mut self, key: impl AsRef<OsStr>) -> Self {
        self.command.env_remove(key);
        self
    }

    pub fn env_clear(mut self) -> Self {
        self.command.env_clear();
        self
    }

    pub fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.command.current_dir(dir);
        self
    }

    pub fn stdin(mut self, cfg: impl Into<Stdio>) -> Self {
        self.command.stdin(cfg);
        self
    }

    pub fn stdout(mut self, cfg: impl Into<Stdio>) -> Self {
        self.command.stdout(cfg);
        self
    }

    pub fn stderr(mut self, cfg: impl Into<Stdio>) -> Self {
        self.command.stderr(cfg);
        self
    }

    /// Starts the child bound to this process. Dropping the returned `Child` kills it.
    pub async fn spawn(mut self) -> io::Result<Child> {
        self.command.kill_on_drop(true);
        strategy::spawn(self.command).await
    }

    /// `spawn` with stdin null and stdout/stderr piped, then `wait_with_output`.
    pub async fn output(self) -> io::Result<Output> {
        self.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .await?
            .wait_with_output()
            .await
    }
}

#[cfg(test)]
mod tests;
