//! The claim's standing scope (claim sources design §7.3): the course's
//! versions worth reading before a claim waits for the user, each with the
//! level a stored fact already gives it (its proxy), read from the trust
//! store and the library without a model. One ownership judgement may be
//! made for a subject the store has none for ([`Judgements::judge_owner`]);
//! nothing else here calls a model.

use super::*;
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use lattice_core::application::services::source_trust::OWNERSHIP_POLICY;
use lattice_core::domain::provenance::{
    work_level, EditorialProcess, JudgementKey, RiskFlag, SiteStanding, Standing, SubjectRelation,
    TldRegistry,
};
use std::collections::HashSet;

/// Flags that say a work is not what it looks like: its content evidence
/// gives it no level (the §5.3 ladder's exclusions).
const NOT_WHAT_IT_LOOKS: [RiskFlag; 3] = [
    RiskFlag::ContentFarmPattern,
    RiskFlag::GeneratedContentPattern,
    RiskFlag::InjectionAttempt,
];

/// Course versions worth reading for a claim's standing, by proxy.
#[derive(Debug, Clone, Default)]
pub(in crate::features::learning) struct StandingScope {
    /// Course versions worth reading for standing, minus what the claim was
    /// shown.
    pub(in crate::features::learning) candidates: HashSet<String>,
    /// Each candidate's proxy level, for the ceiling; a candidate read only
    /// because its host is owner-hinted has none.
    pub(in crate::features::learning) proxies: HashMap<String, TrustLevel>,
    /// Hosts where the subject's owner is believed to publish, from the
    /// stored ownership judgements for the subject.
    pub(in crate::features::learning) owner_hosts: Vec<String>,
}

/// One live course version as the scope reads it.
pub(super) struct CourseVersion {
    version_id: String,
    source_id: String,
    added_by: AddedBy,
    /// `None` when capture recorded no provenance for the version.
    origin: Option<Origin>,
    /// The stored `source-standing-v2` judgement of the version, if made.
    standing: Option<StandingAssessment>,
}

impl Judgements {
    /// Whether the store holds any ownership judgement for `profile`'s
    /// subject under this pass's model.
    pub(in crate::features::learning) async fn owner_judged(
        &self,
        profile: &ClaimProfile,
    ) -> Result<bool> {
        Ok(!self.subject_authorities(profile).await?.is_empty())
    }

    /// The scope's one model call (§7.3): who owns `profile`'s subject,
    /// judged from `version`'s origin once per course, origin and subject.
    /// Its owner hints are what the scope reads; the gate reuses the row
    /// when that origin supports anything.
    pub(in crate::features::learning) async fn judge_owner(
        &self,
        llm: &dyn LLMPort,
        version: &str,
        profile: &ClaimProfile,
    ) -> Result<()> {
        let Some(subject) = profile.authority_subject() else {
            return Ok(());
        };
        let Some(provenance) = self.store.provenance(version).await? else {
            return Ok(());
        };
        if provenance.origin.host.is_some() {
            self.authority(Some(llm), &provenance, &subject).await?;
        }
        Ok(())
    }

    /// `profile`'s standing scope, without `shown` (the claim's supports and
    /// everything it was shown): every course version a stored fact says
    /// stands on the subject, never one the user distrusts.
    pub(in crate::features::learning) async fn standing_scope(
        &mut self,
        profile: &ClaimProfile,
        shown: &[String],
    ) -> Result<StandingScope> {
        let subject = profile.authority_subject();
        let authorities = self.subject_authorities(profile).await?;
        let owners: HashSet<&str> = authorities
            .iter()
            .filter(|judged| judged.relation == SubjectRelation::Owner)
            .map(|judged| judged.origin_key.0.as_str())
            .collect();
        let mut owner_hosts: Vec<String> = Vec::new();
        for hint in authorities
            .iter()
            .flat_map(|judged| &judged.owner_origin_hints)
        {
            let hint = hint.trim().trim_end_matches('.').to_ascii_lowercase();
            if !hint.is_empty() && !owner_hosts.contains(&hint) {
                owner_hosts.push(hint);
            }
        }
        let mut scope = StandingScope::default();
        for version in self.course_versions().await?.iter() {
            if shown.contains(&version.version_id) || self.distrusted(version) {
                continue;
            }
            let hinted = version
                .origin
                .as_ref()
                .and_then(|origin| origin.host.as_deref())
                .is_some_and(|host| {
                    owner_hosts
                        .iter()
                        .any(|hint| host == hint || host.ends_with(&format!(".{hint}")))
                });
            let owner = version
                .origin
                .as_ref()
                .is_some_and(|origin| owners.contains(origin.key.0.as_str()));
            let level = [
                owner.then_some(TrustLevel::Primary),
                version
                    .standing
                    .as_ref()
                    .and_then(|standing| standing_proxy(version, standing)),
                (version.added_by == AddedBy::User).then_some(TrustLevel::Established),
                version
                    .origin
                    .as_ref()
                    .filter(|origin| matches!(origin.tld_registry, TldRegistry::Restricted { .. }))
                    .map(|_| TrustLevel::Established),
                self.override_proxy(version, subject.as_ref()),
            ]
            .into_iter()
            .flatten()
            .max();
            if level.is_none() && !hinted {
                continue;
            }
            scope.candidates.insert(version.version_id.clone());
            if let Some(level) = level {
                scope.proxies.insert(version.version_id.clone(), level);
            }
        }
        scope.owner_hosts = owner_hosts;
        Ok(scope)
    }

