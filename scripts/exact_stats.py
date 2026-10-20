"""Exact small-sample statistics for paired comparisons and judge calibration.

Nothing here uses a normal approximation or a bootstrap. The answer sets these
serve hold tens of questions, and at that size a bootstrap interval or a z-test
on judge scores calls differences that are noise, worst of all when the judge
looks nearly perfect (evalstats, arXiv 2609.35815). Exact tests keep their
stated false-positive rate at every sample size; the price is that they say
"can't tell" more often, which at this size is the true answer.
"""
from math import comb

# The two-sided false-positive rate accepted for one comparison chosen before
# the results are seen: on a change that does nothing, one call in twenty is
# wrong. The exact tests below hold their real rate at or below this at every
# sample size, so no extra small-sample margin is added on top. Comparing many
# arms or slices at once needs a family-wise correction this does not apply.
ALPHA = 0.05


def sign_test_p(wins, losses):
    """Exact two-sided sign test on paired outcomes; ties are dropped first.

    With binary outcomes this is the exact McNemar test: only the discordant
    pairs carry information, and under no difference each is a fair coin.
    """
    if wins < 0 or losses < 0:
        raise ValueError("counts cannot be negative")
    n = wins + losses
    if n == 0:
        return 1.0
    tail = sum(comb(n, i) for i in range(min(wins, losses) + 1))
    return min(1.0, 2 * tail / 2 ** n)


def fewest_to_call(changed, alpha=ALPHA):
    """Of `changed` discordant pairs, the fewest that must go one way for the
    test to reject at `alpha`, or None when no split of that many can."""
    for one_way in range(changed // 2 + 1, changed + 1):
        if sign_test_p(one_way, changed - one_way) <= alpha:
            return one_way
    return None


def fewest_changed_to_call(alpha=ALPHA):
    """The fewest discordant pairs for which any split can reach `alpha`:
    all n going one way gives p = 2 / 2**n."""
    changed = 1
    while sign_test_p(changed, 0) > alpha:
        changed += 1
    return changed


def _hypergeometric_tail(successes, sample, population, marked, upper):
    """P(X >= successes) when `upper`, else P(X <= successes), for X the marked
    items in a sample drawn without replacement."""
    low = max(0, sample - (population - marked))
    high = min(sample, marked)
    values = range(max(successes, low), high + 1) if upper else range(low, min(successes, high) + 1)
    count = sum(comb(marked, x) * comb(population - marked, sample - x) for x in values)
    return count / comb(population, sample)


def hypergeometric_interval(successes, sample, population, alpha=ALPHA):
    """Exact equal-tailed interval for how many of a finite population are
    marked, given `successes` marked among `sample` drawn without replacement.

    Returns the interval as counts (low, high). A population sampled whole has
    no uncertainty left, and an unsampled one has all of it.
    """
    if not 0 <= successes <= sample <= population:
        raise ValueError("need 0 <= successes <= sample <= population")
    candidates = range(successes, population - (sample - successes) + 1)
    low = next(m for m in candidates
               if _hypergeometric_tail(successes, sample, population, m, True) > alpha / 2)
    high = next(m for m in reversed(candidates)
                if _hypergeometric_tail(successes, sample, population, m, False) > alpha / 2)
    return low, high
