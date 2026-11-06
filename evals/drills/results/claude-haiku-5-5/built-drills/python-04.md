# Token bucket rate limiter with injectable clock (python)

- Domain: rate limiting
- Built by: claude-haiku-5-5
- Runner: builtin-python
- Built in 98 seconds
- Patterns asked for: correctness.wrong_operator, errors.partial_update_without_rollback, concurrency.missing_await

## Counts

- Planted: 3
- Proven by a test: 1
- Proven by the judge: 0
- Dropped: 2
- Found in review: 2

## The code

### src/limiter.py

```text
 1 | """Token bucket rate limiter.
 2 | 
 3 | The clock is passed in so that callers and tests control time explicitly.
 4 | """
 5 | 
 6 | from dataclasses import dataclass
 7 | from typing import Callable
 8 | 
 9 | 
10 | @dataclass
11 | class Decision:
12 |     allowed: bool
13 |     retry_after: float
14 | 
15 | 
16 | class RateLimiter:
17 |     def __init__(self, rate: float, capacity: int, clock: Callable[[], float]) -> None:
18 |         if rate <= 0:
19 |             raise ValueError("rate must be positive")
20 |         if capacity < 1:
21 |             raise ValueError("capacity must be at least 1")
22 |         self._rate = rate
23 |         self._capacity = capacity
24 |         self._clock = clock
25 |         self._buckets: dict[str, tuple[float, float]] = {}
26 | 
27 |     def _refill(self, key: str, now: float) -> float:
28 |         tokens, last = self._buckets.get(key, (float(self._capacity), now))
29 |         # Clock may be coarse or stepped backwards; never let elapsed go negative.
30 |         elapsed = max(0.0, now - last)
31 |         tokens = min(self._capacity, tokens + elapsed * self._rate)
32 |         return tokens
33 | 
34 |     def acquire(self, key: str, cost: int = 1) -> Decision:
35 |         if cost < 1 or cost > self._capacity:
36 |             raise ValueError("cost must be between 1 and capacity")
37 |         now = self._clock()
38 |         tokens = self._refill(key, now)
39 |         if tokens > cost:
40 |             self._buckets[key] = (tokens - cost, now)
41 |             return Decision(True, 0.0)
42 |         # Store the refilled state so the next call measures from here.
43 |         self._buckets[key] = (tokens, now)
44 |         wait = (cost - tokens) / self._rate
45 |         return Decision(False, wait)
46 | 
47 |     def forget(self, key: str) -> None:
48 |         self._buckets.pop(key, None)
```

### tests/test_limiter.py

```text
 1 | from src.limiter import RateLimiter
 2 | 
 3 | 
 4 | class FakeClock:
 5 |     def __init__(self, now: float = 0.0) -> None:
 6 |         self.now = now
 7 | 
 8 |     def __call__(self) -> float:
 9 |         return self.now
10 | 
11 | 
12 | def test_burst_then_deny() -> None:
13 |     clock = FakeClock()
14 |     limiter = RateLimiter(rate=1.0, capacity=3, clock=clock)
15 |     results = [limiter.acquire("a").allowed for _ in range(4)]
16 |     assert results == [True, True, True, False]
17 | 
18 | 
19 | def test_retry_after_and_refill() -> None:
20 |     clock = FakeClock()
21 |     limiter = RateLimiter(rate=2.0, capacity=1, clock=clock)
22 |     assert limiter.acquire("a").allowed
23 |     denied = limiter.acquire("a")
24 |     assert not denied.allowed
25 |     assert abs(denied.retry_after - 0.5) < 1e-9
26 |     clock.now = 0.5
27 |     assert limiter.acquire("a").allowed
28 | 
29 | 
30 | def test_keys_are_independent() -> None:
31 |     clock = FakeClock()
32 |     limiter = RateLimiter(rate=1.0, capacity=1, clock=clock)
33 |     assert limiter.acquire("a").allowed
34 |     assert limiter.acquire("b").allowed
35 |     assert not limiter.acquire("a").allowed
36 | 
37 | 
38 | def test_refill_capped_at_capacity() -> None:
39 |     clock = FakeClock()
40 |     limiter = RateLimiter(rate=10.0, capacity=2, clock=clock)
41 |     clock.now = 1000.0
42 |     assert limiter.acquire("a").allowed
43 |     assert limiter.acquire("a").allowed
44 |     assert not limiter.acquire("a").allowed
45 | 
46 | 
47 | def test_clock_going_backwards_is_safe() -> None:
48 |     clock = FakeClock(100.0)
49 |     limiter = RateLimiter(rate=1.0, capacity=1, clock=clock)
50 |     assert limiter.acquire("a").allowed
51 |     clock.now = 50.0
52 |     assert not limiter.acquire("a").allowed
53 | 
54 | 
55 | def test_forget_resets_bucket() -> None:
56 |     clock = FakeClock()
57 |     limiter = RateLimiter(rate=1.0, capacity=1, clock=clock)
58 |     limiter.acquire("a")
59 |     limiter.forget("a")
60 |     assert limiter.acquire("a").allowed
61 | 
62 | 
63 | def test_invalid_arguments_rejected() -> None:
64 |     clock = FakeClock()
65 |     for args in [(0.0, 1), (1.0, 0)]:
66 |         try:
67 |             RateLimiter(rate=args[0], capacity=args[1], clock=clock)
68 |         except ValueError:
69 |             pass
70 |         else:
71 |             raise AssertionError("expected ValueError")
72 |     limiter = RateLimiter(rate=1.0, capacity=2, clock=clock)
73 |     try:
74 |         limiter.acquire("a", cost=3)
75 |     except ValueError:
76 |         pass
77 |     else:
78 |         raise AssertionError("expected ValueError for oversized cost")
```

