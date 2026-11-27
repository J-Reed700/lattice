use super::*;
use serde_json::json;

#[test]
fn catches_invalid_smart_scopes_while_allowing_valid_cruds_and_explicit_negations() {
    assert!(!smart_scope_faults(
        "Request system/DocumentReference.c and system/Binary.w for upload.",
        &[]
    )
    .is_empty());
    assert!(!smart_scope_faults(
        "Fallback to system/Binary.rw when fine-grained scopes fail.",
        &[]
    )
    .is_empty());
    assert!(!smart_scope_faults(
        "SMART v2 .cruds lacks the 'w' (write) permission on Binary.",
        &[]
    )
    .is_empty());

    assert!(smart_scope_faults(
        "Request system/DocumentReference.c and system/Binary.c, or patient/Observation.rs for reads and searches.",
        &[]
    )
    .is_empty());
    assert!(smart_scope_faults(
        "Legacy SMART v1 servers use system/Binary.read and system/Binary.write.",
        &[]
    )
    .is_empty());
    assert!(smart_scope_faults(
        "system/Binary.w does not exist in SMART; SMART v2 uses .c for create.",
        &[]
    )
    .is_empty());
}

#[test]
fn catches_undeclared_ts_injections_refresh_token_naming_and_expiry_buffer_drift() {
    let lesson = json!({
        "blocks": [
            {
                "kind": "explanation",
                "body": "With `expires_in` of 300s, use `TTL = expires_in - 20` so tokens refresh 20s early."
            },
            {
                "kind": "worked_example",
                "body": "```typescript\nimport { Injectable } from '@nestjs/common';\n@Injectable()\nexport class SmartTokenService {\n  constructor(private readonly cfg: ConfigService) {}\n  async refreshToken() {\n    const body = { grant_type: 'client_credentials' };\n    const cacheTtl = (response.expires_in - 30) * 1000;\n    return { body, cacheTtl };\n  }\n}\n```"
            }
        ]
    });
    let faults = block_consistency_faults(&lesson, &[]);
    assert!(
        faults
            .iter()
            .any(|(idx, msg)| *idx == 1 && msg.contains("ConfigService")),
        "{faults:?}"
    );
    assert!(
        faults
            .iter()
            .any(|(idx, msg)| *idx == 1 && msg.contains("refreshToken")),
        "{faults:?}"
    );
    assert!(
        faults
            .iter()
            .any(|(idx, msg)| *idx == 1 && msg.contains("expires_in - 30")),
        "{faults:?}"
    );

    let clean = json!({
        "blocks": [
            {
                "kind": "explanation",
                "body": "With `expires_in` of 300s, use `TTL = expires_in - 20`."
            },
            {
                "kind": "worked_example",
                "body": "```typescript\nexport interface TenantConfigStore { get(): string; }\nexport class SmartTokenService {\n  constructor(private readonly cfg: TenantConfigStore) {}\n  async requestAccessToken() {\n    const body = { grant_type: 'client_credentials' };\n    const cacheTtl = (response.expires_in - 20) * 1000;\n    return { body, cacheTtl };\n  }\n}\n```"
            }
        ]
    });
    assert!(block_consistency_faults(&clean, &[]).is_empty());
}
