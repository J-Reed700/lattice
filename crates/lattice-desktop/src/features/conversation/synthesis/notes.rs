//! The calls a synthesis makes: the map over entries, the merge of chunk notes
//! and the brief, and the notes the app carries between them. A note keeps its
//! entries and its sources as the app's own identities; the model points at
//! what it was shown and never writes a number or a title back.
use super::prompts::rewrite_citation_ids;
use super::{JournalSynthesisEntry, SourceDto};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::CallOptions;
use lattice_core::application::services::model_contract::ask::{
    ask, ask_each, Alone, Answered, Plain, Refusal, Says, Settled,
};
use lattice_core::application::services::model_contract::reply::{split, Markers, Piece, Place};
use lattice_core::application::services::model_contract::shape::{
    Authored, Each, Fields, Form, Value,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Handle, Internal, Kind, Picked, Scope, Shown, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const ENTRY_SEPARATOR: &str = "\n\n---\n\n";
const NONE_IDENTIFIED: &str = "- None identified.";

const MAP_FORM: &str = "Answer with an entries list: each entry names an entry in `of` by the handle shown beside it, and holds one list of bullets for each section: `highlights` (Entry Highlights), `practical_details` (Practical Details), `decisions_constraints` (Decisions & Constraints), `open_questions_risks` (Open Questions & Risks) and `evidence_notes` (Evidence Notes). Each bullet has `text` and `sources`, the handles of the sources that support it.";

const MERGE_FORM: &str = "Answer with one list of bullets for each section: `highlights` (Entry Highlights), `practical_details` (Practical Details), `decisions_constraints` (Decisions & Constraints), `open_questions_risks` (Open Questions & Risks) and `evidence_notes` (Evidence Notes). Each bullet has `text` and `from`, the handles of the bullets it merges.";

const BRIEF_FORM: &str = "Answer with a `brief` field that holds the final journal brief, written as markdown.";

/// The five sections of a chunk's notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Section {
    Highlights,
    Practical,
    Decisions,
    Risks,
    Evidence,
}

impl Section {
    const ALL: [Section; 5] = [
        Section::Highlights,
        Section::Practical,
        Section::Decisions,
        Section::Risks,
        Section::Evidence,
    ];

    fn title(self) -> &'static str {
        match self {
            Section::Highlights => "Entry Highlights",
            Section::Practical => "Practical Details",
            Section::Decisions => "Decisions & Constraints",
            Section::Risks => "Open Questions & Risks",
            Section::Evidence => "Evidence Notes",
        }
    }

    fn field(self) -> &'static str {
        match self {
            Section::Highlights => "highlights",
            Section::Practical => "practical_details",
            Section::Decisions => "decisions_constraints",
            Section::Risks => "open_questions_risks",
            Section::Evidence => "evidence_notes",
        }
    }
}

/// One bullet of a chunk's notes, as the app carries it between steps: the
/// entries it is about, and the global citation ids of the sources behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Note {
    pub(super) section: Section,
    pub(super) entries: Vec<usize>,
    pub(super) text: String,
    pub(super) sources: Vec<u32>,
}

/// One source of an entry, shown with its passage and named within the entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct NoteSource {
    pub(super) id: Internal<u32>,
    title: Told<String>,
    location: Told<Option<String>>,
    passage: Told<String>,
}

impl NoteSource {
    pub(super) fn of(source: &SourceDto) -> Self {
        let title = if source.file_name.trim().is_empty() {
            &source.file_path
        } else {
            &source.file_name
        };
        let location = source
            .section
            .clone()
            .or_else(|| source.page_number.map(|n| format!("page {n}")));
        Self {
            id: Internal::of(source.citation_id.unwrap_or_default()),
            title: Told(title.clone()),
            location: Told(location),
            passage: Told(source.content.clone()),
        }
    }
}

impl Kind for NoteSource {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Number;
    type Ref = u32;

    fn reference(&self) -> u32 {
        *self.id.value()
    }
}

