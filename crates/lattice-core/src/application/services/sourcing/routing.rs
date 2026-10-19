//! The router: turns a need into waves of finder calls (design §3.4).
//!
//! Pure. Which finders serve a kind, and in which wave, is the finders' own
//! descriptor data (`Role`); this file holds only the part of §3.4 that is
//! routing policy: where the general web engines sit in each kind's recipe,
//! with what scope, and which waves use the caller's rewordings.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::application::contracts::settings::SourcingSettingsDto;
use crate::domain::sourcing::{
    FinderDescriptor, FinderId, Freshness, KeyService, NeedKind, Reach, SkipReason, SourceNeed,
    WaveSlot,
};

/// The calls a need makes, wave by wave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Resolve the subject's owner before wave 1 (owner-first routing): for
    /// official docs, and for a package named without its registry, whose
    /// records are then kept only when they sit on the owner's origins.
    pub resolve_owner: bool,
    pub waves: Vec<Wave>,
}

/// One wave: finders run together, then fusion decides whether the next
/// wave is worth paying for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wave {
    pub steps: Vec<Step>,
    /// Finders this wave would have run and why it cannot, each listed once
    /// in the first wave that wanted it. Never silent.
    pub skipped: Vec<(FinderId, SkipReason)>,
    /// Steps use the caller's `alternatives` instead of its query, when it
    /// gave any.
    pub reformulated: bool,
}

/// One finder call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub finder: FinderId,
    pub scope: StepScope,
}

/// The scope a step searches within.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepScope {
    /// The need's own scope, if it has one.
    AsAsked,
    /// The subject owner's origin: resolved before wave 1, or asserted by an
    /// earlier wave's leads (a registry homepage, a standards body). The
    /// runner skips the step as not applicable when no owner is known.
    Owner,
}

/// What may run: the user's settings, the keys in the keyring, and the
/// finders the runner's health refuses right now (open breakers, spent
/// allowances).
#[derive(Debug, Clone, Copy)]
pub struct FinderGate<'a> {
    pub settings: &'a SourcingSettingsDto,
    pub keys: &'a BTreeSet<KeyService>,
    pub refused: &'a BTreeMap<FinderId, SkipReason>,
}

impl FinderGate<'_> {
    /// Why this finder cannot run now, or `None` when it can.
    pub fn check(&self, descriptor: &FinderDescriptor) -> Option<SkipReason> {
        self.settings
            .gate(descriptor, self.keys)
            .or_else(|| self.refused.get(&descriptor.id).cloned())
    }
}

/// Where the general web engines join a kind's wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engines {
    None,
    /// The keyless engines (finders with a `Web` role in the first wave).
    Keyless(StepScope),
    /// The opt-in engines (finders with a `Web` role in the second wave:
    /// keyed engines and the user's SearXNG).
    OptIn(StepScope),
}

#[derive(Debug, Clone, Copy)]
struct WaveRecipe {
    engines: Engines,
    reformulated: bool,
}

const fn wave(engines: Engines, reformulated: bool) -> WaveRecipe {
    WaveRecipe {
        engines,
        reformulated,
    }
}

const NO_ENGINES: WaveRecipe = wave(Engines::None, false);

/// §3.4's engine placements per kind. Each wave also runs the finders whose
/// descriptor gives them that kind and wave.
fn recipe(kind: NeedKind, owner_known: bool) -> [WaveRecipe; 3] {
    use Engines::{Keyless, OptIn};
    use StepScope::{AsAsked, Owner};
    match kind {
        // Wave 1 DDG + Bing, wave 2 the opt-in engines (their own roles),
        // wave 3 the keyless engines again with the caller's rewordings.
        NeedKind::Web => [NO_ENGINES, NO_ENGINES, wave(Keyless(AsAsked), true)],
        NeedKind::OfficialDocs if owner_known => [
            wave(Keyless(Owner), false),
            wave(Keyless(AsAsked), true),
            wave(OptIn(AsAsked), true),
        ],
        // No subject and no scope: nothing to be owner-first about.
        NeedKind::OfficialDocs => [
            wave(Keyless(AsAsked), false),
            wave(OptIn(AsAsked), false),
            wave(Keyless(AsAsked), true),
        ],
        NeedKind::Package => [NO_ENGINES, NO_ENGINES, wave(Keyless(Owner), false)],
        NeedKind::Standard => [
            NO_ENGINES,
            wave(Keyless(Owner), false),
            wave(Keyless(AsAsked), false),
        ],
        NeedKind::Paper | NeedKind::Law | NeedKind::Community => {
            [NO_ENGINES, NO_ENGINES, wave(Keyless(AsAsked), false)]
        }
        NeedKind::Encyclopedia | NeedKind::Archive | NeedKind::Leads => {
            [NO_ENGINES, NO_ENGINES, NO_ENGINES]
        }
    }
}

