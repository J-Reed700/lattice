# Lessons built on learning science

Status: implemented. Research: 2026-10-25; every source below was checked
against Crossref by its DOI.

A lesson used to be a long document to read: explanations and worked examples,
a guided exercise, an assignment, a reflection and a recap, then a quiz at the
end. Reading is the weakest way to learn something that lasts. This design
rebuilds the lesson around what the research supports: the learner does the
thinking at every step, and what they learn comes back later from memory.

The six-step idea the owner brought (establish one mental model, walk one
concrete example, contrast a deceptively similar example, reason through a
case, correct misunderstandings, revisit from memory) is the spine. It is used
as ingredients, not a fixed template: a contrast appears where a real
confusion exists, and the number of ideas follows the objective.

## 1. Decisions at a glance

| Question | Decision |
|---|---|
| What a lesson is built from | Two or three core ideas, each explained, worked and checked; at least one is set beside a near-miss. Then guided practice, the assignment, explaining from memory, and the recap. |
| Where the learner thinks | Before an idea a newcomer would likely guess wrong, a prediction the explanation then answers; in every check, a committed answer to a fresh case before its reasoning is shown. |
| How misunderstandings are corrected | Each check's reveal names the misconception behind each plausible wrong answer; quiz distractors are real misconceptions. |
| How it comes back later | Rating a check schedules it as an FSRS review card. Due cards open the next lesson as a warm-up. |
| How the recap is used | The learner explains the lesson from memory first; the recap is what they compare with. |
| What changed in storage | Two section kinds, `contrast` and `check`. A check's reveal sits in its body under one heading line, so verification, repair and packs read it like any section. No migration: `learning_blocks.kind` is unconstrained text. |
| Versions | None. Lessons in the old shape are not supported; the course has no lessons to keep. The authoring requirements are part of the draft cache key, so a changed rule discards old drafts without a version tag. |
| What is not claimed | No retention percentages and no mastery. The guide states each technique's evidence strength and its limits. |

## 2. What the research supports

The two strongest findings are retrieval and spacing. Dunlosky et al.'s review
of ten common techniques rated practice testing and distributed practice of
high utility. It rated elaborative interrogation, self-explanation and
interleaving moderate, and summarization, highlighting, the keyword mnemonic,
imagery for text and rereading low [D13].

| Finding | Strength | What the lesson does | Limit |
|---|---|---|---|
| Retrieving information strengthens memory more than restudying it; the benefit grows with delay and with feedback [R06, A17]. | Strong | Checks, explaining from memory, warm-ups, quizzes. | Benefits are measured on retention and near transfer; far transfer is less certain. |
| Spacing study across sessions beats massing it. The best gap grows with the retention interval [C06, C08]; equal and expanding gaps both work long term [K07]. | Strong | FSRS schedules every card from the learner's answers; warm-ups surface due cards. | FSRS defaults are not personal parameters; an interval is not a retention promise. |
| Worked examples help novices more than unguided problem solving [A00]; the benefit shrinks as expertise grows [K03]. | Strong for novices | Each idea has a worked example before its check. Its detail follows the learner's stated prior knowledge (every step for a newcomer, only the new steps for someone who knows the basics), and guidance fades from example to guided practice to the assignment. | Prior knowledge is what the learner said, not measured. |
| Comparing cases side by side improves learning and transfer [A13, G03, RJ07]. Interleaving helps most when the categories are similar and easy to confuse [BR19, RT07]. | Moderate | Contrast sections set an idea beside a near-miss and name the one feature that differs. In a lesson with two or more ideas, the last check mixes them: the learner first decides which idea, this lesson's or an earlier one's, applies. The lesson quiz draws items from earlier lessons by their own outcomes ([practice and quizzes](2026-10-26-learning-science-practice-and-quizzes.md)). | Interleaving dissimilar material can help less or not at all [BR19]. |
| Explaining material to oneself improves understanding [B18]; explaining to others helps most when it requires recall [F13]. Generative activities beat rereading [F16]. | Moderate | Explaining from memory before the recap; checks ask "why". | Effects vary with prompt quality. |
| Attempting an answer before learning it, even wrongly, improves later learning when feedback follows [RK09, P23]. Confident errors, once corrected, are corrected especially well [BM01]. | Moderate | Where a newcomer would plausibly guess wrong, an explanation opens with a prediction question about the idea it is about to teach and then answers it, naming the tempting wrong guess and why it fails; asked only there, so lessons do not open every idea the same way. A check asks for a committed answer; "I don't know yet" is allowed, but answering first is encouraged. Checks are pitched so a learner who followed the lesson usually succeeds, about four times in five [R12]. | The benefit depends on feedback following the attempt, which is why the explanation answers the prediction at once. |
| Feedback that addresses the task and how to improve is among the strongest influences on learning [HT07]. | Strong | Reveals and quiz explanations name the misconception, not just the right answer. | Feedback that only grades has weak effects. |
| Interpolated tests during instruction reduce mind wandering and improve learning [S13]. | Moderate | Checks sit inside the lesson, after each idea. | Studied with video lectures. |
| Words with relevant pictures beat words alone [CP91, W18]. | Moderate | A diagram when an idea has structure; none decorative. | Decorative or redundant visuals hurt. |
| Matching instruction to "learning styles" has no good evidence [P08]. Handwriting notes beating laptops did not replicate [M19]. Pomodoro-style breaks mainly help fatigue and mood [B23]. | — | Not built. The guide says so. | — |
| Sleep supports consolidation [RB13]; aerobic exercise can benefit memory [R13]. | Moderate | A private habits checklist, never a score. | Effects vary by person and timing. |

