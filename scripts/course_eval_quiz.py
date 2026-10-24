#!/usr/bin/env python3
"""Quiz items for the course eval: one item shape, the app's answer normalisation, the deterministic reference
grader, the mechanical item checks and the renderings a judge and a labeller read.

Design: docs/design/2026-10-23-quizzes-and-tests.md, sections 3 (formats, keys, partial credit), 5.2 (the
deterministic validation rules) and 7.1 (the `quiz` stage). `course_eval.py` uses it for the stage;
`course_eval_grading.py` uses it for the grading set. No model is called here.

The eval item shape (`course-eval-quiz-bank/1`, written by the live hook and by hand in fixtures) is flat:

  common        id, format, prompt, points, explanation, source_version_id, source_quote, lesson_block_id, origin
  single_choice options, key [index]                    distractor_rationales (optional, one per option)
  true_false    options ["True", "False"], key [0 or 1]
  multi_select  options (4-6), key [2-3 indices]        distractor_rationales (optional)
  short_answer  variants (3-12), numeric {value, tolerance, unit_variants} or null
  cloze         prompt with {{1}}..{{n}}, blanks [[variants], ...] one list per blank
  matching      left (3-5), right (left or left + 1), key [right index per left]
  ordering      values in the correct order (3 or more)
  explanation   rubric [{id, title, max_points, levels {incomplete, adequate, strong}}], key_points, reference_answer

Responses use the form's field names: selected_indices, text, blank_responses, matches, ordered_values.
"""
import hashlib
import math
import re
import statistics
import string
import unicodedata

BANK_SCHEMA = "course-eval-quiz-bank/1"
FORMATS = ("single_choice", "multi_select", "true_false", "short_answer", "cloze", "matching", "ordering",
           "explanation")
CHOICE_FORMATS = ("single_choice", "multi_select", "true_false")
TRUE_FALSE = ["True", "False"]
LETTERS = string.ascii_uppercase
BLANK = re.compile(r"\{\{(\d+)\}\}")
ARTICLES = ("a", "an", "the")
RESPONSE_FIELD = {"single_choice": "selected_indices", "true_false": "selected_indices",
                  "multi_select": "selected_indices", "short_answer": "text", "explanation": "text",
                  "cloze": "blank_responses", "matching": "matches", "ordering": "ordered_values"}

# Section 5.2's validation rules, as numbers the eval re-checks; sizes are the engine's `validate_candidate`.
LEAK_NGRAM = 4                  # a key of four or more words leaks on any shared run of four
LEAK_MIN_CHARS = 3              # a key shorter than this ("B", "7") is not checked, as in the engine
LENGTH_BIAS_SHARE = 0.6         # the key may not be the unique longest option by more than 60 % of the median
NEAR_DUPLICATE_JACCARD = 0.8    # normalised stem token sets this similar are one item asked twice
ABSOLUTES = ("always", "never")  # true/false absolutes the lesson must itself state
# The answer-position check: a bank whose keys sit at one position more often than a fair shuffle would put
# them there, at this significance (exact, Bonferroni over positions). At 0.01 a fair shuffle false-flags at
# most one bank in a hundred, so a seven-lesson run sees a false flag in under 7 % of runs.
POSITION_ALPHA = 0.01
SIZES = {"single_choice_options": (2, 8), "multi_select_options": (4, 6), "multi_select_keys": (2, 3),
         "short_answer_variants": (1, 12), "cloze_blanks": (1, 8), "matching_pairs": (3, 5),
         "ordering_values": (3, 12), "rubric_criteria": (1, 12)}
LEVELS = ("incomplete", "adequate", "strong")

# Checks that decide the quiz slot's mechanical pass. The rest are reported only.
GATING_CHECKS = ("format_invalid", "leak", "length_bias", "duplicate_options", "key_explanation_conflict",
                 "absolute_unstated")
# The bank-level gate beside them: the key-position skew of `key_positions`.


# --- normalisation (section 3, short answer), as the engine's `normalize_answer` and `parse_number` -------

def fold(text):
    """Diacritics removed (NFKD, combining marks dropped) and lowercased, as the engine's `folded`."""
    text = unicodedata.normalize("NFKD", str(text or ""))
    return "".join(c for c in text if not unicodedata.combining(c)).lower()


def _between_digits(chars, index):
    before = chars[index - 1] if index else ""
    after = chars[index + 1] if index + 1 < len(chars) else ""
    return before.isascii() and before.isdigit() and after.isascii() and after.isdigit()


