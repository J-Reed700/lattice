#[deprecated(
    since = "1.0.0",
    note = "Use crate::infrastructure::crash::install_panic_hook() instead"
)]
pub fn install_panic_handler() {
    crate::infrastructure::crash::install_panic_hook();
}