## 3. The lesson

```
Warm up from memory (due cards, if any)
  for each core idea (two or three):
    The idea             explanation: one mental model, a diagram if structural
    Worked example       one concrete case, step by step, with the likely mistake
    Don't confuse it with   contrast: a near-miss and the feature that differs   (≥ 1 per lesson)
    Your turn            check: commit an answer → reveal → rate → scheduled
Try it with guidance   guided practice with the tutor's hint ladder
Your assignment        independent practice, saved as a practice task
From memory            reflection: explain the lesson without looking
Key takeaways          recap: opens once the explanation is written
Lesson quiz
```

### 3.1 Warm-up

`LessonWarmUp` shows up to three due review cards from the course's deck,
longest overdue first, before an unfinished lesson in an active course. Each
is answered from memory, revealed and rated; the rating is an ordinary FSRS
review (`review_learning_recall_card`). It can be skipped, and it disappears
when nothing is due. The queue is fixed when the lesson opens, so reviewing
one card does not reshuffle the rest. A missed card (Forgot, or a wrong
choice) comes back after the others until it is recalled once or has had four
attempts, as in a study session (successive relearning [RD22]); each attempt
is its own review.

### 3.2 Contrast

A `contrast` section puts a deceptively similar case beside the idea: one that
looks the same on the surface and differs in the one feature that changes the
outcome. It shows both, names that feature, and says how to tell them apart.
The authoring rules ask for a near-miss drawn from mistakes learners actually
make; the teaching review rejects a contrast whose near-miss does not differ
in the feature it names.

### 3.3 Check

A `check` section poses a fresh case: predict, decide or explain, never repeat
a sentence. Its body is the case, then one line `### Check your thinking`
(`CHECK_REVEAL`), then the answer in one or two sentences, the reasoning, and
the misconception behind each plausible wrong answer. `lesson_rules` holds the
format: exactly one reveal line, at least 80 characters on each side. It also
requires a worked example since the previous check, so each check applies an
idea the lesson has just worked through. `checkReveal.ts` splits it the same
way.