def normalise(text):
    """The engine's answer normalisation: case, diacritics, punctuation, whitespace, a leading English article
    and thousands separators (",", thin space, "_" between digits) do not matter; a decimal point between digits
    is kept."""
    chars, kept = list(fold(text)), []
    for index, c in enumerate(chars):
        digits = _between_digits(chars, index)
        if c in (",", "\u2009", "_") and digits:
            continue
        kept.append("." if c == "." and digits else c if c.isalnum() else " ")
    found = "".join(kept).split()
    if len(found) > 1 and found[0] in ARTICLES:
        found = found[1:]
    return " ".join(found)


def words(text):
    return normalise(text).split()


def contains_words(haystack, needle):
    """Whether `needle` occurs in `haystack` as whole words, both normalised."""
    needle = normalise(needle)
    return bool(needle) and f" {needle} " in f" {normalise(haystack)} "


def ngrams(tokens, n):
    return {tuple(tokens[i:i + n]) for i in range(len(tokens) - n + 1)}


def parse_number(text, unit_variants=()):
    """The number a typed answer states, with one accepted unit removed (the first that matches, in the key's
    order); a fraction "a/b" counts. None when the answer is not a number or carries a unit the key does not
    accept."""
    text = fold(text).strip().replace("\u2212", "-").replace("\u2013", "-")
    for unit in (fold(u).strip() for u in unit_variants or ()):
        if unit and text.endswith(unit):
            text = text[:-len(unit)].strip()
            break
    chars = list(text)
    cleaned = "".join(c for i, c in enumerate(chars)
                      if not (c in (",", " ", "\u2009", "_") and _between_digits(chars, i))).strip().lstrip("+")
    if not cleaned:
        return None
    try:
        if "/" in cleaned:
            numerator, denominator = (float(part.strip()) for part in cleaned.split("/", 1))
            value = numerator / denominator if denominator else None
        elif all((c.isascii() and c.isdigit()) or c in ".-e" for c in cleaned):
            value = float(cleaned)
        else:
            return None
    except ValueError:
        return None
    return value if value is not None and math.isfinite(value) else None


def text_match(answer, variants, numeric=None):
    """The engine's `text_matches`: True or False when code decides, None when only the model can. An answer that
    parses as a number is always decided in code, against the numeric key within its absolute tolerance, or
    else against the variants."""
    given = parse_number(answer, (numeric or {}).get("unit_variants") or ())
    if given is not None:
        if numeric:
            value = float(numeric["value"])
            return abs(given - value) <= float(numeric.get("tolerance") or 0) + 1e-9 * max(abs(value), 1.0)
        return any((parse_number(v) is not None and abs(parse_number(v) - given) <= 1e-9)
                   or normalise(v) == normalise(answer) for v in variants or ())
    return True if normalise(answer) in {normalise(v) for v in variants or ()} else None


def text_matches(answer, variants, numeric=None):
    return text_match(answer, variants, numeric) is True


# --- the reference grader (section 3) ------------------------------------------

def possible_points(item):
    if item.get("format") == "explanation":
        return float(sum(c.get("max_points", 0) for c in item.get("rubric") or []))
    return float(item.get("points", 1.0))


def kendall_share(key, given):
    """Correctly ordered pairs over all pairs; a value missing from the answer loses every pair it is in."""
    position = {normalise(v): i for i, v in enumerate(given or [])}
    ranks = [position.get(normalise(v)) for v in key]
    pairs = [(a, b) for i, a in enumerate(ranks) for b in ranks[i + 1:]]
    return sum(a is not None and b is not None and a < b for a, b in pairs) / len(pairs) if pairs else 0.0


