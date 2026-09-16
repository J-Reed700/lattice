use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::infrastructure::security::SecureStorage;
use crate::interfaces::di::Container;
use crate::shared::error::AppError;

/// Implementation of set_api_key that takes &Container
pub async fn set_api_key_impl(
    container: &Container,
    service: String,
    key: String,
) -> Result<(), AppError> {
    // Rate limiting (CWE-307 mitigation)
    container
        .security_context()
        .rate_limiters()
        .credentials
        .check_rate_limit("credentials")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let audit_logger = get_audit_logger();
    let resource_id = format!("api_key:{}", service);

    let result = match service.as_str() {
        "ollama" => SecureStorage::set_ollama_key(&key)
            .map_err(|e| AppError::Other(format!("Failed to store Ollama key: {}", e))),
        _ => Err(AppError::Other(format!("Unknown service: {}", service))),
    };

    // Audit the outcome
    match &result {
        Ok(_) => {
            let event = AuditEvent::new(AuditAction::CredentialStored, AuditResult::success())
                .with_resource_id(&resource_id)
                .with_metadata("service", &service)
                .with_metadata("operation", "set_api_key");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::CredentialStored,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&resource_id)
            .with_metadata("service", &service)
            .with_metadata("operation", "set_api_key");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Implementation of get_api_key that takes &Container
pub async fn get_api_key_impl(
    container: &Container,
    service: String,
) -> Result<Option<String>, AppError> {
    // Rate limiting (CWE-307 mitigation)
    container
        .security_context()
        .rate_limiters()
        .credentials
        .check_rate_limit("credentials")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let audit_logger = get_audit_logger();
    let resource_id = format!("api_key:{}", service);

    let result = match service.as_str() {
        "ollama" => SecureStorage::get_ollama_key()
            .map_err(|e| AppError::Other(format!("Failed to retrieve Ollama key: {}", e))),
        _ => Err(AppError::Other(format!("Unknown service: {}", service))),
    };

    // Audit the outcome
    match &result {
        Ok(key_opt) => {
            let event = AuditEvent::new(AuditAction::CredentialAccessed, AuditResult::success())
                .with_resource_id(&resource_id)
                .with_metadata("service", &service)
                .with_metadata("operation", "get_api_key")
                .with_metadata("key_exists", key_opt.is_some().to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::CredentialAccessed,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&resource_id)
            .with_metadata("service", &service)
            .with_metadata("operation", "get_api_key");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Implementation of delete_api_key that takes &Container
pub async fn delete_api_key_impl(container: &Container, service: String) -> Result<(), AppError> {
    // Rate limiting (CWE-307 mitigation)
    container
        .security_context()
        .rate_limiters()
        .credentials
        .check_rate_limit("credentials")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let audit_logger = get_audit_logger();
    let resource_id = format!("api_key:{}", service);

    let result = match service.as_str() {
        "ollama" => SecureStorage::delete_ollama_key()
            .map_err(|e| AppError::Other(format!("Failed to delete Ollama key: {}", e))),
        _ => Err(AppError::Other(format!("Unknown service: {}", service))),
    };

    // Audit the outcome
    match &result {
        Ok(_) => {
            let event = AuditEvent::new(AuditAction::CredentialDeleted, AuditResult::success())
                .with_resource_id(&resource_id)
                .with_metadata("service", &service)
                .with_metadata("operation", "delete_api_key");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::CredentialDeleted,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&resource_id)
            .with_metadata("service", &service)
            .with_metadata("operation", "delete_api_key");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Implementation of has_api_key that takes &Container
pub async fn has_api_key_impl(container: &Container, service: String) -> Result<bool, AppError> {
    // Rate limiting (CWE-307 mitigation)
    container
        .security_context()
        .rate_limiters()
        .credentials
        .check_rate_limit("credentials")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let audit_logger = get_audit_logger();
    let resource_id = format!("api_key:{}", service);

    let result = match service.as_str() {
        "ollama" => SecureStorage::get_ollama_key()
            .map_err(|e| AppError::Other(format!("Failed to check Ollama key: {}", e)))
            .map(|key| key.is_some()),
        _ => Err(AppError::Other(format!("Unknown service: {}", service))),
    };

    // Audit the outcome
    match &result {
        Ok(exists) => {
            let event = AuditEvent::new(AuditAction::CredentialAccessed, AuditResult::success())
                .with_resource_id(&resource_id)
                .with_metadata("service", &service)
                .with_metadata("operation", "has_api_key")
                .with_metadata("exists", exists.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::CredentialAccessed,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&resource_id)
            .with_metadata("service", &service)
            .with_metadata("operation", "has_api_key");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Implementation of clear_all_credentials that takes &Container
pub async fn clear_all_credentials_impl(container: &Container) -> Result<(), AppError> {
    // Rate limiting (CWE-307 mitigation)
    container
        .security_context()
        .rate_limiters()
        .credentials
        .check_rate_limit("credentials")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let audit_logger = get_audit_logger();
    let resource_id = "all_credentials";

    let result = SecureStorage::clear_all()
        .map_err(|e| AppError::Other(format!("Failed to clear credentials: {}", e)));

    // Audit the outcome
    match &result {
        Ok(_) => {
            let event = AuditEvent::new(AuditAction::CredentialDeleted, AuditResult::success())
                .with_resource_id(resource_id)
                .with_metadata("operation", "clear_all")
                .with_metadata("scope", "all_services");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::CredentialDeleted,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(resource_id)
            .with_metadata("operation", "clear_all")
            .with_metadata("scope", "all_services");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Implementation of set_custom_endpoint that takes &Container
pub async fn set_custom_endpoint_impl(
    container: &Container,
    endpoint: String,
) -> Result<(), AppError> {
    // Rate limiting (CWE-307 mitigation)
    container
        .security_context()
        .rate_limiters()
        .credentials
        .check_rate_limit("credentials")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let audit_logger = get_audit_logger();
    let resource_id = "custom_endpoint";

    let result = SecureStorage::set_custom_endpoint(&endpoint)
        .map_err(|e| AppError::Other(format!("Failed to store custom endpoint: {}", e)));

    // Audit the outcome (endpoints are sensitive config, treat as credentials)
    match &result {
        Ok(_) => {
            let event = AuditEvent::new(AuditAction::CredentialStored, AuditResult::success())
                .with_resource_id(resource_id)
                .with_metadata("operation", "set_custom_endpoint")
                .with_metadata("type", "endpoint");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::CredentialStored,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(resource_id)
            .with_metadata("operation", "set_custom_endpoint")
            .with_metadata("type", "endpoint");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Implementation of get_custom_endpoint that takes &Container
pub async fn get_custom_endpoint_impl(container: &Container) -> Result<Option<String>, AppError> {
    // Rate limiting (CWE-307 mitigation)
    container
        .security_context()
        .rate_limiters()
        .credentials
        .check_rate_limit("credentials")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let audit_logger = get_audit_logger();
    let resource_id = "custom_endpoint";

    let result = SecureStorage::get_custom_endpoint()
        .map_err(|e| AppError::Other(format!("Failed to retrieve custom endpoint: {}", e)));

    // Audit the outcome
    match &result {
        Ok(endpoint_opt) => {
            let event = AuditEvent::new(AuditAction::CredentialAccessed, AuditResult::success())
                .with_resource_id(resource_id)
                .with_metadata("operation", "get_custom_endpoint")
                .with_metadata("type", "endpoint")
                .with_metadata("exists", endpoint_opt.is_some().to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::CredentialAccessed,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(resource_id)
            .with_metadata("operation", "get_custom_endpoint")
            .with_metadata("type", "endpoint");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

// Wrappers removed - direct implementation functions above are now used by plugins/credentials/commands.rs
// - set_api_key -> set_api_key_impl
// - get_api_key -> get_api_key_impl
// - delete_api_key -> delete_api_key_impl
// - has_api_key -> has_api_key_impl
// - clear_all_credentials -> clear_all_credentials_impl
// - set_custom_endpoint -> set_custom_endpoint_impl
// - get_custom_endpoint -> get_custom_endpoint_impl
