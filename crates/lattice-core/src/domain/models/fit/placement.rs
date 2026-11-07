use super::companions::Companions;
use super::machine::{Accelerator, AcceleratorKind, MachineProfile};
use super::verdict::{DeviceKind, FitDevice};

/// What the desktop and other programs keep on a card.
pub const DISPLAY_RESERVE_BYTES: u64 = 1 << 30;

/// The share of system RAM a processor-only model may use: 80% of the 70% the catalog has always
/// treated as usable.
const PROCESSOR_RAM_SHARE: f64 = 0.56;

/// Where a chat model will live, and how much memory it may use there.
pub trait Placement: std::fmt::Debug {
    /// Bytes a chat model may need in full.
    fn budget_bytes(&self) -> u64;
    /// The device the budget belongs to.
    fn device(&self) -> FitDevice;
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
        self.card
            .total_bytes
            .saturating_sub(DISPLAY_RESERVE_BYTES)
            .saturating_sub(self.companions_on_card_bytes)
    }
    fn device(&self) -> FitDevice {
        FitDevice {
            name: self.card.name.clone(),
            kind: DeviceKind::Dedicated,
            total_bytes: self.card.total_bytes,
        }
    }
}

impl Placement for UnifiedMemory {
    fn budget_bytes(&self) -> u64 {
        self.device
            .total_bytes
            .saturating_sub(self.companions_bytes)
    }
    fn device(&self) -> FitDevice {
        FitDevice {
            name: self.device.name.clone(),
            kind: DeviceKind::Unified,
            total_bytes: self.device.total_bytes,
        }
    }
}

impl Placement for ProcessorOnly {
    fn budget_bytes(&self) -> u64 {
        (self.system_ram_bytes as f64 * PROCESSOR_RAM_SHARE) as u64
    }
    fn device(&self) -> FitDevice {
        FitDevice {
            name: "the processor".into(),
            kind: DeviceKind::Processor,
            total_bytes: self.system_ram_bytes,
        }
    }
}

/// Picks the strategy for a machine: a dedicated card, else unified memory, else the processor.
/// An integrated GPU alone draws on system RAM, so it counts as the processor.
pub fn choose_placement(profile: &MachineProfile, companions: &Companions) -> Box<dyn Placement> {
    let largest = |kind: AcceleratorKind| {
        profile
            .accelerators
            .iter()
            .filter(|device| device.kind == kind)
            .max_by_key(|device| device.total_bytes)
    };
    if let Some(card) = largest(AcceleratorKind::Dedicated) {
        let spare_integrated = largest(AcceleratorKind::Integrated).is_some();
        return Box::new(DedicatedGpu {
            card: card.clone(),
            companions_on_card_bytes: if spare_integrated {
                0
            } else {
                companions.bytes()
            },
        });
    }
    if let Some(device) = largest(AcceleratorKind::Unified) {
        return Box::new(UnifiedMemory {
            device: device.clone(),
            companions_bytes: companions.bytes(),
        });
    }
    Box::new(ProcessorOnly {
        system_ram_bytes: profile.system_ram_bytes,
    })
}
