use crate::domain::custom_model::ModelArchitecture;
use std::path::Path;
use url::Url;

/// Service for inferring model architecture from URL or file
pub struct ArchitectureInferenceService;

impl ArchitectureInferenceService {
    /// Create a new architecture inference service
    pub fn new() -> Self {
        Self
    }

    /// Infer model architecture from URL
    ///
    /// Uses file extension in URL path to guess architecture:
    /// - .gguf → GGUF
    /// - .onnx → ONNX
    /// - .safetensors → SafeTensors
    /// - .pt, .pth → PyTorch
    /// - Unknown → Unknown
    pub fn infer_from_url(&self, url: &Url) -> ModelArchitecture {
        let path = url.path();
        self.infer_from_extension(path)
    }

    /// Infer model architecture from file path
    ///
    /// Uses file extension to guess architecture
    /// Defaults to GGUF if extension cannot be determined
    pub fn infer_from_file(&self, file_path: &Path) -> ModelArchitecture {
        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            self.infer_from_extension(ext)
        } else {
            ModelArchitecture::Gguf
        }
    }

    /// Infer model architecture from file header magic bytes
    ///
    /// More reliable than extension-based inference
    /// Defaults to GGUF if magic bytes don't match known formats
    pub fn infer_from_magic_bytes(&self, bytes: &[u8]) -> ModelArchitecture {
        if bytes.len() < 4 {
            return ModelArchitecture::Gguf;
        }

        // GGUF magic: "GGUF" (0x47 0x47 0x55 0x46)
        if bytes.len() >= 4 {
            if let Some(slice) = bytes.get(0..4) {
                if slice == b"GGUF" {
                    return ModelArchitecture::Gguf;
                }
            }
        }

        // ONNX magic: Protocol Buffers (0x08 followed by field number)
        if bytes.len() >= 2 {
            if let (Some(&byte0), Some(&byte1)) = (bytes.first(), bytes.get(1)) {
                if byte0 == 0x08 && (byte1 == 0x03 || byte1 == 0x07 || byte1 == 0x01) {
                    return ModelArchitecture::Onnx;
                }
            }
        }

        // SafeTensors magic: starts with 8-byte little-endian header size
        if bytes.len() >= 8 {
            if let Some(header_bytes) = bytes.get(0..8) {
                if let Ok(header_array) = <[u8; 8]>::try_from(header_bytes) {
                    let header_size = u64::from_le_bytes(header_array);
                    if header_size > 0 && header_size < 10_000_000 {
                        return ModelArchitecture::SafeTensors;
                    }
                }
            }
        }

        // PyTorch magic: ZIP archive (0x50 0x4B 0x03 0x04)
        if bytes.len() >= 4 {
            if let Some(slice) = bytes.get(0..4) {
                if slice == b"PK\x03\x04" {
                    return ModelArchitecture::PyTorch;
                }
            }
        }

        ModelArchitecture::Gguf
    }

    /// Infer architecture from file extension
    /// Defaults to GGUF if extension doesn't match known formats
    fn infer_from_extension(&self, ext_or_path: &str) -> ModelArchitecture {
        let ext_lower = ext_or_path.to_lowercase();

        if ext_lower.ends_with(".gguf") || ext_lower == "gguf" {
            ModelArchitecture::Gguf
        } else if ext_lower.ends_with(".onnx") || ext_lower == "onnx" {
            ModelArchitecture::Onnx
        } else if ext_lower.ends_with(".safetensors") || ext_lower == "safetensors" {
            ModelArchitecture::SafeTensors
        } else if ext_lower.ends_with(".pt")
            || ext_lower.ends_with(".pth")
            || ext_lower == "pt"
            || ext_lower == "pth"
        {
            ModelArchitecture::PyTorch
        } else {
            ModelArchitecture::Gguf
        }
    }
}

impl Default for ArchitectureInferenceService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_from_extension() {
        let service = ArchitectureInferenceService::new();

        assert_eq!(
            service.infer_from_extension("model.gguf"),
            ModelArchitecture::Gguf
        );
        assert_eq!(
            service.infer_from_extension("model.onnx"),
            ModelArchitecture::Onnx
        );
        assert_eq!(
            service.infer_from_extension("model.safetensors"),
            ModelArchitecture::SafeTensors
        );
        assert_eq!(
            service.infer_from_extension("model.pt"),
            ModelArchitecture::PyTorch
        );
        assert_eq!(
            service.infer_from_extension("model.pth"),
            ModelArchitecture::PyTorch
        );
        assert_eq!(
            service.infer_from_extension("model.bin"),
            ModelArchitecture::Gguf
        );
    }

    #[test]
    fn test_infer_from_url() {
        let service = ArchitectureInferenceService::new();

        let url = Url::parse("https://example.com/models/model.gguf").unwrap();
        assert_eq!(service.infer_from_url(&url), ModelArchitecture::Gguf);

        let url = Url::parse("https://example.com/models/model.onnx").unwrap();
        assert_eq!(service.infer_from_url(&url), ModelArchitecture::Onnx);
    }

    #[test]
    fn test_infer_from_magic_bytes_gguf() {
        let service = ArchitectureInferenceService::new();

        let gguf_bytes = b"GGUF\x00\x00\x00\x01";
        assert_eq!(
            service.infer_from_magic_bytes(gguf_bytes),
            ModelArchitecture::Gguf
        );
    }

    #[test]
    fn test_infer_from_magic_bytes_pytorch() {
        let service = ArchitectureInferenceService::new();

        let pytorch_bytes = b"PK\x03\x04\x00\x00\x00\x00";
        assert_eq!(
            service.infer_from_magic_bytes(pytorch_bytes),
            ModelArchitecture::PyTorch
        );
    }

    #[test]
    fn test_infer_from_magic_bytes_unknown() {
        let service = ArchitectureInferenceService::new();

        let unknown_bytes = b"INVALID";
        assert_eq!(
            service.infer_from_magic_bytes(unknown_bytes),
            ModelArchitecture::Gguf
        );
    }
}