def grade_keyed(item, response):
    """{"path", "earned", "possible", "share"}: the deterministic grade, or `path: "model"` with no points when
    the item goes to the model (an explanation, or a non-empty short answer that matches no accepted variant)."""
    fmt, response, possible = item["format"], response or {}, possible_points(item)
    key = item.get("key") or []
    model = {"path": "model", "earned": None, "possible": possible, "share": None}
    if fmt == "explanation":
        if not str(response.get("text") or "").strip():
            return {"path": "deterministic", "earned": 0.0, "possible": possible, "share": 0.0}
        return model
    if fmt in ("single_choice", "true_false"):
        share = float(sorted(response.get("selected_indices") or []) == sorted(key[:1]))
    elif fmt == "multi_select":
        picked, wanted = set(response.get("selected_indices") or []), set(key)
        share = max(0, len(picked & wanted) - len(picked - wanted)) / len(wanted) if wanted else 0.0
    elif fmt == "short_answer":
        text = (response.get("text") or "").strip()
        decided = text_match(text, item.get("variants"), item.get("numeric")) if text else False
        if decided is None:
            return model
        share = float(decided)
    elif fmt == "cloze":
        blanks, given = item.get("blanks") or [], response.get("blank_responses") or []
        numeric = item.get("blank_numeric") or [None] * len(blanks)
        right = sum(i < len(given) and bool(given[i].strip()) and text_matches(given[i].strip(), variants, numeric[i])
                    for i, variants in enumerate(blanks))
        share = right / len(blanks) if blanks else 0.0
    elif fmt == "matching":
        given = response.get("matches") or []
        right = sum(i < len(given) and given[i] == index for i, index in enumerate(key))
        share = right / len(key) if key else 0.0
    elif fmt == "ordering":
        share = kendall_share(item.get("values") or [], response.get("ordered_values"))
    else:
        raise ValueError(f"{item.get('id')}: unknown format {fmt!r}")
    return {"path": "deterministic", "earned": round(share * possible, 6), "possible": possible, "share": share}


# --- format validity (section 5.2) ---------------------------------------------

def _within(count, bounds):
    low, high = bounds
    return count >= low and (high is None or count <= high)


def _indices(values, size):
    return isinstance(values, list) and all(isinstance(v, int) and not isinstance(v, bool) and 0 <= v < size
                                            for v in values)


def _strings(values):
    return isinstance(values, list) and all(isinstance(v, str) and v.strip() for v in values)


def format_problems(item):
    """What makes this item invalid for its format; empty when it is valid."""
    fmt, problems = item.get("format"), []
    if fmt not in FORMATS:
        return [f"unknown format {fmt!r}"]
    if not isinstance(item.get("prompt"), str) or not item["prompt"].strip():
        problems.append("empty prompt")
    points = item.get("points", 1.0)
    if fmt != "explanation" and (isinstance(points, bool) or not isinstance(points, (int, float))
                                 or not math.isfinite(points) or points <= 0):
        problems.append("points must be a positive number")
    options, key = item.get("options"), item.get("key")
    if fmt in CHOICE_FORMATS:
        if not _strings(options):
            return problems + ["options must be non-empty strings"]
        if not _indices(key, len(options)) or len(set(key)) != len(key):
            return problems + ["key indices must be distinct and in range"]
    if fmt == "single_choice":
        problems += [] if _within(len(options), SIZES["single_choice_options"]) else ["single choice needs 2-8 options"]
        problems += [] if len(key) == 1 else ["single choice needs exactly one key"]
    elif fmt == "true_false":
        problems += [] if options == TRUE_FALSE else ["true/false options must be exactly True, False"]
        problems += [] if len(key) == 1 else ["true/false needs exactly one key"]
    elif fmt == "multi_select":
        problems += [] if _within(len(options), SIZES["multi_select_options"]) else ["multi-select needs 4-6 options"]
        problems += [] if _within(len(key), SIZES["multi_select_keys"]) else ["multi-select needs 2-3 keys"]
    elif fmt == "short_answer":
        variants, numeric = item.get("variants"), item.get("numeric")
        if not _strings(variants) or not _within(len(variants), SIZES["short_answer_variants"]):
            problems.append("short answer needs 1-12 accepted variants")
        if numeric is not None:
            value, tolerance = numeric.get("value"), numeric.get("tolerance", 0)
            if not all(isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)
                       for v in (value, tolerance)) or tolerance < 0:
                problems.append("numeric key needs a finite value and a tolerance >= 0")
            if not isinstance(numeric.get("unit_variants", []), list):
                problems.append("unit_variants must be a list")
    elif fmt == "cloze":
        numbers = [int(n) for n in BLANK.findall(item.get("prompt") or "")]
        blanks = item.get("blanks")
        if not numbers or numbers != list(range(1, len(numbers) + 1)) or len(numbers) > SIZES["cloze_blanks"][1]:
            problems.append("cloze blanks must be {{1}}..{{n}}, each once and in order, at most 8")
        if not isinstance(blanks, list) or len(blanks) != len(numbers) or not all(_strings(b) and b for b in blanks):
            problems.append("cloze needs one non-empty variant list per blank")
    elif fmt == "matching":
        left, right = item.get("left"), item.get("right")
        if not _strings(left) or not _strings(right):
            return problems + ["matching needs left and right strings"]
        if not _within(len(left), SIZES["matching_pairs"]):
            problems.append("matching needs 3-5 pairs")
        if len(right) not in (len(left), len(left) + 1):
            problems.append("matching's right side holds the pairs and at most one distractor")
        if not _indices(key, len(right)) or len(key) != len(left):
            problems.append("matching key needs one right index per left item")
        elif len(set(key)) != len(key):
            problems.append("matching key repeats a right item")
    elif fmt == "ordering":
        values = item.get("values")
        if not _strings(values) or not _within(len(values), SIZES["ordering_values"]):
            problems.append("ordering needs 3-12 values")
        elif len({normalise(v) for v in values}) != len(values):
            problems.append("ordering values must be distinct")
    elif fmt == "explanation":
        rubric = item.get("rubric")
        if not isinstance(rubric, list) or not _within(len(rubric), SIZES["rubric_criteria"]):
            return problems + ["explanation needs 1-12 rubric criteria"]
        for criterion in rubric:
            points = criterion.get("max_points")
            if not criterion.get("id") or not str(criterion.get("title", "")).strip():
                problems.append("a rubric criterion needs an id and a title")
            if isinstance(points, bool) or not isinstance(points, (int, float)) or not 0 < points <= 100:
                problems.append(f"criterion {criterion.get('id')}: max_points must be 1-100")
            levels = criterion.get("levels") or {}
            if not str(criterion.get("description") or "").strip() and not all(
                    str(levels.get(level, "")).strip() for level in LEVELS):
                problems.append(f"criterion {criterion.get('id')}: needs a description, or incomplete, adequate "
                                f"and strong levels")
    return problems


