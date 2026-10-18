//! Fit and cost arithmetic for each geometry kind. The TypeScript port in
//! `src/utils/visionGeometry.ts` copies these rows, so the two must agree.

use super::*;
use crate::domain::models::model_ref::CloudProvider;

fn fitted(width: u32, height: u32, tokens: u32) -> FittedImage {
    FittedImage {
        width,
        height,
        tokens,
    }
}

#[test]
fn a_28_px_grid_scales_the_area_under_its_cap_and_rounds_to_the_grid() {
    let qwen25 = VisionPreset::Qwen25Vl.geometry();
    assert_eq!(qwen25.plan(1280, 1600), fitted(896, 1120, 1282));
    assert_eq!(qwen25.max_tokens(), 1282);
}

#[test]
fn a_32_px_grid_keeps_a_picture_already_under_its_cap() {
    let qwen3 = VisionPreset::Qwen3Vl.geometry();
    assert_eq!(qwen3.plan(1280, 1600), fitted(1280, 1600, 2002));
}

#[test]
fn a_grid_never_upscales_and_rounds_down_rather_than_past_the_source() {
    let qwen25 = VisionPreset::Qwen25Vl.geometry();
    assert_eq!(qwen25.plan(400, 300), fitted(392, 280, 142));
    let qwen3 = VisionPreset::Qwen3Vl.geometry();
    assert_eq!(qwen3.plan(400, 300), fitted(384, 288, 110));
}

#[test]
fn a_grid_picture_under_the_minimum_area_is_charged_as_the_minimum() {
    let qwen3 = VisionPreset::Qwen3Vl.geometry();
    assert_eq!(qwen3.plan(100, 100), fitted(96, 96, 66));
    assert_eq!(qwen3.min_tokens(), 66);
    assert_eq!(qwen3.plan(10, 10), fitted(32, 32, 66));
}

#[test]
fn a_grid_shrinks_its_area_until_the_budget_holds_and_stops_at_the_minimum() {
    let qwen25 = VisionPreset::Qwen25Vl.geometry();
    assert_eq!(
        qwen25.fit_within(1280, 1600, 2000),
        Some(fitted(896, 1120, 1282))
    );
    assert_eq!(
        qwen25.fit_within(1280, 1600, 642),
        Some(fitted(616, 784, 618))
    );
    assert_eq!(qwen25.fit_within(1280, 1600, 6), Some(fitted(56, 56, 6)));
    assert_eq!(qwen25.min_tokens(), 6);
    assert_eq!(qwen25.fit_within(1280, 1600, 5), None);
}

#[test]
fn a_fixed_encoder_always_costs_the_same_and_cannot_shrink() {
    let gemma = VisionPreset::GemmaSiglip.geometry();
    assert_eq!(gemma.plan(1280, 1600), fitted(716, 896, 256));
    assert_eq!(gemma.plan(400, 300), fitted(400, 300, 256));
    assert_eq!(
        gemma.fit_within(1280, 1600, 256),
        Some(fitted(716, 896, 256))
    );
    assert_eq!(gemma.fit_within(1280, 1600, 255), None);
    assert_eq!((gemma.min_tokens(), gemma.max_tokens()), (256, 256));
}

#[test]
fn tiles_charge_the_overview_plus_each_tile_up_to_the_tile_limit() {
    let openai = VisionPreset::OpenAi.geometry();
    assert_eq!(openai.plan(1024, 1024), fitted(1024, 1024, 765));
    assert_eq!(openai.plan(4096, 4096), fitted(2048, 2048, 2805));
    assert_eq!(openai.max_tokens(), 2805);
    let gemini = VisionPreset::Gemini.geometry();
    assert_eq!(gemini.plan(3072, 3072), fitted(3072, 3072, 1548));
    assert_eq!(gemini.plan(300, 200), fitted(300, 200, 258));
}

#[test]
fn tiles_shrink_down_to_a_single_tile() {
    let openai = VisionPreset::OpenAi.geometry();
    assert_eq!(
        openai.fit_within(1024, 1024, 425),
        Some(fitted(512, 512, 255))
    );
    assert_eq!(openai.min_tokens(), 255);
    assert_eq!(openai.fit_within(1024, 1024, 254), None);
}

#[test]
fn the_conservative_default_charges_at_least_the_local_families_and_openai_for_its_square() {
    let default = VisionGeometry::CONSERVATIVE_DEFAULT;
    assert_eq!(default.max_tokens(), 1026);
    for side in [896, 1792, 4000] {
        let sent = default.plan(side, side);
        assert_eq!(sent, fitted(896, 896, 1026));
        for preset in [
            VisionPreset::Qwen3Vl,
            VisionPreset::Qwen25Vl,
            VisionPreset::GemmaSiglip,
            VisionPreset::OpenAi,
        ] {
            let cost = preset.geometry().plan(sent.width, sent.height).tokens;
            assert!(cost <= sent.tokens, "{preset:?} charges {cost}");
        }
    }
}

#[test]
fn cloud_models_see_by_their_providers_documented_families() {
    let openai = Some(VisionPreset::OpenAi.geometry());
    assert_eq!(
        cloud_vision_default(CloudProvider::Openai, "gpt-4o-mini"),
        openai
    );
    assert_eq!(cloud_vision_default(CloudProvider::Openai, "gpt-5"), openai);
    assert_eq!(
        cloud_vision_default(CloudProvider::Openai, "o3-mini"),
        openai
    );
    assert_eq!(
        cloud_vision_default(CloudProvider::Openai, "gpt-3.5-turbo"),
        None
    );
    assert_eq!(
        cloud_vision_default(CloudProvider::Anthropic, "claude-sonnet-4-5"),
        Some(VisionPreset::Anthropic.geometry())
    );
    assert_eq!(
        cloud_vision_default(CloudProvider::Google, "gemini-2.5-pro"),
        Some(VisionPreset::Gemini.geometry())
    );
    assert_eq!(
        cloud_vision_default(CloudProvider::Google, "gemma-3-27b"),
        None
    );
}

#[test]
fn presets_are_valid_and_degenerate_declarations_are_named() {
    for preset in VisionPreset::ALL {
        assert_eq!(preset.geometry().problem(), None, "{}", preset.label());
    }
    let grid = |token_side, min_pixels, max_pixels| VisionGeometry::Grid {
        token_side,
        min_pixels,
        max_pixels,
        overhead_tokens: 0,
    };
    assert!(grid(0, 0, 100).problem().is_some());
    assert!(grid(28, 200, 100).problem().is_some());
    assert!(VisionGeometry::Fixed { side: 0, tokens: 1 }
        .problem()
        .is_some());
    assert!(VisionGeometry::Tiles {
        tile_side: 0,
        tokens_per_tile: 1,
        base_tokens: 0,
        max_tiles: 1,
        max_side: 1,
    }
    .problem()
    .is_some());
}

#[test]
fn a_geometry_serializes_with_its_kind_tag() {
    let json = serde_json::to_value(VisionPreset::GemmaSiglip.geometry()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "kind": "fixed", "side": 896, "tokens": 256 })
    );
}