## The key

### 1. Wrong operator or comparison (correctness.wrong_operator), Major, planted, src/limiter.py lines 39 to 40

- Consequence: A request whose cost exactly matches the available tokens is denied, so a client can never spend its full balance in one call and is told to wait for tokens it already has. This shows up as spurious 429s whenever cost equals the remaining tokens.
- In an interview: The check uses a strict comparison where the intent is that exactly enough tokens is sufficient. Use >= so a request is allowed when tokens equal cost, since the retry logic and capacity semantics assume that boundary is inclusive.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.limiter import RateLimiter
t=[0.0]
l=RateLimiter(rate=1.0,capacity=5,clock=lambda: t[0])
assert l.acquire('k',cost=3).allowed
t[0]=1.0
assert l.acquire('k',cost=3).allowed
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/limiter.py:

```diff
-        if tokens > cost:
+        if tokens >= cost:
             self._buckets[key] = (tokens - cost, now)
```

### 2. Unbounded growth (resources.unbounded_growth), Major, found in review, src/limiter.py lines 37 to 38

- Consequence: Every distinct key, such as a client IP or user ID, adds an entry to _buckets that is only removed by an explicit forget call, so under high key cardinality or attacker-chosen keys the dict grows until the process runs out of memory.
- In an interview: A bucket left idle for capacity divided by rate seconds has refilled to full, so it is indistinguishable from a fresh bucket and can be dropped without changing any decision. The sweep in this fix runs on every acquire, which is simple and correct but O(n) per call, so a production version would amortise it or use a bounded LRU.

Proven by the judge: Unbounded growth of the _buckets dict for high key cardinality is a real memory risk in the code, not planted in the key and not a defect of the clean version, so it is a valid extra.

The fix, in src/limiter.py:

```diff
         now = self._clock()
+        idle_for = self._capacity / self._rate
+        for k in [k for k, (_, last) in self._buckets.items() if now - last >= idle_for]:
+            del self._buckets[k]
         tokens = self._refill(key, now)
```

### 3. Untested branch (testing.untested_branch), Major, found in review, tests/test_limiter.py lines 38 to 44

- Consequence: The test passes even if the `min(self._capacity, ...)` cap is removed, because a first-seen key starts full. A regression that lets idle buckets accumulate beyond capacity would ship unnoticed and allow bursts far larger than configured.
- In an interview: The key is first seen at t=1000 so the bucket starts full, and the cap is never tested because no refill ever exceeds capacity. Creating the bucket at t=0, draining it, then advancing the clock far ahead makes the cap the only thing that limits the allowed count, so removing the min() would fail the test.

Proven by the judge: The test at lines 38-44 does not actually verify the capacity cap, since the bucket starts full at first sight; this is a real test weakness not in the key.

The fix, in tests/test_limiter.py:

