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
}
