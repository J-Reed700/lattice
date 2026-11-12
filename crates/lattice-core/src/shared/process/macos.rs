//! macOS has no parent-death signal. Between fork and exec the child forks a
//! watcher that waits on a kqueue for this process and for the child to exit.
//! If this process goes first the watcher kills the child; if the child goes
//! first the watcher just ends. The watcher closes every fd, so it holds
//! neither the exec error pipe nor the child's stdio pipes open.

use std::io;

use tokio::process::{Child, Command};

pub(super) async fn spawn(mut command: Command) -> io::Result<Child> {
    bind(&mut command, std::process::id() as libc::pid_t);
    command.spawn()
}

#[allow(unsafe_code)]
fn bind(command: &mut Command, lattice: libc::pid_t) {
    // SAFETY: the closure runs between fork and exec and forks the watcher, which uses only
    // async-signal-safe calls and never returns.
    unsafe {
        command.pre_exec(move || fork_watcher(lattice));
    }
}

#[allow(unsafe_code)]
fn fork_watcher(lattice: libc::pid_t) -> io::Result<()> {
    // SAFETY: fork is async-signal-safe.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        watch(lattice)
    } else if pid < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Runs in the forked watcher: no allocation, no std I/O, no panics. A setup failure kills the
/// child, so a child is never left running unbound.
#[allow(unsafe_code)]
fn watch(lattice: libc::pid_t) -> ! {
    // SAFETY: only async-signal-safe libc calls, on values owned by this process.
    unsafe {
        let child = libc::getppid();
        let limit = libc::getdtablesize();
        let mut fd = 0;
        while fd < limit {
            libc::close(fd);
            fd += 1;
        }
        let kq = libc::kqueue();
        if kq < 0 {
            end(child, true);
        }
        if register(kq, lattice).is_err() {
            end(child, true);
        }
        if let Err(errno) = register(kq, child) {
            end(child, errno != libc::ESRCH);
        }
        let mut events: [libc::kevent; 2] = std::mem::zeroed();
        loop {
            let ready = libc::kevent(
                kq,
                std::ptr::null(),
                0,
                events.as_mut_ptr(),
                events.len() as libc::c_int,
                std::ptr::null(),
            );
            if ready < 0 {
                if *libc::__error() == libc::EINTR {
                    continue;
                }
                end(child, true);
            }
            let mut lattice_gone = false;
            for event in events.iter().take(ready as usize) {
                if event.ident == child as libc::uintptr_t {
                    end(child, false);
                }
                if event.ident == lattice as libc::uintptr_t {
                    lattice_gone = true;
                }
            }
            if lattice_gone {
                end(child, true);
            }
        }
    }
}

/// Registers an exit notification for `pid`; the error is errno (`ESRCH` when `pid` is gone).
#[allow(unsafe_code)]
unsafe fn register(kq: libc::c_int, pid: libc::pid_t) -> Result<(), libc::c_int> {
    let mut change: libc::kevent = std::mem::zeroed();
    change.ident = pid as libc::uintptr_t;
    change.filter = libc::EVFILT_PROC;
    change.flags = libc::EV_ADD;
    change.fflags = libc::NOTE_EXIT;
    if libc::kevent(kq, &change, 1, std::ptr::null_mut(), 0, std::ptr::null()) < 0 {
        Err(*libc::__error())
    } else {
        Ok(())
    }
}

#[allow(unsafe_code)]
unsafe fn end(child: libc::pid_t, kill_child: bool) -> ! {
    if kill_child {
        libc::kill(child, libc::SIGKILL);
    }
    libc::_exit(0)
}
