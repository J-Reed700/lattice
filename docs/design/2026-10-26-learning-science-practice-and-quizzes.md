# Quizzes, the starting point and practice, fitted to how people learn

Status: implemented. Builds on
[Lessons built on learning science](2026-10-25-learning-science-lessons.md),
whose sources keep their keys here.

The lessons and study sessions were rebuilt on learning science on 2026-10-25
and 2026-10-26. Quiz writing, the starting point, the practice workbench and
the Feynman panel were not. Each change below fits what its section is for.
The sections are not given one shared paragraph of method: a quiz tests, a
tutor helps with a task, and the Feynman panel has the learner teach.

| Section | Before | Now | Why |
|---|---|---|---|
| Quiz items (`assessment_generation.rs`) | "Plausible distractors"; the explanation says why the answer is right | Items prefer applying an idea to a fresh case over recognising a sentence. Distractors are misconceptions learners hold, and each rationale names its misconception. The explanation also says why the tempting wrong answer fails | Competitive distractors make a choice item retrieval rather than recognition [L12]; feedback that names the error corrects it [HT07, BM01] |
| Lesson quiz mix and review (`quiz_blueprints.rs`) | See [quizzes §2.3](2026-10-23-quizzes-and-tests.md) | A mix of formats with more typed answers, and two items from earlier lessons | Production formats help where the material suits them [K07a, M07]; choice items with competitive distractors also drive retrieval [L12, A17], so the mix keeps them. Retrieval spaced across sessions and mixed with new material keeps it [RD11, R20] |
| Starting point (`diagnostic_generation.rs`) | Feedback named the reasoning and a next step; the expected answer stayed private | Feedback says what was right, then what a correct answer is and why, then a next step | The starting point is a pretest, and a pretest helps only when corrective feedback follows [RK09, P23] |
| Practice tutor (`practice_generation.rs`) | A hint ladder that never gives the answer; critiques find the first reasoning gap | Also asks the learner to retrieve or explain before telling, and names the misconception behind an error | Self-explanation prompts [B18] |
| Worked solution | "A concise worked reference solution" | Each step with its reason, the mistake learners most often make at the hardest step, and a variation to try from memory | Worked examples with step rationales, moving to independent problems [ARM03, A00] |
| Accepted practice misconceptions (`PracticeWorkbenchPanel.tsx`) | Only changed a status | The learner writes what is actually right, saved as a recall card draft | Writing the correction is generation and retrieval [R06, F16], and the card returns it spaced; a confidently held misconception is corrected especially well once corrected [BM01] |
| Feynman panel (`FeynmanTechniquePanel.tsx`) | Checked style only (jargon, sentence length, copied phrasing), so a wrong explanation could rate "Simple & Clear" | Explain from memory first. The lesson's points for the concept open only after. The learner ticks what they got right, gaps become recall cards, and the lesson closes again for a second explanation | Teaching helps through retrieval: explaining without notes beats explaining with them [KLL18, F13] |

Left as they were, because they already work this way: the free-recall brain
dump (notes closed, compared with the lesson, gaps to recall cards), the
practice workbench's aid modes (the solution stays closed in Practice), module
tests and the course exam (mixed across lessons), written-answer grading
(quotes the learner, says what was missing and what full credit includes), and
key review (an internal answer-key check the learner never sees). The tangent
chat is a place to explore and stays one.

Every changed prompt keeps its opening words, which the test models route on.

## Sources

New sources; the rest are listed in the lessons design.

- **[ARM03]** Atkinson, Renkl & Merrill (2003). Transitioning from studying examples to solving problems: effects of self-explanation prompts and fading worked-out steps. *Journal of Educational Psychology* 95(4). doi:10.1037/0022-0663.95.4.774
- **[K07a]** Kang, McDermott & Roediger (2007). Test format and corrective feedback modify the effect of testing on long-term retention. *European Journal of Cognitive Psychology* 19(4–5). doi:10.1080/09541440601056620
- **[KLL18]** Koh, Lee & Lim (2018). The learning benefits of teaching: a retrieval practice hypothesis. *Applied Cognitive Psychology* 32(3). doi:10.1002/acp.3410
- **[L12]** Little, Bjork, Bjork & Angello (2012). Multiple-choice tests exonerated, at least of some charges. *Psychological Science* 23(11). doi:10.1177/0956797612443370
- **[M07]** McDaniel, Roediger & McDermott (2007). Generalizing test-enhanced learning from the laboratory to the classroom. *Psychonomic Bulletin & Review* 14(2). doi:10.3758/BF03194052
- **[RD11]** Rawson & Dunlosky (2011). Optimizing schedules of retrieval practice for durable and efficient learning: how much is enough? *Journal of Experimental Psychology: General* 140(3). doi:10.1037/a0023956
- **[R20]** Rohrer, Dedrick, Hartwig & Cheung (2020). A randomized controlled trial of interleaved mathematics practice. *Journal of Educational Psychology* 112(1). doi:10.1037/edu0000367