/// An entry, or one piece of an entry too long for a chunk of its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct EntryPart {
    pub(super) entry: Internal<usize>,
    pub(super) part: Told<usize>,
    pub(super) parts: Told<usize>,
    title: Told<String>,
    updated_at: Told<String>,
    message_count: Told<usize>,
    pub(super) transcript: Told<String>,
    pub(super) sources: Vec<NoteSource>,
}

impl EntryPart {
    pub(super) fn of(
        index: usize,
        entry: &JournalSynthesisEntry,
        part: usize,
        parts: usize,
        transcript: String,
        sources: &[SourceDto],
    ) -> Self {
        Self {
            entry: Internal::of(index),
            part: Told(part),
            parts: Told(parts),
            title: Told(entry.title.clone()),
            updated_at: Told(entry.updated_at.clone()),
            message_count: Told(entry.message_count),
            transcript: Told(transcript),
            sources: sources.iter().map(NoteSource::of).collect(),
        }
    }
}

impl Kind for EntryPart {
    const PREFIX: &'static str = "e";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = (usize, usize);

    fn reference(&self) -> (usize, usize) {
        (*self.entry.value(), self.part.0)
    }
}

/// A bullet of a chunk synthesis, as a merge or the brief reads it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct NoteUnit {
    pub(super) chunk: Internal<usize>,
    pub(super) at: Internal<usize>,
    pub(super) section: Section,
    pub(super) entries: Vec<Told<String>>,
    pub(super) text: Told<String>,
}

impl Kind for NoteUnit {
    const PREFIX: &'static str = "n";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = (usize, usize);

    fn reference(&self) -> (usize, usize) {
        (*self.chunk.value(), *self.at.value())
    }
}

pub(super) fn units_of(chunks: &[Vec<Note>], titles: &[String]) -> Vec<NoteUnit> {
    let mut units = Vec::new();
    for (chunk, notes) in chunks.iter().enumerate() {
        for (at, note) in notes.iter().enumerate() {
            units.push(NoteUnit {
                chunk: Internal::of(chunk),
                at: Internal::of(at),
                section: note.section,
                entries: note
                    .entries
                    .iter()
                    .map(|entry| Told(titles.get(*entry).cloned().unwrap_or_default()))
                    .collect(),
                text: Told(note.text.clone()),
            });
        }
    }
    units
}

/// The characters `notes` take when shown to a merge or the brief.
pub(super) fn notes_chars(notes: &[Note], titles: &[String]) -> usize {
    notes
        .iter()
        .map(|note| {
            let entries: usize = note
                .entries
                .iter()
                .map(|entry| titles.get(*entry).map_or(0, |title| title.chars().count()) + 10)
                .sum();
            note.text.chars().count() + entries + 8
        })
        .sum()
}

fn sections_form<V: Authored>() -> Form {
    let mut form = Form::new();
    for section in Section::ALL {
        form = form.list::<V>(section.field());
    }
    form
}

fn read_sections<V: Authored>(fields: &mut Fields<'_, '_>) -> Option<Vec<(Section, Vec<V>)>> {
    let mut read = Vec::with_capacity(Section::ALL.len());
    for section in Section::ALL {
        read.push((section, fields.list::<V>(section.field())?));
    }
    Some(read)
}

fn section_names() -> Vec<String> {
    Section::ALL
        .iter()
        .map(|section| section.title().to_string())
        .collect()
}

fn unique<T: PartialEq + Clone>(into: &mut Vec<T>, from: impl IntoIterator<Item = T>) {
    for item in from {
        if !into.contains(&item) {
            into.push(item);
        }
    }
}

/// A bullet of one entry's notes.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MapBullet {
    text: String,
    sources: Vec<Picked<NoteSource>>,
}

impl Authored for MapBullet {
    fn form() -> Form {
        Form::new().text("text").cite_own::<NoteSource>("sources")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            text: fields.text("text")?,
            sources: fields.cite_own("sources"),
        })
    }
}

/// What the model wrote of one entry part, a list of bullets in each section.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Mapped(Vec<(Section, Vec<MapBullet>)>);

impl Authored for Mapped {
    fn form() -> Form {
        sections_form::<MapBullet>()
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        read_sections::<MapBullet>(fields).map(Self)
    }
}

