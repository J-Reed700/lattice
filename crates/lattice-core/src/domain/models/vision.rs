//! What a model's vision encoder does with a picture, declared per model.
//!
//! A geometry says what size a picture is resized to and what that costs in
//! prompt tokens. It comes from the catalog, from Settings or from a cloud
//! provider's default, never from asking a server. The same declaration drives
//! downscaling before a picture is sent and the token charge every window
//! check adds for it.

use serde::{Deserialize, Serialize};

mod cloud_defaults;

pub use cloud_defaults::cloud_vision_default;

/// How a model's vision encoder takes a picture: what size it is resized to
/// and what that costs in prompt tokens. Declared per model (catalog, Settings
/// or provider default), never measured from a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VisionGeometry {
    /// Dynamic resolution on a token grid (Qwen2-VL, Qwen2.5-VL, Qwen3-VL and
    /// the Qwen3.5+ chat models built on them). The picture is resized so both
    /// sides are multiples of `token_side` pixels (vision patch × spatial
    /// merge: 28 for Qwen2/2.5-VL, 32 for Qwen3-VL and later) with its area
    /// held within `min_pixels..=max_pixels`; one token per `token_side²`
    /// square plus `overhead_tokens` (vision start/end markers).
    Grid {
        token_side: u32,
        min_pixels: u32,
        max_pixels: u32,
        overhead_tokens: u32,
    },
    /// A fixed input (SigLIP-style encoders: Gemma 3/4, PaliGemma, most LLaVA
    /// builds): resized to fit `side`×`side`, always `tokens` tokens.
    Fixed { side: u32, tokens: u32 },
    /// Tiled encoders (OpenAI "high" detail, Gemini, Llama 3.2 Vision): the
    /// picture is scaled so its long side is at most `max_side`, cut into
    /// `tile_side` tiles (at most `max_tiles`), `tokens_per_tile` each plus
    /// `base_tokens` for the overview.
    Tiles {
        tile_side: u32,
        tokens_per_tile: u32,
        base_tokens: u32,
        max_tiles: u32,
        max_side: u32,
    },
}

/// A picture's size after fitting, and what the geometry charges for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FittedImage {
    pub width: u32,
    pub height: u32,
    pub tokens: u32,
}

impl VisionGeometry {
    /// For a model whose geometry nobody declared. Pictures are sent no larger
    /// than 1,024 grid squares of 28 px (≈ 896×896) and charged 1,026 tokens
    /// at that size: at or above what the local families and OpenAI charge for
    /// the same square (a 32 px grid 786, a Fixed 896 encoder 256–280, OpenAI
    /// 765). Anthropic (≈ 1,089) and Gemini (1,032) charge a little more for
    /// it, which is why every `claude-*` and `gemini-*` model carries its own
    /// provider default ([`cloud_vision_default`]) instead.
    pub const CONSERVATIVE_DEFAULT: Self = Self::Grid {
        token_side: 28,
        min_pixels: 4 * 28 * 28,
        max_pixels: 1024 * 28 * 28,
        overhead_tokens: 2,
    };

    /// The size to send for a `width`×`height` source: downscaled to the
    /// geometry, never upscaled, aspect ratio kept (rounded to the grid for
    /// `Grid`; letterboxed by the encoder itself for `Fixed`).
    pub fn fit(self, width: u32, height: u32) -> (u32, u32) {
        let (width, height) = (width.max(1), height.max(1));
        match self {
            Self::Grid {
                token_side,
                max_pixels,
                ..
            } => grid_fit(token_side, u64::from(max_pixels), width, height),
            Self::Fixed { side, .. } => long_side_within(side, width, height),
            Self::Tiles { max_side, .. } => long_side_within(max_side, width, height),
        }
    }

    /// Prompt tokens for a picture already at `fit` size.
    ///
    /// A `Grid` picture smaller than `min_pixels` is charged as `min_pixels`:
    /// the encoder enlarges it to that area itself, and an under-charge would
    /// let a request through that the window cannot hold.
    pub fn tokens(self, width: u32, height: u32) -> u32 {
        let (width, height) = (width.max(1), height.max(1));
        match self {
            Self::Grid {
                token_side,
                overhead_tokens,
                ..
            } => {
                let side = token_side.max(1);
                let squares = (width / side).max(1).saturating_mul((height / side).max(1));
                squares
                    .max(self.min_squares())
                    .saturating_add(overhead_tokens)
            }
            Self::Fixed { tokens, .. } => tokens,
            Self::Tiles {
                tile_side,
                tokens_per_tile,
                base_tokens,
                max_tiles,
                ..
            } => {
                let side = tile_side.max(1);
                let tiles = width
                    .div_ceil(side)
                    .saturating_mul(height.div_ceil(side))
                    .min(max_tiles.max(1));
                base_tokens.saturating_add(tokens_per_tile.saturating_mul(tiles))
            }
        }
    }

