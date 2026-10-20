#!/usr/bin/env python3
"""Small-sample statistics for shipped-path runs: paired comparison and judge calibration.

`shipped_path_eval.py score` grades one run. This script answers the two
questions a grade alone cannot, with exact tests that stay honest on tens of
questions (see `exact_stats.py`):

  compare --baseline RUN... --candidate RUN...
      Is the candidate better, worse, or indistinguishable from noise? The
      question is the unit: each is paired with itself across the two arms and
      only the questions whose outcome changed carry evidence (exact McNemar /
      sign test).

  calibration-sample QUESTIONS ANSWERS SCORED OUT
      Draws a stratified sample of the judge's verdicts (judge pass, judge
      fail) into a labelling file, sized from a stated precision target.

  calibration-score LABELS [--judge SCORED]
      Once `human_verdict` is filled in: a judge's accuracy, its false-pass
      and false-fail rates, and the run's pass rate corrected for the judge's
      errors (prediction-powered inference) with an exact interval. The rows
      carry no verdicts; the judge's are looked up from the sampled run, or
      from any other scored run of the same answers given with --judge.

The recipe is in evals/retrieval/EVALUATION_PROTOCOL.md.
"""
import argparse
import hashlib
import json
import random
from pathlib import Path

from chat_rag_eval import load_jsonl
from eval_integrity import unique_records, wilson
from exact_stats import (ALPHA, fewest_changed_to_call, fewest_to_call, hypergeometric_interval,
                         sign_test_p)
from shipped_path_eval import rubric_lines

ROOT = Path(__file__).resolve().parent.parent
LABELS_SCHEMA = "lattice-judge-calibration/2"
OUTCOMES = ("judge", "mechanical", "both")
STRATA = ("judge pass", "judge fail")
# Items with a turn that produced no answer: the judge was never asked, and
# they fail whoever grades them, so they are a known stratum needing no labels.
UNANSWERED = "unanswered"

# The default precision target for the corrected pass rate, as a share of the
# uncertainty the question set already carries (its Wilson 95% half-width).
# Labels can only remove the judge's error, not the question-sampling error, so
# labelling past the point where the judge's term is small next to the other
# buys little. At one half, the judge's residual error widens the combined
# (root-sum-square) interval by at most sqrt(1 + 0.5**2) - 1, about 12%.
JUDGE_NOISE_SHARE = 0.5

# For the labelling-time estimate only. 200 words a minute is the slow end of
# adult silent reading, the pace for checking claims against a rubric; each
# turn then needs a moment to decide.
READING_WORDS_PER_MINUTE = 200
DECISION_SECONDS_PER_TURN = 15

# Seed for the calibration draw, recorded in the labelling file so the same
# run always gives the same sample.
DEFAULT_SEED = 2026


# --- reading scored rows -----------------------------------------------------

def item_judge(row):
    """The item's judge verdict.

    Rows scored before 2026-10-19 carry three greedy grades per turn as
    `judge_pass_k`. Greedy grades of one input are one verdict repeated (they
    never split in those runs), so that field is read as the verdict it was.
    """
    for key in ("judge_pass", "judge_pass_k"):
        if key in row:
            return bool(row[key])
    raise ValueError(f"{row['query_id']} has no judge verdict; score the run with --endpoint")


def turn_judge(turn_score):
    """(passed, reason, graded) for one turn; `graded` is False when the turn
    had no answer for the judge to read."""
    judge = turn_score.get("judge")
    if judge is None:
        raise ValueError("a turn has no judge verdict; score the run with --endpoint")
    if "pass" in judge:
        return bool(judge["pass"]), judge.get("reason", ""), bool(judge.get("graded", True))
    verdicts = judge.get("verdicts") or []
    return (bool(judge.get("pass_k")), verdicts[0]["reason"] if verdicts else "no answer to grade",
            bool(verdicts))


def stratum(row):
    if not all(turn_judge(t)[2] for t in row["turn_scores"]):
        return UNANSWERED
    return STRATA[0] if item_judge(row) else STRATA[1]


def outcome(row, kind):
    if kind == "judge":
        return item_judge(row)
    if kind == "mechanical":
        return bool(row["mechanical_checks_passed"])
    return item_judge(row) and bool(row["mechanical_checks_passed"])


# --- paired comparison -------------------------------------------------------