impl Mapped {
    fn notes_of(&self, part: &EntryPart) -> Vec<Note> {
        let mut notes = Vec::new();
        for (section, bullets) in &self.0 {
            for bullet in bullets {
                let mut sources = Vec::new();
                unique(&mut sources, bullet.sources.iter().map(|pick| pick.reference));
                notes.push(Note {
                    section: *section,
                    entries: vec![*part.entry.value()],
                    text: bullet.text.clone(),
                    sources,
                });
            }
        }
        notes
    }
}

/// The entries of a chunk, and which chunk of how many it is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct MapInput {
    pub(super) chunk: Told<usize>,
    pub(super) chunks: Told<usize>,
    pub(super) parts: Vec<EntryPart>,
}

pub(super) struct MapNotes;

impl Contract for MapNotes {
    const ID: ContractId = ContractId::new("journal-map");
    type Input = MapInput;
}

fn render_part(
    showing: &mut Showing<'_>,
    handle: &Handle<EntryPart>,
    part: &EntryPart,
) -> Result<String> {
    let own = showing.under(handle, &part.sources)?;
    let labels: HashMap<u32, String> = own
        .iter()
        .map(|(label, source)| (*source.id.value(), label.written()))
        .collect();
    let first = part.part.0 <= 1;
    let mut lines = Vec::new();
    if part.parts.0 > 1 {
        lines.push(format!(
            "ENTRY {} (part {} of {}{})",
            handle.bracketed(),
            part.part.0,
            part.parts.0,
            if first { "" } else { ", continued" }
        ));
    } else {
        lines.push(format!("ENTRY {}", handle.bracketed()));
    }
    if first {
        lines.push(format!("Title: {}", part.title.0));
        lines.push(format!("Updated: {}", part.updated_at.0));
        lines.push(format!("Message Count: {}", part.message_count.0));
        lines.push("Transcript (citation markers resolve to the source list below):".to_string());
    }
    lines.push(if part.transcript.0.is_empty() {
        "(No message text captured.)".to_string()
    } else {
        rewrite_citation_ids(&part.transcript.0, &labels)
    });
    if own.is_empty() {
        if first {
            lines.push("Sources: (none recorded)".to_string());
        }
    } else {
        let sources = own
            .iter()
            .map(|(label, source)| {
                format!(
                    "{} {}{}\nPassage: {}",
                    label.bracketed(),
                    source.title.0,
                    source
                        .location
                        .0
                        .as_ref()
                        .map(|value| format!(" — {value}"))
                        .unwrap_or_default(),
                    source.passage.0
                )
            })
            .collect::<Vec<_>>();
        lines.push(format!(
            "Sources (cite only when supporting a claim):\n{}",
            sources.join("\n")
        ));
    }
    Ok(lines.join("\n"))
}

impl Answered for MapNotes {
    type Answer = Each<EntryPart, Mapped>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &MapInput, showing: &mut Showing<'_>) -> Result<Says> {
        let shown = showing.show(&input.parts)?;
        let mut blocks = Vec::new();
        for (handle, part) in shown.iter() {
            blocks.push(render_part(showing, handle, part)?);
        }
        let mut lines = vec![
            format!(
                "Below are journal entries, chunk {} of {}. You will synthesize them.",
                input.chunk.0, input.chunks.0
            ),
            String::new(),
            "Entries:".to_string(),
            blocks.join(ENTRY_SEPARATOR),
            String::new(),
            format!(
                "Now write the synthesis of these entries (chunk {} of {}).",
                input.chunk.0, input.chunks.0
            ),
            "Use only the entry transcripts above.".to_string(),
            "Goal: capture practical details so the final output can replace re-reading every entry."
                .to_string(),
            "Write notes in exactly these sections for each entry:".to_string(),
        ];
        lines.extend(section_names());
        lines.extend([
            "Rules:".to_string(),
            "- Write concise but information-dense bullets (not generic summaries).".to_string(),
            "- Include concrete values when present (counts, ranges, timing, limits, caveats)."
                .to_string(),
            "- In Evidence Notes, map each important claim to one or more supporting entry titles."
                .to_string(),
            "- Cite the sources that support a bullet in its `sources`, by the handles shown beside the sources of that entry; never invent a source or cite one that is not listed with that entry.".to_string(),
            "- If a section has no data, leave its list empty.".to_string(),
            String::new(),
            "- An entry marked (part i of n) continues across chunks; describe the part you have."
                .to_string(),
        ]);
        Ok(Says::new(MAP_FORM, lines.join("\n")))
    }
}