# --- mechanical item checks (sections 5.2 and 7.1) ------------------------------

def key_leaks(stem, key_text, other_options=()):
    """The engine's leak check: the key's own text in the stem, as whole words. A key of four or more words
    leaks on any shared run of four; a shorter key leaks only when it is the one option the stem names, since a
    comparison question names several; a key under LEAK_MIN_CHARS characters is not checked."""
    stem, key = normalise(stem), normalise(key_text)
    if len(key) < LEAK_MIN_CHARS:
        return False
    tokens = key.split()
    if len(tokens) >= LEAK_NGRAM:
        return any(f" {' '.join(gram)} " in f" {stem} " for gram in
                   (tokens[i:i + LEAK_NGRAM] for i in range(len(tokens) - LEAK_NGRAM + 1)))
    others = (normalise(o) for o in other_options)
    return f" {key} " in f" {stem} " and not any(len(o) >= LEAK_MIN_CHARS and f" {o} " in f" {stem} " for o in others)


def leak_hits(item):
    """Keys or accepted variants found verbatim in the stem (section 7.1's leak rule). Other options in the stem
    are fine: comparison questions name them."""
    fmt, stem, key = item.get("format"), item.get("prompt") or "", item.get("key") or []
    if fmt in ("single_choice", "multi_select"):
        options = item.get("options") or []
        others = [o for i, o in enumerate(options) if i not in key]
        texts = [(options[i], others) for i in key if i < len(options)]
    elif fmt == "short_answer":
        texts = [(v, ()) for v in item.get("variants") or []]
    elif fmt == "cloze":
        stem = BLANK.sub(" ", stem)
        texts = [(v, ()) for blank in item.get("blanks") or [] for v in blank]
    else:
        texts = []
    hits, seen = [], set()
    for text, others in texts:
        if normalise(text) not in seen and key_leaks(stem, text, others):
            hits.append(f"the stem contains the key {text!r}")
        seen.add(normalise(text))
    return hits


def length_bias(item):
    """(key is the unique longest option, by more than LENGTH_BIAS_SHARE of the median option length)."""
    if item.get("format") not in ("single_choice", "multi_select"):
        return False, False
    lengths = [len(" ".join(option.split())) for option in item["options"]]
    longest = max(lengths)
    unique = lengths.count(longest) == 1
    index = lengths.index(longest)
    is_key = unique and index in (item.get("key") or [])
    runner_up = max((n for i, n in enumerate(lengths) if i != index), default=0)
    return is_key, is_key and longest - runner_up > LENGTH_BIAS_SHARE * statistics.median(lengths)