```diff
 def test_refill_capped_at_capacity() -> None:
     clock = FakeClock()
     limiter = RateLimiter(rate=10.0, capacity=2, clock=clock)
+    assert limiter.acquire("a").allowed
+    assert limiter.acquire("a").allowed
+    assert not limiter.acquire("a").allowed
     clock.now = 1000.0
     assert limiter.acquire("a").allowed
     assert limiter.acquire("a").allowed
     assert not limiter.acquire("a").allowed
```

## Dropped

- errors.partial_update_without_rollback: The edit adds a pop of the key before reassigning it in the denied branch. The claimed pattern is a multi-step update without rollback where a failure partway leaves data half updated. The pop and the assignment are two dictionary operations, but the assignment cannot raise for a str key and a tuple value, and the claimed consequence (dropping stored state) requires a failure between them that the code path cannot produce. The edit therefore does not introduce a real partial-update hazard. The original code already has the same two-step structure in the denied path conceptually, though it is a single assignment with no separate removal, so the original does not have a two-step write that could leave the key missing. The edit adds a non-atomic remove-then-write sequence, but the consequence is theoretical, not a real defect introduced in practice, so the edit does not introduce exactly this problem in a way that can occur.
  Consequence claimed: Only a failure between the two dictionary operations would leave the key unstored, and nothing in this code path can raise there, so the damage is theoretical. In a version where the write is part of a multi-step update, a failure partway would drop the stored state for the key.
- concurrency.missing_await: The edit changes `self._clock()` to `self._clock`, so the clock callable is never invoked and `now` becomes a bound method object rather than a float. This is not a missing await: the code is synchronous, there is no async call, and the defect is a missing function call (wrong value). The claimed consequence is also wrong in detail: `now - last` with a method object and float raises TypeError, but the original code has no async calls at all so it cannot be a missing-await defect. The edit does introduce a bug, but it is a missing call on a sync callable, not the 'missing await' pattern, so the pattern is not exactly introduced; and the original lacks any missing-await problem.
  Consequence claimed: The clock is never called in this path, so the bucket stores a function object where a timestamp belongs and arithmetic on it raises TypeError on the next call. This would fail on the first acquire rather than silently.

## The clean code

### src/limiter.py

```text
 1 | """Token bucket rate limiter.
 2 | 
 3 | The clock is passed in so that callers and tests control time explicitly.
 4 | """
 5 | 
 6 | from dataclasses import dataclass
 7 | from typing import Callable
 8 | 
 9 | 
10 | @dataclass
11 | class Decision:
12 |     allowed: bool
13 |     retry_after: float
14 | 
15 | 
16 | class RateLimiter:
17 |     def __init__(self, rate: float, capacity: int, clock: Callable[[], float]) -> None:
18 |         if rate <= 0:
19 |             raise ValueError("rate must be positive")
20 |         if capacity < 1:
21 |             raise ValueError("capacity must be at least 1")
22 |         self._rate = rate
23 |         self._capacity = capacity
24 |         self._clock = clock
25 |         self._buckets: dict[str, tuple[float, float]] = {}
26 | 
27 |     def _refill(self, key: str, now: float) -> float:
28 |         tokens, last = self._buckets.get(key, (float(self._capacity), now))
29 |         # Clock may be coarse or stepped backwards; never let elapsed go negative.
30 |         elapsed = max(0.0, now - last)
31 |         tokens = min(self._capacity, tokens + elapsed * self._rate)
32 |         return tokens
33 | 
34 |     def acquire(self, key: str, cost: int = 1) -> Decision:
35 |         if cost < 1 or cost > self._capacity:
36 |             raise ValueError("cost must be between 1 and capacity")
37 |         now = self._clock()
38 |         tokens = self._refill(key, now)
39 |         if tokens >= cost:
40 |             self._buckets[key] = (tokens - cost, now)
41 |             return Decision(True, 0.0)
42 |         # Store the refilled state so the next call measures from here.
43 |         self._buckets[key] = (tokens, now)
44 |         wait = (cost - tokens) / self._rate
45 |         return Decision(False, wait)
46 | 
47 |     def forget(self, key: str) -> None:
48 |         self._buckets.pop(key, None)
```

### tests/test_limiter.py

