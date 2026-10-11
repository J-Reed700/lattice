//! Stage 3: the material the turn reads whole rather than searches for.
use super::prepare::PreparedTurn;
use super::*;
use crate::application::ports::llm_port::{chars_within_tokens, DEFAULT_CHARS_PER_TOKEN};
use crate::application::services::context_assembler::{EvidenceBudget, EvidenceShare};

/// Attachments and the Explorer folder block, already charged to the budget.
pub(super) struct CarriedMaterial {
    pub(super) attachments: TurnAttachments,
    pub(super) explorer_context: Option<String>,
    pub(super) attachment_names: Vec<String>,
    pub(super) attachment_ids: Vec<String>,
}

pub(super) async fn carry_material(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    attachment_names: Vec<String>,
    attachment_ids: Vec<String>,
    evidence: &mut EvidenceBudget,
) -> CarriedMaterial {
    // The files this turn brought in are read, not searched for. Retrieval can
    // miss them, and on a forced-web turn the vault is not searched at all, so
    // carrying them is the only way an attachment reliably reaches the model.
    // `closed_book` still wins: that turn carries its own material by
    // definition and reads nothing else.
    let attachments = if flags.closed_book {
        TurnAttachments::default()
    } else if attachment_ids.is_empty() {
        // Files that never made it into the library — an import still running
        // when the message was sent, or one that failed. The chip is on the
        // message either way, so the turn says the file arrived and could not
        // be read rather than denying it.
        TurnAttachments::still_importing(&attachment_names)
    } else {
        build_turn_attachments(
            container,
            &turn.conv_service,
            &turn.conv_id,
            &attachment_ids,
            evidence.allowance(EvidenceShare::ATTACHMENTS),
            &turn.llm,
            &turn.highlight_terms,
            turn.settings.llm.tool_output.excerpt_chars as usize,
        )
        .await
    };
    evidence.charge(attachments.prompt_tokens(&turn.llm));

    // The folder block is the reader's own material too, so it is budgeted
    // before retrieval the way an attachment is: what it fills is not there
    // for library passages. When the folder has an index, the block also
    // carries what it finds for the question, which is how a model without
    // tool calling gets past the open file. A closed-book turn searches
    // nothing, the folder's index included.
    let explorer_context = match &turn.explorer {
        Some(explorer) => Some(
            explorer
                .context_block(
                    chars_within_tokens(
                        evidence.allowance(EvidenceShare::FOLDER),
                        DEFAULT_CHARS_PER_TOKEN,
                    ),
                    turn.llm.supports_tool_calling() && !flags.closed_book,
                    (!flags.closed_book).then_some(turn.message.as_str()),
                )
                .await,
        ),
        None => None,
    };
    evidence.charge(
        explorer_context
            .as_deref()
            .map_or(0, |text| turn.llm.count_tokens(text)),
    );

    if !attachments.is_empty() {
        let carried = attachments.carried_count();
        let unreadable = attachments.unreadable_names().len();
        turn.recorder.note(
            turn_record::TurnStepKind::OpenDocument,
            if carried == 1 && unreadable == 0 {
                "Read the attached file".to_string()
            } else {
                "Read the attached files".to_string()
            },
            None,
            Some(match (carried, unreadable) {
                (0, _) => "attached, but no readable text yet".to_string(),
                (_, 0) => format!("{carried} file{}", if carried == 1 { "" } else { "s" }),
                _ => format!("{carried} read, {unreadable} unreadable"),
            }),
        );
    }

    CarriedMaterial {
        attachments,
        explorer_context,
        attachment_names,
        attachment_ids,
    }
}
