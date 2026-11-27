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

#[test]
fn accepts_multiline_typescript_imports_with_and_without_semicolons() {
    let lesson = json!({
        "blocks": [
            {
                "kind": "worked_example",
                "body": "```typescript\nimport {\n  Injectable,\n  ConfigService,\n} from '@nestjs/config'\n@Injectable()\nexport class SmartTokenService {\n  constructor(private readonly cfg: ConfigService) {}\n}\n```"
            }
        ]
    });
    assert!(block_consistency_faults(&lesson, &[]).is_empty());
}

#[test]
fn ignores_non_smart_paths_in_unrelated_courses() {
    let unrelated = "Mount system/Key.pem and user/Config.cfg under /etc/tls, inspect system/App.rb, back up patient/Record.db, and within project scope check user/State.raw.";
    assert!(smart_scope_faults(unrelated, &[]).is_empty());
}

#[test]
fn catches_backticked_smart_scope_before_trailing_punctuation() {
    let faults = smart_scope_faults("Check `patient/Observation.w`.", &[]);
    assert!(
        faults.iter().any(|f| f.contains("patient/Observation.w")),
        "{faults:?}"
    );
}

#[test]
fn catches_scope_401_vs_403_contradiction_and_smart_token_launch_placement() {
    let lesson = json!({
        "blocks": [
            {
                "kind": "explanation",
                "body": "```mermaid\nflowchart TD\nS[Scope Validation] -->|Fail| H[403 Forbidden]\n```\nIn SMART on FHIR, exchange the authorization code at the token endpoint, including the launch context parameter."
            },
            {
                "kind": "check",
                "body": "Expect a 401 Unauthorized error for invalid scopes after registration."
            }
        ]
    });
    let faults = block_consistency_faults(&lesson, &[]);
    assert!(
        faults
            .iter()
            .any(|(idx, m)| *idx == 0 && m.contains("`/authorize`")),
        "{faults:?}"
    );
    assert!(
        faults
            .iter()
            .any(|(idx, m)| *idx == 1 && m.contains("`401`")),
        "{faults:?}"
    );

    let fenced_only = json!({
        "blocks": [
            {
                "kind": "explanation",
                "body": "```http\nPOST /token HTTP/1.1\nlaunch=xyz\n```\nSend `launch` to `/authorize` in SMART App Launch, and return HTTP 403 Forbidden for insufficient_scope."
            },
            {
                "kind": "example",
                "body": "```text\nHTTP 401 due to invalid scope inside fence only.\n```\nClient credentials requests succeed with valid scopes."
            }
        ]
    });
    assert!(block_consistency_faults(&fenced_only, &[]).is_empty());
    assert!(!contains_401_scope_error(
        "```text\nHTTP 401 due to invalid scope inside fence.\n```\nUse 403 Forbidden."
    ));
    assert!(contains_401_scope_error(
        "An invalid scope mismatch returns HTTP 401 Unauthorized."
    ));
}