    /// `fit` and `tokens` together, for the common call.
    pub fn plan(self, width: u32, height: u32) -> FittedImage {
        let (width, height) = self.fit(width, height);
        FittedImage {
            width,
            height,
            tokens: self.tokens(width, height),
        }
    }

    /// The largest fit that costs at most `max_tokens`, or `None` when even
    /// the geometry's smallest rendering costs more. `Grid` shrinks the area
    /// down to `min_pixels`; `Tiles` reduces tiles down to a single one;
    /// `Fixed` cannot shrink.
    pub fn fit_within(self, width: u32, height: u32, max_tokens: u32) -> Option<FittedImage> {
        let full = self.plan(width, height);
        if full.tokens <= max_tokens {
            return Some(full);
        }
        match self {
            Self::Grid {
                token_side,
                min_pixels,
                max_pixels,
                overhead_tokens,
            } => {
                let squares = max_tokens.checked_sub(overhead_tokens).filter(|s| *s > 0)?;
                let side = u64::from(token_side.max(1));
                let area = (u64::from(squares) * side * side).min(u64::from(max_pixels));
                if area < u64::from(min_pixels) {
                    return None;
                }
                let (w, h) = grid_fit(token_side, area, width.max(1), height.max(1));
                let tokens = self.tokens(w, h);
                (tokens <= max_tokens).then_some(FittedImage {
                    width: w,
                    height: h,
                    tokens,
                })
            }
            Self::Fixed { .. } => None,
            Self::Tiles { .. } => {
                // Tile count only falls as the long side does, so the
                // largest long side that fits the budget is a binary search.
                let long = full.width.max(full.height);
                let at = |side: u32| {
                    let (w, h) = long_side_within(side, full.width, full.height);
                    FittedImage {
                        width: w,
                        height: h,
                        tokens: self.tokens(w, h),
                    }
                };
                let (mut low, mut high) = (1, long);
                if at(low).tokens > max_tokens {
                    return None;
                }
                while low < high {
                    let mid = low + (high - low).div_ceil(2);
                    if at(mid).tokens <= max_tokens {
                        low = mid;
                    } else {
                        high = mid - 1;
                    }
                }
                Some(at(low))
            }
        }
    }

    /// The most a single picture can cost on this geometry.
    pub fn max_tokens(self) -> u32 {
        match self {
            Self::Grid {
                token_side,
                max_pixels,
                overhead_tokens,
                ..
            } => {
                let side = u64::from(token_side.max(1));
                let squares =
                    u32::try_from(u64::from(max_pixels) / (side * side)).unwrap_or(u32::MAX);
                squares
                    .max(self.min_squares())
                    .saturating_add(overhead_tokens)
            }
            Self::Fixed { tokens, .. } => tokens,
            Self::Tiles {
                tokens_per_tile,
                base_tokens,
                max_tiles,
                ..
            } => base_tokens.saturating_add(tokens_per_tile.saturating_mul(max_tiles.max(1))),
        }
    }

    /// The least a picture can cost (a single tile, the minimum area, or the
    /// fixed count). What the `find_image` pick is sized from.
    pub fn min_tokens(self) -> u32 {
        match self {
            Self::Grid {
                overhead_tokens, ..
            } => self.min_squares().saturating_add(overhead_tokens),
            Self::Fixed { tokens, .. } => tokens,
            Self::Tiles {
                tokens_per_tile,
                base_tokens,
                ..
            } => base_tokens.saturating_add(tokens_per_tile),
        }
    }

    /// Why this declaration cannot be used, for the settings check.
    pub fn problem(self) -> Option<&'static str> {
        match self {
            Self::Grid { token_side: 0, .. } => Some("a grid geometry needs a token side above 0"),
            Self::Grid {
                min_pixels,
                max_pixels,
                ..
            } if min_pixels > max_pixels => {
                Some("a grid geometry's minimum area cannot exceed its maximum")
            }
            Self::Fixed { side: 0, .. } => Some("a fixed geometry needs a side above 0"),
            Self::Tiles { tile_side: 0, .. } => Some("a tiled geometry needs a tile side above 0"),
            _ => None,
        }
    }

    /// The fewest grid squares a `Grid` picture is charged: the minimum area,
    /// and never less than one square.
    fn min_squares(self) -> u32 {
        match self {
            Self::Grid {
                token_side,
                min_pixels,
                ..
            } => {
                let square = u64::from(token_side.max(1)).pow(2);
                u32::try_from(u64::from(min_pixels).div_ceil(square))
                    .unwrap_or(u32::MAX)
                    .max(1)
            }
            _ => 0,
        }
    }
}

