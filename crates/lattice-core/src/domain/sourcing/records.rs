//! When a finder's leads may speak for a need's subject.
//!
//! A registry record names one package's homepage and repository, and a code
//! host's record one repository's. Either speaks for the subject only when
//! the need named that package or repository: a record that matched a
//! search's words is a candidate the caller can read, never the subject's
//! owner.

use super::descriptor::Reach;
use super::lead::Lead;
use super::need::{Identifier, Origin, SourceNeed};

/// The owner a wave's steps assert for the need's subject, taking the first
/// step, in order, that asserts one. A registry asserts only the owner of
/// the package the need names ([`registry_owner`]), a code host only the
/// owner of the repository the need names, and any other finder (a standards
/// body, an encyclopedia) the owner its first lead names.
pub fn asserted_owner<'a, I>(need: &SourceNeed, steps: I) -> Option<Origin>
where
    I: IntoIterator<Item = (Reach, &'a [Lead])>,
    I::IntoIter: Clone,
{
    let mut steps = steps.into_iter();
    let registry = registry_owner(
        need,
        steps
            .clone()
            .filter(|(reach, _)| matches!(reach, Reach::Registry(_)))
            .flat_map(|(_, leads)| leads),
    );
    let names_repository = need.subject.as_ref().is_some_and(|subject| {
        subject
            .identifiers
            .iter()
            .any(|id| matches!(id, Identifier::Repo { .. }))
    });
    steps.find_map(|(reach, leads)| match reach {
        Reach::Registry(_) => leads.first().and(registry.clone()),
        Reach::CodeHost if !names_repository => None,
        _ => leads.iter().find_map(|lead| lead.owner.clone()),
    })
}

/// The owner the registry leads assert for the package `need` names, or
/// `None` when it names none or the records disagree. The leads must be the
/// named package's records only (the finders keep no other when a package
/// is named). With its registry named, the record is the package's. Without
/// it, a name that several registries hold under different owners
/// identifies none of them.
pub fn registry_owner<'a>(
    need: &SourceNeed,
    leads: impl IntoIterator<Item = &'a Lead>,
) -> Option<Origin> {
    let package = need.package()?;
    let mut owners = leads.into_iter().filter_map(|lead| lead.owner.as_ref());
    let first = owners.next()?;
    if package.ecosystem.is_some() {
        return Some(first.clone());
    }
    owners.all(|owner| owner == first).then(|| first.clone())
}

/// The registry records that sit on one of the subject owner's `origins`:
/// a record whose homepage, repository or registry page an origin covers,
/// or whose asserted owner is one of them. A record's leads share its
/// asserted owner, so they are kept or dropped together.
pub fn records_on_owner(leads: Vec<Lead>, origins: &[Origin]) -> Vec<Lead> {
    let on_owner = |lead: &Lead| {
        origins
            .iter()
            .any(|origin| origin.covers(&lead.url) || lead.owner.as_ref() == Some(origin))
    };
    let kept: Vec<Origin> = leads
        .iter()
        .filter(|lead| on_owner(lead))
        .filter_map(|lead| lead.owner.clone())
        .collect();
    leads
        .into_iter()
        .filter(|lead| on_owner(lead) || lead.owner.as_ref().is_some_and(|o| kept.contains(o)))
        .collect()
}

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;
