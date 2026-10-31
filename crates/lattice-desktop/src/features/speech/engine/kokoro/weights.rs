//! Reading `kokoro-v1_0.pth`: one state dict per component, with weight norm
//! folded into plain kernels at load.

use std::collections::HashMap;
use std::path::Path;

use candle_core::{DType, Device, Result, Tensor, D};
use candle_nn::{Conv1dConfig, ConvTranspose1d, ConvTranspose1dConfig, Linear};

use super::conv::Conv;

pub(super) struct Weights {
    tensors: HashMap<String, Tensor>,
}

impl Weights {
    /// The state dict saved under `component` (`bert`, `predictor`, …), with
    /// the `module.` prefix the training wrapper left on every name removed.
    pub(super) fn load(path: &Path, component: &str, device: &Device) -> Result<Self> {
        let mut tensors = HashMap::new();
        for (name, tensor) in candle_core::pickle::read_all_with_key(path, Some(component))? {
            let name = name.strip_prefix("module.").unwrap_or(&name).to_string();
            tensors.insert(name, tensor.to_dtype(DType::F32)?.to_device(device)?);
        }
        Ok(Self { tensors })
    }

    pub(super) fn has(&self, name: &str) -> bool {
        self.tensors.contains_key(name)
    }

    pub(super) fn get(&self, name: &str) -> Result<Tensor> {
        self.tensors
            .get(name)
            .cloned()
            .ok_or_else(|| candle_core::Error::Msg(format!("Kokoro weights have no {name}")))
    }

    /// A linear layer; an empty prefix names a component that is one layer.
    pub(super) fn linear(&self, prefix: &str) -> Result<Linear> {
        let name = |field: &str| {
            if prefix.is_empty() {
                field.to_string()
            } else {
                format!("{prefix}.{field}")
            }
        };
        Ok(Linear::new(
            self.get(&name("weight"))?,
            Some(self.get(&name("bias"))?),
        ))
    }

    /// A kernel, computing `g · v / ‖v‖` (the norm over every axis but the
    /// first) when the layer was trained with weight norm.
    fn kernel(&self, prefix: &str) -> Result<Tensor> {
        if let Some(weight) = self.tensors.get(&format!("{prefix}.weight")) {
            return Ok(weight.clone());
        }
        let g = self.get(&format!("{prefix}.weight_g"))?;
        let v = self.get(&format!("{prefix}.weight_v"))?;
        let norm = v.sqr()?.sum_keepdim(D::Minus1)?.sum_keepdim(1)?.sqrt()?;
        v.broadcast_mul(&g.broadcast_div(&norm)?)
    }

    fn bias(&self, prefix: &str) -> Option<Tensor> {
        self.tensors.get(&format!("{prefix}.bias")).cloned()
    }

    pub(super) fn conv(&self, prefix: &str, config: Conv1dConfig) -> Result<Conv> {
        Conv::new(
            &self.kernel(prefix)?,
            self.bias(prefix),
            config.padding,
            config.stride,
            config.dilation,
        )
    }

    pub(super) fn conv_transpose(
        &self,
        prefix: &str,
        config: ConvTranspose1dConfig,
    ) -> Result<ConvTranspose1d> {
        Ok(ConvTranspose1d::new(
            self.kernel(prefix)?,
            self.bias(prefix),
            config,
        ))
    }

    /// A depthwise kernel `[channels, 1, k]` and its bias, for layers run
    /// without Candle's per-group convolution.
    pub(super) fn depthwise(&self, prefix: &str) -> Result<(Tensor, Option<Tensor>)> {
        Ok((self.kernel(prefix)?, self.bias(prefix)))
    }
}

/// A same-length convolution with the given dilation, as every non-strided
/// layer in Kokoro is.
pub(super) fn same(kernel_size: usize, dilation: usize) -> Conv1dConfig {
    Conv1dConfig {
        padding: (kernel_size * dilation - dilation) / 2,
        dilation,
        ..Default::default()
    }
}

pub(super) fn leaky_relu(x: &Tensor, slope: f64) -> Result<Tensor> {
    x.maximum(&(x * slope)?)
}
