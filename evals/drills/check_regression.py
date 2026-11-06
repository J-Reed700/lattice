#!/usr/bin/env python3
"""Compare a drill grading eval's report with a baseline of earlier runs on the same model.

    python -X utf8 -I evals/drills/check_regression.py baseline REPORT.json [REPORT.json ...] > baseline.json
    python -X utf8 -I evals/drills/check_regression.py check baseline.json REPORT.json

A model's answers vary between runs, so a drop counts only past the noise: 2 items, or the spread of wrong items
between the baseline's runs when that is wider, taken from its worst run. Rates are compared, so a changed case set still compares.
An item every baseline run got right and this run got wrong is listed to read by hand; it fails nothing on its own.
Exit status: 0 no regression, 1 a regression, 2 the reports cannot be compared.
"""
import json
import sys
from dataclasses import dataclass, field

KINDS = ("issues", "findings")
SMALLEST_NOISE = 2
AGREEMENT_BAR = 0.9
TOLERANCE = 1e-9


def _item(disagreement):
    return f"{disagreement['case']}/{disagreement['kind']}/{disagreement['index']}"


def _rate(tally):
    if not tally["total"]:
        raise ValueError("a report holds no items of a kind, so it cannot be compared")
    return tally["agree"] / tally["total"]


def make_baseline(reports):
    """What later runs are compared with: each kind's worst run and spread, and every item any run got wrong."""
    if not reports:
        raise ValueError("a baseline needs at least one report")
    models = {report["model"]["name"] for report in reports}
    if len(models) != 1:
        raise ValueError(f"a baseline is of one model, these are of {sorted(models)}")
    baseline = {
        "model": models.pop(),
        "runs": len(reports),
        "failed": max(report["failed"] for report in reports),
        "ever_wrong": sorted({_item(item) for report in reports for item in report["disagreements"]}),
    }
    for kind in KINDS:
        rates = [_rate(report[kind]) for report in reports]
        wrong = [report[kind]["total"] - report[kind]["agree"] for report in reports]
        worst = min(range(len(reports)), key=rates.__getitem__)
        baseline[kind] = {
            "worst_agree": reports[worst][kind]["agree"],
            "total": reports[worst][kind]["total"],
            "worst_rate": rates[worst],
            "best_rate": max(rates),
            "spread": max(wrong) - min(wrong),
        }
    return baseline


@dataclass
class Result:
    failures: list = field(default_factory=list)
    to_read: list = field(default_factory=list)
    lines: list = field(default_factory=list)


def check(baseline, report):
    """Whether `report` regressed from `baseline`. A report of another model is not comparable."""
    model = report["model"]["name"]
    if model != baseline["model"]:
        raise ValueError(f"the baseline is of {baseline['model']}, this report of {model}")
    result = Result()
    for kind in KINDS:
        then = baseline[kind]
        rate = _rate(report[kind])
        noise = max(SMALLEST_NOISE, then["spread"]) / then["total"]
        floor = then["worst_rate"] - noise
        result.lines.append(
            f"{kind}: {report[kind]['agree']} of {report[kind]['total']} ({rate:.3f}), "
            f"baseline worst {then['worst_rate']:.3f}, floor {floor:.3f}, bar {AGREEMENT_BAR}"
        )
        if rate < floor - TOLERANCE:
            result.failures.append(f"{kind} agreement {rate:.3f} is below {floor:.3f}")
    if report["failed"] > baseline["failed"]:
        result.failures.append(
            f"{report['failed']} cases not graded, the baseline's worst run had {baseline['failed']}"
        )
    known = set(baseline["ever_wrong"])
    result.to_read = sorted({_item(item) for item in report["disagreements"]} - known)
    return result


def _load(path):
    with open(path, encoding="utf-8") as file:
        return json.load(file)


def main(arguments):
    if len(arguments) >= 2 and arguments[0] == "baseline":
        json.dump(make_baseline([_load(path) for path in arguments[1:]]), sys.stdout, indent=2)
        print()
        return 0
    if len(arguments) == 3 and arguments[0] == "check":
        result = check(_load(arguments[1]), _load(arguments[2]))
        for line in result.lines:
            print(line)
        for item in result.to_read:
            print(f"read by hand, right in every baseline run: {item}")
        for failure in result.failures:
            print(f"REGRESSION: {failure}")
        print("regression" if result.failures else "no regression")
        return 1 if result.failures else 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except ValueError as error:
        print(f"cannot compare: {error}", file=sys.stderr)
        sys.exit(2)
