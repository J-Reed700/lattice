//! Whether the sweeps that delete files by the absence of a row may run.

use std::sync::atomic::{AtomicBool, Ordering};

/// Shut until startup finds that this database owns the files under the
/// lattice home. Every sweep that deletes a file because no row names it reads
/// the gate; none reads the database or the owner file again.
#[derive(Debug, Default)]
pub struct SweepGate(AtomicBool);

impl SweepGate {
    /// Let the sweeps run. Done once per start; the gate is never shut again.
    pub fn open(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_open(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_gate_starts_shut_and_opens_for_everyone_who_holds_it() {
        let gate = Arc::new(SweepGate::default());
        let reader = Arc::clone(&gate);
        assert!(!reader.is_open());

        gate.open();

        assert!(reader.is_open());
    }
}
