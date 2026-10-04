use super::*;

/// Process-wide registry of live sidecars.
///
/// Held as Tauri-managed state via `app.manage()` so the app's
/// `RunEvent::ExitRequested` handler can synchronously drain it before
/// the tokio runtime tears down. Each entry holds a strong reference to a
/// child and an id its owner releases on drop, so a child whose only other
/// owner is a cancelled startup future is still reachable here.
///
/// # Four layers of zombie protection
///
/// 1. **Normal Drop.** Handle goes out of scope → `Drop` calls
///    `kill()`. Covers happy path (model swap, container teardown
///    while runtime is alive).
///
/// 2. **`RunEvent::ExitRequested` sweep.** Tauri's "user closed the
///    window" hook walks the registry and synchronously kills
///    everything before returning to shutdown. Covers normal app
///    quit where the tokio runtime is mid-teardown and Drop racing
///    against runtime shutdown is unreliable.
///
/// 3. **Windows Job Object.** On Windows, every sidecar is assigned
///    to a kernel Job Object configured with `KILL_ON_JOB_CLOSE`.
///    The OS guarantees they die when our process handle closes —
///    even if Lattice segfaults, gets `taskkill /f`'d, or otherwise
///    bypasses our shutdown hooks. This is the load-bearing layer:
///    layers 1+2 are best-effort, 3 is a hardware-level guarantee.
///    See `JobObjectGuard` below.
///
/// 4. **Boot-time orphan reap.** `reap_orphan_sidecars()` at app
///    startup catches anything that survived the previous run.
///    This is the load-bearing layer **on macOS and Linux** where
///    no Job Object equivalent exists: `kill -9` on Lattice itself
///    will leak the sidecar until Lattice is next started, at which
///    point the reaper finds and kills it. A future improvement on
///    Linux is to use `prctl(PR_SET_PDEATHSIG, SIGKILL)` as a
///    pre-exec hook on the spawn — but `tauri-plugin-shell` doesn't
///    expose a pre-exec hook, so doing that requires dropping to
///    raw `std::process::Command::pre_exec` and giving up the
///    plugin's bundled-binary resolution. Tracked as a follow-up;
///    the boot-time reap is good enough for v1.
///
/// # Why `parking_lot::Mutex` (sync) not `tokio::Mutex` (async)
///
/// The shutdown-time consumer is a synchronous Tauri callback. Using
/// a tokio::Mutex would force `Handle::current().block_on()`, which
/// can deadlock during runtime teardown. `CommandChild::kill()` itself
/// is synchronous (it just calls into `shared_child::SharedChild`),
/// so the entire kill path can stay sync.
pub struct SidecarRegistry {
    pub(super) entries: SyncMutex<RegistryState>,

    /// Live servers keyed by the configuration that started them, so the
    /// chat, router and utility roles pointing at one GGUF share a process
    /// instead of loading the same weights once each. Lives here because
    /// this type is already the process-wide truth about sidecars, and
    /// sharing is a fact about the set as a whole, not about any one child.
    ///
    /// Orthogonal to `entries`: that is the kill list, this is the sharing
    /// map. A shared server holds exactly one `entries` slot however many
    /// roles are using it.
    pub(super) shared: SharedProcesses<SidecarConfig, SidecarHandle>,

    /// Windows Job Object (no-op on other platforms). Sidecar PIDs
    /// get assigned to this object on spawn so Windows kernel kills
    /// them automatically when our process handle closes — even on
    /// segfault.
    #[cfg(windows)]
    pub(super) job: SyncMutex<Option<JobObjectGuard>>,
}

pub(super) struct RegistryState {
    pub(super) entries: Vec<RegistryEntry>,
    pub(super) closed: bool,
}

pub(super) struct RegistryEntry {
    /// Identifies the slot so its owner can release it. A `Weak` used to
    /// stand in for ownership, but a `Weak` cannot keep alive the one case
    /// registering early exists for: a child whose only strong reference is
    /// a cancelled startup future.
    pub(super) id: u64,
    pub(super) child: Arc<SyncMutex<Option<CommandChild>>>,
    pub(super) endpoint: String,
}

impl SidecarRegistry {
    pub fn new() -> Self {
        #[cfg(windows)]
        let job = match JobObjectGuard::create() {
            Ok(guard) => {
                tracing::debug!("Created Windows Job Object for sidecar lifecycle");
                Some(guard)
            }
            Err(err) => {
                // Non-fatal: layers 1+2+4 still apply. Job Object is
                // belt-and-suspenders. Most likely cause of failure
                // is running under another Job Object that doesn't
                // allow nesting (Tauri test harnesses, AppContainer
                // sandboxes), in which case we just warn and move on.
                tracing::warn!(
                    "Failed to create Windows Job Object; sidecar processes may \
                     leak on hard kill: {err}"
                );
                None
            }
        };

        Self {
            entries: SyncMutex::new(RegistryState {
                entries: Vec::new(),
                closed: false,
            }),
            shared: SharedProcesses::new(),
            #[cfg(windows)]
            job: SyncMutex::new(job),
        }
    }

