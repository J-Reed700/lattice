#!/usr/bin/env python3
"""Sweeps the two memory relevance floors over the rows `memory_floors_eval` wrote.

    python3 scripts/analyse_memory_floors.py ROWS.jsonl [--step 0.05]

Each row is one probe against one candidate (see crates/lattice-desktop/tests/memory_floors_eval.rs). The gates are
applied here, as the code combines their signals:

- recall (history_tools/recall.rs): the shipped search returned the candidate, and it is an exact identifier, or its
  lexical share of the turn is at least the recall floor, or its similarity is at least the shared floor;
- shared (repository/knowledge.rs): the item shares a word with the turn, or its similarity is at least the shared
  floor, where no score counts as 0.

Floors are compared as 32-bit floats, as the code compares them. The output is, per probe kind, the share of the
labelled-relevant candidates a floor lets through and the share of the irrelevant ones, then the floors the selection
rule of evals/conversation-memory/RECALL-FLOORS.md picks.
"""
import argparse
import json
import math
import struct
import sys
from dataclasses import dataclass

RECALL_KINDS = ("exact_restatement", "paraphrase", "follow_up", "instruction_only", "distractor")
SHARED_KINDS = ("shared_bearing", "shared_unrelated")
# The kinds whose candidates are all irrelevant, which the recall floor's rule counts.
RECALL_RULE_KINDS = ("instruction_only", "distractor")

# The floors at the commit the eval was written against, shown for comparison only.
SHIPPED_RECALL_FLOOR = 0.25
SHIPPED_SHARED_FLOOR = 0.6

# The selection rule's caps: shared items offered, and older messages recalled, that are irrelevant.
SHARED_CAP = 0.05
RECALL_CAP = 0.10

# The spacing of the floors the selection searches.
GRID_STEP = 0.001


def f32(value):
    """`value` as the 32-bit float the code holds it as."""
    return struct.unpack("f", struct.pack("f", value))[0]


def round_down(value, places=2):
    """`value` rounded down to `places` decimals."""
    scale = 10 ** places
    return math.floor(value * scale + 1e-9) / scale


def grid(top, step):
    """The floors from 0 to `top`, `step` apart."""
    return [round(i * step, 6) for i in range(int(round(top / step)) + 1)]


@dataclass(frozen=True)
class Tally:
    """Candidates a gate saw: how many it let through of each label."""

    relevant: int = 0
    kept: int = 0
    irrelevant: int = 0
    let_through: int = 0

    def kept_share(self):
        """The share of relevant candidates let through, or None when there were none."""
        return self.kept / self.relevant if self.relevant else None

    def let_through_share(self):
        """The share of irrelevant candidates let through, or None when there were none."""
        return self.let_through / self.irrelevant if self.irrelevant else None


def tally(rows, passes):
    """What the gate `passes` let through of `rows`."""
    relevant = kept = irrelevant = let_through = 0
    for row in rows:
        passed = bool(passes(row))
        if row["relevant"]:
            relevant += 1
            kept += passed
        else:
            irrelevant += 1
            let_through += passed
    return Tally(relevant, kept, irrelevant, let_through)


def lets_through_recall(row, recall_floor, shared_floor):
    """The recall gate: found, and exact, or enough of the turn's weight, or close enough in meaning."""
    if not row["found"]:
        return False
    if row["exact_identifier"]:
        return True
    share = row["lexical_share"]
    if share is not None and share >= recall_floor:
        return True
    similarity = row["similarity"]
    return similarity is not None and similarity >= shared_floor


def offers_shared(row, shared_floor):
    """The shared gate: a shared word, or enough closeness in meaning, an unscored item counting as 0."""
    if row["shares_word"]:
        return True
    similarity = row["similarity"]
    return (0.0 if similarity is None else similarity) >= shared_floor


def recall_gate(recall_floor, shared_floor):
    """The recall gate at two floors, as a test of one row."""
    recall_floor, shared_floor = f32(recall_floor), f32(shared_floor)
    return lambda row: lets_through_recall(row, recall_floor, shared_floor)


def shared_gate(shared_floor):
    """The shared gate at one floor, as a test of one row."""
    shared_floor = f32(shared_floor)
    return lambda row: offers_shared(row, shared_floor)


