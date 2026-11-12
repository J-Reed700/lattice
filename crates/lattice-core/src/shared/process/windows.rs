//! RED: a plain spawn, as every call site did before.

use std::io;

use tokio::process::{Child, Command};

pub(super) async fn spawn(mut command: Command) -> io::Result<Child> {
    command.spawn()
}
