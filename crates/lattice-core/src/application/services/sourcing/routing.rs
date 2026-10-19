//! The router: turns a need into waves of finder calls (design §3.4).
//!
//! Pure. Which finders serve a kind, and in which wave, is the finders' own
//! descriptor data (`Role`); this file holds only the part of §3.4 that is
//! routing policy: where the general web engines sit in each kind's recipe,
//! with what scope, and which waves use the caller's rewordings.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::application::contracts::settings::SourcingSettingsDto;
use crate::domain::sourcing::{
    Cost, FinderDescriptor, FinderId, Freshness, KeyService, NeedKind, Reach, SkipReason,
    SourceNeed, WaveSlot,
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
    /// the user's SearXNG, Marginalia, and the paid engines unless they
    /// lead, see `is_paid_engine`).
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
        // Paid engines that can run go ahead of all of them.
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

/// A general web engine the user pays for. When one of them can run, the
/// paid engines lead: they get a wave of their own ahead of the first wave
/// that runs engines, and no later wave calls them again, so the free
/// engines run only if the paid ones come up thin. When none can run, the
/// free engines lead.
fn is_paid_engine(descriptor: &FinderDescriptor) -> bool {
    descriptor.slot_for(NeedKind::Web).is_some() && descriptor.cost != Cost::Free
}

/// An owner-scoped engine call needs a native site filter; filtering an
/// unscoped list afterwards would leave nothing.
fn engine_reaches(descriptor: &FinderDescriptor, scope: StepScope) -> bool {
    scope == StepScope::AsAsked || descriptor.supports_scope
}

/// What earlier waves already did: the steps they made and the skips they
/// reported.
#[derive(Default)]
struct Planned {
    steps: HashSet<(FinderId, StepScope, bool)>,
    reported: HashSet<FinderId>,
}

impl Planned {
    /// A wave of the candidates the gate lets run, leaving out steps an
    /// earlier wave made; `None` when it has nothing to run or report.
    fn wave<'a>(
        &mut self,
        candidates: impl IntoIterator<Item = (&'a FinderDescriptor, StepScope)>,
        reformulated: bool,
        uses_alternatives: bool,
        gate: &FinderGate<'_>,
    ) -> Option<Wave> {
        let mut wave = Wave {
            steps: Vec::new(),
            skipped: Vec::new(),
            reformulated,
        };
        for (descriptor, scope) in candidates {
            if let Some(reason) = gate.check(descriptor) {
                if self.reported.insert(descriptor.id.clone()) {
                    wave.skipped.push((descriptor.id.clone(), reason));
                }
                continue;
            }
            if self
                .steps
                .insert((descriptor.id.clone(), scope, uses_alternatives))
            {
                wave.steps.push(Step {
                    finder: descriptor.id.clone(),
                    scope,
                });
            }
        }
        (!wave.steps.is_empty() || !wave.skipped.is_empty()).then_some(wave)
    }
}

/// Plans the waves for a need over the registry's descriptors (in registry
/// order). Steps an earlier wave already made (same finder, scope and query)
/// are not repeated, and a wave left with nothing to run or report is
/// dropped. A paid engine that can run puts the paid engines first
/// (`is_paid_engine`).
pub fn plan(need: &SourceNeed, registry: &[FinderDescriptor], gate: &FinderGate<'_>) -> Plan {
    let kind = routed_kind(need);
    let owner_known = need.subject.is_some() || need.scope.is_some();
    let recipes = recipe(kind, owner_known);
    let has_alternatives = !need.alternatives.is_empty();
    let ecosystem = need.ecosystem();
    let paid_lead = registry
        .iter()
        .any(|descriptor| is_paid_engine(descriptor) && gate.check(descriptor).is_none());

    let mut planned = Planned::default();
    let mut paid_placed = false;
    let mut waves = Vec::new();

    for (slot, recipe) in WaveSlot::ALL.into_iter().zip(recipes) {
        // Each candidate with whether it is a general web engine.
        let mut candidates: Vec<(&FinderDescriptor, StepScope, bool)> = Vec::new();

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
                candidates.push((descriptor, scope, serving == NeedKind::Web));
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
                if is_engine && engine_reaches(descriptor, scope) {
                    candidates.push((descriptor, scope, true));
                }
            }
        }

        let uses_alternatives = recipe.reformulated && has_alternatives;
        if paid_lead {
            candidates.retain(|(descriptor, _, engine)| !(*engine && is_paid_engine(descriptor)));
            let first_engine_scope = candidates
                .iter()
                .find(|(_, _, engine)| *engine)
                .map(|(_, scope, _)| *scope);
            if let (false, Some(scope)) = (paid_placed, first_engine_scope) {
                paid_placed = true;
                let paid = registry
                    .iter()
                    .filter(|descriptor| {
                        is_paid_engine(descriptor) && engine_reaches(descriptor, scope)
                    })
                    .map(|descriptor| (descriptor, scope));
                waves.extend(planned.wave(paid, recipe.reformulated, uses_alternatives, gate));
            }
        }
        let candidates = candidates
            .into_iter()
            .map(|(descriptor, scope, _)| (descriptor, scope));
        waves.extend(planned.wave(candidates, recipe.reformulated, uses_alternatives, gate));
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