Keeping the reveal in the body, not a separate field, is deliberate: claim
extraction, verification, section repair, teaching review and packs all read
sections as title and body, so the reveal is verified like any teaching.
Because a reveal states misconceptions in order to reject them, claim
extraction is told so for that section (`inventory::inputs` adds a
`checkRole`, as it marks a quiz's wrong options as unendorsed). The extractor
records what the lesson asserts about a misconception, that it fails and why,
never the misconception as a fact; otherwise verification would find it false
and a repair would rewrite it into a true statement, removing the lesson. Code
in contrasts and checks runs during verification exactly like a worked
example's (`execution::shows_examples`); a `mermaid` fence is a diagram, never
reported as unexecuted code.

`LessonCheck` shows the case and an answer box. "Check my thinking" opens once
something is written; "I don't know yet: show me" opens it anyway. The reveal
then asks how close the answer was: Missed it, Partly or Got it (FSRS Again,
Hard, Good; Easy is not offered on first sight). Choosing one creates a
question-and-answer card and records that first review. The case is the
prompt, the reveal's first paragraph the answer, and the rest of the
reasoning the explanation, each clipped at a sentence to the card's bounds.
The card cites the section's source versions. Its id is a version 5 UUID of
the lesson, section and body, so answering again finds the same card and a
rewritten check names a new one. The save and the review carry operation and
review ids, so a retry never creates or rates twice.

What the learner typed is kept on the device (`useLessonDraft`), with whether
they revealed it, so leaving and returning keeps their thinking.

### 3.4 From memory, then the recap

The reflection asks the learner to explain the lesson as if teaching someone
new, without looking back: three to five prompts, no answers.
`LessonFromMemory` gives them a box for it. The recap after it stays closed
until they choose "Compare with the key takeaways" (or "Show the takeaways
now"). The recap holds one takeaway per idea and the distinction the contrast
drew, so the comparison shows what they recalled and what they missed.

### 3.5 Revisit

Checks become review cards when rated, and their first review sets their
first gap: a miss comes back tomorrow, a good answer in a few days. From then
on FSRS schedules them like any card, in Recall and in warm-ups. Quiz misses
already join Recall (quizzes design §4.5). A card from a check carries the
lesson's Markdown, so every place a card is read (`CardText`: the warm-up,
Recall, the memory panel and Study flashcards) renders it like the lesson.
Compact rows show a one-line plain preview (`cardPreview`).

## 4. Authoring

`lesson_prompt` states the structure as requirements, not a template: two or
three core ideas, each explained, worked and checked; a prediction question
opening an explanation where a newcomer would likely guess wrong, answered by
it; each idea met in at least two
concrete cases; guidance matched to prior knowledge and fading toward the
assignment; a contrast where learners plausibly confuse the idea; diagrams
only for structure; a check's format, with at least one "why or how" check
per idea, a pitch most learners meet, and a last check that mixes the ideas;
explaining from memory before the recap; quiz distractors that are real
misconceptions. A lesson's recall cards (`recall_draft_prompt`) are
retrievals, not recognition: they mix stating an idea, explaining why it
works, applying it to a case whose answer follows from the quoted passage,
and telling contrasted ideas apart, never give their answer away in the cue,
and explain the common mistake with the answer. The schema allows 10 to 16 sections of eight kinds.
`lesson_rules` reports the mix (at least two explanations, two worked
examples, one contrast and two checks; exactly one guided practice,
independent practice, reflection and recap), the check format, and the order.
It also reports a check with no worked example since the previous check, and a
recap before the reflection. As before, a broken rule is a finding for repair and then for
the user; it never stops publication. Minimum lengths: 600 characters for
explanations, examples and practice, 300 for contrasts and checks, 150 for
reflections and recaps.

Teaching review applies these rules only under the lesson's own schema, so
the retained-lesson eval (`tests/teaching_course_evals.rs`) reviews with
`generation::lesson_schema` rather than a copy, and asserts the reviewed
lesson breaks none of them (`lesson_rule_notes`). A copy had drifted and
silently skipped every rule.

## 5. The guide

The "How to learn so it lasts" guide (`RetentionFrameworksModal`) replaces the
previous panels' figures ("~80% retention vs 34%", "prevents 70%+ decay",
"saves up to 60%", "Harvard Summer School Retention Principles", "Mayo Clinic
clinical guidelines"). None of those came from the literature, and they
contradicted the studio's rule that no retention figure is promised. Each
technique now states what to do, its evidence strength, what research found
with its limits, DOI-linked citations, and where the lessons build it in. The
habits tab is a private checklist with cited one-line reasons, and a short list
of what the evidence does not support.

## 6. Later

- Model feedback on a check answer (quoting the learner's words, as practice
  grading does), for cases where self-rating is unreliable.
- Confidence ratings before reveal, to use hypercorrection deliberately.
- Delayed transfer checks in a changed context (studio design §3.2.3).
- FSRS parameter optimisation once a learner has enough review history.

## 7. Sources

- **[A00]** Atkinson, Derry, Renkl & Wortham (2000). Learning from examples. *Review of Educational Research* 70(2). doi:10.3102/00346543070002181
- **[A13]** Alfieri, Nokes-Malach & Schunn (2013). Learning through case comparisons: a meta-analytic review. *Educational Psychologist* 48(2). doi:10.1080/00461520.2013.775712
- **[A17]** Adesope, Trevisan & Sundararajan (2017). Rethinking the use of tests: a meta-analysis of practice testing. *Review of Educational Research* 87(3). doi:10.3102/0034654316689306
- **[B18]** Bisra, Liu, Nesbit, Salimi & Winne (2018). Inducing self-explanation: a meta-analysis. *Educational Psychology Review* 30. doi:10.1007/s10648-018-9434-x
- **[B23]** Biwer et al. (2023). Understanding effort regulation: comparing Pomodoro breaks and self-regulated breaks. *British Journal of Educational Psychology*. doi:10.1111/bjep.12593
- **[BM01]** Butterfield & Metcalfe (2001). Errors committed with high confidence are hypercorrected. *JEP: LMC* 27(6). doi:10.1037/0278-7393.27.6.1491
- **[BR19]** Brunmair & Richter (2019). Similarity matters: a meta-analysis of interleaved learning and its moderators. *Psychological Bulletin* 145(11). doi:10.1037/bul0000209
- **[C06]** Cepeda, Pashler, Vul, Wixted & Rohrer (2006). Distributed practice in verbal recall tasks. *Psychological Bulletin* 132(3). doi:10.1037/0033-2909.132.3.354
- **[C08]** Cepeda, Vul, Rohrer, Wixted & Pashler (2008). Spacing effects in learning: a temporal ridgeline of optimal retention. *Psychological Science* 19(11). doi:10.1111/j.1467-9280.2008.02209.x
- **[CP91]** Clark & Paivio (1991). Dual coding theory and education. *Educational Psychology Review* 3(3). doi:10.1007/BF01320076
- **[D13]** Dunlosky, Rawson, Marsh, Nathan & Willingham (2013). Improving students' learning with effective learning techniques. *Psychological Science in the Public Interest* 14(1). doi:10.1177/1529100612453266
- **[F13]** Fiorella & Mayer (2013). The relative benefits of learning by teaching and teaching expectancy. *Contemporary Educational Psychology* 38(4). doi:10.1016/j.cedpsych.2013.06.001
- **[F16]** Fiorella & Mayer (2016). Eight ways to promote generative learning. *Educational Psychology Review* 28. doi:10.1007/s10648-015-9348-9
- **[G03]** Gentner, Loewenstein & Thompson (2003). Learning and transfer: a general role for analogical encoding. *Journal of Educational Psychology* 95(2). doi:10.1037/0022-0663.95.2.393
- **[HT07]** Hattie & Timperley (2007). The power of feedback. *Review of Educational Research* 77(1). doi:10.3102/003465430298487
- **[K03]** Kalyuga, Ayres, Chandler & Sweller (2003). The expertise reversal effect. *Educational Psychologist* 38(1). doi:10.1207/S15326985EP3801_4
- **[K07]** Karpicke & Roediger (2007). Expanding retrieval practice promotes short-term retention, but equally spaced retrieval enhances long-term retention. *JEP: LMC* 33(4). doi:10.1037/0278-7393.33.4.704
- **[M19]** Morehead, Dunlosky & Rawson (2019). How much mightier is the pen than the keyboard for note-taking? *Educational Psychology Review* 31. doi:10.1007/s10648-019-09468-2
- **[P08]** Pashler, McDaniel, Rohrer & Bjork (2008). Learning styles: concepts and evidence. *Psychological Science in the Public Interest* 9(3). doi:10.1111/j.1539-6053.2009.01038.x
- **[P23]** Pan & Carpenter (2023). Prequestioning and pretesting effects: a review. *Educational Psychology Review* 35. doi:10.1007/s10648-023-09814-5
- **[R06]** Roediger & Karpicke (2006). Test-enhanced learning. *Psychological Science* 17(3). doi:10.1111/j.1467-9280.2006.01693.x
- **[R13]** Roig, Nordbrandt, Geertsen & Nielsen (2013). The effects of cardiovascular exercise on human memory: a review with meta-analysis. *Neuroscience & Biobehavioral Reviews* 37(8). doi:10.1016/j.neubiorev.2013.06.012
- **[RB13]** Rasch & Born (2013). About sleep's role in memory. *Physiological Reviews* 93(2). doi:10.1152/physrev.00032.2012
- **[R12]** Rosenshine (2012). Principles of instruction: research-based strategies that all teachers should know. *American Educator* 36(1), 12–19, 39. (No DOI.)
- **[RD22]** Rawson & Dunlosky (2022). Successive relearning: an underexplored but potent technique for obtaining and maintaining knowledge. *Current Directions in Psychological Science* 31(4). doi:10.1177/09637214221100484
- **[RJ07]** Rittle-Johnson & Star (2007). Does comparing solution methods facilitate conceptual and procedural knowledge? *Journal of Educational Psychology* 99(3). doi:10.1037/0022-0663.99.3.561
- **[RK09]** Richland, Kornell & Kao (2009). The pretesting effect: do unsuccessful retrieval attempts enhance learning? *JEP: Applied* 15(3). doi:10.1037/a0016496
- **[RT07]** Rohrer & Taylor (2007). The shuffling of mathematics problems improves learning. *Instructional Science* 35. doi:10.1007/s11251-007-9015-8
- **[S13]** Szpunar, Khan & Schacter (2013). Interpolated memory tests reduce mind wandering and improve learning of online lectures. *PNAS* 110(16). doi:10.1073/pnas.1221764110
- **[W18]** Weinstein, Madan & Sumeracki (2018). Teaching the science of learning. *Cognitive Research: Principles and Implications* 3. doi:10.1186/s41235-017-0087-y
- **[FSRS]** Ye, Su & Cao (2022). A stochastic shortest path algorithm for optimizing spaced repetition scheduling. *KDD '22*. doi:10.1145/3534678.3539081
