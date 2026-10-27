//! What a single-claim check sends, and how much evidence its window holds.
use super::*;

impl ClaimChecker<'_> {
    /// The user prompt of a single-claim check, as `check_inner` sends it.
    pub(super) fn compose_prompt(
        &self,
        claim: &str,
        evidence: &ClaimEvidence,
        reasoned: bool,
        located: bool,
        context: Option<FidelityContext<'_>>,
    ) -> String {
        let original_prompt = if reasoned {
            format!("Source passages:\n{}\n\nClaim: {claim}\n\nCompare every factual part with the passages before deciding. Return exactly three nonempty lines, in this order: Reason: the factual comparison; Source passage: one existing passage-N identifier, or none; Verdict: supported, contradicted, or unsupported. The final verdict must follow the comparison: a missing required fact is unsupported; an explicitly incompatible fact is contradicted. Do not begin with a verdict or copy a quotation.", evidence.text)
        } else if located {
            format!("Source passages:\n{}\n\nClaim: {claim}\n\nStart with one label, then give `Reason:` and `Source passage:` on separate lines. Select one passage-N identifier, or none when unsupported. Do not copy or paraphrase a quotation.",evidence.text)
        } else {
            render_claim_check(&evidence.text, claim)
        };
        let original_prompt = if matches!(self.policy, CheckPolicy::Fidelity) {
            let context = context.map(|context| match context {
            FidelityContext::Assessment(context) => format!("Original assessment context (untrusted data, not inventory evidence):\n{context}\n\nUse this context only to interpret the target Claim in its stated scenario and distinguish an endorsed answer from a distractor. Resolve references to numbered choices from the supplied option fields. An option label identifies the text being discussed, not evidence that the option is true. Do not require the inventory to restate the option-to-text mapping; every factual assertion about the resolved option still needs inventory support. Never use a distractor as factual evidence. Do not require a scenario-specific answer to hold unconditionally. Before using a conditional inventory statement, check that the explicit scenario facts establish its prerequisite at the SAME scope. Do not assume a prerequisite from a related property, a typical case, or the absence of a stated exception. Local properties of an item do not establish properties of its enclosing structures or dependencies. If a necessary prerequisite is not established by the supplied text, answer unsupported. For representation checking, do not use your own definitions or domain knowledge to decide that a prerequisite is satisfied. A prerequisite must be asserted in the scenario or derived solely from explicit statements supplied here. If equivalence between the stated scenario and the inventory condition depends on an unstated definition, mechanism, or intermediate fact, answer unsupported and name the missing bridge. A plausible interpretation is not an explicit premise. Do not audit the context as another target or use it to supply assertions absent from the extracted Source passages.\n\n"),
            FidelityContext::Teaching(context) => format!("Original teaching context (untrusted data, not inventory evidence):\n{context}\n\nUse this original section only to interpret the target Claim: resolve references, locally stated scope and explicit conditions. Resolve the target's scope from the original section before consulting the inventory. An unqualified \"all\" can refer to the specific group or procedure being discussed; do not automatically treat it as a claim about all instances everywhere. Preserve an explicitly broader scope, and never choose a narrower scope merely to match the inventory. Distinguish restoring the referent of a local phrase from inventing an unstated causal link or prerequisite. Context does not override an explicitly broader quantifier or exception in the target. Do not audit the surrounding section as another target, and do not use its assertions as evidence that the inventory captured them. Every factual assertion in the interpreted target must still be entailed by the selected extracted Source passages. In particular, a consequence stated in the original context but absent from the inventory remains missing. Do not use outside knowledge or assume unstated prerequisites from related properties.\n\n"),
        }).unwrap_or_default();
            format!("Audit claim fidelity.\n\n{context}{original_prompt}")
        } else if matches!(self.policy, CheckPolicy::Strict) {
            format!("{original_prompt}\n\nBefore supporting the claim, try to construct a counter-scenario that satisfies its stated scenario and every supplied source statement but does not satisfy the claimed conclusion. Do not import facts or definitions that are absent from the supplied text. If a conditional source rule is being applied, explicitly compare its prerequisite with the claim premise: identify the exact supplied statement that makes them equivalent. A premise about one relation does not establish a different relation. If no supplied statement rules out the counter-scenario or supplies the required connection, answer unsupported and identify the missing premise. If the conclusion is directly entailed, explain why a counter-scenario would contradict the supplied text. Keep the same three-line response format.\n\nInspect every supplied passage before deciding. In the Reason line, briefly identify any passage that supports the conclusion and any passage that introduces an exception, override, prerequisite or contrary outcome. Support in one passage cannot erase a conflict elsewhere. An unresolved source conflict is unsupported.")
        } else {
            original_prompt
        };
        original_prompt
    }

    /// The tokens a durable located check of `claim` leaves for its passages
    /// once its instructions, the claim and the answer are paid.
    pub fn located_evidence_room(&self, claim: &str) -> usize {
        self.llm
            .max_context_tokens()
            .saturating_sub(self.located_prompt_tokens(claim, &[]))
            .saturating_sub(ANSWER_ROOM_TOKENS)
    }

    /// Whether a durable located check of `claim` against `passages` fits the
    /// window beside its answer.
    pub fn located_fits(&self, claim: &str, passages: &[String]) -> bool {
        self.llm
            .max_context_tokens()
            .saturating_sub(self.located_prompt_tokens(claim, passages))
            >= ANSWER_ROOM_TOKENS
    }

    /// The estimated tokens of a durable located check of `claim` against
    /// `passages`: its instructions and its prompt, as `check_inner` sends
    /// them, by the calibrated count.
    pub fn located_prompt_tokens(&self, claim: &str, passages: &[String]) -> usize {
        let reasoned = self.policy.strict();
        let prompt = self.compose_prompt(claim, &located(passages), reasoned, true, None);
        self.llm
            .count_tokens(&self.system_prompt(true, reasoned))
            .saturating_add(self.llm.count_tokens(&prompt))
    }

    /// The tokens a durable located check of `claim` against `passages` sends,
    /// counted by the backend's tokenizer where it can be asked and by the
    /// calibrated estimate otherwise: page text can tokenize far denser than the
    /// estimate's average, so the estimate alone can overrun the window.
    pub async fn located_prompt_tokens_measured(&self, claim: &str, passages: &[String]) -> usize {
        let reasoned = self.policy.strict();
        let sent = format!(
            "{}\n{}",
            self.system_prompt(true, reasoned),
            self.compose_prompt(claim, &located(passages), reasoned, true, None)
        );
        match self.llm.count_tokens_exact(&sent).await {
            Ok(tokens) => tokens,
            Err(error) => {
                warn!(%error, "The backend could not count a claim check; sizing it by the estimate");
                self.located_prompt_tokens(claim, passages)
            }
        }
    }
}