def duplicate_options(item):
    fmt = item.get("format")
    groups = {"single_choice": ["options"], "multi_select": ["options"], "matching": ["left", "right"],
              "ordering": ["values"]}.get(fmt, [])
    found = []
    for name in groups:
        seen = [normalise(v) for v in item.get(name) or []]
        found += [f"{name} repeat {value!r}" for value in sorted({v for v in seen if seen.count(v) > 1})]
    return found


# "The correct answer is B", "Answer: (C)", "the right option is D": a claim naming one letter.
LETTER_CLAIM = re.compile(r"(?i:correct (?:answer|option|choice)|right (?:answer|option)|the answer|answer)"
                          r"\s*(?:(?i:is|would be)|:|=)\s*(?i:option\s+|choice\s+)?\(?([A-F])\)?(?![\w'])")
LETTERS_CLAIM = re.compile(r"(?i:correct (?:answers|options|choices) are)\s*((?:\(?[A-F]\)?(?:\s*,\s*|\s+and\s+|"
                           r"\s*&\s*)?)+)")
LETTER_REF = re.compile(r"(?:\b(?i:option|choice|answer)\s+\(?[A-F]\)?(?![\w'])|\([A-F]\)|^[A-F][.):]\s)")
TEXT_CLAIM = re.compile(r"(?i:correct (?:answer|option|choice) is|the answer is)\s+[\"“']?([^.;\n\"”]+)")
TRUTH_CLAIM = re.compile(r"^\s*(true|false)\b|\b(?:statement|claim|this) is (true|false)\b", re.I)


def key_explanation_conflicts(item):
    """Where the explanation names an answer other than the key. Precise rather than complete: it reads explicit
    claims ("the correct answer is B", "this is false") and leaves meaning to the judge's Q2."""
    fmt, text, key = item.get("format"), item.get("explanation") or "", item.get("key") or []
    conflicts = []
    if fmt in ("single_choice", "multi_select"):
        options = item["options"]
        claimed = {LETTERS.index(m) for m in LETTER_CLAIM.findall(text)}
        for group in LETTERS_CLAIM.findall(text):
            claimed |= {LETTERS.index(c) for c in re.findall(r"[A-F]", group)}
        conflicts += [f"names option {LETTERS[i]} as the answer" for i in sorted(claimed - set(key))]
        for match in TEXT_CLAIM.findall(text):
            said = normalise(match)
            named = [i for i, option in enumerate(options) if normalise(option)
                     and f"{said} ".startswith(normalise(option) + " ")]
            named = sorted(named, key=lambda i: len(normalise(options[i])))[-1:]
            conflicts += [f"names {options[i]!r} as the answer" for i in named if i not in key]
    elif fmt == "true_false":
        truth = {m.lower() for pair in TRUTH_CLAIM.findall(text) for m in pair if m}
        wanted = "true" if key[:1] == [0] else "false"
        conflicts += [f"says the statement is {t}" for t in sorted(truth - {wanted})]
    elif fmt == "short_answer" and item.get("numeric"):
        numeric = item["numeric"]
        for match in TEXT_CLAIM.findall(text):
            said = (match.split() or [""])[0]
            value = parse_number(said, numeric.get("unit_variants") or ())
            if value is not None and abs(value - float(numeric["value"])) > float(numeric.get("tolerance") or 0):
                conflicts.append(f"states the answer as {said!r}")
    return conflicts


def letter_references(item):
    """Option letters in the explanation: the form reshuffles options, so a letter points at the wrong one."""
    if item.get("format") not in ("single_choice", "multi_select"):
        return []
    text = item.get("explanation") or ""
    return sorted({m.strip() for m in LETTER_REF.findall(text)} | {f"answer {m}" for m in LETTER_CLAIM.findall(text)})


def absolute_unstated(item, lesson_text):
    """True/false statements with "always" or "never" that the lesson does not itself state."""
    if item.get("format") != "true_false" or lesson_text is None:
        return []
    lesson = set(words(lesson_text))
    return [word for word in ABSOLUTES if word in words(item.get("prompt")) and word not in lesson]


