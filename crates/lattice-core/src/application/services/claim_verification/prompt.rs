//! What a single-claim check sends the model: its system prompt for the
//! policy, and the request that carries it.
use super::*;

/// One model reply to a single-claim check.
pub(super) struct Reply {
    pub(super) text: String,
    /// The first token's alternatives, when reported.
    pub(super) logprobs: Option<Vec<(String, f32)>>,
    /// Output tokens the backend reports it generated for the reply.
    pub(super) output_tokens: u64,
}

impl ClaimChecker<'_> {
    pub(super) fn system_prompt(&self, located: bool, reasoned: bool) -> String {
        let mut base = if located {
            CLAIM_CHECK_SYSTEM.replace("Source quote: an exact contiguous quote from a passage that supports that comparison, or none.", "Source passage: the application-assigned passage-N identifier that supports that comparison, or none.")
                .replace("do not invent a quote", "do not invent a passage identifier")
        } else {
            CLAIM_CHECK_SYSTEM.into()
        };
        if matches!(self.policy, CheckPolicy::Strict) {
            // Chat's one-supporting-source rule conflicts with publication's
            // requirement to resolve all supplied counter-evidence.
            base = base.replace(
                ", and one source supporting the claim is enough unless the claim says the sources agree",
                "",
            );
        }
        if reasoned {
            base = base.replace(
                "Start with exactly one label: supported, contradicted, or unsupported. Then write two short labeled lines:",
                "Compare the evidence before classifying. Write two short labeled lines first:",
            );
            base.push_str(" End with a third line: Verdict: supported, contradicted, or unsupported. Choose this final label from the completed factual comparison, never before it. If the comparison identifies a missing fact or incompatible fact, the final label cannot be supported.");
        }
        let system = if self.policy.strict() {
            let citation = if located {
                "Select one supplied passage-N identifier after Source passage:; the application attaches its exact original text. A supported or contradicted verdict requires a valid identifier and explicit reason. Do not quote the passage yourself."
            } else {
                "A valid quotation and an explicit reason are required for any factual finding. Preserve whitespace inside code and data exactly."
            };
            format!("{base} For this strict check, inspect every supplied passage for conflicting evidence. If sources conflict on the scoped claim and the conflict cannot be resolved from their text, answer unsupported and explain the conflict. For words such as always, never, all, ensures and guarantees, actively check documented exceptions and missing conditions. Do not turn a recommendation into a universal requirement, or a guarantee about one input/property into a guarantee about the overall outcome. A guarantee about one property does not establish every claimed consequence. If the passages leave the scope or a required condition ambiguous, answer unsupported. Compare the precise relationship asserted, not just the named things: establishing that something exists does not establish every claimed property or observation about it. For a list, check every item and the conditions under which that list applies. A general statement about a subject does not establish each claimed detail. Missing definitions, unstated properties and support for only part of a claim are unsupported, never contradicted: absence is not an opposing fact. If your reason says the source does not state or confirm a required fact, your label must be unsupported. {citation}")
        } else {
            base
        };
        let system = if matches!(self.policy, CheckPolicy::Strict) {
            format!("{system} Before relying on any conditional source statement, identify its prerequisite and determine whether the claim's stated scenario explicitly meets it. Preserve scope: a property of an item does not establish properties of its surroundings or dependencies. Use only premises in the supplied text; do not use your own definitions or domain knowledge to bridge different descriptions. If matching the claim's scenario to a source condition requires an unstated definition, mechanism, or intermediate fact, answer unsupported and name that missing premise. A plausible interpretation is not an explicit premise. This is semantic entailment, not verbatim matching: ordinary synonyms, faithful paraphrases and logically necessary consequences of the supplied statements do not need additional sources merely because the wording differs. A missing premise is an additional substantive assumption, not a routine lexical equivalence. Explicitly stipulated inputs in a hypothetical or worked example are premises for that example, not claims that those observations occurred in the world. You may apply an explicitly supplied rule to those inputs and check elementary arithmetic; do not treat the claimed result as a premise. Preserve the distinction between an instructional choice and an empirical guarantee, and between the role of a quantity and coincidental equality of its value. Do not add an unstated domain rule, mechanism, empirical observation, exception or causal bridge. State the exact deduction when supporting a derived result; if a claimed deduction is wrong, reject it.")
        } else {
            system
        };
        let system = if matches!(self.policy, CheckPolicy::Strict) {
            format!("{system} A label assigned locally to an instructional example, case, sequence or specimen is a reference within that example, not a claim that the source uses the same label. When the claim itself fully specifies the example's inputs and steps, judge those steps and results against the source rules; do not reject solely because its local label is absent. This does not exempt real-world names, source attribution, scientific terminology, actual observations, or any substantive condition, property or result. If a local label is the only description and its referent is unspecified, do not guess what it means.")
        } else {
            system
        };
        if matches!(self.policy, CheckPolicy::Fidelity) {
            format!("{system} This is an inventory-fidelity check. Source passages are extracted assertions, and Claim contains original lesson text. Judge whether the inventory entails every externally checkable assertion in the original text, not whether either text is true in the world. Faithfully repeating an incorrect fact is supported for this representation check only; a separate evidence check decides truth. Preserve every qualifier and independently asserted consequence. This is not a transcription-completeness check. Instructor choices, course scope, assumed prior learner skills, exercise deliverables, instructor-defined grading levels and stipulated example inputs are lesson requirements, not empirical claims requiring inventory support. A prerequisite states an assumed starting skill; a rubric states the instructor's chosen expectations. Do not require inventory assertions restating those expectations, grading levels or required submission details. Ignore those normative portions while still checking every factual premise, numerical result, mechanism, capability or guarantee embedded in them. A prerequisite or rubric heading does not exempt empirical claims. Do not turn an empirical guarantee into a mere course choice. Do not infer an unmentioned outcome from a statement about one input or property.")
        } else {
            system
        }
    }

    /// The model's reply to `prompt`. A strict check's reply that did not
    /// finish on its own comes back empty, its output tokens still counted.
    pub(super) async fn request(&self, prompt: &str, located: bool) -> Result<Reply> {
        let reasoned = located && self.policy.strict();
        let system = self.system_prompt(located, reasoned);
        self.llm
            .complete(&CompletionRequest {
                input: vec![
                    CompletionInput::Message {
                        role: "system".into(),
                        content: system.clone(),
                    },
                    CompletionInput::Message {
                        role: "user".into(),
                        content: prompt.to_string(),
                    },
                ],
                // Keep the model's reasoning enabled. Durable publication
                // checks ask for deliberation; chat uses the model default.
                reasoning_effort: self.policy.strict().then(|| "low".into()),
                sampling: Some(self.sampling),
                // A durable lesson check must not fail merely because a
                // reasoning provider needs more than the chat-sized reply
                // allowance. Its configured provider/context limit still
                // applies, and incomplete output can never approve a claim.
                max_output_tokens: Some(if self.policy.strict() {
                    self.llm
                        .max_context_tokens()
                        .saturating_sub(self.llm.count_tokens(&system))
                        .saturating_sub(self.llm.count_tokens(prompt))
                        .min(u32::MAX as usize) as u32
                } else {
                    self.max_output_tokens
                }),
                want_logprobs: !reasoned,
                // A slow model is not a failed one: stall detection and the
                // caller's cancellation end a call that will not finish.
                no_time_limit: true,
                // Checking what an answer or a lesson already says: behind
                // whoever is waiting on the model, ahead of upkeep.
                priority: InferencePriority::Verification,
                ..Default::default()
            })
            .await
            .map(|response| {
                let output_tokens = response.output_tokens;
                if self.policy.strict()
                    && !matches!(
                        response.finish_reason.as_str(),
                        "stop" | "end_turn" | "completed"
                    )
                {
                    return Reply {
                        text: String::new(),
                        logprobs: None,
                        output_tokens,
                    };
                }
                Reply {
                    text: response.text,
                    logprobs: response.first_token_logprobs,
                    output_tokens,
                }
            })
    }
}
