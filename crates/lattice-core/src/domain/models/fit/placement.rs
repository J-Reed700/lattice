use super::companions::Companions;
use super::machine::{Accelerator, MachineProfile};

/// What the desktop and other programs keep on a card.
pub const DISPLAY_RESERVE_BYTES: u64 = 1 << 30;

/// Where a chat model will live, and how much memory it may use there.
pub trait Placement: std::fmt::Debug {
    /// Bytes a chat model may need in full.
    fn budget_bytes(&self) -> u64;
    /// The device the budget belongs to, for display.
    fn device_name(&self) -> String;
    /// The device's total memory, of which the budget is a share.
    fn device_total_bytes(&self) -> u64;
    /// Whether the model runs on the processor, which is slow.
    fn is_slow(&self) -> bool;
}

/// A dedicated card: its memory less the display reserve, less the companions when no integrated GPU
/// takes them.
#[derive(Debug, Clone)]
pub struct DedicatedGpu {
    pub card: Accelerator,
    pub companions_on_card_bytes: u64,
}

/// Apple Silicon: the Metal device's working-set size, less the companions.
#[derive(Debug, Clone)]
pub struct UnifiedMemory {
    pub device: Accelerator,
    pub companions_bytes: u64,
}

/// No usable accelerator: a share of system RAM, marked slow.
#[derive(Debug, Clone)]
pub struct ProcessorOnly {
    pub system_ram_bytes: u64,
}

impl Placement for DedicatedGpu {
    fn budget_bytes(&self) -> u64 {
        todo!("stage 2")
    }
    fn device_total_bytes(&self) -> u64 {
        todo!("stage 2")
    }
    fn device_name(&self) -> String {
        todo!("stage 2")
    }
    fn is_slow(&self) -> bool {
        todo!("stage 2")
    }
}

impl Placement for UnifiedMemory {
    fn budget_bytes(&self) -> u64 {
        todo!("stage 2")
    }
    fn device_total_bytes(&self) -> u64 {
        todo!("stage 2")
    }
    fn device_name(&self) -> String {
        todo!("stage 2")
    }
    fn is_slow(&self) -> bool {
        todo!("stage 2")
    }
}

impl Placement for ProcessorOnly {
    fn budget_bytes(&self) -> u64 {
        todo!("stage 2")
    }
    fn device_total_bytes(&self) -> u64 {
        todo!("stage 2")
    }
    fn device_name(&self) -> String {
        todo!("stage 2")
    }
    fn is_slow(&self) -> bool {
        todo!("stage 2")
    }
}

/// Picks the strategy for a machine: a dedicated card, else unified memory, else the processor.
pub fn choose_placement(_profile: &MachineProfile, _companions: &Companions) -> Box<dyn Placement> {
    todo!("stage 2")
}
