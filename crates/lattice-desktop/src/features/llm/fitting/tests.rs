//! The reply ceiling a backend sends: what it wants, then what the window leaves.

use super::*;
use lattice_core::application::services::model_contract::legacy;

fn request(cap: Option<u32>) -> CompletionRequest {
    legacy::completion(legacy::Completion {
        max_output_tokens: cap,
        ..Default::default()
    })
}

fn wanted(cap: Option<u32>, model: Option<u32>, reasons: bool) -> Option<u32> {
    let thinking = if reasons { Thinking::On } else { Thinking::Off };
    OutputLimits::of(model).wanted(&request(cap), thinking)
}

#[test]
fn a_request_cap_tightens_the_model_ceiling_but_never_raises_it() {
    assert_eq!(wanted(None, Some(4096), false), Some(4096));
    assert_eq!(wanted(Some(512), Some(4096), false), Some(512));
    assert_eq!(wanted(Some(100_000), Some(4096), false), Some(4096));
    // Zero would produce an empty reply, so it counts as unset.
    assert_eq!(wanted(Some(0), Some(4096), false), Some(4096));
}

#[test]
fn no_ceiling_is_made_up_when_none_is_known() {
    assert_eq!(wanted(None, None, false), None);
    assert_eq!(wanted(None, None, true), None);
    assert_eq!(wanted(Some(0), None, false), None);
    assert_eq!(wanted(Some(512), None, false), Some(512));
}

#[test]
fn a_model_that_reasons_gets_its_whole_ceiling_whatever_the_request_caps() {
    assert_eq!(wanted(Some(512), Some(4096), true), Some(4096));
}

#[test]
fn a_model_that_may_reason_is_not_cut_by_the_request_cap_either() {
    let limits = OutputLimits::known(4096);
    assert_eq!(
        limits.wanted(&request(Some(512)), Thinking::Unknown),
        Some(4096)
    );
}

#[test]
fn the_room_the_window_leaves_cuts_the_ceiling() {
    assert_eq!(cut(wanted(Some(300), Some(4096), false), 200), Some(200));
    assert_eq!(cut(wanted(Some(300), Some(4096), true), 200), Some(200));
    assert_eq!(cut(wanted(Some(300), Some(4096), false), 10_000), Some(300));
    assert_eq!(cut(wanted(Some(300), None, false), 10_000), Some(300));
    assert_eq!(cut(wanted(None, None, false), 50), None);
}

#[test]
fn response_budget_scales_with_output_tokens_from_small_ceilings_to_large_windows() {
    let small = ResponseBudget::for_tokens(16);
    assert_eq!(small.max_token_events, 16);
    assert_eq!(small.max_tool_calls, 4);
    assert!(small.max_event_bytes < 64 * 1024);
    assert!(small.max_stream_bytes > small.max_event_bytes);

    let large = ResponseBudget::for_tokens(128_000);
    assert_eq!(large.max_token_events, 128_000);
    assert_eq!(large.max_tool_calls, 32_000);
    assert!(large.max_stream_bytes > 62 * 1024 * 1024);
    assert!(large.max_event_bytes > 16 * 1024 * 1024);

    // Zero clamps to 1 so even degenerate caps keep a floor envelope.
    let zero = ResponseBudget::for_tokens(0);
    assert_eq!(zero.max_token_events, 1);
    assert_eq!(zero.max_tool_calls, 1);
}