/// The kind whose recipe a need follows: a dated read (`AsOf`) goes to the
/// archive finders whatever its kind.
pub fn routed_kind(need: &SourceNeed) -> NeedKind {
    match need.freshness {
        Freshness::AsOf(_) => NeedKind::Archive,
        Freshness::Any | Freshness::After(_) => need.kind,
    }
}

/// Plans the waves for a need over the registry's descriptors (in registry
/// order). Steps an earlier wave already made (same finder, scope and query)
/// are not repeated, and a wave left with nothing to run or report is
/// dropped.
pub fn plan(need: &SourceNeed, registry: &[FinderDescriptor], gate: &FinderGate<'_>) -> Plan {
    let kind = routed_kind(need);
    let owner_known = need.subject.is_some() || need.scope.is_some();
    let recipes = recipe(kind, owner_known);
    let has_alternatives = !need.alternatives.is_empty();
    let ecosystem = need.ecosystem();

    let mut planned: HashSet<(FinderId, StepScope, bool)> = HashSet::new();
    let mut reported: HashSet<FinderId> = HashSet::new();
    let mut waves = Vec::new();

    for (slot, recipe) in WaveSlot::ALL.into_iter().zip(recipes) {
        let mut candidates: Vec<(&FinderDescriptor, StepScope)> = Vec::new();

        let mut kinds = vec![kind];
        if slot == WaveSlot::First {
            kinds.extend(need.also.iter().copied().filter(|also| *also != kind));
        }
        for serving in kinds {
            for descriptor in registry {
                if descriptor.slot_for(serving) != Some(slot) {
                    continue;
                }
                if let (Reach::Registry(serves), Some(wanted)) = (descriptor.reaches, ecosystem) {
                    if serving == NeedKind::Package && serves != wanted {
                        continue; // that registry, exactly
                    }
                }
                let scope = if descriptor.reaches == Reach::OwnerSites {
                    StepScope::Owner
                } else {
                    StepScope::AsAsked
                };
                if scope == StepScope::Owner && kind == NeedKind::OfficialDocs && !owner_known {
                    continue;
                }
                candidates.push((descriptor, scope));
            }
        }

        let engines = match recipe.engines {
            Engines::None => None,
            Engines::Keyless(scope) => Some((WaveSlot::First, scope)),
            Engines::OptIn(scope) => Some((WaveSlot::Second, scope)),
        };
        if let Some((engine_slot, scope)) = engines {
            for descriptor in registry {
                let is_engine = descriptor.slot_for(NeedKind::Web) == Some(engine_slot);
                // An owner-scoped engine call needs a native site filter;
                // filtering an unscoped list afterwards would leave nothing.
                if is_engine && (scope == StepScope::AsAsked || descriptor.supports_scope) {
                    candidates.push((descriptor, scope));
                }
            }
        }

        let uses_alternatives = recipe.reformulated && has_alternatives;
        let mut wave = Wave {
            steps: Vec::new(),
            skipped: Vec::new(),
            reformulated: recipe.reformulated,
        };
        for (descriptor, scope) in candidates {
            if let Some(reason) = gate.check(descriptor) {
                if reported.insert(descriptor.id.clone()) {
                    wave.skipped.push((descriptor.id.clone(), reason));
                }
                continue;
            }
            if planned.insert((descriptor.id.clone(), scope, uses_alternatives)) {
                wave.steps.push(Step {
                    finder: descriptor.id.clone(),
                    scope,
                });
            }
        }
        if !wave.steps.is_empty() || !wave.skipped.is_empty() {
            waves.push(wave);
        }
    }

    let package_without_registry = kind == NeedKind::Package
        && need
            .package()
            .is_some_and(|package| package.ecosystem.is_none());
    Plan {
        resolve_owner: need.subject.is_some()
            && (kind == NeedKind::OfficialDocs || package_without_registry),
        waves,
    }
}

#[cfg(test)]
#[path = "routing_tests.rs"]
mod tests;