    /// Register a live sidecar. Called by `SidecarManager::start_attempt`
    /// after readiness confirmation. Cleans up dead weak refs from
    /// previous spawns at the same time.
    ///
    /// On Windows, also assigns the sidecar PID to the registry's
    /// Job Object so the kernel kills it automatically when our
    /// process handle closes. The PID is read from the
    /// `CommandChild` while the registry holds it locked, so we know
    /// it hasn't been killed/swapped underneath us.
    pub(super) fn register(
        &self,
        child: &Arc<SyncMutex<Option<CommandChild>>>,
        endpoint: &str,
    ) -> Option<u64> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

        let admitted = self.with_open_entries(|entries| {
            // Admission and insertion share the state lock with `kill_all`.
            #[cfg(windows)]
            {
                let pid_opt = child.lock().as_ref().map(|c| c.pid());
                if let Some(pid) = pid_opt {
                    if let Some(job) = self.job.lock().as_ref() {
                        if let Err(err) = job.assign_pid(pid) {
                            tracing::warn!(
                                pid,
                                endpoint,
                                "Failed to assign sidecar to Job Object: {err}"
                            );
                        } else {
                            tracing::debug!(
                                pid,
                                endpoint,
                                "Assigned sidecar to Windows Job Object"
                            );
                        }
                    }
                }
            }

            entries.retain(|entry| entry.child.lock().is_some());
            entries.push(RegistryEntry {
                id,
                child: Arc::clone(child),
                endpoint: endpoint.to_string(),
            });
            tracing::debug!(
                registry_size = entries.len(),
                endpoint = endpoint,
                "Registered sidecar in process registry"
            );
            id
        });
        let Some(id) = admitted else {
            if let Some(child) = child.lock().take() {
                let _ = child.kill();
            }
            return None;
        };
        Some(id)
    }

    pub(super) fn with_open_entries<T>(
        &self,
        register: impl FnOnce(&mut Vec<RegistryEntry>) -> T,
    ) -> Option<T> {
        let mut state = self.entries.lock();
        if state.closed {
            return None;
        }
        Some(register(&mut state.entries))
    }

    /// Release the slot `id` owns. Called by whoever owns the child — the
    /// handle, or the spawn guard on a cancelled startup.
    pub(super) fn unregister(&self, id: u64) {
        self.entries.lock().entries.retain(|e| e.id != id);
    }

    /// Synchronously kill every registered sidecar. Idempotent — call
    /// from the Tauri RunEvent::ExitRequested handler. Returns the
    /// number of children killed (for logging).
    ///
    /// Lock discipline: we drain `entries` into a local Vec first, then
    /// release the registry mutex before walking children. `child.kill()`
    /// is a blocking syscall (TerminateProcess on Windows, SIGKILL on
    /// Unix) — holding the registry lock across it would block any
    /// concurrent `register()` calls (e.g. a spawn-in-flight on another
    /// thread) for the full kill duration.
    pub fn kill_all(&self) -> usize {
        // Drain into a local Vec, then release entries lock immediately.
        let drained: Vec<RegistryEntry> = {
            let mut state = self.entries.lock();
            state.closed = true;
            state.entries.drain(..).collect()
        };

        let mut killed = 0;
        for entry in drained {
            // Lock the per-child mutex briefly: take the CommandChild
            // out, then release before calling kill so a concurrent
            // SidecarHandle::stop sees an empty slot and no-ops.
            let child_opt = entry.child.lock().take();
            if let Some(child) = child_opt {
                match child.kill() {
                    Ok(()) => {
                        killed += 1;
                        tracing::info!(
                            endpoint = %entry.endpoint,
                            "Killed sidecar via registry shutdown"
                        );
                    }
                    Err(err) => {
                        tracing::warn!(
                            endpoint = %entry.endpoint,
                            error = %err,
                            "Failed to kill sidecar at shutdown"
                        );
                    }
                }
            }
        }
        killed
    }

    /// GPU memory held by running, GPU-offloaded servers other than those on
    /// `model_path`. A role on the same GGUF reuses that server rather than
    /// adding one, so counting it would shrink the window this model is sized
    /// to and give it a different configuration key — a second process.
    pub fn resident_gpu_bytes(&self, model_path: &std::path::Path) -> u64 {
        self.shared
            .running()
            .into_iter()
            .filter(|(config, handle)| config.model_path != model_path && handle.n_gpu_layers() > 0)
            .map(|(config, handle)| resident_bytes_of(&config.model_path, handle.context_size()))
            .fold(0, u64::saturating_add)
    }

    /// Number of currently-tracked live entries. For diagnostics.
    pub fn live_count(&self) -> usize {
        let entries = self.entries.lock();
        entries
            .entries
            .iter()
            .filter(|e| e.child.lock().is_some())
            .count()
    }
}

