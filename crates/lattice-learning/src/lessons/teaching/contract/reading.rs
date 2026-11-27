//! Reads the model's answers to the lesson contract checks into findings.
use super::asks::{Finding, Mark, Taught, Verdict};
use super::confirm;
use super::sentence::{self, Whose};
use super::{how_to_teach, teaching_before, Item, Located, Part};
use crate::lessons::content_verification::Protected;
use crate::lessons::teaching::protected;
use crate::review_findings::{subject_of, FindingSite, PlanCheck, ReviewFinding};
use lattice_core::application::services::model_contract::ask::Settled;
use lattice_core::application::services::model_contract::shown::Picked;
use lattice_core::application::services::model_contract::units::{Texts, Unit, UnitRef};
use serde_json::Value;

/// A sentence of the lesson the model cited: the part, its place in the order the
/// learner meets the parts, and where in the part.
pub(super) struct Hit<'a> {
    pub(super) order: usize,
    pub(super) part: &'a Part,
    pub(super) found: Located,
}

/// The sentence of the lesson `unit` is, when it is one.
pub(super) fn hit_at<'a>(
    parts: &'a [Part],
    texts: &Texts<Whose>,
    unit: UnitRef,
) -> Option<Hit<'a>> {
    let (Whose::Lesson { part, text }, words) = texts.found(unit)? else {
        return None;
    };
    let span = sentence::span_of(texts, unit)?;
    Some(Hit {
        order: *part,
        part: parts.get(*part)?,
        found: Located {
            text: *text,
            start: span.start,
            end: span.end,
            exact: words.to_owned(),
        },
    })
}

/// The sentences `cited` name in the lesson, in the order the learner meets them, each once.
pub(super) fn hits_of<'a>(
    parts: &'a [Part],
    texts: &Texts<Whose>,
    cited: &[Picked<Unit>],
) -> Vec<Hit<'a>> {
    let mut hits: Vec<Hit> = cited
        .iter()
        .filter_map(|unit| hit_at(parts, texts, unit.reference))
        .collect();
    hits.sort_by_key(|hit| place_of(hit));
    hits.dedup_by_key(|hit| place_of(hit));
    hits
}

/// Where `hit` begins, as an order over the lesson.
fn place_of(hit: &Hit) -> (usize, usize, usize) {
    (hit.order, hit.found.text, hit.found.start)
}

/// Something the lesson may use before it teaches it: every place that uses it, in the order the learner meets them, and where it teaches it afterwards.
pub(super) struct Use<'a> {
    pub(super) what: &'a str,
    pub(super) uses: Vec<Hit<'a>>,
    pub(super) taught_later: Option<Hit<'a>>,
}

/// Whether `later` begins after the end of `used`.
pub(super) fn follows(later: &Hit, used: &Hit) -> bool {
    (later.order, later.found.text, later.found.start)
        > (used.order, used.found.text, used.found.end)
}

/// Two statements the check says cannot both be true, each located in the lesson.
pub(super) struct Clash<'a> {
    pub(super) first: Hit<'a>,
    pub(super) second: Hit<'a>,
}

/// What the asks of the check say once their pointers are read in the lesson.
pub(super) struct Reading<'a> {
    pub(super) taught: Vec<(&'a Item, Vec<Hit<'a>>)>,
    pub(super) untaught: Vec<&'a Item>,
    pub(super) uses: Vec<Use<'a>>,
    pub(super) clashes: Vec<Clash<'a>>,
}

/// Words the check says point at what the learner never sees.
pub(super) struct Unseen<'a> {
    hit: Hit<'a>,
    what: &'a str,
}

