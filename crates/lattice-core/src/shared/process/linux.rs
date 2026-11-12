//! The child gets `PR_SET_PDEATHSIG`. The kernel sends it when the *thread*
//! that forked the child exits, so every spawn runs on one thread that lives
//! as long as the process.

use std::io;
use std::sync::mpsc;
use std::sync::OnceLock;

use tokio::process::{Child, Command};
use tokio::runtime::Handle;
use tokio::sync::oneshot;

struct Job {
    command: Command,
    runtime: Handle,
    reply: oneshot::Sender<io::Result<Child>>,
}

static SPAWNER: OnceLock<Result<mpsc::Sender<Job>, String>> = OnceLock::new();

fn start_spawner() -> Result<mpsc::Sender<Job>, String> {
    let (sender, jobs) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name("bound-process-spawner".to_string())
        .spawn(move || {
            for job in jobs {
                let _runtime = job.runtime.enter();
                let mut command = job.command;
                let _ = job.reply.send(command.spawn());
            }
        })
        .map_err(|e| format!("could not start the process spawner thread: {e}"))?;
    Ok(sender)
}

pub(super) async fn spawn(mut command: Command) -> io::Result<Child> {
    let sender = SPAWNER
        .get_or_init(start_spawner)
        .as_ref()
        .map_err(|message| io::Error::other(message.clone()))?;
    let runtime = Handle::try_current().map_err(io::Error::other)?;
    bind(&mut command, std::process::id() as libc::pid_t);
    let (reply, result) = oneshot::channel();
    sender
        .send(Job {
            command,
            runtime,
            reply,
        })
        .map_err(|_| io::Error::other("the process spawner thread ended"))?;
    result
        .await
        .map_err(|_| io::Error::other("the process spawner thread ended"))?
}

#[allow(unsafe_code)]
fn bind(command: &mut Command, parent: libc::pid_t) {
    // SAFETY: the closure runs between fork and exec and calls only prctl and getppid.
    unsafe {
        command.pre_exec(move || die_with_parent(parent));
    }
}

/// Runs in the forked child. If the parent already died, `getppid` no longer matches.
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