def anchored(item, sources=None):
    """(has a lesson-block anchor or a source quote, the quote is verbatim in its cited source or None if the
    source text was not recorded)."""
    quote, version = item.get("source_quote"), item.get("source_version_id")
    has = bool(item.get("lesson_block_id")) or bool(quote and str(quote).strip())
    if not quote:
        return has, None
    text = (sources or {}).get(version)
    return has, None if text is None else " ".join(str(quote).split()) in " ".join(str(text).split())


def stem_tokens(item):
    return set(words(BLANK.sub(" ", item.get("prompt") or "")))


def jaccard(a, b):
    return len(a & b) / len(a | b) if a | b else 1.0


def item_checks(item, lesson_text=None, sources=None):
    """Every mechanical check on one item: {check: [problems]} plus the reported-only facts."""
    invalid = format_problems(item)
    checks = {"format_invalid": invalid}
    if invalid:
        checks |= {name: [] for name in GATING_CHECKS if name != "format_invalid"}
        return {"checks": checks, "key_longest": False, "letter_refs": [], "anchored": False,
                "quote_verbatim": None, "gating_failures": [f"format_invalid: {p}" for p in invalid]}
    key_longest, biased = length_bias(item)
    has_anchor, verbatim = anchored(item, sources)
    checks |= {"leak": leak_hits(item), "length_bias": ["key is the unique longest option"] if biased else [],
               "duplicate_options": duplicate_options(item),
               "key_explanation_conflict": key_explanation_conflicts(item),
               "absolute_unstated": absolute_unstated(item, lesson_text)}
    return {"checks": checks, "key_longest": key_longest, "letter_refs": letter_references(item),
            "anchored": has_anchor, "quote_verbatim": verbatim,
            "gating_failures": [f"{name}: {p}" for name in GATING_CHECKS for p in checks[name]]}


def upper_tail(probabilities, observed):
    """P(X >= observed) for a sum of independent Bernoulli(p) draws (exact, by convolution)."""
    distribution = [1.0]
    for p in probabilities:
        distribution = [a * (1 - p) + b * p for a, b in zip(distribution + [0.0], [0.0] + distribution)]
    return min(1.0, sum(distribution[observed:]))


def position_test(shapes, counts):
    """Bonferroni-corrected exact p-value that some position holds more keys than a fair shuffle gives:
    `shapes` is one (option count, key count) per item, `counts` the keys found at each position."""
    if not shapes:
        return None
    positions = max(n for n, _ in shapes)
    tails = [upper_tail([k / n if j < n else 0.0 for n, k in shapes], counts[j]) for j in range(positions)]
    return min(1.0, positions * min(tails))


def key_positions(items):
    """Where the keys sit. The authoring path shuffles options, so in a bank of any size each position should
    hold about its share; a bank keyed mostly at one position (six questions all keyed B) is flagged. True/false
    is tested apart, on True against False."""
    choice = [i for i in items if i.get("format") in ("single_choice", "multi_select") and not format_problems(i)]
    shapes = [[len(i["options"]), len(i["key"])] for i in choice]
    width = max((n for n, _ in shapes), default=0)
    counts = [sum(j in i["key"] for i in choice) for j in range(width)]
    p_choice = position_test(shapes, counts)
    truth = [i["key"][0] for i in items if i.get("format") == "true_false" and not format_problems(i)]
    tf_counts = [truth.count(0), truth.count(1)]
    p_truth = position_test([[2, 1]] * len(truth), tf_counts)
    return {"choice": {"items": len(choice), "counts": dict(zip(LETTERS, counts)), "shapes": shapes,
                       "p_value": p_choice, "skewed": p_choice is not None and p_choice < POSITION_ALPHA},
            "true_false": {"items": len(truth), "true": tf_counts[0], "false": tf_counts[1], "p_value": p_truth,
                           "skewed": p_truth is not None and p_truth < POSITION_ALPHA},
            "alpha": POSITION_ALPHA}


