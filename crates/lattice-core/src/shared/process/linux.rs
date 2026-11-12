//! RED: PDEATHSIG set on the calling thread, with no spawner thread.

use std::io;

use tokio::process::{Child, Command};

pub(super) async fn spawn(mut command: Command) -> io::Result<Child> {
    bind(&mut command, std::process::id() as libc::pid_t);
    command.spawn()
}

#[allow(unsafe_code)]
fn bind(command: &mut Command, parent: libc::pid_t) {
    // SAFETY: the closure runs between fork and exec and calls only prctl and getppid.
    unsafe {
        command.pre_exec(move || die_with_parent(parent));
    }
}

#[allow(unsafe_code)]
fn die_with_parent(parent: libc::pid_t) -> io::Result<()> {
    // SAFETY: prctl with these arguments only sets a signal on the calling process.
    let set = unsafe {
        libc::prctl(
            libc::PR_SET_PDEATHSIG,
            libc::SIGKILL as libc::c_ulong,
            0,
            0,
            0,
        )
    };
    if set != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: getppid takes no arguments and cannot fail.
    if unsafe { libc::getppid() } != parent {
        return Err(io::Error::from_raw_os_error(libc::ESRCH));
    }
    Ok(())
}
