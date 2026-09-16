use std::path::PathBuf;
use tauri::Manager;

pub fn setup_app_directories(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_dir = app.path().app_data_dir()
        .map_err(|e| format!("Could not determine app data directory. This may indicate a system configuration issue.\n\nError: {}", e))?;

    std::fs::create_dir_all(&app_dir).map_err(|e| {
        format!(
            "Failed to create app data directory at {:?}.\n\n\
                              Possible causes:\n\
                              - Missing write permissions\n\
                              - Insufficient disk space\n\
                              - Antivirus blocking file operations\n\n\
                              Suggested actions:\n\
                              - Run as administrator\n\
                              - Free up disk space\n\
                              - Check antivirus settings\n\n\
                              Error: {}",
            app_dir, e
        )
    })?;

    Ok(app_dir)
}

pub fn setup_model_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let model_dir =
        crate::get_model_dir(app).map_err(|e| format!("Failed to get model directory: {}", e))?;
    std::fs::create_dir_all(&model_dir).map_err(|e| {
        format!(
            "Failed to create models directory at {:?}.\n\n\
                              Possible causes:\n\
                              - Missing write permissions\n\
                              - Insufficient disk space (need ~500MB for AI models)\n\
                              - Antivirus blocking file operations\n\n\
                              Suggested actions:\n\
                              - Run as administrator\n\
                              - Free up at least 500MB disk space\n\
                              - Check antivirus settings\n\n\
                              Error: {}",
            model_dir, e
        )
    })?;

    Ok(model_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directory_error_messages() {
        let err = format!(
            "Failed to create app data directory at {:?}.\n\n\
                          Possible causes:\n\
                          - Missing write permissions\n\
                          - Insufficient disk space\n\
                          - Antivirus blocking file operations\n\n\
                          Suggested actions:\n\
                          - Run as administrator\n\
                          - Free up disk space\n\
                          - Check antivirus settings\n\n\
                          Error: test error",
            PathBuf::from("/test/path")
        );

        assert!(err.contains("Missing write permissions"));
        assert!(err.contains("Insufficient disk space"));
        assert!(err.contains("/test/path"));
    }
}