/// A bullet of merged notes: its words, and the bullets it merges.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MergeBullet {
    text: String,
    from: Vec<Picked<NoteUnit>>,
}

impl Authored for MergeBullet {
    fn form() -> Form {
        Form::new().text("text").cite::<NoteUnit>("from")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            text: fields.text("text")?,
            from: fields.cite("from"),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Merged(Vec<(Section, Vec<MergeBullet>)>);

impl Authored for Merged {
    fn form() -> Form {
        sections_form::<MergeBullet>()
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        read_sections::<MergeBullet>(fields).map(Self)
    }
}

fn note_at(chunks: &[Vec<Note>], at: (usize, usize)) -> Option<&Note> {
    chunks.get(at.0).and_then(|notes| notes.get(at.1))
}

impl Merged {
    fn notes_over(&self, chunks: &[Vec<Note>]) -> Vec<Note> {
        let mut notes = Vec::new();
        for (section, bullets) in &self.0 {
            for bullet in bullets {
                let mut entries = Vec::new();
                let mut sources = Vec::new();
                for pick in &bullet.from {
                    if let Some(note) = note_at(chunks, pick.reference) {
                        unique(&mut entries, note.entries.iter().copied());
                        unique(&mut sources, note.sources.iter().copied());
                    }
                }
                notes.push(Note {
                    section: *section,
                    entries,
                    text: bullet.text.clone(),
                    sources,
                });
            }
        }
        notes
    }
}

/// Chunk notes to merge into one, and which group of how many this is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct MergeInput {
    pub(super) group: Told<usize>,
    pub(super) groups: Told<usize>,
    pub(super) chunks: usize,
    pub(super) notes: Vec<NoteUnit>,
}

pub(super) struct MergeNotes;

impl Contract for MergeNotes {
    const ID: ContractId = ContractId::new("journal-merge");
    type Input = MergeInput;
}

/// The chunk syntheses as blocks of bullets under their section headings; a
/// section without a bullet says so.
fn render_chunks(shown: &Shown<'_, NoteUnit>, chunks: usize) -> String {
    let mut blocks = Vec::with_capacity(chunks);
    for chunk in 0..chunks {
        let mut lines = vec![format!("CHUNK SYNTHESIS {}", chunk + 1)];
        for section in Section::ALL {
            lines.push(format!("### {}", section.title()));
            let bullets: Vec<String> = shown
                .iter()
                .filter(|(_, unit)| *unit.chunk.value() == chunk && unit.section == section)
                .map(|(handle, unit)| {
                    let entries: String = unit
                        .entries
                        .iter()
                        .map(|title| format!("[Entry: {}] ", title.0))
                        .collect();
                    format!("- {} {entries}{}", handle.bracketed(), unit.text.0)
                })
                .collect();
            if bullets.is_empty() {
                lines.push(NONE_IDENTIFIED.to_string());
            } else {
                lines.extend(bullets);
            }
        }
        blocks.push(lines.join("\n"));
    }
    blocks.join(ENTRY_SEPARATOR)
}

impl Answered for MergeNotes {
    type Answer = Value<Merged>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &MergeInput, showing: &mut Showing<'_>) -> Result<Says> {
        let shown = showing.show(&input.notes)?;
        let mut lines = vec![
            format!(
                "Merge these chunk syntheses into one chunk synthesis (group {} of {}).",
                input.group.0, input.groups.0
            ),
            "Use only the chunk syntheses below; keep every concrete detail, decision, open question and evidence note.".to_string(),
            "Write notes in exactly these sections:".to_string(),
        ];
        lines.extend(section_names());
        lines.extend([
            "Rules:".to_string(),
            "- Give each merged bullet, in `from`, the handles of the bullets it merges; name only bullets shown.".to_string(),
            "- Merge duplicates; drop nothing else.".to_string(),
            "- If a section has no data, leave its list empty.".to_string(),
            String::new(),
            render_chunks(&shown, input.chunks),
        ]);
        Ok(Says::new(MERGE_FORM, lines.join("\n")))
    }

