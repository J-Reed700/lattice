//! Architecture and tensor-layout support for the exact pinned local engine.
//! This is a format check against the bundled `llama-server` binary before
//! downloading gigabytes of weights.
use super::gguf_metadata::{
    is_integer, read_integer, read_string, read_u32, read_u64, skip_value, value_type, GGUF_MAGIC,
};
use futures::StreamExt;
use lattice_core::domain::models::llm_architecture::{
    classify_llm_architecture, LlmArchitectureSupport,
};
use lattice_core::shared::error::{AppError, Result};
use std::io::Cursor;
use std::time::Duration;

/// Qwen 3.5 / 3.8 headers carry a 248K vocabulary and merge table whose KV
/// section and tensor directory end around 10.6 MiB.
const HEADER_LIMIT: usize = 16 * 1024 * 1024;
const DEFAULT_ALIGNMENT: u64 = 32;
const MAX_DIMS: u32 = 4;

/// Exact `(type_name, blck_size, type_size)` table from `ggml-org/llama.cpp`
/// `v0.7.0` (`GGML_TYPE_COUNT = 43`). Deprecated/removed slots have `blck_size = 0`.
const GGML_TYPE_TRAITS: [(&str, u64, u64); 43] = [
    ("f32", 1, 4),                // 0
    ("f16", 1, 2),                // 1
    ("q4_0", 32, 18),             // 2
    ("q4_1", 32, 20),             // 3
    ("deprecated", 0, 0),         // 4
    ("deprecated", 0, 0),         // 5
    ("q5_0", 32, 22),             // 6
    ("q5_1", 32, 24),             // 7
    ("q8_0", 32, 34),             // 8
    ("q8_1", 32, 36),             // 9
    ("q2_K", 256, 84),            // 10
    ("q3_K", 256, 110),           // 11
    ("q4_K", 256, 144),           // 12
    ("q5_K", 256, 176),           // 13
    ("q6_K", 256, 210),           // 14
    ("q8_K", 256, 292),           // 15
    ("iq2_xxs", 256, 66),         // 16
    ("iq2_xs", 256, 74),          // 17
    ("iq3_xxs", 256, 98),         // 18
    ("iq1_s", 256, 50),           // 19
    ("iq4_nl", 32, 18),           // 20
    ("iq3_s", 256, 110),          // 21
    ("iq2_s", 256, 82),           // 22
    ("iq4_xs", 256, 136),         // 23
    ("i8", 1, 1),                 // 24
    ("i16", 1, 2),                // 25
    ("i32", 1, 4),                // 26
    ("i64", 1, 8),                // 27
    ("f64", 1, 8),                // 28
    ("iq1_m", 256, 56),           // 29
    ("bf16", 1, 2),               // 30
    ("removed_q4_0_4_4", 0, 0),   // 31
    ("removed_q4_0_4_8", 0, 0),   // 32
    ("removed_q4_0_8_8", 0, 0),   // 33
    ("tq1_0", 256, 54),           // 34
    ("tq2_0", 256, 66),           // 35
    ("removed_iq4_nl_4_4", 0, 0), // 36
    ("removed_iq4_nl_4_8", 0, 0), // 37
    ("removed_iq4_nl_8_8", 0, 0), // 38
    ("mxfp4", 32, 17),            // 39
    ("nvfp4", 64, 36),            // 40
    ("q1_0", 128, 18),            // 41
    ("q2_0", 64, 18),             // 42
];

fn check_architecture(architecture: &str) -> Result<()> {
    match classify_llm_architecture(architecture) {
        LlmArchitectureSupport::Standalone => Ok(()),
        LlmArchitectureSupport::SidecarOnly => Err(AppError::InvalidInput(format!(
            "This GGUF file uses the '{architecture}' sidecar/codec architecture rather than a standalone language model. The model weights were not downloaded."
        ))),
        LlmArchitectureSupport::Unsupported => Err(AppError::InvalidInput(format!(
            "This model uses the '{architecture}' architecture, which Lattice's bundled local engine does not support. Update Lattice or choose a compatible model. The model weights were not downloaded."
        ))),
    }
}

#[derive(Debug)]
enum PrefixVerdict {
    Incomplete,
    Compatible,
    Rejected(AppError),
}