def compare(baseline_runs, candidate_runs, kind="judge", alpha=ALPHA):
    """Paired comparison of two arms, each one or more scored runs of a question set.

    A question's outcome in an arm is the share of that arm's runs it passed;
    with one run per arm that is pass or fail and the test is exact McNemar.
    Only questions present, with the same number of turns, in every run are
    paired; the rest are listed, never silently dropped.
    """
    if kind not in OUTCOMES:
        raise ValueError(f"outcome must be one of {OUTCOMES}")
    if not baseline_runs or not candidate_runs:
        raise ValueError("each arm needs at least one scored run")
    runs = [unique_records(run, "query_id") for run in baseline_runs + candidate_runs]
    ids = [qid for qid in runs[0] if all(qid in run for run in runs)]
    every = set().union(*runs)
    unpaired = sorted(qid for qid in every if not all(qid in run for run in runs))
    changed = sorted(qid for qid in ids if len({len(run[qid]["turn_scores"]) for run in runs}) > 1)
    ids = [qid for qid in ids if qid not in changed]
    split = len(baseline_runs)

    def share(arm, qid):
        return sum(outcome(run[qid], kind) for run in arm) / len(arm)

    questions = []
    for qid in ids:
        before, after = share(runs[:split], qid), share(runs[split:], qid)
        questions.append({"query_id": qid, "baseline": before, "candidate": after,
                          "change": "better" if after > before else "worse" if after < before else "same"})
    wins = sum(q["change"] == "better" for q in questions)
    losses = sum(q["change"] == "worse" for q in questions)
    p = sign_test_p(wins, losses)
    if p <= alpha and wins != losses:
        verdict = "better" if wins > losses else "worse"
    else:
        verdict = "can't tell from noise"

    def per_run(arm):
        return [{"passed": sum(outcome(run[qid], kind) for qid in ids), "total": len(ids),
                 "rate": (sum(outcome(run[qid], kind) for qid in ids) / len(ids)) if ids else None,
                 "wilson95": wilson(sum(outcome(run[qid], kind) for qid in ids), len(ids))}
                for run in arm]

    single = len(baseline_runs) == 1 and len(candidate_runs) == 1
    return {
        "outcome": kind,
        "alpha": alpha,
        "paired": len(ids),
        "unpaired": unpaired,
        "changed_items": changed,
        "questions": questions,
        "discordant": {"better": wins, "worse": losses},
        "concordant": ({"both pass": sum(q["baseline"] == q["candidate"] == 1 for q in questions),
                        "both fail": sum(q["baseline"] == q["candidate"] == 0 for q in questions)}
                       if single else {"same": len(questions) - wins - losses}),
        "p_value": p,
        "fewest_one_way_to_call": fewest_to_call(wins + losses, alpha),
        "fewest_changed_to_call": fewest_changed_to_call(alpha),
        "per_run_pass_rates": {"baseline": per_run(runs[:split]), "candidate": per_run(runs[split:])},
        "verdict": verdict,
    }


def compare_markdown(report):
    lines = [f"Outcome: {report['outcome']}. Paired questions: {report['paired']}."]
    if report["unpaired"]:
        lines.append(f"Not in every run (left out): {', '.join(report['unpaired'])}.")
    if report["changed_items"]:
        lines.append(f"Turn count differs between runs (left out): {', '.join(report['changed_items'])}.")
    lines += ["", "| question | baseline | candidate | change |", "|---|---:|---:|---|"]
    for q in report["questions"]:
        lines.append(f"| {q['query_id']} | {q['baseline']:.2g} | {q['candidate']:.2g} | {q['change']} |")
    lines.append("")
    for arm in ("baseline", "candidate"):
        rates = "; ".join(f"{r['passed']}/{r['total']}" for r in report["per_run_pass_rates"][arm])
        lines.append(f"- {arm} pass rate per run: {rates}")
    wins, losses = report["discordant"]["better"], report["discordant"]["worse"]
    lines.append(f"- discordant: {wins} better, {losses} worse; concordant: "
                 + ", ".join(f"{v} {k}" for k, v in report["concordant"].items()))
    lines.append(f"- exact two-sided p = {report['p_value']:.4g} (alpha {report['alpha']})")
    need = report["fewest_one_way_to_call"]
    if need is None:
        lines.append(f"- {wins + losses} changed questions can never reach alpha; it takes at least "
                     f"{report['fewest_changed_to_call']} all going one way")
    else:
        lines.append(f"- with {wins + losses} changed, a call needs at least {need} going one way")
    lines.append(f"- **verdict: {report['verdict']}**")
    return "\n".join(lines) + "\n"


# --- judge calibration -------------------------------------------------------

