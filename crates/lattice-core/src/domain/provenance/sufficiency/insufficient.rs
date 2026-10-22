//! What an insufficient claim lacks and what research may do next (§6.1,
//! claim sources design §6.1 for `StrongerSourceSilent`).

use super::*;

pub(super) fn insufficient(
    profile: &ClaimProfile,
    counted: &[Counted<'_>],
    lowest: Option<TrustLevel>,
    bars: &EffectiveBars,
    available: &Available,
    mut reasons: Vec<Reason>,
) -> Decision {
    let kind = profile.kind;
    let usable: Vec<&Counted<'_>> = counted.iter().filter(|c| c.level.is_some()).collect();
    let clusters_at = |floor: TrustLevel| {
        let mut seen: Vec<&ClusterId> = Vec::new();
        for c in usable.iter().filter(|c| c.level >= Some(floor)) {
            if !seen.contains(&&c.support.origin_cluster) {
                seen.push(&c.support.origin_cluster);
            }
        }
        seen.len()
    };
    let stale: Vec<&str> = usable
        .iter()
        .filter_map(|c| match &c.support.temporal {
            TemporalValidity::Stale { newer_primary } => Some(newer_primary.as_str()),
            _ => None,
        })
        .collect();
    let testimonial_would_meet =
        lowest.is_some_and(|lowest| usable.iter().any(|c| c.reported && c.page >= lowest));
    let best_support = usable.iter().filter_map(|c| c.level).max();
    let stronger_silent = available.ceiling.filter(|ceiling| {
        available.considered > 0 && best_support.is_none_or(|best| *ceiling > best)
    });
    let missing = if !usable.is_empty() && stale.len() == usable.len() {
        Missing::OnlyStale {
            newer_primary: stale.first().map(|s| (*s).to_owned()).unwrap_or_default(),
        }
    } else if testimonial_would_meet {
        Missing::OnlyTestimonial
    } else if let Some(level) = stronger_silent {
        reasons.push(Reason::new(
            ReasonCode::StrongerSourceSilent,
            format!(
                "The course has a {} source on this subject that does not state it",
                level.as_str()
            ),
        ));
        Missing::StrongerSourceSilent { level }
    } else {
        match kind {
            ClaimKind::Specification | ClaimKind::VersionBound => Missing::NoPrimaryForSubject,
            ClaimKind::Measurement => Missing::NoMeasurementOrigin,
            ClaimKind::Historical => Missing::NoIndependentCorroboration {
                have: clusters_at(TrustLevel::Established),
            },
            ClaimKind::Definition | ClaimKind::Practice
                if clusters_at(TrustLevel::Community) == 1 =>
            {
                Missing::NoIndependentCorroboration { have: 1 }
            }
            _ => Missing::NoIdentifiedSource,
        }
    };

    let mut next = Vec::new();
    let mut push = |step: NextStep| {
        if !next.contains(&step) {
            next.push(step);
        }
    };
    for newer in &stale {
        push(NextStep::FetchNewer {
            newer_primary: (*newer).to_owned(),
        });
    }
    let mut by_level: Vec<&&Counted<'_>> = usable.iter().collect();
    by_level.sort_by_key(|c| std::cmp::Reverse(c.page));
    for c in &by_level {
        for url in &c.support.cited_urls {
            push(NextStep::FollowCitation {
                url: url.clone(),
                from: c.support.source_version_id.clone(),
            });
        }
    }
    if matches!(
        kind,
        ClaimKind::Specification | ClaimKind::VersionBound | ClaimKind::Measurement
    ) {
        let mut hints: Vec<String> = Vec::new();
        for c in counted {
            for hint in &c.support.owner_hints {
                if !hints.contains(hint) {
                    hints.push(hint.clone());
                }
            }
        }
        if !hints.is_empty() {
            push(NextStep::SearchOwner {
                origin_hints: hints,
            });
        }
    }
    push(NextStep::SearchIndependent);
    push(NextStep::AskUser);

    let have_levels: Vec<&str> = usable
        .iter()
        .filter_map(|c| c.level.map(TrustLevel::as_str))
        .collect();
    let about = profile
        .subject
        .as_ref()
        .map(|subject| format!(" about {}", subject.name))
        .unwrap_or_default();
    reasons.push(Reason::new(
        ReasonCode::BelowBar,
        format!(
            "{} claim{about}: {}; needs {}",
            capitalised(kind_name(kind)),
            if have_levels.is_empty() {
                "no support that can be counted".to_owned()
            } else {
                format!("supported only by {} sources", have_levels.join(", "))
            },
            requirement(kind, lowest)
        ),
    ));
    reasons.extend(bars.moved_reason(kind));
    Decision::Insufficient {
        bar: Bar::for_kind(kind),
        have: counted
            .iter()
            .map(|c| CountedSupport {
                source_version_id: c.support.source_version_id.clone(),
                origin_cluster: c.support.origin_cluster.clone(),
                counted: c.level,
            })
            .collect(),
        missing,
        next,
        reasons,
    }
}

/// What meets the bar, in words.
pub(super) fn requirement(kind: ClaimKind, lowest: Option<TrustLevel>) -> String {
    let alone = match lowest {
        Some(TrustLevel::Primary) if kind == ClaimKind::Measurement => {
            "the measurement's own source".to_owned()
        }
        Some(TrustLevel::Primary) => "the owner's own source".to_owned(),
        Some(level) => format!("one {} source or better", level.as_str()),
        None => "independent corroboration (calibration accepts no single source)".to_owned(),
    };
    let corroboration = match kind {
        ClaimKind::Specification | ClaimKind::VersionBound | ClaimKind::Historical => {
            " or two independent established sources"
        }
        ClaimKind::Measurement => " or two independent reports naming the same origin",
        ClaimKind::Definition | ClaimKind::Practice => " or two independent community sources",
        ClaimKind::Opinion | ClaimKind::General => "",
    };
    format!("{alone}{corroboration}")
}