    fn accept(_input: &MergeInput, statement: &Merged) -> std::result::Result<(), Refusal> {
        let lacking = statement
            .0
            .iter()
            .flat_map(|(_, bullets)| bullets)
            .any(|bullet| bullet.from.is_empty());
        if lacking {
            return Err(Refusal(
                "Every bullet must name, in `from`, at least one of the bullets it merges."
                    .to_string(),
            ));
        }
        Ok(())
    }
}

/// The final brief: the model's prose, whose markers name chunk bullets.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Brief {
    text: String,
}

impl Authored for Brief {
    fn form() -> Form {
        Form::new().text("brief")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            text: fields.text("brief")?,
        })
    }
}

/// The chunk syntheses the brief merges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct BriefInput {
    pub(super) chunks: usize,
    pub(super) notes: Vec<NoteUnit>,
}

pub(super) struct WriteBrief;

impl Contract for WriteBrief {
    const ID: ContractId = ContractId::new("journal-brief");
    type Input = BriefInput;
}

impl Answered for WriteBrief {
    type Answer = Value<Brief>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &BriefInput, showing: &mut Showing<'_>) -> Result<Says> {
        let shown = showing.show(&input.notes)?;
        let lines = [
            "Below are chunk syntheses of a journal. You will merge them into one final brief."
                .to_string(),
            String::new(),
            render_chunks(&shown, input.chunks),
            String::new(),
            "Now merge the chunk syntheses above into one final journal brief.".to_string(),
            "Use only the chunk syntheses above.".to_string(),
            "Audience: someone who wants to understand and act quickly without reading every entry."
                .to_string(),
            "Output markdown with exactly these sections (in this order):".to_string(),
            "## Executive Summary".to_string(),
            "## Detailed Synthesis".to_string(),
            "## Action Plan".to_string(),
            "## Decisions and Assumptions".to_string(),
            "## Open Questions and Risks".to_string(),
            "## Evidence Map".to_string(),
            "Formatting rules:".to_string(),
            "- Executive Summary: 1 short paragraph (4-6 sentences), plain language.".to_string(),
            "- Detailed Synthesis: 3-6 subsections with '### <Theme>' headers. Under each theme, write one short paragraph plus bullets for critical specifics.".to_string(),
            "- Action Plan: numbered list (3-7 steps) with enough detail to execute.".to_string(),
            "- Decisions and Assumptions: bullets with explicit rationale when available."
                .to_string(),
            "- Open Questions and Risks: bullets with impact noted briefly.".to_string(),
            "- Evidence Map: markdown table with columns `Claim`, `Supporting entries`, `Confidence`."
                .to_string(),
            "- After a sentence that rests on bullets of the chunk syntheses, cite them by their handles in square brackets; cite only bullets shown.".to_string(),
            "- Keep every name, date, figure and limit the chunk syntheses state; cut repetition, not detail.".to_string(),
            "- If data is missing in a section, write `None identified.`".to_string(),
        ];
        Ok(Says::new(BRIEF_FORM, lines.join("\n")))
    }
}

fn options() -> CallOptions {
    CallOptions {
        no_time_limit: true,
        ..CallOptions::default()
    }
}

fn unanswered(what: &str, why: impl std::fmt::Debug) -> AppError {
    AppError::ModelAnswerUnusable(format!("The model wrote no {what}: {why:?}"))
}