/// Inspect a bounded GGUF prefix through both the key-value header and the
/// tensor info directory. Returns `Incomplete` when more prefix bytes are
/// needed, `Rejected` as soon as an incompatible header key or tensor entry is
/// seen, and `Compatible` only once the entire header and tensor table pass.
fn inspect_prefix(bytes: &[u8]) -> PrefixVerdict {
    if bytes.len() < 4 {
        return PrefixVerdict::Incomplete;
    }
    let mut reader = Cursor::new(bytes);
    if read_u32(&mut reader) != Some(GGUF_MAGIC) {
        return PrefixVerdict::Rejected(AppError::InvalidInput(
            "Selected file is not a valid GGUF model. The model weights were not downloaded."
                .into(),
        ));
    }
    let Some(version) = read_u32(&mut reader) else {
        return PrefixVerdict::Incomplete;
    };
    if !(2..=3).contains(&version) {
        return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
            "This GGUF file uses format version {version}, which Lattice's bundled local engine does not support. The model weights were not downloaded."
        )));
    }
    let Some(tensor_count) = read_u64(&mut reader) else {
        return PrefixVerdict::Incomplete;
    };
    let Some(kv_count) = read_u64(&mut reader) else {
        return PrefixVerdict::Incomplete;
    };
    if tensor_count > 100_000 || kv_count > 100_000 {
        return PrefixVerdict::Rejected(AppError::InvalidInput(
            "This GGUF header declares an invalid metadata or tensor count. The model weights were not downloaded.".into(),
        ));
    }

    let mut architecture: Option<String> = None;
    let mut alignment = DEFAULT_ALIGNMENT;
    let mut block_count: Option<u32> = None;

    for _ in 0..kv_count {
        let Some(key) = read_string(&mut reader) else {
            return PrefixVerdict::Incomplete;
        };
        let Some(kind) = read_u32(&mut reader) else {
            return PrefixVerdict::Incomplete;
        };

        if key.starts_with("prism.hadamard.") {
            return PrefixVerdict::Rejected(AppError::InvalidInput(
                "This model requires a fork-specific Hadamard activation rotation runtime (prism.hadamard metadata) that Lattice's bundled llama.cpp engine does not support. The model weights were not downloaded.".into(),
            ));
        }

        if key == "general.type" && kind == value_type::STRING {
            let Some(file_type) = read_string(&mut reader) else {
                return PrefixVerdict::Incomplete;
            };
            if file_type != "model" {
                return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                    "This GGUF file is a '{file_type}' artifact rather than a standalone language model. The file was not downloaded."
                )));
            }
            continue;
        }

        if key == "general.architecture" && kind == value_type::STRING {
            let Some(arch) = read_string(&mut reader) else {
                return PrefixVerdict::Incomplete;
            };
            if arch.is_empty() {
                return PrefixVerdict::Rejected(AppError::InvalidInput(
                    "This GGUF file declares an empty general.architecture. The model weights were not downloaded.".into(),
                ));
            }
            if let Err(err) = check_architecture(&arch) {
                return PrefixVerdict::Rejected(err);
            }
            architecture = Some(arch);
            continue;
        }

        if key == "general.alignment" && is_integer(kind) {
            let Some(parsed) = read_integer(&mut reader, kind) else {
                return PrefixVerdict::Incomplete;
            };
            match parsed.map(u64::from) {
                Some(val) if val > 0 && val.is_power_of_two() => alignment = val,
                _ => {
                    return PrefixVerdict::Rejected(AppError::InvalidInput(
                        "This GGUF file declares an invalid general.alignment. The model weights were not downloaded.".into(),
                    ));
                }
            }
            continue;
        }

        if key.ends_with(".block_count") && !key.contains(".vision.") && is_integer(kind) {
            let Some(parsed) = read_integer(&mut reader, kind) else {
                return PrefixVerdict::Incomplete;
            };
            if let Some(count) = parsed {
                block_count = Some(count);
            }
            continue;
        }

        if skip_value(&mut reader, kind).is_none() || reader.position() > bytes.len() as u64 {
            return PrefixVerdict::Incomplete;
        }
    }

    let Some(arch) = architecture else {
        return PrefixVerdict::Rejected(AppError::InvalidInput(
            "Could not determine this model's GGUF architecture from its header. Its compatibility is unknown, so the model weights were not downloaded.".into(),
        ));
    };
    if let Err(err) = check_architecture(&arch) {
        return PrefixVerdict::Rejected(err);
    }

    let mut expected_offset = 0u64;
    let mut has_any_blk_tensor = false;
    let mut has_blk_0_tensor = false;

    for _ in 0..tensor_count {
        let Some(name) = read_string(&mut reader) else {
            return PrefixVerdict::Incomplete;
        };
        let Some(n_dims) = read_u32(&mut reader) else {
            return PrefixVerdict::Incomplete;
        };
        if !(1..=MAX_DIMS).contains(&n_dims) {
            return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                "Tensor '{name}' has an invalid dimension count ({n_dims}). The model weights were not downloaded."
            )));
        }
        let mut ne = [1u64; 4];
        for dim in ne.iter_mut().take(n_dims as usize) {
            let Some(extent) = read_u64(&mut reader) else {
                return PrefixVerdict::Incomplete;
            };
            if extent == 0 {
                return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                    "Tensor '{name}' has a zero dimension in its GGUF header. The model weights were not downloaded."
                )));
            }
            *dim = extent;
        }
        let Some(tensor_type) = read_u32(&mut reader) else {
            return PrefixVerdict::Incomplete;
        };
        let Some(offset) = read_u64(&mut reader) else {
            return PrefixVerdict::Incomplete;
        };

        let Some((type_name, blck_size, type_size)) = usize::try_from(tensor_type)
            .ok()
            .and_then(|idx| GGML_TYPE_TRAITS.get(idx).copied())
            .filter(|(_, blck_size, _)| *blck_size > 0)
        else {
            return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                "This model uses an unsupported tensor quantization format (tensor '{name}' has ggml_type {tensor_type}, which Lattice's bundled llama.cpp engine does not support). Choose a standard GGUF quantization. The model weights were not downloaded."
            )));
        };

        if ne[0] % blck_size != 0 {
            return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                "Tensor '{name}' ({type_name}) has row width {}, which is not divisible by block size {blck_size} in Lattice's bundled llama.cpp engine. The model weights were not downloaded.",
                ne[0]
            )));
        }

        let Some(total_elements) = ne[0]
            .checked_mul(ne[1])
            .and_then(|n| n.checked_mul(ne[2]))
            .and_then(|n| n.checked_mul(ne[3]))
        else {
            return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                "Tensor '{name}' dimensions overflow in GGUF header. The model weights were not downloaded."
            )));
        };
        let Some(tensor_bytes) = (total_elements / blck_size).checked_mul(type_size) else {
            return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                "Tensor '{name}' byte size overflows in GGUF header. The model weights were not downloaded."
            )));
        };

        if offset != expected_offset {
            return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                "This model uses a non-standard '{type_name}' block layout (tensor '{name}' is at byte offset {offset}, expected {expected_offset} for stock llama.cpp). Choose a standard GGUF variant (such as g64 or Q4_K_M). The model weights were not downloaded."
            )));
        }

        let Some(padded_bytes) = tensor_bytes
            .checked_add(alignment - 1)
            .map(|n| (n / alignment) * alignment)
        else {
            return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                "Tensor '{name}' alignment padding overflows. The model weights were not downloaded."
            )));
        };
        let Some(next_offset) = expected_offset.checked_add(padded_bytes) else {
            return PrefixVerdict::Rejected(AppError::InvalidInput(format!(
                "Tensor '{name}' cumulative offset overflows. The model weights were not downloaded."
            )));
        };
        expected_offset = next_offset;

        if name.starts_with("blk.") {
            has_any_blk_tensor = true;
            if name.starts_with("blk.0.") {
                has_blk_0_tensor = true;
            }
        }
    }

    if (has_any_blk_tensor || block_count.unwrap_or(0) > 0) && tensor_count > 0 && !has_blk_0_tensor
    {
        return PrefixVerdict::Rejected(AppError::InvalidInput(
            "This GGUF file only contains auxiliary or draft layers (missing base 'blk.0.*' transformer layers) and cannot be loaded as a standalone model. The model weights were not downloaded.".into(),
        ));
    }

    PrefixVerdict::Compatible
}