/// What the answers of the asks say about `items` and the lesson. An item left unanswered goes to the confirming read, which is the verdict.
pub(super) fn read<'a>(
    items: &'a [Item],
    planned: Vec<Settled<Taught>>,
    found: &'a [Finding],
    parts: &'a [Part],
    texts: &Texts<Whose>,
) -> (Reading<'a>, Vec<Unseen<'a>>) {
    let mut reading = Reading {
        taught: Vec::new(),
        untaught: Vec::new(),
        uses: Vec::new(),
        clashes: Vec::new(),
    };
    for (item, settled) in items.iter().zip(planned) {
        match settled {
            Settled::Answered(Taught {
                verdict: Verdict::Taught,
                units,
            }) => {
                let hits = hits_of(parts, texts, &units);
                if hits.is_empty() {
                    tracing::warn!(item = %item.id, "The check against the course plan cited no sentence of the lesson for a planned item; it is put to the confirming read");
                    reading.untaught.push(item);
                } else {
                    reading.taught.push((item, hits));
                }
            }
            Settled::Answered(_) => reading.untaught.push(item),
            Settled::Unanswered(why) => {
                tracing::warn!(item = %item.id, ?why, "The check against the course plan left a planned item unanswered; it is put to the confirming read");
                reading.untaught.push(item);
            }
        }
    }
    let mut unseen = Vec::new();
    for finding in found {
        match finding {
            Finding::Used { what, uses, later } => {
                let uses = hits_of(parts, texts, uses);
                if uses.is_empty() {
                    continue;
                }
                reading.uses.push(Use {
                    what,
                    uses,
                    taught_later: hits_of(parts, texts, later).into_iter().next(),
                });
            }
            Finding::Clash { first, second } => {
                if let (Some(first), Some(second)) = (
                    hit_at(parts, texts, first.reference),
                    hit_at(parts, texts, second.reference),
                ) {
                    reading.clashes.push(Clash { first, second });
                }
            }
            Finding::Unseen {
                words,
                what,
                never_shown: Mark::Yes,
            } => {
                if let Some(hit) = hit_at(parts, texts, words.reference) {
                    unseen.push(Unseen { hit, what });
                }
            }
            Finding::Unseen { .. } => {}
        }
    }
    (reading, unseen)
}