/// The notes of one chunk, every entry part of it answered.
pub(super) async fn write_notes(
    llm: &dyn LLMPort,
    chunk: &[EntryPart],
    index: usize,
    chunks: usize,
) -> Result<Vec<Note>> {
    let mut scope = Scope::for_question();
    let settled = ask_each::<MapNotes>(
        llm,
        &Plain,
        &mut scope,
        chunk,
        |open, _| MapInput {
            chunk: Told(index + 1),
            chunks: Told(chunks),
            parts: open.iter().map(|part| (*part).clone()).collect(),
        },
        &options(),
    )
    .await?;
    let mut notes = Vec::new();
    for (part, settled) in chunk.iter().zip(settled) {
        match settled {
            Settled::Answered(mapped) => notes.extend(mapped.notes_of(part)),
            Settled::Unanswered(why) => return Err(unanswered("notes for an entry", why)),
        }
    }
    Ok(notes)
}

/// `chunks` merged into one chunk's notes.
pub(super) async fn merge_notes(
    llm: &dyn LLMPort,
    chunks: &[Vec<Note>],
    titles: &[String],
    group: usize,
    groups: usize,
) -> Result<Vec<Note>> {
    let input = MergeInput {
        group: Told(group),
        groups: Told(groups),
        chunks: chunks.len(),
        notes: units_of(chunks, titles),
    };
    let mut scope = Scope::for_question();
    match ask::<MergeNotes>(llm, &Plain, &mut scope, &input, &options()).await? {
        Settled::Answered(merged) => Ok(merged.notes_over(chunks)),
        Settled::Unanswered(why) => Err(unanswered("merged notes", why)),
    }
}

/// The final brief, its markers written as the numbers of the sources behind
/// the bullets they name.
pub(super) async fn write_brief(
    llm: &dyn LLMPort,
    chunks: &[Vec<Note>],
    titles: &[String],
) -> Result<String> {
    let input = BriefInput {
        chunks: chunks.len(),
        notes: units_of(chunks, titles),
    };
    let mut scope = Scope::for_question();
    let brief = match ask::<WriteBrief>(llm, &Plain, &mut scope, &input, &options()).await? {
        Settled::Answered(brief) => brief,
        Settled::Unanswered(why) => return Err(unanswered("brief", why)),
    };
    let pieces = split(&scope, Markers::new().of::<NoteUnit>(Place::Inline), &brief.text);
    Ok(numbered(&pieces, chunks))
}

fn numbered(pieces: &[Piece], chunks: &[Vec<Note>]) -> String {
    let mut text = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(words) => text.push_str(words),
            Piece::Reference(reference) => {
                let at = serde_json::from_value::<(usize, usize)>(reference.reference.clone());
                if let Some(note) = at.ok().and_then(|at| note_at(chunks, at)) {
                    for id in &note.sources {
                        text.push_str(&format!("[{id}]"));
                    }
                }
            }
        }
    }
    text
}

fn says_chars(says: &Says) -> usize {
    says.instructions.chars().count() + says.task.chars().count()
}

/// The characters a map request holds before its entries.
pub(super) fn map_overhead() -> Result<usize> {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let input = MapInput {
        chunk: Told(1_000),
        chunks: Told(1_000),
        parts: Vec::new(),
    };
    Ok(says_chars(&MapNotes::says(&input, &mut showing)?))
}

/// The characters a merge or brief request holds before its notes.
pub(super) fn synthesis_overhead() -> Result<usize> {
    let mut scope = Scope::for_question();
    let merge = MergeInput {
        group: Told(1_000),
        groups: Told(1_000),
        chunks: 0,
        notes: Vec::new(),
    };
    let merged = says_chars(&MergeNotes::says(&merge, &mut scope.request())?);
    let brief = BriefInput {
        chunks: 0,
        notes: Vec::new(),
    };
    let briefed = says_chars(&WriteBrief::says(&brief, &mut scope.request())?);
    Ok(merged.max(briefed))
}

/// The characters one entry part takes in a map request.
pub(super) fn part_size(part: &EntryPart) -> Result<usize> {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = showing.show(std::slice::from_ref(part))?;
    let mut size = 0;
    for (handle, part) in shown.iter() {
        size += render_part(&mut showing, handle, part)?.chars().count();
    }
    Ok(size)
}