/// Read a bounded prefix, including when a server ignores Range. Never buffer a
/// whole GGUF to decide whether downloading it is worthwhile.
pub(crate) async fn check_download(url: &str, token: Option<&str>) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| AppError::Network("Could not start the model compatibility check".into()))?;
    let mut request = client
        .get(url)
        .header("Range", format!("bytes=0-{}", HEADER_LIMIT - 1));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|_| {
        AppError::Network(
            "Could not read the model header to check compatibility. Please retry.".into(),
        )
    })?;
    if !response.status().is_success() {
        return Err(AppError::Network(format!(
            "The model compatibility check failed (HTTP {}). Check access to the selected model and retry.",
            response.status().as_u16()
        )));
    }
    let mut stream = response.bytes_stream();
    let mut prefix = Vec::new();
    let mut next_check_at = 1usize;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| {
            AppError::Network("The model header download was interrupted. Please retry.".into())
        })?;
        prefix.extend(chunk.iter().take(HEADER_LIMIT.saturating_sub(prefix.len())));
        if prefix.len() >= next_check_at || prefix.len() == HEADER_LIMIT {
            match inspect_prefix(&prefix) {
                PrefixVerdict::Compatible => return Ok(()),
                PrefixVerdict::Rejected(err) => return Err(err),
                PrefixVerdict::Incomplete => {
                    let step = (prefix.len() / 2).clamp(16 * 1024, 512 * 1024);
                    next_check_at = prefix.len().saturating_add(step).min(HEADER_LIMIT);
                }
            }
        }
        if prefix.len() == HEADER_LIMIT {
            break;
        }
    }

    match inspect_prefix(&prefix) {
        PrefixVerdict::Compatible => Ok(()),
        PrefixVerdict::Rejected(err) => Err(err),
        PrefixVerdict::Incomplete => Err(AppError::InvalidInput(
            "Could not verify this model's GGUF header and tensor layout. Its compatibility is unknown, so the model weights were not downloaded.".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn push_string(buf: &mut Vec<u8>, value: &str) {
        buf.extend((value.len() as u64).to_le_bytes());
        buf.extend(value.as_bytes());
    }

    fn gguf(architecture: &str) -> Vec<u8> {
        gguf_with_tensor(architecture, "blk.0.attn_norm.weight", &[64], 0, 0, &[])
    }

    fn gguf_with_tensor(
        architecture: &str,
        tensor_name: &str,
        dims: &[u64],
        tensor_type: u32,
        offset: u64,
        extra_str_kvs: &[(&str, &str)],
    ) -> Vec<u8> {
        let mut data = b"GGUF".to_vec();
        data.extend(3u32.to_le_bytes());
        data.extend(1u64.to_le_bytes()); // 1 tensor
        data.extend((1 + extra_str_kvs.len() as u64).to_le_bytes());
        push_string(&mut data, "general.architecture");
        data.extend(value_type::STRING.to_le_bytes());
        push_string(&mut data, architecture);
        for (k, v) in extra_str_kvs {
            push_string(&mut data, k);
            data.extend(value_type::STRING.to_le_bytes());
            push_string(&mut data, v);
        }
        push_string(&mut data, tensor_name);
        data.extend((dims.len() as u32).to_le_bytes());
        for &d in dims {
            data.extend(d.to_le_bytes());
        }
        data.extend(tensor_type.to_le_bytes());
        data.extend(offset.to_le_bytes());
        data
    }

    #[test]
    fn manifest_matches_the_engine_pin() {
        let lock = include_str!("../../../../scripts/llama-server.lock");
        let tag = lock
            .lines()
            .find_map(|line| line.strip_prefix("llama_cpp_tag "))
            .unwrap();
        assert_eq!(
            lattice_core::domain::models::llm_architecture::PINNED_ENGINE_ARCHITECTURES
                .lines()
                .nth(1),
            Some(tag)
        );
        assert!(check_architecture("llama").is_ok());
        assert!(check_architecture("future-unknown-family").is_err());
        assert!(check_architecture("dflash").is_err());
        assert!(check_architecture("eagle3").is_err());
    }

    #[tokio::test]
    async fn inspects_the_exact_file_with_range_and_auth_before_accepting_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/selected.gguf"))
            .and(header("range", format!("bytes=0-{}", HEADER_LIMIT - 1)))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(gguf("llama")))
            .expect(1)
            .mount(&server)
            .await;
        check_download(
            &format!("{}/selected.gguf", server.uri()),
            Some("test-token"),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn rejects_unsupported_or_missing_architecture_even_if_range_is_ignored() {
        let server = MockServer::start().await;
        for (file, body) in [
            ("unknown", gguf("future-unknown-family")),
            ("invalid", b"not GGUF".to_vec()),
        ] {
            Mock::given(path(format!("/{file}")))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&server)
                .await;
            let error = check_download(&format!("{}/{file}", server.uri()), None)
                .await
                .unwrap_err();
            assert!(error.to_string().contains("not downloaded"));
        }
    }

    #[test]
    fn rejects_fork_only_quantizations_hadamard_metadata_and_draft_heads() {
        // PrismML PQ2_0 custom ggml_type = 142
        let pq2 = gguf_with_tensor("qwen35", "output.weight", &[5120, 248320], 142, 0, &[]);
        assert!(matches!(
            inspect_prefix(&pq2),
            PrefixVerdict::Rejected(err) if err.to_string().contains("ggml_type 142")
        ));

        // Hadamard rotation metadata even on standard F16 tensor
        let hadamard = gguf_with_tensor(
            "qwen35",
            "blk.0.attn_norm.weight",
            &[64],
            1,
            0,
            &[(
                "prism.hadamard.transform",
                "normalized-sylvester-walsh-hadamard",
            )],
        );
        assert!(matches!(
            inspect_prefix(&hadamard),
            PrefixVerdict::Rejected(err) if err.to_string().contains("prism.hadamard")
        ));

        // Non-model GGUF (imatrix)
        let imatrix = gguf_with_tensor(
            "qwen35",
            "blk.0.attn_norm.weight",
            &[64],
            0,
            0,
            &[("general.type", "imatrix")],
        );
        assert!(matches!(
            inspect_prefix(&imatrix),
            PrefixVerdict::Rejected(err) if err.to_string().contains("imatrix")
        ));

        // Standalone MTP draft file with only blk.64.* and no blk.0.*
        let mtp_only = gguf_with_tensor("qwen35", "blk.64.attn_norm.weight", &[64], 0, 0, &[]);
        assert!(matches!(
            inspect_prefix(&mtp_only),
            PrefixVerdict::Rejected(err) if err.to_string().contains("blk.0.*")
        ));
    }

    #[test]
    fn rejects_mismatched_tensor_offset_from_fork_redefined_block_size() {
        // Two tensors where tensor 0 is ggml_type 42 (q2_0: 64 elements = 18 bytes, padded to 32)
        // but tensor 1 is placed at offset 64 instead of 32.
        let mut data = b"GGUF".to_vec();
        data.extend(3u32.to_le_bytes());
        data.extend(2u64.to_le_bytes()); // 2 tensors
        data.extend(1u64.to_le_bytes()); // 1 kv
        push_string(&mut data, "general.architecture");
        data.extend(value_type::STRING.to_le_bytes());
        push_string(&mut data, "qwen35");

        push_string(&mut data, "blk.0.attn_q.weight");
        data.extend(1u32.to_le_bytes());
        data.extend(128u64.to_le_bytes()); // 2 blocks of 64 = 36 bytes -> padded to 64 in g64, 34 -> 64 in g128
        data.extend(42u32.to_le_bytes());
        data.extend(0u64.to_le_bytes());

        push_string(&mut data, "blk.0.attn_k.weight");
        data.extend(1u32.to_le_bytes());
        data.extend(64u64.to_le_bytes());
        data.extend(42u32.to_le_bytes());
        data.extend(32u64.to_le_bytes()); // Wrong offset (expected 64)

        assert!(matches!(
            inspect_prefix(&data),
            PrefixVerdict::Rejected(err) if err.to_string().contains("expected 64")
        ));
    }

    #[test]
    fn truncated_headers_never_claim_compatibility() {
        let header = gguf("llama");
        for length in 0..header.len() {
            assert!(
                matches!(inspect_prefix(&header[..length]), PrefixVerdict::Incomplete),
                "prefix of length {length} should be Incomplete"
            );
        }
        assert!(matches!(inspect_prefix(&header), PrefixVerdict::Compatible));
    }
}