impl Default for SidecarRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard around a Windows Job Object configured to kill all
/// member processes when the job handle closes.
///
/// The Job Object uses `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`,
/// which means: when the OS handle to this job closes (because our
/// process exits — including segfault, taskkill /f, or panic),
/// every process assigned to it is terminated by the kernel.
///
/// Drop closes the handle. We rely on Drop running when the
/// `SidecarRegistry` is dropped (which happens at app shutdown OR
/// process abort, since Tauri-managed state lifetime ends with the
/// process). For abnormal exits where Drop doesn't run, the OS
/// closes orphaned handles automatically — same effect.
#[cfg(windows)]
pub(super) struct JobObjectGuard {
    pub(super) handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl JobObjectGuard {
    pub(super) fn create() -> Result<Self, String> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_BASIC_LIMIT_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        // SAFETY: CreateJobObjectW with null `name` and null security
        // attributes returns a handle owned by the calling process.
        // Per Microsoft docs, the only failure modes are out-of-resources
        // and access denied, both of which surface as INVALID_HANDLE
        // and we can convert to a Rust error. No memory borrowed across
        // the FFI boundary.
        #[allow(unsafe_code)]
        let handle: HANDLE = unsafe { CreateJobObjectW(None, windows::core::PCWSTR::null()) }
            .map_err(|e| format!("CreateJobObjectW failed: {e}"))?;

        if handle.is_invalid() {
            return Err("CreateJobObjectW returned invalid handle".to_string());
        }

        // Configure: kill all child processes when the job handle
        // closes. This is the kill_on_close behavior; without this
        // flag, dead Lattice would leave llama-server running.
        let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..Default::default()
            },
            ..Default::default()
        };

        // SAFETY: SetInformationJobObject reads the `info` struct
        // through the void pointer for the size we specify
        // (`size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()`).
        // `info` is a stack-local of exactly that type, owned by us
        // for the duration of the call. The handle is the one we
        // just created and haven't closed.
        #[allow(unsafe_code)]
        let result = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            )
        };

        if let Err(err) = result {
            // Best-effort: try to close the handle we just created.
            #[allow(unsafe_code)]
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(handle);
            }
            return Err(format!("SetInformationJobObject failed: {err}"));
        }

        Ok(Self { handle })
    }

    pub(super) fn assign_pid(&self, pid: u32) -> Result<(), String> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::JobObjects::AssignProcessToJobObject;
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };

        // Open a handle to the child process with the rights needed
        // for AssignProcessToJobObject (set_quota + terminate).
        // SAFETY: OpenProcess returns a HANDLE we own and must close.
        // PID was just read from a CommandChild we control — the
        // process is alive and we have permission to inspect it
        // (we spawned it).
        #[allow(unsafe_code)]
        let child_handle =
            unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid) }
                .map_err(|e| format!("OpenProcess(pid={pid}) failed: {e}"))?;

        // SAFETY: Both handles are valid — we created `self.handle`
        // in `create()` and just opened `child_handle`.
        #[allow(unsafe_code)]
        let assign_result = unsafe { AssignProcessToJobObject(self.handle, child_handle) };

        // Always close the child handle once assignment completes;
        // the process stays alive (Windows holds its own ref), the
        // job-object membership persists.
        #[allow(unsafe_code)]
        unsafe {
            let _ = CloseHandle(child_handle);
        }

        assign_result.map_err(|e| format!("AssignProcessToJobObject(pid={pid}) failed: {e}"))?;
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for JobObjectGuard {
    fn drop(&mut self) {
        // Closing the handle triggers KILL_ON_JOB_CLOSE. Every
        // process assigned to this job dies. This is the
        // shutdown-time guarantee.
        // SAFETY: Handle was returned from CreateJobObjectW and
        // never closed by anyone else (we own it exclusively).
        #[allow(unsafe_code)]
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

// `windows::Win32::Foundation::HANDLE` already implements `Send` +
// `Sync` (kernel handles are thread-safe by definition; HANDLE is
// just an index into a kernel table). The compiler auto-derives both
// for `JobObjectGuard` since its only field is HANDLE — no manual
// `unsafe impl` needed.

/// Kill orphan `llama-server` processes left by a crashed Lattice instance.
///
/// Called once from `main.rs` at app boot, after the registry is
/// managed but before the DI container spawns any new sidecars.
///
/// # Match strategy
///
/// A process is ours only if its executable is an installed sidecar file
/// (`llama-server[.exe]` or `llama-server-cpu[.exe]`) sitting in the same
/// directory as the running Lattice executable — the only place
/// `tauri-plugin-shell` launches sidecars from (`is_installed_sidecar`).
/// A user's own `~/code/llama.cpp/build/bin/llama-server`, or another
/// app's bundled copy, lives elsewhere and is never touched.
///
/// The executable path comes from the OS (`Process::exe`), falling back
/// to argv[0], which tauri-plugin-shell sets to the full resolved path.
/// An install whose directory changes between launches (a re-mounted
/// AppImage, a translocated macOS app) leaves its orphans alone; missing
/// one is better than killing someone else's server.
///
/// A matching process is left alone while its parent is a running Lattice
/// from the same directory (`has_live_lattice_parent`): launching the app a
/// second time used to SIGKILL the first window's model mid-turn.
///
/// Best-effort: `sysinfo` failures collapse to "no orphans found" and
/// the scan continues. We never panic from here.
pub fn reap_orphan_sidecars() {
    let Some(lattice_dir) = tauri::utils::platform::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(canonical_or_raw))
    else {
        tracing::debug!("Skipping orphan sidecar scan: Lattice executable directory unknown");
        return;
    };

    // `refresh_processes()` never loads argv; ask for it (and the exe path)
    // explicitly.
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessRefreshKind::new()
            .with_exe(sysinfo::UpdateKind::OnlyIfNotSet)
            .with_cmd(sysinfo::UpdateKind::OnlyIfNotSet),
    );

    let mut reaped = 0usize;
    for (pid, process) in system.processes() {
        let name = process.name();
        // Cheap prefilter before touching the filesystem. Linux reports the
        // 15-byte `comm`, so `llama-server-cpu` shows up as `llama-server-cp`.
        if !name.to_ascii_lowercase().starts_with(SIDECAR_BIN) {
            continue;
        }

        let exe = process
            .exe()
            .map(std::path::Path::to_path_buf)
            .or_else(|| process.cmd().first().map(PathBuf::from));
        let is_ours = exe
            .as_deref()
            .is_some_and(|exe| is_installed_sidecar(&canonical_or_raw(exe), &lattice_dir));
        if !is_ours {
            tracing::debug!(
                pid = pid.as_u32(),
                process_name = name,
                exe = ?exe,
                "Skipping `llama-server` process — not a sidecar of this Lattice install"
            );
            continue;
        }

        // A second launch of Lattice runs this scan too; a sidecar whose
        // parent is a running Lattice belongs to that window, mid-turn.
        let parent_exe = process
            .parent()
            .and_then(|ppid| system.process(ppid))
            .and_then(|parent| parent.exe());
        if has_live_lattice_parent(parent_exe, &lattice_dir) {
            tracing::info!(
                pid = pid.as_u32(),
                process_name = name,
                "Leaving llama-server alone — its Lattice is still running"
            );
            continue;
        }

        let killed = process.kill();
        if killed {
            reaped += 1;
            tracing::warn!(
                pid = pid.as_u32(),
                process_name = name,
                "Reaped orphan llama-server process from previous Lattice run"
            );
        } else {
            // sysinfo returns false for both "permission denied" and
            // "process already terminated." The latter is success for
            // our purposes (the orphan is gone), the former leaves it
            // for the next-boot pass.
            tracing::debug!(
                pid = pid.as_u32(),
                process_name = name,
                "Found orphan llama-server but kill returned false (already dead or perms)"
            );
        }
    }

    if reaped > 0 {
        tracing::info!(
            count = reaped,
            "Reaped {} orphan llama-server process(es) on startup",
            reaped
        );
    }
}

/// Whether `exe` is one of the installed sidecar files in `lattice_dir`.
/// Both paths must already be in the same (canonical) form.
pub(super) fn is_installed_sidecar(exe: &std::path::Path, lattice_dir: &std::path::Path) -> bool {
    let is_sidecar_file = exe
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            let stem = name.strip_suffix(".exe").unwrap_or(&name);
            stem == SIDECAR_BIN || stem == SIDECAR_CPU_BIN
        });
    is_sidecar_file && exe.parent() == Some(lattice_dir)
}

/// Whether a sidecar's parent is a live Lattice from this install: the parent
/// process still exists and its executable sits in `lattice_dir`. An orphan's
/// parent is gone (Windows), or it has been re-parented to init/launchd, or its
/// PID has been reused by something else — none of which live there.
pub(super) fn has_live_lattice_parent(
    parent_exe: Option<&std::path::Path>,
    lattice_dir: &std::path::Path,
) -> bool {
    parent_exe.is_some_and(|exe| canonical_or_raw(exe).parent() == Some(lattice_dir))
}

/// Canonical form for path comparison (symlinks resolved; `\\?\` form on
/// Windows), or the path as given when it can't be resolved.
pub(super) fn canonical_or_raw(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