def allocate(sizes, total):
    """Proportional allocation of `total` labels over strata, largest remainder
    first, at least one per non-empty stratum and never more than it holds."""
    population = sum(sizes)
    if not 0 <= total <= population:
        raise ValueError("cannot sample more than the run holds")
    shares = [total * size / population if population else 0 for size in sizes]
    counts = [min(size, max(1 if size and total else 0, int(share))) for size, share in zip(sizes, shares)]
    order = sorted(range(len(sizes)), key=lambda i: shares[i] - int(shares[i]), reverse=True)
    while sum(counts) < total:
        for i in order:
            if sum(counts) < total and counts[i] < sizes[i]:
                counts[i] += 1
    while sum(counts) > total:
        i = max(range(len(sizes)), key=lambda j: counts[j] - shares[j])
        counts[i] -= 1
    return counts


def stratified_interval(strata, alpha=ALPHA):
    """Population rate of a stratified sample: Σ N_s/N · k_s/n_s, with an exact
    interval made from each stratum's exact hypergeometric interval at
    alpha / (strata not sampled whole) (Bonferroni), so coverage holds at any
    sample size. A stratum sampled whole is known exactly and spends no alpha.

    `strata` is a list of (population, sample, successes). The estimate is None
    while a non-empty stratum is unsampled; the interval is still honest.
    """
    strata = [s for s in strata if s[0]]
    population = sum(s[0] for s in strata)
    if not population:
        return None
    each = alpha / max(1, sum(1 for size, sample, _ in strata if sample < size))
    low = high = 0
    for size, sample, successes in strata:
        lo, hi = hypergeometric_interval(successes, sample, size, each)
        low, high = low + lo, high + hi
    estimate = (sum(size * successes / sample for size, sample, successes in strata) / population
                if all(sample for _, sample, _ in strata) else None)
    return {"estimate": estimate, "interval": [low / population, high / population]}


