//! The one subscriber of a test process, carrying every test observer as a layer.

#![allow(clippy::expect_used)]

use std::sync::OnceLock;
use tracing_subscriber::prelude::*;

pub(super) fn install() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(super::query_counts::layer())
                .with(super::logs::layer()),
        )
        .expect("the test subscriber is installed once");
    });
}
