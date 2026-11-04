//! The iSTFTNet vocoder: upsamples the decoder's features, mixes in a
//! harmonic excitation built from the pitch curve, and predicts a short-time
//! spectrum that is inverted to samples.

use candle_core::{Module, Result, Tensor, D};
use candle_nn::{Conv1dConfig, ConvTranspose1d, ConvTranspose1dConfig};

use super::conv::Conv;
use super::decoder::AdaIn;
use super::source::{excitation, inverse_stft, stft, Noise, BINS, HARMONICS};
use super::weights::{leaky_relu, same, Weights};

const KERNELS: [usize; 3] = [3, 7, 11];
const DILATIONS: [usize; 3] = [1, 3, 5];

/// `AdaINResBlock1`: dilated convolutions with Snake activations.
struct SnakeBlock {
    layers: Vec<SnakeLayer>,
}

struct SnakeLayer {
    adain1: AdaIn,
    alpha1: Vec<f32>,
    conv1: Conv,
    adain2: AdaIn,
    alpha2: Vec<f32>,
    conv2: Conv,
}

/// AdaIN followed by Snake, `y + sin²(αy)/α` with a per-channel `α`.
fn adain_snake(norm: &AdaIn, x: &Tensor, style: &Tensor, alpha: &[f32]) -> Result<Tensor> {
    norm.forward_then(x, style, |channel, y| {
        let a = alpha.get(channel).copied().unwrap_or(1.0);
        y + (a * y).sin().powi(2) / a
    })
}