```text
 1 | from src.limiter import RateLimiter
 2 | 
 3 | 
 4 | class FakeClock:
 5 |     def __init__(self, now: float = 0.0) -> None:
 6 |         self.now = now
 7 | 
 8 |     def __call__(self) -> float:
 9 |         return self.now
10 | 
11 | 
12 | def test_burst_then_deny() -> None:
13 |     clock = FakeClock()
14 |     limiter = RateLimiter(rate=1.0, capacity=3, clock=clock)
15 |     results = [limiter.acquire("a").allowed for _ in range(4)]
16 |     assert results == [True, True, True, False]
17 | 
18 | 
19 | def test_retry_after_and_refill() -> None:
20 |     clock = FakeClock()
21 |     limiter = RateLimiter(rate=2.0, capacity=1, clock=clock)
22 |     assert limiter.acquire("a").allowed
23 |     denied = limiter.acquire("a")
24 |     assert not denied.allowed
25 |     assert abs(denied.retry_after - 0.5) < 1e-9
26 |     clock.now = 0.5
27 |     assert limiter.acquire("a").allowed
28 | 
29 | 
30 | def test_keys_are_independent() -> None:
31 |     clock = FakeClock()
32 |     limiter = RateLimiter(rate=1.0, capacity=1, clock=clock)
33 |     assert limiter.acquire("a").allowed
34 |     assert limiter.acquire("b").allowed
35 |     assert not limiter.acquire("a").allowed
36 | 
37 | 
38 | def test_refill_capped_at_capacity() -> None:
39 |     clock = FakeClock()
40 |     limiter = RateLimiter(rate=10.0, capacity=2, clock=clock)
41 |     clock.now = 1000.0
42 |     assert limiter.acquire("a").allowed
43 |     assert limiter.acquire("a").allowed
44 |     assert not limiter.acquire("a").allowed
45 | 
46 | 
47 | def test_clock_going_backwards_is_safe() -> None:
48 |     clock = FakeClock(100.0)
49 |     limiter = RateLimiter(rate=1.0, capacity=1, clock=clock)
50 |     assert limiter.acquire("a").allowed
51 |     clock.now = 50.0
52 |     assert not limiter.acquire("a").allowed
53 | 
54 | 
55 | def test_forget_resets_bucket() -> None:
56 |     clock = FakeClock()
57 |     limiter = RateLimiter(rate=1.0, capacity=1, clock=clock)
58 |     limiter.acquire("a")
59 |     limiter.forget("a")
60 |     assert limiter.acquire("a").allowed
61 | 
62 | 
63 | def test_invalid_arguments_rejected() -> None:
64 |     clock = FakeClock()
65 |     for args in [(0.0, 1), (1.0, 0)]:
66 |         try:
67 |             RateLimiter(rate=args[0], capacity=args[1], clock=clock)
68 |         except ValueError:
69 |             pass
70 |         else:
71 |             raise AssertionError("expected ValueError")
72 |     limiter = RateLimiter(rate=1.0, capacity=2, clock=clock)
73 |     try:
74 |         limiter.acquire("a", cost=3)
75 |     except ValueError:
76 |         pass
77 |     else:
78 |         raise AssertionError("expected ValueError for oversized cost")
```

### The hidden suite

