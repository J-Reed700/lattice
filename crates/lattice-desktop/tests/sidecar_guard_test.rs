//! Runs the unit tests of the build script's llama-server sidecar guard.
//! Cargo never runs tests inside `build.rs`, so the module is compiled a
//! second time here.

#[allow(dead_code)]
#[path = "../build_support/sidecar_guard.rs"]
mod sidecar_guard;