/// Scale `width`×`height` so its area is at most `max_area`, then round each
/// side to the nearest multiple of `token_side` (at least one square, never
/// above the source). When rounding up pushes the area past `max_area`, both
/// sides round down instead.
fn grid_fit(token_side: u32, max_area: u64, width: u32, height: u32) -> (u32, u32) {
    let side = token_side.max(1);
    let area = u64::from(width) * u64::from(height);
    let scale = if area > max_area {
        (max_area as f64 / area as f64).sqrt()
    } else {
        1.0
    };
    let nearest = |source: u32| {
        let squares = (f64::from(source) * scale / f64::from(side)).round() as u32;
        let squares = if squares.saturating_mul(side) > source {
            source / side
        } else {
            squares
        };
        squares.max(1) * side
    };
    let (w, h) = (nearest(width), nearest(height));
    if u64::from(w) * u64::from(h) <= max_area {
        return (w, h);
    }
    let floor =
        |source: u32| ((f64::from(source) * scale / f64::from(side)).floor() as u32).max(1) * side;
    (floor(width), floor(height))
}

/// `width`×`height` scaled down so its long side is at most `limit`, aspect
/// kept, never enlarged. Integer arithmetic, so every caller rounds alike.
fn long_side_within(limit: u32, width: u32, height: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= limit || limit == 0 {
        return (width, height);
    }
    let scaled = |side: u32| ((u64::from(side) * u64::from(limit) / u64::from(long)) as u32).max(1);
    (scaled(width), scaled(height))
}

/// Named geometries the Settings editor offers. Each is a documented reading
/// of a model family's published preprocessing; "Custom" is the raw enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum VisionPreset {
    Qwen3Vl,
    Qwen25Vl,
    GemmaSiglip,
    OpenAi,
    Anthropic,
    Gemini,
}

/// Qwen3-VL's processor: 16 px patches merged 2×2 (32 px per token), at least
/// 64 tokens' area (`shortest_edge` 65,536 in its `preprocessor_config.json`).
/// The 2,048-token ceiling (≈ 2.1 MP) caps what a user-declared model is sent;
/// catalog entries carry the values read from their own projector headers.
const QWEN3_VL: VisionGeometry = VisionGeometry::Grid {
    token_side: 32,
    min_pixels: 64 * 32 * 32,
    max_pixels: 2048 * 32 * 32,
    overhead_tokens: 2,
};

/// Qwen2.5-VL's processor: 14 px patches merged 2×2 (28 px per token); the
/// model card's recommended `max_pixels` of 1,280·28².
const QWEN25_VL: VisionGeometry = VisionGeometry::Grid {
    token_side: 28,
    min_pixels: 4 * 28 * 28,
    max_pixels: 1280 * 28 * 28,
    overhead_tokens: 2,
};

/// Gemma 3's SigLIP encoder: every picture is resized to 896×896 and pooled
/// to 256 soft tokens (the Gemma 3 technical report).
const GEMMA_SIGLIP: VisionGeometry = VisionGeometry::Fixed {
    side: 896,
    tokens: 256,
};

/// OpenAI "high" detail: 512 px tiles at 170 tokens each plus an 85-token
/// overview (OpenAI's vision guide), long side within 2,048 px.
const OPEN_AI: VisionGeometry = VisionGeometry::Tiles {
    tile_side: 512,
    tokens_per_tile: 170,
    base_tokens: 85,
    max_tiles: 16,
    max_side: 2048,
};

/// Anthropic: about `width·height / 750` tokens (Anthropic's vision guide),
/// which a 27 px grid reproduces within 3 %. The long edge is capped at
/// 1,568 px; bounding the area by that square over-charges a picture the
/// provider shrinks further, never under-charges it.
const ANTHROPIC: VisionGeometry = VisionGeometry::Grid {
    token_side: 27,
    min_pixels: 0,
    max_pixels: 1568 * 1568,
    overhead_tokens: 0,
};

/// Gemini: 258 tokens per 768 px tile (Gemini's media-resolution guide).
const GEMINI: VisionGeometry = VisionGeometry::Tiles {
    tile_side: 768,
    tokens_per_tile: 258,
    base_tokens: 0,
    max_tiles: 6,
    max_side: 3072,
};

impl VisionPreset {
    /// Every preset, in the order the Settings editor lists them.
    pub const ALL: [Self; 6] = [
        Self::Qwen3Vl,
        Self::Qwen25Vl,
        Self::GemmaSiglip,
        Self::OpenAi,
        Self::Anthropic,
        Self::Gemini,
    ];

    pub fn geometry(self) -> VisionGeometry {
        match self {
            Self::Qwen3Vl => QWEN3_VL,
            Self::Qwen25Vl => QWEN25_VL,
            Self::GemmaSiglip => GEMMA_SIGLIP,
            Self::OpenAi => OPEN_AI,
            Self::Anthropic => ANTHROPIC,
            Self::Gemini => GEMINI,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Qwen3Vl => "Qwen3-VL",
            Self::Qwen25Vl => "Qwen2.5-VL",
            Self::GemmaSiglip => "Gemma (SigLIP)",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Gemini",
        }
    }
}

/// The catalog capability tag, beside `tool-calling`.
pub const VISION_CAPABILITY: &str = "vision";

#[cfg(test)]
mod tests;