    /// Every ownership judgement stored for `profile`'s subject.
    async fn subject_authorities(
        &self,
        profile: &ClaimProfile,
    ) -> Result<Vec<AuthorityAssessment>> {
        let Some(subject) = profile.authority_subject() else {
            return Ok(Vec::new());
        };
        self.store
            .authority_for_subject(
                &self.program_id,
                &subject.key,
                OWNERSHIP_POLICY,
                &self.model,
            )
            .await
    }

    /// Whether the user distrusts the version's source or its site.
    fn distrusted(&self, version: &CourseVersion) -> bool {
        let source = self
            .overrides
            .source(&self.program_id, &version.source_id)
            .is_some_and(|(level, _)| level == TrustLevel::Distrusted);
        let site = version
            .origin
            .as_ref()
            .and_then(Origin::site)
            .and_then(|site| self.overrides.site(site))
            .is_some_and(|(standing, _)| standing == SiteStanding::Distrusted);
        source || site
    }

    /// The level an override in force gives the version, when it is
    /// Established or above: a source override, a site said to be
    /// institutional or identified, or a site-for-subject override.
    fn override_proxy(
        &self,
        version: &CourseVersion,
        subject: Option<&ClaimSubject>,
    ) -> Option<TrustLevel> {
        let source = self
            .overrides
            .source(&self.program_id, &version.source_id)
            .map(|(level, _)| level);
        let site = version.origin.as_ref().and_then(Origin::site);
        let standing = site
            .and_then(|site| self.overrides.site(site))
            .and_then(|(standing, _)| match standing {
                SiteStanding::Institutional | SiteStanding::Identified => {
                    Some(TrustLevel::Established)
                }
                SiteStanding::Community | SiteStanding::Distrusted => None,
            });
        let for_subject = site
            .zip(subject)
            .and_then(|(site, subject)| self.overrides.site_subject(site, subject))
            .map(|(level, _, _)| level);
        [source, standing, for_subject]
            .into_iter()
            .flatten()
            .filter(|level| *level >= TrustLevel::Established)
            .max()
    }

    /// The course's live versions with their origins and stored standing,
    /// read once per pass: each version's v2 key is computed from its text,
    /// as the gate computes it, and the stored judgements read by key.
    async fn course_versions(&mut self) -> Result<Arc<Vec<CourseVersion>>> {
        if let Some(course) = &self.course {
            return Ok(course.clone());
        }
        let records: HashMap<String, ProvenanceRecord> = self
            .store
            .program_provenance(&self.program_id)
            .await?
            .into_iter()
            .map(|record| (record.source_version_id.clone(), record))
            .collect();
        let library = LearningSourceLibraryRepository::new(self.pool.clone());
        let mut versions = Vec::new();
        let mut keys: Vec<JudgementKey> = Vec::new();
        for chosen in library.chosen_versions(&self.program_id).await? {
            let added_by = AddedBy::from_column(chosen.added_by.as_str()).ok_or_else(|| {
                AppError::InternalError(format!(
                    "Unknown source origin \"{}\".",
                    chosen.added_by.as_str()
                ))
            })?;
            let record = records.get(&chosen.version_id);
            let key = record.map(|record| {
                let content = ContentEvidence::of(
                    &chosen.text,
                    &record.links,
                    &record.origin,
                    Chosen {
                        by: added_by,
                        note: chosen.selection_note.clone(),
                    },
                );
                standing_key(&record.signals, &content, &self.model)
            });
            keys.extend(key.clone());
            versions.push((
                chosen,
                added_by,
                record.map(|record| record.origin.clone()),
                key,
            ));
        }
        let stored: HashMap<String, StandingAssessment> = self
            .store
            .standings(&keys)
            .await?
            .into_iter()
            .map(|standing| (standing.signals_sha256.clone(), standing))
            .collect();
        let course = Arc::new(
            versions
                .into_iter()
                .map(|(chosen, added_by, origin, key)| CourseVersion {
                    version_id: chosen.version_id,
                    source_id: chosen.source_id,
                    added_by,
                    origin,
                    standing: key.and_then(|key| stored.get(&key.input_sha256).cloned()),
                })
                .collect(),
        );
        self.course = Some(Arc::clone(&course));
        Ok(course)
    }
}

/// The level a stored v2 standing judgement gives a version, without its
/// subject: an editorial institution Authoritative, any other identified
/// publisher Established, an unidentified publisher's work by the §5.3
/// ladder when the course chose it.
fn standing_proxy(version: &CourseVersion, standing: &StandingAssessment) -> Option<TrustLevel> {
    match (standing.standing, standing.editorial) {
        (_, EditorialProcess::CommunityEdited) | (Standing::Community, _) => None,
        (Standing::Institutional, EditorialProcess::PeerReviewed | EditorialProcess::Editorial) => {
            Some(TrustLevel::Authoritative)
        }
        (Standing::Institutional | Standing::Identified, _) => Some(TrustLevel::Established),
        (Standing::Unknown, _) => {
            let chosen = matches!(
                version.added_by,
                AddedBy::Research | AddedBy::Outline | AddedBy::Import
            );
            let excluded = standing
                .flags
                .iter()
                .any(|flag| NOT_WHAT_IT_LOOKS.contains(flag));
            (chosen && !excluded)
                .then(|| work_level(standing.nature, &standing.work))
                .flatten()
        }
    }
}
