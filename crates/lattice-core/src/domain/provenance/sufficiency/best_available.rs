//! The bar relative to what the course has (claim sources design §6.1):
//! after the static bar and corroboration fail, a claim is sufficient when
//! its support is the strongest the course has on its subject and nothing
//! at or above it contradicts it. At Community or below it takes two
//! independent origins.

use super::*;

pub(super) fn best_available(
    profile: &ClaimProfile,
    usable: &[&Counted<'_>],
    contrary_levels: &[TrustLevel],
    available: &Available,
    reasons: &mut Vec<Reason>,
) -> Option<Decision> {
    // Nothing considered: the standing reach has not weighed this claim, so
    // the static bars and corroboration decide alone.
    if available.considered == 0 || profile.kind == ClaimKind::Measurement {
        return None;
    }
    let mut candidates: Vec<(&&Counted<'_>, TrustLevel)> = usable
        .iter()
        .filter(|c| matches!(c.support.passage, PassageKind::Direct))
        .filter_map(|c| Some((c, c.level?)))
        .filter(|(_, level)| available.ceiling.is_none_or(|ceiling| *level >= ceiling))
        .collect();
    candidates.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| {
            a.0.support
                .source_version_id
                .cmp(&b.0.support.source_version_id)
        })
    });
    let best = candidates.first()?.1;
    let spec_like = matches!(
        profile.kind,
        ClaimKind::Specification | ClaimKind::VersionBound
    );
    let blocked = contrary_levels.iter().any(|level| *level >= best)
        || (spec_like
            && best < TrustLevel::Primary
            && contrary_levels.contains(&TrustLevel::Primary));
    if blocked {
        reasons.push(Reason::new(
            ReasonCode::ContradictionBlocksCorroboration,
            format!(
                "A source at {} or above contradicts it, so the course's strongest source does not settle it",
                best.as_str()
            ),
        ));
        return None;
    }
    let subject = profile
        .subject
        .as_ref()
        .map_or("this subject".to_owned(), |subject| subject.name.clone());
    let met_by: Vec<String> = if best >= TrustLevel::Established {
        let top = candidates.first()?.0;
        reasons.push(Reason::new(
            ReasonCode::MetAsBestAvailable,
            format!(
                "{} ({}) is the strongest source the course has on {subject}; nothing in the \
                 course stands higher on it ({} sources considered)",
                top.support.source_version_id,
                best.as_str(),
                available.considered
            ),
        ));
        candidates
            .iter()
            .map(|(c, _)| c.support.source_version_id.clone())
            .collect()
    } else {
        let clusters = best_per_cluster(candidates.iter().copied());
        if clusters.len() < CORROBORATING_ORIGINS {
            return None;
        }
        reasons.push(Reason::new(
            ReasonCode::MetAsBestAvailable,
            format!(
                "{} independent origins, the strongest the course has on {subject}",
                clusters.len()
            ),
        ));
        let mut ranked = clusters;
        ranked.sort_by(|a, b| b.1.cmp(&a.1));
        ranked
            .iter()
            .map(|(c, _)| c.support.source_version_id.clone())
            .collect()
    };
    Some(Decision::Sufficient {
        met_by,
        label: Some(Label::BestAvailable { level: best }),
        reasons: std::mem::take(reasons),
    })
}