def bank_checks(items, lesson_text=None, sources=None, metrics=None):
    """The bank's mechanical report for metrics.json: per-check hits, duplicate and near-duplicate items, the
    format mix, the key-longest share against chance, anchors, and whether the bank passes the gating checks.
    `metrics` is the harness's own block (validation, key review, target, cost), passed through."""
    per_item = {item.get("id"): item_checks(item, lesson_text, sources) for item in items}
    seen, duplicates, near = {}, [], []
    tokens = {item.get("id"): stem_tokens(item) for item in items}
    for item in items:
        fingerprint = (item.get("format"), normalise(BLANK.sub(" ", item.get("prompt") or "")))
        if fingerprint in seen:
            duplicates.append([seen[fingerprint], item.get("id")])
        else:
            seen[fingerprint] = item.get("id")
    ids = [item.get("id") for item in items]
    for i, a in enumerate(ids):
        for b in ids[i + 1:]:
            if [a, b] not in duplicates and jaccard(tokens[a], tokens[b]) >= NEAR_DUPLICATE_JACCARD:
                near.append([a, b])
    choice = [item for item in items if item.get("format") in ("single_choice", "multi_select")
              and not per_item[item.get("id")]["checks"]["format_invalid"]]
    chance = (statistics.mean(len(i["key"]) / len(i["options"]) for i in choice) if choice else None)
    hits = {name: sorted(i for i, report in per_item.items() if report["checks"][name]) for name in GATING_CHECKS}
    mix = {}
    for item in items:
        mix[item.get("format")] = mix.get(item.get("format"), 0) + 1
    verbatim = [r["quote_verbatim"] for r in per_item.values() if r["quote_verbatim"] is not None]
    report = {
        "items": len(items), "format_mix": dict(sorted(mix.items())),
        "hits": hits, "duplicate_items": duplicates, "near_duplicate_items": near,
        "key_longest": {"items": sum(per_item[i.get("id")]["key_longest"] for i in choice), "of": len(choice),
                        "chance": chance},
        "letter_references": sorted(i for i, r in per_item.items() if r["letter_refs"]),
        "anchored": {"items": sum(r["anchored"] for r in per_item.values()), "of": len(items)},
        "quotes_verbatim": {"verbatim": sum(verbatim), "checked": len(verbatim)},
        "key_positions": key_positions(items),
        "harness": metrics or None,
        "problems": {i: r["gating_failures"] for i, r in per_item.items() if r["gating_failures"]},
    }
    skew = report["key_positions"]
    report["passed"] = (bool(items) and not any(hits.values()) and not duplicates and not near
                        and not skew["choice"]["skewed"] and not skew["true_false"]["skewed"])
    return report


# --- renderings --------------------------------------------------------------

def shown_order(item):
    """The order an ordering item's values are shown in: a fixed shuffle by the item's id, never the key's."""
    values = list(item.get("values") or [])
    shown = sorted(values, key=lambda v: hashlib.sha256(f"{item.get('id')}\0{v}".encode()).hexdigest())
    return shown[1:] + shown[:1] if shown == values and len(values) > 1 else shown


def format_line(item):
    return {"single_choice": "single choice (exactly one option is correct)",
            "multi_select": "multi-select (two or three options are correct)",
            "true_false": "true or false", "short_answer": "short answer (typed)",
            "cloze": "cloze (fill each numbered blank)", "matching": "matching (pair each left item with one right item)",
            "ordering": "ordering (put the items in order)",
            "explanation": "explanation (a written answer graded against a rubric)"}[item["format"]]


def key_text(item):
    """The key in the same words the solve format uses."""
    fmt, key = item["format"], item.get("key") or []
    if fmt in CHOICE_FORMATS:
        return ", ".join(LETTERS[i] for i in key)
    if fmt == "short_answer":
        numeric = item.get("numeric")
        text = " | ".join(item.get("variants") or [])
        return text + (f" (numeric {numeric['value']} ± {numeric.get('tolerance', 0)})" if numeric else "")
    if fmt == "cloze":
        return " | ".join(f"{n}: {' / '.join(b)}" for n, b in enumerate(item.get("blanks") or [], 1))
    if fmt == "matching":
        return ", ".join(f"{n}-{LETTERS[r]}" for n, r in enumerate(key, 1))
    if fmt == "ordering":
        shown = shown_order(item)
        return ", ".join(LETTERS[shown.index(v)] for v in item.get("values") or [])
    return "rubric"


