//! Release guard for the bundled `llama-server` sidecar.
//!
//! `build.rs` includes this file with `#[path]`. Cargo never runs tests that
//! live in a build script, so `tests/sidecar_guard_test.rs` includes it a
//! second time to run the unit tests at the bottom under `cargo test`.
//!
//! For the target being compiled the guard checks that the Tauri config
//! bundles exactly the sidecars the policy requires, that each sidecar file
//! is byte-for-byte the build pinned in `scripts/llama-server.lock`, and that
//! the app's declared macOS minimum matches the sidecar's. It only reports
//! problems; `build.rs` decides whether they fail the build.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Set to `1` to downgrade release-build failures to warnings (local
/// experiments only).
pub const ALLOW_UNPINNED_ENV: &str = "LATTICE_ALLOW_UNPINNED_SIDECAR";
/// The pin file, relative to the crate root (`src-tauri/`).
pub const LOCK_PATH: &str = "scripts/llama-server.lock";

const FETCH_FIX: &str = "fix: bash src-tauri/scripts/fetch-llama-binaries.sh";
const PRIMARY_BIN: &str = "binaries/llama-server";
const CPU_BIN: &str = "binaries/llama-server-cpu";
/// Keys every lock carries exactly once, besides the `sha256` lines.
const LOCK_KEYS: [&str; 5] = ["llama_cpp_tag", "release", "repo", "macos_min", "glibc_max"];

/// The parts of `llama-server.lock` the guard consumes.
#[derive(Debug)]
pub struct SidecarLock {
    pub macos_min: String,
    /// Sidecar file name (e.g. `llama-server-aarch64-apple-darwin`) to its
    /// lowercase hex sha256.
    pub checksums: BTreeMap<String, String>,
}

/// Parses the lock strictly. Blank lines and `#` lines are ignored; every
/// other line is `<key> <value>` for a known key (each exactly once) or
/// `sha256 <64 lowercase hex> <file name>` (each file at most once).
pub fn parse_lock(text: &str) -> Result<SidecarLock, String> {
    let mut values: BTreeMap<&str, &str> = BTreeMap::new();
    let mut checksums = BTreeMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |message: String| format!("line {}: {message}", index + 1);
        let tokens: Vec<&str> = line.split_whitespace().collect();
        match tokens.as_slice() {
            ["sha256", hex, file] => {
                if !is_sha256_hex(hex) {
                    return Err(at(format!("`{hex}` is not a lowercase hex sha256")));
                }
                if file.contains(['/', '\\']) {
                    return Err(at(format!("`{file}` must be a bare file name")));
                }
                if checksums
                    .insert(file.to_string(), hex.to_string())
                    .is_some()
                {
                    return Err(at(format!("duplicate checksum for `{file}`")));
                }
            }
            ["sha256", ..] => return Err(at("expected `sha256 <hex> <file>`".to_string())),
            [key, value] if LOCK_KEYS.contains(key) => {
                if values.insert(*key, *value).is_some() {
                    return Err(at(format!("duplicate key `{key}`")));
                }
            }
            [key, ..] if LOCK_KEYS.contains(key) => {
                return Err(at(format!("expected `{key} <value>`")));
            }
            [key, ..] => return Err(at(format!("unknown key `{key}`"))),
            [] => {}
        }
    }
    if let Some(missing) = LOCK_KEYS.iter().find(|key| !values.contains_key(*key)) {
        return Err(format!("missing required key `{missing}`"));
    }
    let macos_min = values
        .get("macos_min")
        .ok_or("missing required key `macos_min`")?;
    Ok(SidecarLock {
        macos_min: macos_min.to_string(),
        checksums,
    })
}

fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Tauri's platform for a target triple. Mirrors
/// `tauri_utils::platform::Target::from_triple`, which decides the
/// `tauri.<platform>.conf.json` tauri-build and the tauri CLI merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Windows,
    Linux,
    Android,
    Ios,
}

impl Platform {
    pub fn from_triple(triple: &str) -> Self {
        if triple.contains("darwin") {
            Self::MacOS
        } else if triple.contains("windows") {
            Self::Windows
        } else if triple.contains("android") {
            Self::Android
        } else if triple.contains("ios") {
            Self::Ios
        } else {
            Self::Linux
        }
    }

