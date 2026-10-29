//! Kokoro's 1-D convolutions as im2col plus one matmul per block of frames.
//! Candle's own `conv1d` spends most of its time arranging columns; at the
//! vocoder's 4,800 frames a second that was four times the matmul itself.

use candle_core::{Module, Result, Tensor};
use rayon::prelude::*;

/// Output frames per matmul, which bounds the column buffer
/// (`in_channels × taps × BLOCK` floats).
const BLOCK: usize = 4096;

pub(super) struct Conv {
    /// `[out, in × taps]`, row `c × taps + j` matching the column order.
    kernel: Tensor,
    /// `[out, 1]`.
    bias: Option<Tensor>,
    taps: usize,
    padding: usize,
    stride: usize,
    dilation: usize,
}

impl Conv {
    /// `kernel` is `[out, in, taps]`, as PyTorch stores it.
    pub(super) fn new(
        kernel: &Tensor,
        bias: Option<Tensor>,
        padding: usize,
        stride: usize,
        dilation: usize,
    ) -> Result<Self> {
        let (out, input, taps) = kernel.dims3()?;
        Ok(Self {
            kernel: kernel.reshape((out, input * taps))?.contiguous()?,
            bias: bias.map(|b| b.reshape((out, 1))).transpose()?,
            taps,
            padding,
            stride: stride.max(1),
            dilation: dilation.max(1),
        })
    }
}

impl Module for Conv {
    /// `[1, in, L]` → `[1, out, L']`.
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let (_, channels, len) = x.dims3()?;
        let reach = self.dilation * (self.taps - 1);
        let out_len = (len + 2 * self.padding).saturating_sub(reach + 1) / self.stride + 1;
        let input: Vec<f32> = x.flatten_all()?.to_vec1()?;
        let mut blocks = Vec::with_capacity(out_len.div_ceil(BLOCK));
        for start in (0..out_len).step_by(BLOCK) {
            let width = BLOCK.min(out_len - start);
            let mut columns = vec![0f32; channels * self.taps * width];
            columns
                .par_chunks_mut(width)
                .enumerate()
                .for_each(|(row, column)| {
                    let (channel, tap) = (row / self.taps, row % self.taps);
                    let Some(source) = input.get(channel * len..(channel + 1) * len) else {
                        return;
                    };
                    // Frame `t` of this row reads input sample `first + t * stride`.
                    let first = (start * self.stride + tap * self.dilation) as isize
                        - self.padding as isize;
                    if self.stride == 1 {
                        let from = (-first).clamp(0, width as isize) as usize;
                        let to =
                            (len as isize - first).clamp(from as isize, width as isize) as usize;
                        let source_from = (first + from as isize) as usize;
                        if let (Some(dst), Some(src)) = (
                            column.get_mut(from..to),
                            source.get(source_from..source_from + (to - from)),
                        ) {
                            dst.copy_from_slice(src);
                        }
                    } else {
                        for (t, value) in column.iter_mut().enumerate() {
                            let at = first + (t * self.stride) as isize;
                            if at >= 0 {
                                if let Some(sample) = source.get(at as usize) {
                                    *value = *sample;
                                }
                            }
                        }
                    }
                });
            let columns = Tensor::from_vec(columns, (channels * self.taps, width), x.device())?;
            blocks.push(self.kernel.matmul(&columns)?);
        }
        let y = Tensor::cat(&blocks, 1)?;
        let y = match &self.bias {
            Some(bias) => y.broadcast_add(bias)?,
            None => y,
        };
        y.unsqueeze(0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use candle_core::{Device, Module, Tensor};

    use super::Conv;

    #[test]
    fn matches_candle_for_padding_stride_and_dilation() {
        let device = Device::Cpu;
        let x = Tensor::randn(0f32, 1.0, (1, 6, 9001), &device).unwrap();
        let kernel = Tensor::randn(0f32, 1.0, (5, 6, 4), &device).unwrap();
        let bias = Tensor::randn(0f32, 1.0, 5, &device).unwrap();
        for (padding, stride, dilation) in [(0, 1, 1), (3, 1, 1), (5, 1, 3), (3, 6, 1), (1, 2, 1)] {
            let ours = Conv::new(&kernel, Some(bias.clone()), padding, stride, dilation)
                .unwrap()
                .forward(&x)
                .unwrap();
            let theirs = x
                .conv1d(&kernel, padding, stride, dilation, 1)
                .unwrap()
                .broadcast_add(&bias.reshape((1, 5, 1)).unwrap())
                .unwrap();
            assert_eq!(ours.dims(), theirs.dims(), "{padding} {stride} {dilation}");
            let worst = (ours - theirs)
                .unwrap()
                .abs()
                .unwrap()
                .flatten_all()
                .unwrap()
                .max(0)
                .unwrap()
                .to_scalar::<f32>()
                .unwrap();
            assert!(worst < 1e-3, "{padding} {stride} {dilation}: {worst}");
        }
    }
}