def render_item(item, key=False):
    """An item as a judge (key=False: what a learner sees) or a labeller (key=True: everything) reads it."""
    fmt = item["format"]
    lines = [f"Format: {format_line(item)}", f"Question: {item.get('prompt', '')}"]
    if fmt in CHOICE_FORMATS:
        lines += ["Options:", *[f"{LETTERS[i]}. {o}" for i, o in enumerate(item.get("options") or [])]]
    elif fmt == "matching":
        lines += ["Left:", *[f"{n}. {v}" for n, v in enumerate(item.get("left") or [], 1)],
                  "Right:", *[f"{LETTERS[i]}. {v}" for i, v in enumerate(item.get("right") or [])]]
    elif fmt == "ordering":
        lines += ["Items:", *[f"{LETTERS[i]}. {v}" for i, v in enumerate(shown_order(item))]]
    elif fmt == "explanation" and item.get("rubric"):
        lines += ["Graded on:", *[f"- {c.get('title', '')} ({c.get('max_points', '?')} points)"
                                  for c in item["rubric"]]]
    if not key:
        return "\n".join(lines) + "\n"
    if fmt == "explanation":
        lines += ["Rubric levels:"]
        for c in item.get("rubric") or []:
            levels = c.get("levels") or {}
            described = "; ".join(f"{level}: {levels[level]}" for level in LEVELS if levels.get(level))
            lines += [f"- {c.get('title', '')}: {described or c.get('description', '')}"]
        lines += ["Key points:", *[f"- {p}" for p in item.get("key_points") or []]]
    else:
        lines.append(f"Key: {key_text(item)}")
    lines += [f"Explanation: {item['explanation']}"] if item.get("explanation") else []
    rationales = [r for r in item.get("distractor_rationales") or [] if r]
    lines += ["Why the others fail:", *[f"- {r}" for r in rationales]] if rationales else []
    lines += [f"Source quote: {item['source_quote']}"] if item.get("source_quote") else []
    return "\n".join(lines) + "\n"


def lesson_plain(document):
    """A prepared lesson's block text, for the absolutes check."""
    lesson = (document or {}).get("lesson") or {}
    return "\n".join(f"{b.get('title', '')}\n{b.get('body', '')}" for b in lesson.get("blocks") or [])


# --- the blind solve (Q2) and learner answers in the same words ----------------

def answer_format(item):
    """How a solver (Q2) or a persona writes its answer, one string per item."""
    return {"single_choice": "the letter of the one correct option, such as \"B\"",
            "true_false": "\"A\" for True or \"B\" for False",
            "multi_select": "the letters of every correct option, such as \"A, C\"",
            "short_answer": "the answer as a learner would type it",
            "cloze": "each blank as \"1: word | 2: word\"",
            "matching": "each left number with its right letter, such as \"1-B, 2-A, 3-C\"",
            "ordering": "the item letters in order, such as \"C, A, B\"",
            "explanation": "the written answer"}[item["format"]]


def parse_answer(item, answer):
    """A response in the form's field names from one answer string; None when it cannot be read."""
    fmt, answer = item["format"], str(answer or "").strip()
    if fmt in ("single_choice", "multi_select", "true_false"):
        text = answer
        if fmt == "true_false":
            text = re.sub(r"(?i)^true\b", "A", re.sub(r"(?i)^false\b", "B", text))
        letters = re.findall(r"(?<![A-Za-z])([A-F])(?![A-Za-z])", text)
        indices = sorted({LETTERS.index(c) for c in letters if LETTERS.index(c) < len(item.get("options") or [])})
        return {"selected_indices": indices} if indices else None
    if fmt in ("short_answer", "explanation"):
        return {"text": answer}
    if fmt == "cloze":
        parts = dict(re.findall(r"(\d+)\s*:\s*([^|]*)", answer))
        count = len(item.get("blanks") or [])
        return {"blank_responses": [parts.get(str(n), "").strip() for n in range(1, count + 1)]} if parts else None
    if fmt == "matching":
        pairs = dict(re.findall(r"(\d+)\s*[-–:>=]+\s*([A-F])", answer))
        if not pairs:
            return None
        return {"matches": [LETTERS.index(pairs[str(n)]) if str(n) in pairs else None
                            for n in range(1, len(item.get("left") or []) + 1)]}
    if fmt == "ordering":
        shown = shown_order(item)
        letters = [c for c in re.findall(r"(?<![A-Za-z])([A-Z])(?![A-Za-z])", answer) if LETTERS.index(c) < len(shown)]
        return {"ordered_values": [shown[LETTERS.index(c)] for c in letters]} if letters else None
    raise ValueError(f"unknown format {fmt!r}")


def agrees_with_key(item, response):
    """Whether a solver's response earns the item's full credit."""
    if item["format"] == "explanation" or response is None:
        return False
    graded = grade_keyed(item, response)
    return graded["path"] == "deterministic" and graded["share"] == 1.0