    fn config_file(self) -> &'static str {
        match self {
            Self::MacOS => "tauri.macos.conf.json",
            Self::Windows => "tauri.windows.conf.json",
            Self::Linux => "tauri.linux.conf.json",
            Self::Android => "tauri.android.conf.json",
            Self::Ios => "tauri.ios.conf.json",
        }
    }
}

/// The `bundle.externalBin` entries a build for `platform` must ship: the
/// primary sidecar everywhere, plus the CPU fallback where the primary is
/// the Vulkan build (Windows, Linux).
pub fn required_external_bins(platform: Platform) -> Vec<&'static str> {
    match platform {
        Platform::Windows | Platform::Linux => vec![PRIMARY_BIN, CPU_BIN],
        Platform::MacOS | Platform::Android | Platform::Ios => vec![PRIMARY_BIN],
    }
}

/// The file Tauri bundles for an `externalBin` entry, relative to the crate
/// root. Mirrors `tauri_utils::resources::external_binaries`.
pub fn sidecar_path(external_bin: &str, triple: &str, platform: Platform) -> String {
    let extension = if platform == Platform::Windows {
        ".exe"
    } else {
        ""
    };
    format!("{external_bin}-{triple}{extension}")
}

/// RFC 7396 JSON merge patch, the merge Tauri applies to platform configs and
/// `TAURI_CONFIG`: objects merge recursively, `null` deletes a key, and any
/// other value (arrays included) replaces the old one.
pub fn merge_patch(doc: &mut Value, patch: &Value) {
    let Value::Object(patch_map) = patch else {
        *doc = patch.clone();
        return;
    };
    if !doc.is_object() {
        *doc = Value::Object(serde_json::Map::new());
    }
    if let Value::Object(doc_map) = doc {
        for (key, value) in patch_map {
            if value.is_null() {
                doc_map.remove(key);
            } else {
                merge_patch(doc_map.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
    }
}

/// Reads the Tauri config the way tauri-build does for `platform`:
/// `tauri.conf.json`, the platform file merged over it when present, then the
/// `TAURI_CONFIG` patch the tauri CLI exports for `--config`. Returns the
/// merged config and the files it was read from.
pub fn load_tauri_config(
    crate_dir: &Path,
    platform: Platform,
    config_override: Option<&str>,
) -> Result<(Value, Vec<PathBuf>), String> {
    let read = |path: &Path| -> Result<Value, String> {
        let text = fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
        serde_json::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))
    };
    let base_path = crate_dir.join("tauri.conf.json");
    let mut config = read(&base_path)?;
    let mut paths = vec![base_path];
    let platform_path = crate_dir.join(platform.config_file());
    if platform_path.exists() {
        merge_patch(&mut config, &read(&platform_path)?);
        paths.push(platform_path);
    }
    if let Some(json) = config_override {
        let patch: Value =
            serde_json::from_str(json).map_err(|err| format!("TAURI_CONFIG: {err}"))?;
        merge_patch(&mut config, &patch);
    }
    Ok((config, paths))
}

/// Problems with the merged Tauri config for `triple`.
pub fn check_config(config: &Value, triple: &str, lock: &SidecarLock) -> Vec<String> {
    let platform = Platform::from_triple(triple);
    let mut problems = Vec::new();

    let mut bundled: Vec<String> = config
        .pointer("/bundle/externalBin")
        .and_then(Value::as_array)
        .map(|bins| {
            bins.iter()
                .map(|bin| bin.as_str().map_or_else(|| bin.to_string(), str::to_string))
                .collect()
        })
        .unwrap_or_default();
    bundled.sort();
    let mut required = required_external_bins(platform);
    required.sort_unstable();
    if !bundled
        .iter()
        .map(String::as_str)
        .eq(required.iter().copied())
    {
        problems.push(format!(
            "bundle.externalBin for {triple} is {bundled:?}, expected {required:?} \
             (tauri.conf.json merged with {})",
            platform.config_file()
        ));
    }

    let macos_min = config
        .pointer("/bundle/macOS/minimumSystemVersion")
        .and_then(Value::as_str);
    if macos_min != Some(lock.macos_min.as_str()) {
        problems.push(format!(
            "tauri.conf.json bundle.macOS.minimumSystemVersion is {} but src-tauri/{LOCK_PATH} \
             macos_min is \"{}\"; keep them equal",
            macos_min.map_or_else(|| "unset".to_string(), |v| format!("\"{v}\"")),
            lock.macos_min
        ));
    }
    problems
}

/// Why a sidecar file must not be bundled.
#[derive(Debug, PartialEq, Eq)]
pub enum SidecarProblem {
    Missing,
    Empty,
    Unpinned,
    Mismatch { expected: String, actual: String },
    Unreadable(String),
}

impl SidecarProblem {
    /// One line naming the file, what is wrong, and the fix.
    pub fn describe(&self, display_path: &str, file_name: &str) -> String {
        match self {
            Self::Missing => format!("{display_path} is missing; {FETCH_FIX}"),
            Self::Empty => format!(
                "{display_path} is empty (a placeholder, not a llama-server build); {FETCH_FIX}"
            ),
            Self::Unpinned => format!(
                "{display_path}: lock has no checksum for {file_name}; publish the release and \
                 run fetch-llama-binaries.sh --update-lock"
            ),
            Self::Mismatch { expected, actual } => format!(
                "{display_path} does not match the lock: expected sha256 {expected}, \
                 actual {actual}; {FETCH_FIX}"
            ),
            Self::Unreadable(err) => {
                format!("{display_path} could not be read ({err}); {FETCH_FIX}")
            }
        }
    }
}

/// Checks one sidecar file against the lock; `None` means it may ship.
pub fn check_sidecar(path: &Path, lock: &SidecarLock) -> Option<SidecarProblem> {
    let len = match fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Some(SidecarProblem::Missing),
        Err(err) => return Some(SidecarProblem::Unreadable(err.to_string())),
    };
    if len == 0 {
        return Some(SidecarProblem::Empty);
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let Some(expected) = lock.checksums.get(file_name) else {
        return Some(SidecarProblem::Unpinned);
    };
    match sha256_file(path) {
        Ok(actual) if actual == *expected => None,
        Ok(actual) => Some(SidecarProblem::Mismatch {
            expected: expected.clone(),
            actual,
        }),
        Err(err) => Some(SidecarProblem::Unreadable(err.to_string())),
    }
}

/// Lowercase hex sha256 of a file's contents.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// The guard's verdict for one target.
#[derive(Debug)]
pub struct Evaluation {
    /// Files the verdict depends on (for `cargo:rerun-if-changed`).
    pub watched: Vec<PathBuf>,
    /// One line per problem; empty means the sidecars are safe to ship.
    pub problems: Vec<String>,
}

/// Runs every check for `triple` against the crate at `crate_dir`.
pub fn evaluate(crate_dir: &Path, triple: &str, config_override: Option<&str>) -> Evaluation {
    let platform = Platform::from_triple(triple);
    let lock_path = crate_dir.join(LOCK_PATH);
    let mut watched = vec![lock_path.clone()];
    let mut problems = Vec::new();

    let lock = match fs::read_to_string(&lock_path)
        .map_err(|err| err.to_string())
        .and_then(|text| parse_lock(&text))
    {
        Ok(lock) => Some(lock),
        Err(err) => {
            problems.push(format!("src-tauri/{LOCK_PATH}: {err}"));
            None
        }
    };

    match load_tauri_config(crate_dir, platform, config_override) {
        Ok((config, paths)) => {
            watched.extend(paths);
            if let Some(lock) = &lock {
                problems.extend(check_config(&config, triple, lock));
            }
        }
        Err(err) => problems.push(err),
    }

    for bin in required_external_bins(platform) {
        let relative = sidecar_path(bin, triple, platform);
        let path = crate_dir.join(&relative);
        if let Some(problem) = lock.as_ref().and_then(|lock| check_sidecar(&path, lock)) {
            let file_name = relative.rsplit('/').next().unwrap_or_default();
            problems.push(problem.describe(&format!("src-tauri/{relative}"), file_name));
        }
        watched.push(path);
    }

    Evaluation { watched, problems }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use serde_json::json;

    const MAC: &str = "aarch64-apple-darwin";
    const WINDOWS: &str = "x86_64-pc-windows-msvc";
    const LINUX: &str = "x86_64-unknown-linux-gnu";
    const HELLO_SHA: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn lock_text(extra: &str) -> String {
        format!(
            "# comment\n\nllama_cpp_tag b1\nrelease llama/b1-r1\nrepo o/r\n\
             macos_min 13.3\nglibc_max 2.35\n{extra}"
        )
    }

    #[test]
    fn repo_lock_parses() {
        let lock = parse_lock(include_str!("../scripts/llama-server.lock")).unwrap();
        assert!(!lock.macos_min.is_empty());
    }

    #[test]
    fn repo_config_matches_policy_and_lock_for_every_shipped_target() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let lock = parse_lock(include_str!("../scripts/llama-server.lock")).unwrap();
        for triple in [MAC, WINDOWS, LINUX] {
            let (config, _) =
                load_tauri_config(crate_dir, Platform::from_triple(triple), None).unwrap();
            assert_eq!(
                check_config(&config, triple, &lock),
                Vec::<String>::new(),
                "{triple}"
            );
        }
    }

    #[test]
    fn lock_collects_checksums() {
        let lock = parse_lock(&lock_text(&format!("sha256 {HELLO_SHA} llama-server-x\n"))).unwrap();
        assert_eq!(lock.macos_min, "13.3");
        assert_eq!(lock.checksums.get("llama-server-x").unwrap(), HELLO_SHA);
    }

    #[test]
    fn lock_rejects_malformed_input() {
        let upper = HELLO_SHA.to_uppercase();
        let cases = [
            ("color blue\n".to_string(), "unknown key `color`"),
            ("repo other/repo\n".to_string(), "duplicate key `repo`"),
            ("release a b\n".to_string(), "expected `release <value>`"),
            (
                "sha256 abc f\n".to_string(),
                "is not a lowercase hex sha256",
            ),
            (
                format!("sha256 {upper} f\n"),
                "is not a lowercase hex sha256",
            ),
            (
                format!("sha256 {HELLO_SHA} binaries/f\n"),
                "must be a bare file name",
            ),
            (
                format!("sha256 {HELLO_SHA} f\nsha256 {HELLO_SHA} f\n"),
                "duplicate checksum",
            ),
            (
                format!("sha256 {HELLO_SHA}\n"),
                "expected `sha256 <hex> <file>`",
            ),
        ];
        for (extra, expected) in cases {
            let err = parse_lock(&lock_text(&extra)).unwrap_err();
            assert!(err.contains(expected), "{extra:?}: {err}");
        }
        let err = parse_lock("llama_cpp_tag b1\n").unwrap_err();
        assert!(err.contains("missing required key `release`"), "{err}");
    }

    #[test]
    fn platform_policy_and_file_names_follow_tauri() {
        assert_eq!(Platform::from_triple(MAC), Platform::MacOS);
        assert_eq!(Platform::from_triple(WINDOWS), Platform::Windows);
        assert_eq!(Platform::from_triple(LINUX), Platform::Linux);
        assert_eq!(required_external_bins(Platform::MacOS), [PRIMARY_BIN]);
        assert_eq!(
            required_external_bins(Platform::Windows),
            [PRIMARY_BIN, CPU_BIN]
        );
        assert_eq!(
            required_external_bins(Platform::Linux),
            [PRIMARY_BIN, CPU_BIN]
        );
        assert_eq!(
            sidecar_path(CPU_BIN, WINDOWS, Platform::Windows),
            "binaries/llama-server-cpu-x86_64-pc-windows-msvc.exe"
        );
        assert_eq!(
            sidecar_path(PRIMARY_BIN, LINUX, Platform::Linux),
            "binaries/llama-server-x86_64-unknown-linux-gnu"
        );
    }

    #[test]
    fn merge_patch_replaces_arrays_and_deletes_nulls() {
        let mut doc = json!({"bundle": {"externalBin": ["a"], "keep": 1, "drop": 2}});
        merge_patch(
            &mut doc,
            &json!({"bundle": {"externalBin": ["b", "c"], "drop": null}}),
        );
        assert_eq!(
            doc,
            json!({"bundle": {"externalBin": ["b", "c"], "keep": 1}})
        );
    }

    #[test]
    fn config_drift_is_reported() {
        let lock = parse_lock(&lock_text("")).unwrap();
        let config = json!({"bundle": {
            "externalBin": [PRIMARY_BIN],
            "macOS": {"minimumSystemVersion": "10.15"}
        }});
        let problems = check_config(&config, LINUX, &lock);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0]
            .contains("expected [\"binaries/llama-server\", \"binaries/llama-server-cpu\"]"));
        assert!(problems[1].contains("is \"10.15\" but"));
        assert!(problems[1].contains("macos_min is \"13.3\""));
        assert_eq!(check_config(&config, MAC, &lock).len(), 1);
    }

    #[test]
    fn sidecar_files_are_classified() {
        let dir = tempfile::tempdir().unwrap();
        let lock = parse_lock(&lock_text(&format!(
            "sha256 {HELLO_SHA} good\nsha256 {HELLO_SHA} tampered\nsha256 {HELLO_SHA} empty\n"
        )))
        .unwrap();
        fs::write(dir.path().join("good"), "hello").unwrap();
        fs::write(dir.path().join("tampered"), "hellO").unwrap();
        fs::write(dir.path().join("empty"), "").unwrap();
        fs::write(dir.path().join("unpinned"), "hello").unwrap();

        assert_eq!(check_sidecar(&dir.path().join("good"), &lock), None);
        assert_eq!(
            check_sidecar(&dir.path().join("empty"), &lock),
            Some(SidecarProblem::Empty)
        );
        assert_eq!(
            check_sidecar(&dir.path().join("absent"), &lock),
            Some(SidecarProblem::Missing)
        );
        assert_eq!(
            check_sidecar(&dir.path().join("unpinned"), &lock),
            Some(SidecarProblem::Unpinned)
        );
        let Some(SidecarProblem::Mismatch { expected, actual }) =
            check_sidecar(&dir.path().join("tampered"), &lock)
        else {
            panic!("tampered file must mismatch");
        };
        assert_eq!(expected, HELLO_SHA);
        assert_eq!(actual, sha256_file(&dir.path().join("tampered")).unwrap());
    }

    #[test]
    fn evaluate_checks_the_merged_config_and_every_required_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::create_dir_all(root.join("binaries")).unwrap();
        let primary = format!("llama-server-{WINDOWS}.exe");
        let cpu = format!("llama-server-cpu-{WINDOWS}.exe");
        fs::write(
            root.join(LOCK_PATH),
            lock_text(&format!(
                "sha256 {HELLO_SHA} {primary}\nsha256 {HELLO_SHA} {cpu}\n"
            )),
        )
        .unwrap();
        let base = json!({"bundle": {
            "externalBin": [PRIMARY_BIN],
            "macOS": {"minimumSystemVersion": "13.3"}
        }});
        fs::write(root.join("tauri.conf.json"), base.to_string()).unwrap();
        fs::write(
            root.join("tauri.windows.conf.json"),
            json!({"bundle": {"externalBin": [PRIMARY_BIN, CPU_BIN]}}).to_string(),
        )
        .unwrap();
        fs::write(root.join("binaries").join(&primary), "hello").unwrap();
        fs::write(root.join("binaries").join(&cpu), "hello").unwrap();

        let clean = evaluate(root, WINDOWS, None);
        assert_eq!(clean.problems, Vec::<String>::new());
        assert!(clean
            .watched
            .contains(&root.join("tauri.windows.conf.json")));
        assert!(clean.watched.contains(&root.join("binaries").join(&cpu)));

        // `tauri build --config` can drop the CPU sidecar; the guard must see it.
        let dropped = json!({"bundle": {"externalBin": [PRIMARY_BIN]}}).to_string();
        assert_eq!(evaluate(root, WINDOWS, Some(&dropped)).problems.len(), 1);

        fs::write(root.join("binaries").join(&cpu), "").unwrap();
        let problems = evaluate(root, WINDOWS, None).problems;
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with(&format!("src-tauri/binaries/{cpu} is empty")));

        fs::write(root.join(LOCK_PATH), "bogus line\n").unwrap();
        let problems = evaluate(root, WINDOWS, None).problems;
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("unknown key `bogus`"), "{problems:?}");
    }
}