/// The findings of the asks and of the confirming reads that the lesson's
/// own words bear out.
pub(super) fn findings(
    unseen: &[Unseen<'_>],
    settled: &confirm::Settled<'_>,
    candidate: &Value,
    protected: &[Protected],
) -> Vec<ReviewFinding> {
    let mut found = Vec::new();
    for (item, belongs) in &settled.missing {
        let site = belongs.map_or(FindingSite::Lesson, |part| part.site);
        let fits = belongs.map_or_else(String::new, |part| {
            format!(" It fits best in {}.", part.site.label().to_lowercase())
        });
        let label = if item.kind == "term" {
            ""
        } else {
            "The quoted name is a short label for it, not wording for the lesson: teach what it names in the lesson's own sentences, worked into what the part already says, without copying the label. "
        };
        found.push(
            ReviewFinding::plan(
                PlanCheck::Planned,
                &item.id,
                site,
                format!(
                    "The lesson does not yet teach the {} \"{}\". {label}Teach it in the lesson's own words: {}{fits}",
                    item.kind,
                    item.text,
                    how_to_teach(item.kind)
                ),
            )
            .with_summary(format!(
                "The course plan gives this lesson the {} \"{}\", and the lesson does not teach it.",
                item.kind, item.text
            )),
        );
    }
    for unmet in &settled.uses {
        let (part, passage) = (unmet.hit.part, &unmet.hit.found.exact);
        let guarded =
            |part: &Part, words: &str| protected::guards(candidate, part.site, words, protected);
        let in_guarded = guarded(part, passage);
        let teach = if in_guarded {
            concat!("Teach it in this step, where it is first used (", each_shown!(), "), in sentences added before or after the quoted words; the quoted words stay exactly as they are.")
        } else {
            concat!("Teach it in this step, where it is first used (", each_shown!(), "), or change the step to use only what the lesson has taught (a record asked for in the learner's own notes needs nothing taught).")
        };
        let later = unmet
            .later
            .as_ref()
            .filter(|later| !guarded(later.part, &later.found.exact));
        let keep = if in_guarded {
            " The words that use it stay exactly as they are."
        } else {
            ""
        };
        let before = match later {
            None => teaching_before(candidate, part.site),
            Some(_) => None,
        };
        let how = match later {
            None => match before {
                Some(site) if in_guarded => format!(
                    concat!("Teach it in {place}, the last part before this one that teaches (", each_shown!(), "), in sentences added there; the quoted words stay exactly as they are."),
                    place = site.label()
                ),
                Some(site) => format!(
                    concat!("Teach it in {place}, the last part before this one that teaches (", each_shown!(), "), in sentences added there, and do not teach it inside this step: a step that sets a task does not give away what it asks for. Or change this step to use only what the lesson has taught (a record asked for in the learner's own notes needs nothing taught)."),
                    place = site.label()
                ),
                None => teach.to_owned(),
            },
            Some(later) if later.part.id == part.id => format!(
                concat!("This step teaches it only after it is used (\"{words}\"). Move that teaching before the words that use it (", each_shown!(), "), so that the lesson teaches it once.{keep}"),
                words = later.found.exact,
                keep = keep
            ),
            Some(later) => format!(
                concat!("The lesson teaches it only afterwards, in {place} (\"{words}\"). Move that teaching to this step, where it is first used (", each_shown!(), "), so that the lesson teaches it once.{keep}"),
                place = later.part.site.label().to_lowercase(),
                words = later.found.exact,
                keep = keep
            ),
        };
        let labels: Vec<String> = unmet
            .elsewhere
            .iter()
            .map(|other| other.site.label())
            .collect();
        let one_place = before.map_or_else(|| "this step".to_owned(), FindingSite::label);
        let too = match labels.split_last() {
            None => String::new(),
            Some((only, [])) => {
                format!(" {only} uses it too: {one_place} is the one place that teaches it.")
            }
            Some((last, rest)) => format!(
                " {} and {last} use it too: {one_place} is the one place that teaches it.",
                rest.join(", ")
            ),
        };
        let finding = ReviewFinding::plan(
            PlanCheck::Used,
            subject_of(passage),
            part.site,
            format!(
                "{} asks the learner to use {} (\"{passage}\"), and nothing before this point in the lesson, the earlier lessons or the learner's prior knowledge has taught it. {how}{too}",
                part.site.label(),
                unmet.what,
                passage = passage
            ),
        )
        .with_summary(format!("{} uses {} before the lesson teaches it.", part.site.label(), unmet.what))
        .with_passage(passage);
        found.push(
            match (later.filter(|later| later.part.id != part.id), before) {
                (Some(later), _) => finding.with_also(later.part.site),
                (None, Some(site)) => finding.with_also(site),
                (None, None) => finding,
            },
        );
    }
    for upheld in &settled.clashes {
        let Clash { first, second } = &upheld.clash;
        let (one, other) = (&first.found.exact, &second.found.exact);
        let guarded = |part: &Part, passage: &str| {
            protected::guards(candidate, part.site, passage, protected)
        };
        let (at, passage) = if guarded(first.part, one) && !guarded(second.part, other) {
            (second.part, other)
        } else {
            (first.part, one)
        };
        let mut digests = [subject_of(one), subject_of(other)];
        digests.sort();
        let both_open = !guarded(first.part, one) && !guarded(second.part, other);
        let described = format!(
            "The lesson contradicts itself: {} says \"{one}\" and {} says \"{other}\". Correct the one that is wrong so the lesson says one thing.",
            first.part.site.label(),
            second.part.site.label()
        );
        let case = upheld.case.trim();
        let finding = ReviewFinding::plan(
            PlanCheck::Contradiction,
            digests.join("-"),
            at.site,
            if case.is_empty() {
                described
            } else {
                format!("{described} Where they conflict: {case}")
            },
        )
        .with_summary(format!(
            "{} and {} contradict each other.",
            first.part.site.label(),
            second.part.site.label()
        ))
        .with_passage(passage);
        found.push(if both_open {
            finding.with_also(second.part.site)
        } else {
            finding
        });
    }
    for pointer in unseen {
        let (part, passage) = (pointer.hit.part, &pointer.hit.found.exact);
        found.push(
            ReviewFinding::plan(
                PlanCheck::Unseen,
                subject_of(passage),
                part.site,
                format!(
                    "{} points the learner at something the lesson does not contain (\"{passage}\": {}). Put what is meant in the lesson's own words, or remove the reference.",
                    part.site.label(),
                    pointer.what
                ),
            )
            .with_summary(format!(
                "{} refers to something the learner is never shown.",
                part.site.label()
            ))
            .with_passage(passage),
        );
    }
    found
}