def planned_half_width(sizes, total, alpha=ALPHA, known=0):
    """The corrected pass rate's half-width with `total` labels, at the widest
    case: an even human split inside every stratum, as if the judge were no
    better than a coin. That assumes nothing about the judge, which is the
    thing being measured; a good judge gives a narrower interval than planned."""
    counts = allocate(sizes, total)
    strata = [(size, n, n // 2) for size, n in zip(sizes, counts)] + [(known, known, 0)]
    result = stratified_interval(strata, alpha)
    return (result["interval"][1] - result["interval"][0]) / 2


def plan_sample(sizes, half_width, alpha=ALPHA, known=0):
    """The fewest labels whose planned half-width meets `half_width`; `known`
    items are unanswered fails that need none."""
    low, high = sum(1 for size in sizes if size), sum(sizes)
    while low < high:
        middle = (low + high) // 2
        if planned_half_width(sizes, middle, alpha, known) <= half_width:
            high = middle
        else:
            low = middle + 1
    return low


def default_half_width(passes, total):
    lo, hi = wilson(passes, total)
    return JUDGE_NOISE_SHARE * (hi - lo) / 2


def calibration_sample(questions, answers, scored, half_width=None, seed=DEFAULT_SEED, alpha=ALPHA):
    items = unique_records(questions["items"], "id")
    rows = unique_records(answers, "query_id")
    graded = unique_records(scored, "query_id")
    by_stratum = {name: [] for name in STRATA + (UNANSWERED,)}
    for qid, row in graded.items():
        by_stratum[stratum(row)].append(qid)
    sizes = [len(by_stratum[name]) for name in STRATA]
    known = len(by_stratum[UNANSWERED])
    target = half_width if half_width is not None else default_half_width(sizes[0], sum(sizes) + known)
    total = plan_sample(sizes, target, alpha, known)
    counts = allocate(sizes, total)
    rng = random.Random(seed)
    chosen = [(name, qid) for name, count in zip(STRATA, counts)
              for qid in rng.sample(sorted(by_stratum[name]), count)]
    rng.shuffle(chosen)
    out = []
    for _, qid in chosen:
        turns = []
        for number, spec in enumerate(items[qid]["turns"]):
            asked = rows[qid]["turns"][number] if number < len(rows[qid]["turns"]) else {}
            turns.append({
                "turn": number + 1,
                "question": spec["ask"],
                "rubric": rubric_lines(spec["rubric"]),
                "answer": (asked.get("answer") or "(no answer)").splitlines(),
            })
        out.append({"query_id": qid, "turns": turns, "human_verdict": None, "human_note": ""})
    words = sum(len(" ".join([t["question"], *t["rubric"], *t["answer"]]).split())
                for row in out for t in row["turns"])
    turns = sum(len(row["turns"]) for row in out)
    minutes = words / READING_WORDS_PER_MINUTE + turns * DECISION_SECONDS_PER_TURN / 60
    return {
        "schema": LABELS_SCHEMA,
        "instructions": (
            "For each item, read every turn's question, rubric and answer, then set human_verdict "
            "to \"pass\" when every turn meets its rubric and \"fail\" otherwise. Grade on the "
            "rubric alone. Rows carry no judge verdict and no stratum, and their order is random: "
            "label in file order, and stopping early leaves a smaller valid sample. "
            "Put anything worth recording in human_note."),
        "alpha": alpha,
        "precision_target_half_width": target,
        "planned_half_width": planned_half_width(sizes, total, alpha, known),
        "seed": seed,
        "frame": {**{name: {"population": size, "sample": count}
                     for name, size, count in zip(STRATA, sizes, counts)},
                  UNANSWERED: {"population": known, "sample": 0}},
        "estimated_minutes": round(minutes),
        "items": out,
    }


def calibration_score(design_scored, labels, judge_scored=None, alpha=ALPHA):
    """Score a judge against the human labels.

    `design_scored` is the scored run the sample was drawn from: its judge's
    verdicts are the sampling strata. `judge_scored` is the judge being
    measured, any judge that graded the same answers (default: the same run).
    Each design stratum is split by the measured judge's verdict into cells;
    inside a cell the labelled items are still a simple random sample, so every
    rate below is a cell-weighted sum with exact per-cell intervals, and one
    labelled file serves every judge.
    """
    if labels.get("schema") != LABELS_SCHEMA:
        raise ValueError(f"labels are not {LABELS_SCHEMA}")
    design = unique_records(design_scored, "query_id")
    judged = unique_records(judge_scored, "query_id") if judge_scored is not None else design
    if set(judged) != set(design):
        raise ValueError("the judged run does not cover the same items as the sampled run")
    changed = sorted(qid for qid in design
                     if design[qid].get("answer_sha256") != judged[qid].get("answer_sha256")
                     or len(design[qid]["turn_scores"]) != len(judged[qid]["turn_scores"]))
    if changed:
        raise ValueError(f"the judged run graded different answers: {changed}")
    strata = {qid: stratum(row) for qid, row in design.items()}
    verdict = {qid: item_judge(judged[qid]) for qid in judged if strata[qid] != UNANSWERED}
    known = sum(s == UNANSWERED for s in strata.values())
    cells = {}
    for qid, passed in verdict.items():
        cells.setdefault((strata[qid], passed), {"population": 0, "labelled": 0, "human_pass": 0})
        cells[(strata[qid], passed)]["population"] += 1
    disagreements, unlabelled = [], 0
    for row in labels["items"]:
        qid = row["query_id"]
        if qid not in verdict:
            raise ValueError(f"{qid} is labelled but is not an answered item of the scored run")
        human = row.get("human_verdict")
        if human is None:
            unlabelled += 1
            continue
        if human not in ("pass", "fail"):
            raise ValueError(f"{qid}: human_verdict must be \"pass\", \"fail\" or null, not {human!r}")
        cell = cells[(strata[qid], verdict[qid])]
        cell["labelled"] += 1
        cell["human_pass"] += human == "pass"
        if (human == "pass") != verdict[qid]:
            disagreements.append({"query_id": qid, "judge": "pass" if verdict[qid] else "fail", "human": human,
                                  "note": row.get("human_note", "")})

    def over(selected, successes):
        return stratified_interval([(c["population"], c["labelled"], successes(c, passed))
                                    for (_, passed), c in cells.items() if selected(passed)], alpha)

    corrected = stratified_interval([(c["population"], c["labelled"], c["human_pass"]) for c in cells.values()]
                                    + [(known, known, 0)], alpha)
    items = len(strata)
    judge_rate = sum(verdict.values()) / items
    return {
        "alpha": alpha,
        "items": items,
        "unanswered": known,
        "labelled": sum(c["labelled"] for c in cells.values()),
        "unlabelled": unlabelled,
        "cells": [{"sampled_as": s, "judge": "pass" if passed else "fail", **c}
                  for (s, passed), c in sorted(cells.items())],
        "judge_accuracy": over(lambda passed: True,
                               lambda c, passed: c["human_pass"] if passed else c["labelled"] - c["human_pass"]),
        "false_pass_rate": over(lambda passed: passed, lambda c, passed: c["labelled"] - c["human_pass"]),
        "false_fail_rate": over(lambda passed: not passed, lambda c, passed: c["human_pass"]),
        "judge_pass_rate": judge_rate,
        "corrected_pass_rate": corrected,
        "correction": (corrected["estimate"] - judge_rate) if corrected and corrected["estimate"] is not None else None,
        "disagreements": disagreements,
    }


def calibration_markdown(report):
    def fmt(value):
        return "-" if value is None else f"{value:.2f}"

    def span(result):
        if result is None:
            return "- (empty stratum)"
        lo, hi = result["interval"]
        return f"{fmt(result['estimate'])} [{lo:.2f}, {hi:.2f}]"

    lines = [f"Labelled {report['labelled']} of {report['items']} items"
             + (f" ({report['unlabelled']} drawn but not yet labelled)" if report["unlabelled"] else "")
             + f"; {report['unanswered']} unanswered items count as known fails.",
             "", "| measure | estimate [exact 95%] |", "|---|---|",
             f"| judge accuracy (answered items) | {span(report['judge_accuracy'])} |",
             f"| false-pass rate (judge passes a human fails) | {span(report['false_pass_rate'])} |",
             f"| false-fail rate (judge fails a human passes) | {span(report['false_fail_rate'])} |",
             f"| judge pass rate | {report['judge_pass_rate']:.2f} |",
             f"| corrected pass rate (PPI) | {span(report['corrected_pass_rate'])} |", ""]
    if report["disagreements"]:
        lines.append("Disagreements: " + ", ".join(f"{d['query_id']} (judge {d['judge']}, human {d['human']})"
                                                     for d in report["disagreements"]))
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    pair = sub.add_parser("compare")
    pair.add_argument("--baseline", type=Path, nargs="+", required=True, help="scored run(s)")
    pair.add_argument("--candidate", type=Path, nargs="+", required=True, help="scored run(s)")
    pair.add_argument("--outcome", choices=OUTCOMES, default="judge")
    pair.add_argument("--json", type=Path, help="also write the report as JSON")
    draw = sub.add_parser("calibration-sample")
    draw.add_argument("questions", type=Path)
    draw.add_argument("answers", type=Path, help="the run's answer rows")
    draw.add_argument("scored", type=Path, help="the same run, scored with a judge")
    draw.add_argument("output", type=Path)
    draw.add_argument("--half-width", type=float,
                      help="precision target for the corrected pass rate; default: "
                           "JUDGE_NOISE_SHARE of the run's own Wilson half-width")
    draw.add_argument("--seed", type=int, default=DEFAULT_SEED)
    check = sub.add_parser("calibration-score")
    check.add_argument("labels", type=Path)
    check.add_argument("--judge", type=Path,
                       help="a scored run of the same answers by the judge to measure; "
                            "default: the run the sample was drawn from")
    check.add_argument("--json", type=Path, help="also write the report as JSON")
    args = parser.parse_args()

    if args.command == "compare":
        report = compare([load_jsonl(p) for p in args.baseline], [load_jsonl(p) for p in args.candidate],
                         args.outcome)
        print(compare_markdown(report))
    elif args.command == "calibration-sample":
        if args.output.exists():
            raise SystemExit(f"{args.output} exists; a labelling file is never overwritten")
        labels = calibration_sample(json.loads(args.questions.read_text()), load_jsonl(args.answers),
                                    load_jsonl(args.scored), args.half_width, args.seed)
        scored = args.scored.resolve()
        labels["scored_run"] = str(scored.relative_to(ROOT) if scored.is_relative_to(ROOT) else scored)
        labels["scored_sha256"] = hashlib.sha256(args.scored.read_bytes()).hexdigest()
        args.output.write_text(json.dumps(labels, indent=2, ensure_ascii=False) + "\n")
        frame = ", ".join(f"{v['sample']} of {v['population']} {k}" for k, v in labels["frame"].items()
                          if k != UNANSWERED)
        frame += f"; {labels['frame'][UNANSWERED]['population']} unanswered need no label"
        print(f"{args.output}: {frame}; planned half-width {labels['planned_half_width']:.3f} "
              f"for a target of {labels['precision_target_half_width']:.3f}; "
              f"about {labels['estimated_minutes']} minutes to label")
        return
    else:
        labels = json.loads(args.labels.read_text())
        design = Path(labels["scored_run"])
        design = design if design.is_absolute() else ROOT / design
        if labels.get("scored_sha256") != hashlib.sha256(design.read_bytes()).hexdigest():
            raise SystemExit(f"{design} has changed since {args.labels} was drawn from it")
        report = calibration_score(load_jsonl(design), labels,
                                   load_jsonl(args.judge) if args.judge else None)
        print(calibration_markdown(report))
    if args.json:
        args.json.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