def embedding_model_of(rows):
    """The embedding model `rows` came from, refusing rows that name none or more than one."""
    models = {row.get("embedding_model") for row in rows}
    if len(models) != 1 or None in models or "" in models:
        raise SystemExit(f"these rows must name one embedding model, found {sorted(map(str, models))}")
    return models.pop()


def load_rows(lines):
    """The rows in `lines`, JSON objects one to a line, with their floats read as 32-bit floats."""
    rows = []
    for number, line in enumerate(lines, 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except ValueError as error:
            raise SystemExit(f"line {number} is not JSON: {error}")
        needed = ["probe", "kind", "gate", "candidate", "relevant", "similarity", "exact_identifier", "embedding_model"]
        if row.get("gate") == "recall":
            needed += ["found", "lexical_share"]
        elif row.get("gate") == "shared":
            needed += ["shares_word"]
        else:
            raise SystemExit(f"line {number} has gate {row.get('gate')!r}, not recall or shared")
        missing = [key for key in needed if key not in row]
        if missing:
            raise SystemExit(f"line {number} lacks {', '.join(missing)}")
        for key in ("similarity", "lexical_share"):
            if row.get(key) is not None:
                row[key] = f32(row[key])
        rows.append(row)
    if not rows:
        raise SystemExit("there are no rows")
    return rows


def lowest_floor(rows, gate_at, cap, top, step=GRID_STEP):
    """The lowest floor from 0 to `top` at which the gate `gate_at(floor)` lets through at most `cap` of `rows`.

    `rows` are all irrelevant. None when no floor in range does.
    """
    allowed = math.floor(cap * len(rows) + 1e-9)
    for floor in grid(top, step):
        passes = gate_at(floor)
        if sum(1 for row in rows if passes(row)) <= allowed:
            return floor
    return None


def unrelated_pool_items(rows):
    """The shared rows of items that do not bear on their turn."""
    return [row for row in rows if row["gate"] == "shared" and not row["relevant"]]


def rule_recall_rows(rows):
    """The irrelevant older messages of the instruction-only and distractor probes."""
    return [
        row
        for row in rows
        if row["gate"] == "recall" and row["kind"] in RECALL_RULE_KINDS and not row["relevant"]
    ]


def recall_top(rows):
    """The highest recall floor worth trying: past it no lexical share clears the floor."""
    shares = [row["lexical_share"] for row in rows if row["gate"] == "recall" and row["lexical_share"] is not None]
    return max([1.0] + [math.ceil(share * 20) / 20 for share in shares])


def choose_shared_floor(rows):
    """(lowest, rounded down) shared floor under the rule, or (None, None) when no floor meets it."""
    unrelated = unrelated_pool_items(rows)
    if not unrelated:
        return None, None
    lowest = lowest_floor(unrelated, shared_gate, SHARED_CAP, 1.0)
    return lowest, None if lowest is None else round_down(lowest)


def choose_recall_floor(rows, shared_floor):
    """(lowest, rounded down) recall floor under the rule with the meaning arm at `shared_floor`."""
    rule_rows = rule_recall_rows(rows)
    if not rule_rows:
        return None, None
    lowest = lowest_floor(rule_rows, lambda floor: recall_gate(floor, shared_floor), RECALL_CAP, recall_top(rows))
    return lowest, None if lowest is None else round_down(lowest)


def pct(count, total):
    """`count` of `total` as a percentage to one decimal, or a dash when there is no total."""
    return "-" if not total else f"{100 * count / total:.1f}"


def census(rows):
    """What the rows hold, per probe kind."""
    lines = [f"{'kind':<18}{'probes':>8}{'relevant':>10}{'irrelevant':>12}"]
    for kind in RECALL_KINDS + SHARED_KINDS:
        of_kind = [row for row in rows if row["kind"] == kind]
        if not of_kind:
            continue
        probes = len({row["probe"] for row in of_kind})
        relevant = sum(1 for row in of_kind if row["relevant"])
        lines.append(f"{kind:<18}{probes:>8}{relevant:>10}{len(of_kind) - relevant:>12}")
    return lines


def sweep_table(rows, kinds, gate_at, floors, pooled_label):
    """Per floor and kind: the share of relevant (rel) and of irrelevant (irr) candidates let through.

    The last column pools the irrelevant candidates the selection rule counts.
    """
    pooled_rows = unrelated_pool_items(rows) if kinds == SHARED_KINDS else rule_recall_rows(rows)
    names = [kind for kind in kinds if any(row["kind"] == kind for row in rows)]
    widths = {name: max(len(name), 11) for name in names}
    head = f"{'floor':<7}" + "".join(f"{name:^{widths[name]}}  " for name in names) + f"{pooled_label:^9}"
    sub = f"{'':<7}" + "".join(f"{'rel%':>{widths[name] - 6}} {'irr%':>5}  " for name in names) + f"{'irr%':>9}"
    lines = [head, sub]
    for floor in floors:
        passes = gate_at(floor)
        label = "off" if math.isinf(floor) else f"{floor:.2f}"
        cells = ""
        for name in names:
            t = tally([row for row in rows if row["kind"] == name], passes)
            cells += f"{pct(t.kept, t.relevant):>{widths[name] - 6}} {pct(t.let_through, t.irrelevant):>5}  "
        pooled = tally(pooled_rows, passes)
        lines.append(f"{label:<7}{cells}{pct(pooled.let_through, pooled.irrelevant):>9}")
    return lines


def describe(label, t, rule):
    """One line on what a gate did: the irrelevant `rule` let through and, when there are any, the relevant kept."""
    line = f"{label}: {pct(t.let_through, t.irrelevant)}% of {rule} let through (n={t.irrelevant})"
    if t.relevant:
        line += f", {pct(t.kept, t.relevant)}% of relevant kept (n={t.relevant})"
    return line


def recall_kept_by_kind(rows, gate):
    """The share of each recall kind's relevant candidates the gate kept."""
    parts = []
    for kind in RECALL_KINDS:
        t = tally([row for row in rows if row["gate"] == "recall" and row["kind"] == kind], gate)
        if t.relevant:
            parts.append(f"{kind} {pct(t.kept, t.relevant)}% (n={t.relevant})")
    return ", ".join(parts)


def report(rows, step=0.05):
    """The whole analysis as text."""
    model = embedding_model_of(rows)
    recall = [row for row in rows if row["gate"] == "recall"]
    shared = [row for row in rows if row["gate"] == "shared"]
    lines = [f"Embedding model: {model}", f"Rows: {len(rows)} ({len(recall)} recall, {len(shared)} shared)", ""]
    lines += census(rows)
    lines += ["", "rel% is the share of labelled-relevant candidates a floor lets through, irr% the share of the irrelevant ones."]

    shared_lowest, shared_floor = choose_shared_floor(rows)
    semantic = SHIPPED_SHARED_FLOOR if shared_floor is None else shared_floor
    floors = grid(1.0, step) + [math.inf]

    if shared:
        lines += ["", "Shared gate: a shared word, or similarity at least the shared floor ('off' leaves the shared word alone)."]
        lines += sweep_table(shared, SHARED_KINDS, shared_gate, floors, "unrelated")
    if recall:
        source = "the shared floor chosen below" if shared_floor is not None else "the shipped floor, since none was chosen"
        lines += [
            "",
            f"Recall gate: found, and exact, or lexical share at least the recall floor, or similarity at least {semantic:.2f}, "
            f"{source} ('off' leaves the other two arms alone).",
        ]
        lines += sweep_table(recall, RECALL_KINDS, lambda floor: recall_gate(floor, semantic), floors, "instr+dist")

    lines += ["", "Selection (the rule in evals/conversation-memory/RECALL-FLOORS.md)"]
    lines += _shared_selection(rows, shared_lowest, shared_floor)
    lines += _recall_selection(rows, semantic, shared_floor is not None)
    lines += ["", "Shipped floors, for comparison"]
    if shared:
        lines.append(
            "  "
            + describe(
                f"shared floor {SHIPPED_SHARED_FLOOR:.2f}",
                tally(shared, shared_gate(SHIPPED_SHARED_FLOOR)),
                "unrelated pool items",
            )
        )
    if recall:
        gate = recall_gate(SHIPPED_RECALL_FLOOR, SHIPPED_SHARED_FLOOR)
        lines.append(
            "  "
            + describe(
                f"recall floor {SHIPPED_RECALL_FLOOR:.2f} (shared {SHIPPED_SHARED_FLOOR:.2f})",
                tally(rule_recall_rows(rows), gate),
                "irrelevant older messages of instruction-only and distractor probes",
            )
        )
        lines.append("    relevant kept: " + recall_kept_by_kind(rows, gate))
    return "\n".join(lines)


def _shared_selection(rows, lowest, floor):
    unrelated = unrelated_pool_items(rows)
    if not unrelated:
        return ["  Shared floor: no unrelated pool items in these rows."]
    if floor is None:
        alone = tally(unrelated, shared_gate(math.inf))
        return [
            f"  Shared floor: no value up to 1.00 offers at most {SHARED_CAP:.0%} of unrelated pool items; "
            f"a shared word alone offers {pct(alone.let_through, alone.irrelevant)}% (n={alone.irrelevant})."
        ]
    at_floor = tally([row for row in rows if row["gate"] == "shared"], shared_gate(floor))
    lines = [
        f"  Shared floor: lowest value on the {GRID_STEP} grid with at most {SHARED_CAP:.0%} of unrelated pool items "
        f"offered is {lowest:.3f}, rounded down to {floor:.2f}.",
        "    " + describe(f"at {floor:.2f}", at_floor, "unrelated pool items"),
    ]
    if at_floor.let_through > math.floor(SHARED_CAP * at_floor.irrelevant + 1e-9):
        lines.append(f"    Rounding down put {floor:.2f} above the {SHARED_CAP:.0%} cap; {lowest:.3f} meets it.")
    return lines


def _recall_selection(rows, semantic, chosen):
    rule_rows = rule_recall_rows(rows)
    if not rule_rows:
        return ["  Recall floor: no instruction-only or distractor rows."]
    lowest, floor = choose_recall_floor(rows, semantic)
    note = "" if chosen else " (the shared floor was not chosen, so the shipped one)"
    if floor is None:
        alone = tally(rule_rows, recall_gate(math.inf, semantic))
        return [
            f"  Recall floor: no value up to {recall_top(rows):.2f} recalls at most {RECALL_CAP:.0%} of irrelevant older "
            f"messages; exact identifiers and similarity {semantic:.2f}{note} alone recall "
            f"{pct(alone.let_through, alone.irrelevant)}% (n={alone.irrelevant})."
        ]
    gate = recall_gate(floor, semantic)
    at_floor = tally(rule_rows, gate)
    lines = [
        f"  Recall floor: lowest value on the {GRID_STEP} grid with at most {RECALL_CAP:.0%} of irrelevant older "
        f"messages recalled for instruction-only and distractor probes, similarity arm at {semantic:.2f}{note}, "
        f"is {lowest:.3f}, rounded down to {floor:.2f}.",
        "    " + describe(f"at {floor:.2f}", at_floor, "irrelevant older messages"),
        "    relevant kept: " + recall_kept_by_kind(rows, gate),
    ]
    overall = tally([row for row in rows if row["gate"] == "recall"], gate)
    lines.append(f"    relevant kept overall: {pct(overall.kept, overall.relevant)}% (n={overall.relevant})")
    if at_floor.let_through > math.floor(RECALL_CAP * at_floor.irrelevant + 1e-9):
        lines.append(f"    Rounding down put {floor:.2f} above the {RECALL_CAP:.0%} cap; {lowest:.3f} meets it.")
    return lines


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("rows", help="the JSONL file memory_floors_eval wrote")
    parser.add_argument("--step", type=float, default=0.05, help="spacing of the floors printed (default 0.05)")
    args = parser.parse_args(argv)
    if not 0 < args.step <= 1:
        parser.error("--step must be above 0 and at most 1")
    with open(args.rows, encoding="utf-8") as handle:
        rows = load_rows(handle)
    print(report(rows, args.step))


if __name__ == "__main__":
    sys.exit(main())
