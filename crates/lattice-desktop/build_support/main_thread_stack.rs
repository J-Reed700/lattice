/// The outline command's dispatch overflowed a 2 MiB main thread and fit 4 MiB; 32 MiB is what the runtime's threads get.
pub const MAIN_THREAD_STACK_BYTES: u64 = 32 * 1024 * 1024;

/// Link arguments that reserve `MAIN_THREAD_STACK_BYTES` for the main thread.
pub fn link_args(target_os: &str, target_env: &str) -> Vec<String> {
    match (target_os, target_env) {
        ("windows", "msvc") => vec![format!("/STACK:{MAIN_THREAD_STACK_BYTES}")],
        ("macos", _) => vec![format!("-Wl,-stack_size,{MAIN_THREAD_STACK_BYTES:#x}")],
        _ => Vec::new(),
    }
}

/// The build script's lines that reserve `MAIN_THREAD_STACK_BYTES`.
pub fn directives(target_os: &str, target_env: &str) -> Vec<String> {
    link_args(target_os, target_env)
        .into_iter()
        .flat_map(|arg| {
            [
                format!("cargo::rustc-link-arg-bins={arg}"),
                format!("cargo::rustc-link-arg-examples={arg}"),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_msvc_reserves_the_stack_with_the_linker_flag() {
        assert_eq!(
            link_args("windows", "msvc"),
            vec![format!("/STACK:{MAIN_THREAD_STACK_BYTES}")]
        );
    }

    #[test]
    fn macos_reserves_the_stack_through_the_cc_driver() {
        assert_eq!(
            link_args("macos", ""),
            vec![format!("-Wl,-stack_size,{MAIN_THREAD_STACK_BYTES:#x}")]
        );
    }

    #[test]
    fn other_targets_get_no_link_arguments() {
        assert!(link_args("windows", "gnu").is_empty());
        assert!(link_args("linux", "gnu").is_empty());
    }

    #[test]
    fn the_examples_get_the_reserve_the_app_gets() {
        let arg = format!("/STACK:{MAIN_THREAD_STACK_BYTES}");
        assert_eq!(
            directives("windows", "msvc"),
            vec![
                format!("cargo::rustc-link-arg-bins={arg}"),
                format!("cargo::rustc-link-arg-examples={arg}"),
            ]
        );
        let arg = format!("-Wl,-stack_size,{MAIN_THREAD_STACK_BYTES:#x}");
        assert_eq!(
            directives("macos", ""),
            vec![
                format!("cargo::rustc-link-arg-bins={arg}"),
                format!("cargo::rustc-link-arg-examples={arg}"),
            ]
        );
        assert!(directives("linux", "gnu").is_empty());
    }
}