```text
import sys
import traceback

sys.path.insert(0, '.')

from src.limiter import RateLimiter


class FakeClock:
    def __init__(self, now=0.0):
        self.now = now

    def __call__(self):
        return self.now


def check_multi_cost_and_partial():
    clock = FakeClock()
    limiter = RateLimiter(rate=1.0, capacity=5, clock=clock)
    assert limiter.acquire('k', cost=3).allowed
    d = limiter.acquire('k', cost=3)
    assert not d.allowed
    assert abs(d.retry_after - 1.0) < 1e-9
    clock.now = 1.0
    assert limiter.acquire('k', cost=3).allowed


def check_denied_does_not_consume():
    clock = FakeClock()
    limiter = RateLimiter(rate=1.0, capacity=2, clock=clock)
    assert limiter.acquire('k', cost=2).allowed
    assert not limiter.acquire('k').allowed
    clock.now = 1.0
    assert limiter.acquire('k').allowed
    assert not limiter.acquire('k').allowed


def check_fractional_refill_accumulates():
    clock = FakeClock()
    limiter = RateLimiter(rate=1.0, capacity=1, clock=clock)
    assert limiter.acquire('k').allowed
    clock.now = 0.4
    assert not limiter.acquire('k').allowed
    clock.now = 0.8
    assert not limiter.acquire('k').allowed
    clock.now = 1.2
    assert limiter.acquire('k').allowed
    assert not limiter.acquire('k').allowed


def check_invalid_inputs():
    clock = FakeClock()
    for rate, cap in [(-1.0, 1), (1.0, 0), (0.0, 3)]:
        try:
            RateLimiter(rate=rate, capacity=cap, clock=clock)
        except ValueError:
            continue
        raise AssertionError('constructor accepted bad input')
    limiter = RateLimiter(rate=1.0, capacity=2, clock=clock)
    for cost in (0, -1, 3):
        try:
            limiter.acquire('k', cost=cost)
        except ValueError:
            continue
        raise AssertionError('acquire accepted cost %r' % cost)


def check_forget_only_affects_one_key():
    clock = FakeClock()
    limiter = RateLimiter(rate=1.0, capacity=1, clock=clock)
    limiter.acquire('a')
    limiter.acquire('b')
    limiter.forget('a')
    assert limiter.acquire('a').allowed
    assert not limiter.acquire('b').allowed


def check_forget_unknown_key_is_noop():
    clock = FakeClock()
    limiter = RateLimiter(rate=1.0, capacity=1, clock=clock)
    limiter.forget('never-seen')
    assert limiter.acquire('never-seen').allowed


CHECKS = [
    check_multi_cost_and_partial,
    check_denied_does_not_consume,
    check_fractional_refill_accumulates,
    check_invalid_inputs,
    check_forget_only_affects_one_key,
    check_forget_unknown_key_is_noop,
]


if __name__ == '__main__':
    failures = 0
    for check in CHECKS:
        try:
            check()
        except Exception:
            failures += 1
            print('FAIL', check.__name__)
            traceback.print_exc()
        else:
            print('ok', check.__name__)
    if failures:
        raise SystemExit(1)
```

## The clean review

It found nothing to repair.


## The blind review

1. Critical in src/limiter.py: Off-by-one: a request costing `cost` tokens should be allowed when `tokens >= cost`. With `>`, a full bucket of capacity 3 allows only two requests, and a capacity-1 bucket never allows anything on a fresh key. This breaks `test_burst_then_deny`, `test_retry_after_and_refill` and `test_clock_going_backwards_is_safe`, which all expect the first request to succeed. Change it to `>=`.
   Quoted: if tokens > cost:
   Judged: Matches { issues: [0] }. Correctly identifies the off-by-one comparison at lines 39-40 that denies requests whose cost equals available tokens.
2. Major in src/limiter.py: Buckets are never evicted except through explicit `forget`. Each distinct key (for example, a client IP or user ID) leaves an entry behind forever, so memory grows without bound under high key cardinality. Add an idle-expiry sweep or a bounded LRU.
   Quoted: self._buckets: dict[str, tuple[float, float]] = {}
   Judged: ValidExtra. Unbounded growth of the _buckets dict for high key cardinality is a real memory risk in the code, not planted in the key and not a defect of the clean version, so it is a valid extra.
3. Major in tests/test_limiter.py: This test does not exercise the cap. The key is first seen at t=1000, so the bucket starts full by construction. The test would pass even if `min(self._capacity, ...)` were removed. To prove the cap, create the bucket at t=0, drain it, advance the clock far ahead, and then check that exactly `capacity` requests are allowed.
   Quoted: def test_refill_capped_at_capacity() -> None:     clock = FakeClock()     limiter = RateLimiter(rate=10.0, capacity=2, clock=clock)     clock.now = 1000.0     assert limiter.acquire("a").allowed     assert limiter.acquire("a").allowed     assert not limiter.acquire("a").allowed
   Judged: ValidExtra. The test at lines 38-44 does not actually verify the capacity cap, since the bucket starts full at first sight; this is a real test weakness not in the key.
4. Nit in tests/test_limiter.py: Hand-rolled try/except/else is verbose and easy to get wrong. Use `pytest.raises(ValueError)` with parametrization, which also reports which case failed.
   Quoted:     for args in [(0.0, 1), (1.0, 0)]:         try:             RateLimiter(rate=args[0], capacity=args[1], clock=clock)         except ValueError:             pass         else:             raise AssertionError("expected ValueError")
   Judged: FalsePositive. Preference for pytest.raises over hand-rolled try/except is a style remark, not a defect.

- planted-1 (correctness.wrong_operator): the review Found { finding: 0 }. Finding 1 names the exact cause: the comparison uses '>' where a request costing 'cost' tokens should be allowed when tokens >= cost, so a full-balance request is wrongly denied. It states the consequence (capacity-1 bucket never allows on fresh key) and the fix.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean code (asked again): claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
