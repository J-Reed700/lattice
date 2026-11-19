//! One process-wide Job Object kills its members when this process dies. The
//! child starts suspended, joins the job, then runs, so it is never unbound.

use std::ffi::c_void;
use std::io;
use std::sync::OnceLock;

use tokio::process::{Child, Command};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_BASIC_LIMIT_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{
    OpenThread, ResumeThread, CREATE_NO_WINDOW, CREATE_SUSPENDED, THREAD_SUSPEND_RESUME,
};

// global-state: the job is the process's own; only the process's end closes it, and that
// ends every child in it.
/// The job handle, or why it could not be made.
static JOB: OnceLock<Result<isize, String>> = OnceLock::new();

fn windows_error(what: &str, error: windows::core::Error) -> io::Error {
    io::Error::other(format!("{what} failed: {error}"))
}

fn job() -> io::Result<HANDLE> {
    match JOB.get_or_init(create_job) {
        Ok(raw) => Ok(HANDLE(*raw)),
        Err(message) => Err(io::Error::other(message.clone())),
    }
}

#[allow(unsafe_code)]
fn create_job() -> Result<isize, String> {
    // SAFETY: null name and attributes make a private, non-inheritable job owned by this process.
    let handle = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
        .map_err(|e| format!("CreateJobObjectW failed: {e}"))?;
    let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
            LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: `info` is a live value of the size passed, and `handle` is the job just created.
    let set = unsafe {
        SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if let Err(error) = set {
        // SAFETY: `handle` is open and owned here.
        let _ = unsafe { CloseHandle(handle) };
        return Err(format!("SetInformationJobObject failed: {error}"));
    }
    Ok(handle.0)
}

pub(super) async fn spawn(mut command: Command) -> io::Result<Child> {
    let job = job()?;
    command.creation_flags(CREATE_NO_WINDOW.0 | CREATE_SUSPENDED.0);
    let mut child = command.spawn()?;
    if let Err(error) = bind_and_resume(&child, job) {
        let _ = child.start_kill();
        return Err(error);
    }
    Ok(child)
}

#[allow(unsafe_code)]
fn bind_and_resume(child: &Child, job: HANDLE) -> io::Result<()> {
    let pid = child
        .id()
        .ok_or_else(|| io::Error::other("child exited before it was bound"))?;
    let process = child
        .raw_handle()
        .ok_or_else(|| io::Error::other("child exited before it was bound"))?;
    // SAFETY: both handles are open; the child's own handle stays valid while `child` is borrowed.
    unsafe { AssignProcessToJobObject(job, HANDLE(process as isize)) }
        .map_err(|e| windows_error("AssignProcessToJobObject", e))?;
    if resume_threads(pid)? == 0 {
        return Err(io::Error::other("suspended child had no thread to resume"));
    }
    Ok(())
}

/// Resumes every thread of `pid` (a suspended new process has one) and returns how many.
#[allow(unsafe_code)]
fn resume_threads(pid: u32) -> io::Result<u32> {
    // SAFETY: plain snapshot call; the handle is closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
        .map_err(|e| windows_error("CreateToolhelp32Snapshot", e))?;
    let result = resume_from(snapshot, pid);
    // SAFETY: `snapshot` is open and owned here.
    let _ = unsafe { CloseHandle(snapshot) };
    result
}

#[allow(unsafe_code)]
fn resume_from(snapshot: HANDLE, pid: u32) -> io::Result<u32> {
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut resumed = 0;
    // SAFETY: `entry` has `dwSize` set and outlives each call.
    let mut more = unsafe { Thread32First(snapshot, &mut entry) }.is_ok();
    while more {
        if entry.th32OwnerProcessID == pid {
            resume_thread(entry.th32ThreadID)?;
            resumed += 1;
        }
        // SAFETY: as above.
        more = unsafe { Thread32Next(snapshot, &mut entry) }.is_ok();
    }
    Ok(resumed)
}

#[allow(unsafe_code)]
fn resume_thread(thread_id: u32) -> io::Result<()> {
    // SAFETY: the thread handle is opened, used and closed here.
    unsafe {
        let thread = OpenThread(THREAD_SUSPEND_RESUME, false, thread_id)
            .map_err(|e| windows_error("OpenThread", e))?;
        let previous = ResumeThread(thread);
        let _ = CloseHandle(thread);
        if previous == u32::MAX {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
