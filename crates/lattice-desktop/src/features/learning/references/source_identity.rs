pub(crate) fn source_request_hash<T: serde::Serialize>(
    request: &T,
) -> lattice_core::shared::error::Result<String> {
    use sha2::Digest;
    let bytes = serde_json::to_vec(request)
        .map_err(|error| lattice_core::shared::error::AppError::Serialization(error.to_string()))?;
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}

pub(crate) fn validate_source_ids(
    program_id: &str,
    source_id: &str,
    version_id: &str,
    operation_id: &str,
) -> lattice_core::shared::error::Result<()> {
    for (value, label) in [
        (program_id, "program"),
        (source_id, "source"),
        (version_id, "source version"),
        (operation_id, "operation"),
    ] {
        uuid::Uuid::parse_str(value).map_err(|_| {
            lattice_core::shared::error::AppError::InvalidInput(format!("Invalid {label} ID"))
        })?;
    }
    Ok(())
}