impl SnakeBlock {
    fn load(weights: &Weights, prefix: &str, kernel: usize) -> Result<Self> {
        let layers = DILATIONS
            .iter()
            .enumerate()
            .map(|(j, &dilation)| {
                Ok(SnakeLayer {
                    adain1: AdaIn::load(weights, &format!("{prefix}.adain1.{j}"))?,
                    alpha1: weights
                        .get(&format!("{prefix}.alpha1.{j}"))?
                        .flatten_all()?
                        .to_vec1()?,
                    conv1: weights.conv(&format!("{prefix}.convs1.{j}"), same(kernel, dilation))?,
                    adain2: AdaIn::load(weights, &format!("{prefix}.adain2.{j}"))?,
                    alpha2: weights
                        .get(&format!("{prefix}.alpha2.{j}"))?
                        .flatten_all()?
                        .to_vec1()?,
                    conv2: weights.conv(&format!("{prefix}.convs2.{j}"), same(kernel, 1))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { layers })
    }

    fn forward(&self, x: &Tensor, style: &Tensor) -> Result<Tensor> {
        let mut x = x.clone();
        for layer in &self.layers {
            let y = adain_snake(&layer.adain1, &x, style, &layer.alpha1)?;
            let y = layer.conv1.forward(&y)?;
            let y = adain_snake(&layer.adain2, &y, style, &layer.alpha2)?;
            x = (layer.conv2.forward(&y)? + x)?;
        }
        Ok(x)
    }
}

struct Stage {
    /// Run unpadded, which keeps Candle on its col2im path; padding a
    /// transposed convolution only trims `crop` samples from each end.
    up: ConvTranspose1d,
    crop: usize,
    noise_conv: Conv,
    noise_res: SnakeBlock,
    resblocks: Vec<SnakeBlock>,
}

pub(super) struct Generator {
    stages: Vec<Stage>,
    conv_post: Conv,
    merge_weights: Vec<f32>,
    merge_bias: f32,
}

impl Generator {
    pub(super) fn load(weights: &Weights) -> Result<Self> {
        let prefix = "generator";
        // (upsample rate, kernel, noise-conv stride, noise-res kernel)
        let shapes: [(usize, usize, usize, usize); 2] = [(10, 20, 6, 7), (6, 12, 1, 11)];
        let stages = shapes
            .iter()
            .enumerate()
            .map(|(i, &(rate, kernel, noise_stride, noise_res))| {
                Ok(Stage {
                    up: weights.conv_transpose(
                        &format!("{prefix}.ups.{i}"),
                        ConvTranspose1dConfig {
                            stride: rate,
                            ..Default::default()
                        },
                    )?,
                    crop: (kernel - rate) / 2,
                    noise_conv: weights.conv(
                        &format!("{prefix}.noise_convs.{i}"),
                        Conv1dConfig {
                            padding: if noise_stride > 1 {
                                noise_stride.div_ceil(2)
                            } else {
                                0
                            },
                            stride: noise_stride,
                            ..Default::default()
                        },
                    )?,
                    noise_res: SnakeBlock::load(
                        weights,
                        &format!("{prefix}.noise_res.{i}"),
                        noise_res,
                    )?,
                    resblocks: KERNELS
                        .iter()
                        .enumerate()
                        .map(|(j, &k)| {
                            SnakeBlock::load(
                                weights,
                                &format!("{prefix}.resblocks.{}", i * 3 + j),
                                k,
                            )
                        })
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let merge = weights.linear(&format!("{prefix}.m_source.l_linear"))?;
        let merge_weights = merge.weight().flatten_all()?.to_vec1::<f32>()?;
        let merge_bias = merge
            .bias()
            .map(|b| b.flatten_all()?.to_vec1::<f32>())
            .transpose()?
            .and_then(|b| b.first().copied())
            .unwrap_or(0.0);
        if merge_weights.len() != HARMONICS {
            return Err(candle_core::Error::Msg(format!(
                "Kokoro's source merges {} harmonics, expected {HARMONICS}",
                merge_weights.len()
            )));
        }
        Ok(Self {
            stages,
            conv_post: weights.conv(&format!("{prefix}.conv_post"), same(7, 1))?,
            merge_weights,
            merge_bias,
        })
    }

    /// `x` `[1, 512, 2F]`, `f0` `[1, 2F]` → samples.
    pub(super) fn forward(
        &self,
        x: &Tensor,
        style: &Tensor,
        f0: &Tensor,
        noise: &mut dyn Noise,
    ) -> Result<Vec<f32>> {
        let device = x.device();
        let f0 = f0.flatten_all()?.to_vec1::<f32>()?;
        let source = excitation(&f0, &self.merge_weights, self.merge_bias, noise);
        let (frames, spectrum) = stft(&source);
        let harmonics = Tensor::from_vec(spectrum, (1, 2 * BINS, frames), device)?;

        let mut x = x.clone();
        let last = self.stages.len().saturating_sub(1);
        for (i, stage) in self.stages.iter().enumerate() {
            x = leaky_relu(&x, 0.1)?;
            let source = stage
                .noise_res
                .forward(&stage.noise_conv.forward(&harmonics)?, style)?;
            x = stage.up.forward(&x)?;
            let len = x.dim(D::Minus1)?;
            x = x.narrow(D::Minus1, stage.crop, len - 2 * stage.crop)?;
            if i == last {
                x = Tensor::cat(&[&x.narrow(D::Minus1, 1, 1)?, &x], D::Minus1)?;
            }
            x = (x + source)?;
            let mut sum = stage
                .resblocks
                .first()
                .map(|block| block.forward(&x, style))
                .transpose()?
                .unwrap_or_else(|| x.clone());
            for block in stage.resblocks.iter().skip(1) {
                sum = (sum + block.forward(&x, style)?)?;
            }
            x = (sum / stage.resblocks.len() as f64)?;
        }
        let post = self.conv_post.forward(&leaky_relu(&x, 0.01)?)?;
        let magnitude = post
            .narrow(1, 0, BINS)?
            .exp()?
            .squeeze(0)?
            .to_vec2::<f32>()?;
        let phase = post
            .narrow(1, BINS, BINS)?
            .sin()?
            .squeeze(0)?
            .to_vec2::<f32>()?;
        Ok(inverse_stft(&magnitude, &phase))
    }
}
