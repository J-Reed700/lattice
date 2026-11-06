# Inventory stock reservation ledger with expiring holds (python)

- Domain: inventory and stock reservations
- Built by: claude-haiku-5-5
- Runner: builtin-python
- Built in 97 seconds
- Patterns asked for: correctness.off_by_one, errors.error_as_success, concurrency.unsynchronised_shared_state

## Counts

- Planted: 3
- Proven by a test: 3
- Proven by the judge: 0
- Dropped: 0
- Found in review: 0

## The code

### src/reservations.py

```text
 1 | from dataclasses import dataclass, field
 2 | 
 3 | 
 4 | @dataclass
 5 | class Hold:
 6 |     hold_id: str
 7 |     sku: str
 8 |     quantity: int
 9 |     expires_at: float
10 | 
11 | 
12 | @dataclass
13 | _SHARED_HOLDS: dict[str, Hold] = {}
14 | 
15 | 
16 | class StockLedger:
17 |     on_hand: dict[str, int] = field(default_factory=dict)
18 |     holds: dict[str, Hold] = field(default_factory=lambda: _SHARED_HOLDS)
19 | 
20 |     def reserved_for(self, sku: str, now: float) -> int:
21 |         # Expired holds stop counting immediately, even before they are swept,
22 |         # so a stale hold never blocks a real order.
23 |         return sum(h.quantity for h in self.holds.values()
24 |                    if h.sku == sku and h.expires_at >= now)
25 | 
26 |     def available(self, sku: str, now: float) -> int:
27 |         return self.on_hand.get(sku, 0) - self.reserved_for(sku, now)
28 | 
29 |     def reserve(self, hold_id: str, sku: str, quantity: int, now: float, ttl: float) -> Hold:
30 |         if quantity <= 0:
31 |             raise ValueError("quantity must be positive")
32 |         if ttl <= 0:
33 |             raise ValueError("ttl must be positive")
34 |         existing = self.holds.get(hold_id)
35 |         if existing is not None and existing.expires_at > now:
36 |             raise KeyError(f"duplicate hold id: {hold_id}")
37 |         if quantity > self.available(sku, now):
38 |             raise ValueError(f"insufficient stock for {sku}")
39 |         hold = Hold(hold_id, sku, quantity, now + ttl)
40 |         self.holds[hold_id] = hold
41 |         return hold
42 | 
43 |     def release(self, hold_id: str) -> None:
44 |         self.holds.pop(hold_id, None)
45 | 
46 |     def commit(self, hold_id: str, now: float) -> int:
47 |         # Committing consumes stock; an expired hold can no longer be committed
48 |         # because another order may already have been given that stock.
49 |         hold = self.holds.pop(hold_id, None)
50 |         if hold is None or hold.expires_at <= now:
51 |             return 0
52 |         self.on_hand[hold.sku] = self.on_hand.get(hold.sku, 0) - hold.quantity
53 |         return hold.quantity
54 | 
55 |     def sweep(self, now: float) -> int:
56 |         # Drops expired holds so the table does not grow without bound.
57 |         expired = [k for k, h in self.holds.items() if h.expires_at <= now]
58 |         for key in expired:
59 |             del self.holds[key]
60 |         return len(expired)
```

### src/__init__.py

```text
1 | # Marks the package root; intentionally empty.
```

### tests/test_reservations.py

```text
 1 | from src.reservations import StockLedger
 2 | 
 3 | 
 4 | def test_reserve_and_available():
 5 |     ledger = StockLedger(on_hand={"A": 10})
 6 |     ledger.reserve("h1", "A", 4, now=0.0, ttl=60)
 7 |     assert ledger.available("A", now=1.0) == 6
 8 | 
 9 | 
10 | def test_insufficient_stock_rejected():
11 |     ledger = StockLedger(on_hand={"A": 3})
12 |     try:
13 |         ledger.reserve("h1", "A", 5, now=0.0, ttl=60)
14 |     except ValueError:
15 |         pass
16 |     else:
17 |         raise AssertionError("expected ValueError")
18 |     assert ledger.available("A", now=0.0) == 3
19 | 
20 | 
21 | def test_expired_hold_frees_stock():
22 |     ledger = StockLedger(on_hand={"A": 5})
23 |     ledger.reserve("h1", "A", 5, now=0.0, ttl=10)
24 |     assert ledger.available("A", now=9.0) == 0
25 |     assert ledger.available("A", now=10.0) == 5
26 | 
27 | 
28 | def test_commit_consumes_stock():
29 |     ledger = StockLedger(on_hand={"A": 8})
30 |     ledger.reserve("h1", "A", 3, now=0.0, ttl=60)
31 |     assert ledger.commit("h1", now=1.0) == 3
32 |     assert ledger.on_hand["A"] == 5
33 |     assert ledger.available("A", now=1.0) == 5
34 | 
35 | 
36 | def test_commit_expired_hold_fails():
37 |     ledger = StockLedger(on_hand={"A": 8})
38 |     ledger.reserve("h1", "A", 3, now=0.0, ttl=5)
39 |     try:
40 |         ledger.commit("h1", now=5.0)
41 |     except LookupError:
42 |         pass
43 |     else:
44 |         raise AssertionError("expected LookupError")
45 |     assert ledger.on_hand["A"] == 8
46 | 
47 | 
48 | def test_release_and_sweep():
49 |     ledger = StockLedger(on_hand={"A": 4})
50 |     ledger.reserve("h1", "A", 2, now=0.0, ttl=5)
51 |     ledger.reserve("h2", "A", 2, now=0.0, ttl=50)
52 |     ledger.release("h2")
53 |     assert ledger.sweep(now=10.0) == 1
54 |     assert ledger.holds == {}
55 | 
56 | 
57 | def test_duplicate_hold_id_rejected():
58 |     ledger = StockLedger(on_hand={"A": 4})
59 |     ledger.reserve("h1", "A", 1, now=0.0, ttl=60)
60 |     try:
61 |         ledger.reserve("h1", "A", 1, now=0.0, ttl=60)
62 |     except KeyError:
63 |         pass
64 |     else:
65 |         raise AssertionError("expected KeyError")
66 | 
67 | 
68 | if __name__ == "__main__":
69 |     for name, fn in list(globals().items()):
70 |         if name.startswith("test_"):
71 |             fn()
```

## The key

### 1. Off-by-one error (correctness.off_by_one), Major, planted, src/reservations.py lines 24 to 24

- Consequence: At the exact expiry instant a hold still counts against stock, so available() reports zero for one tick too long and a customer's order is wrongly rejected as insufficient stock at the moment the hold lapses.
- In an interview: The comparison makes a hold live at now == expires_at, contradicting the comment that expired holds stop counting immediately and the sweep/commit boundary, which treat expires_at as exclusive. Use a strict comparison so expiry is consistent everywhere.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.reservations import StockLedger
ledger = StockLedger(on_hand={"A": 5})
ledger.reserve("h1", "A", 5, now=0.0, ttl=10)
assert ledger.available("A", now=10.0) == 5
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/reservations.py:

```diff
-if h.sku == sku and h.expires_at >= now)
+if h.sku == sku and h.expires_at > now)
```

### 2. Error reported as success (errors.error_as_success), Major, planted, src/reservations.py lines 51 to 51

- Consequence: Committing an unknown or expired hold returns 0 instead of failing, so the caller records a successful order with no stock consumed and the customer is charged or shipped for nothing.
- In an interview: The commit path now reports a missing or expired hold as a successful zero-quantity commit, so callers cannot distinguish failure from success. Raise LookupError for a dead hold so the caller must handle the failure explicitly.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.reservations import StockLedger
ledger = StockLedger(on_hand={"A": 5})
try:
    ledger.commit("nope", now=0.0)
except LookupError:
    pass
else:
    raise AssertionError("expected LookupError")
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/reservations.py:

```diff
-            return 0
+            raise LookupError(f"no live hold: {hold_id}")
```

### 3. Unsynchronised shared state (concurrency.unsynchronised_shared_state), Critical, planted, src/reservations.py lines 13 to 18

- Consequence: Every ledger instance shares one unguarded hold table, so concurrent reserve and release calls from different requests interleave their check-then-insert and a stock level read by one request reflects another request's holds, overselling stock or losing holds.
- In an interview: The hold table is a module-level dict that every StockLedger mutates with no lock, so the availability check in reserve and the insert are not atomic across threads. Keep state per instance and guard shared mutation with a threading.Lock so check-and-reserve is one critical section.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.reservations import StockLedger
l1 = StockLedger(on_hand={"A": 5})
l1.reserve("h1", "A", 5, now=0.0, ttl=60)
l2 = StockLedger(on_hand={"A": 5})
assert l2.available("A", now=0.0) == 5
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/reservations.py:

```diff
-_SHARED_HOLDS: dict[str, Hold] = {}
-
-
 class StockLedger:
     on_hand: dict[str, int] = field(default_factory=dict)
-    holds: dict[str, Hold] = field(default_factory=lambda: _SHARED_HOLDS)
+    holds: dict[str, Hold] = field(default_factory=dict)
```

## Dropped

Nothing was dropped.


## The clean code

### src/reservations.py

```text
 1 | from dataclasses import dataclass, field
 2 | 
 3 | 
 4 | @dataclass
 5 | class Hold:
 6 |     hold_id: str
 7 |     sku: str
 8 |     quantity: int
 9 |     expires_at: float
10 | 
11 | 
12 | @dataclass
13 | class StockLedger:
14 |     on_hand: dict[str, int] = field(default_factory=dict)
15 |     holds: dict[str, Hold] = field(default_factory=dict)
16 | 
17 |     def reserved_for(self, sku: str, now: float) -> int:
18 |         # Expired holds stop counting immediately, even before they are swept,
19 |         # so a stale hold never blocks a real order.
20 |         return sum(h.quantity for h in self.holds.values()
21 |                    if h.sku == sku and h.expires_at > now)
22 | 
23 |     def available(self, sku: str, now: float) -> int:
24 |         return self.on_hand.get(sku, 0) - self.reserved_for(sku, now)
25 | 
26 |     def reserve(self, hold_id: str, sku: str, quantity: int, now: float, ttl: float) -> Hold:
27 |         if quantity <= 0:
28 |             raise ValueError("quantity must be positive")
29 |         if ttl <= 0:
30 |             raise ValueError("ttl must be positive")
31 |         existing = self.holds.get(hold_id)
32 |         if existing is not None and existing.expires_at > now:
33 |             raise KeyError(f"duplicate hold id: {hold_id}")
34 |         if quantity > self.available(sku, now):
35 |             raise ValueError(f"insufficient stock for {sku}")
36 |         hold = Hold(hold_id, sku, quantity, now + ttl)
37 |         self.holds[hold_id] = hold
38 |         return hold
39 | 
40 |     def release(self, hold_id: str) -> None:
41 |         self.holds.pop(hold_id, None)
42 | 
43 |     def commit(self, hold_id: str, now: float) -> int:
44 |         # Committing consumes stock; an expired hold can no longer be committed
45 |         # because another order may already have been given that stock.
46 |         hold = self.holds.pop(hold_id, None)
47 |         if hold is None or hold.expires_at <= now:
48 |             raise LookupError(f"no live hold: {hold_id}")
49 |         self.on_hand[hold.sku] = self.on_hand.get(hold.sku, 0) - hold.quantity
50 |         return hold.quantity
51 | 
52 |     def sweep(self, now: float) -> int:
53 |         # Drops expired holds so the table does not grow without bound.
54 |         expired = [k for k, h in self.holds.items() if h.expires_at <= now]
55 |         for key in expired:
56 |             del self.holds[key]
57 |         return len(expired)
```

### src/__init__.py

```text
1 | # Marks the package root; intentionally empty.
```

### tests/test_reservations.py

```text
 1 | from src.reservations import StockLedger
 2 | 
 3 | 
 4 | def test_reserve_and_available():
 5 |     ledger = StockLedger(on_hand={"A": 10})
 6 |     ledger.reserve("h1", "A", 4, now=0.0, ttl=60)
 7 |     assert ledger.available("A", now=1.0) == 6
 8 | 
 9 | 
10 | def test_insufficient_stock_rejected():
11 |     ledger = StockLedger(on_hand={"A": 3})
12 |     try:
13 |         ledger.reserve("h1", "A", 5, now=0.0, ttl=60)
14 |     except ValueError:
15 |         pass
16 |     else:
17 |         raise AssertionError("expected ValueError")
18 |     assert ledger.available("A", now=0.0) == 3
19 | 
20 | 
21 | def test_expired_hold_frees_stock():
22 |     ledger = StockLedger(on_hand={"A": 5})
23 |     ledger.reserve("h1", "A", 5, now=0.0, ttl=10)
24 |     assert ledger.available("A", now=9.0) == 0
25 |     assert ledger.available("A", now=10.0) == 5
26 | 
27 | 
28 | def test_commit_consumes_stock():
29 |     ledger = StockLedger(on_hand={"A": 8})
30 |     ledger.reserve("h1", "A", 3, now=0.0, ttl=60)
31 |     assert ledger.commit("h1", now=1.0) == 3
32 |     assert ledger.on_hand["A"] == 5
33 |     assert ledger.available("A", now=1.0) == 5
34 | 
35 | 
36 | def test_commit_expired_hold_fails():
37 |     ledger = StockLedger(on_hand={"A": 8})
38 |     ledger.reserve("h1", "A", 3, now=0.0, ttl=5)
39 |     try:
40 |         ledger.commit("h1", now=5.0)
41 |     except LookupError:
42 |         pass
43 |     else:
44 |         raise AssertionError("expected LookupError")
45 |     assert ledger.on_hand["A"] == 8
46 | 
47 | 
48 | def test_release_and_sweep():
49 |     ledger = StockLedger(on_hand={"A": 4})
50 |     ledger.reserve("h1", "A", 2, now=0.0, ttl=5)
51 |     ledger.reserve("h2", "A", 2, now=0.0, ttl=50)
52 |     ledger.release("h2")
53 |     assert ledger.sweep(now=10.0) == 1
54 |     assert ledger.holds == {}
55 | 
56 | 
57 | def test_duplicate_hold_id_rejected():
58 |     ledger = StockLedger(on_hand={"A": 4})
59 |     ledger.reserve("h1", "A", 1, now=0.0, ttl=60)
60 |     try:
61 |         ledger.reserve("h1", "A", 1, now=0.0, ttl=60)
62 |     except KeyError:
63 |         pass
64 |     else:
65 |         raise AssertionError("expected KeyError")
66 | 
67 | 
68 | if __name__ == "__main__":
69 |     for name, fn in list(globals().items()):
70 |         if name.startswith("test_"):
71 |             fn()
```

### The hidden suite

```text
from src.reservations import StockLedger


def expect(exc, fn):
    try:
        fn()
    except exc:
        return
    raise AssertionError(f"expected {exc.__name__}")


def test_multiple_skus_are_independent():
    ledger = StockLedger(on_hand={"A": 5, "B": 2})
    ledger.reserve("h1", "A", 5, now=0.0, ttl=30)
    assert ledger.available("B", now=0.0) == 2
    assert ledger.available("A", now=0.0) == 0


def test_rejects_bad_arguments():
    ledger = StockLedger(on_hand={"A": 5})
    expect(ValueError, lambda: ledger.reserve("h1", "A", 0, now=0.0, ttl=1))
    expect(ValueError, lambda: ledger.reserve("h2", "A", 1, now=0.0, ttl=0))
    assert ledger.holds == {}


def test_commit_unknown_hold_fails():
    ledger = StockLedger(on_hand={"A": 5})
    expect(LookupError, lambda: ledger.commit("nope", now=0.0))


def test_commit_twice_fails():
    ledger = StockLedger(on_hand={"A": 5})
    ledger.reserve("h1", "A", 2, now=0.0, ttl=60)
    ledger.commit("h1", now=1.0)
    expect(LookupError, lambda: ledger.commit("h1", now=1.0))
    assert ledger.on_hand["A"] == 3


def test_sweep_keeps_live_holds():
    ledger = StockLedger(on_hand={"A": 5})
    ledger.reserve("h1", "A", 2, now=0.0, ttl=100)
    assert ledger.sweep(now=50.0) == 0
    assert "h1" in ledger.holds


def test_expired_hold_id_can_be_reused_after_sweep():
    ledger = StockLedger(on_hand={"A": 5})
    ledger.reserve("h1", "A", 2, now=0.0, ttl=5)
    ledger.sweep(now=6.0)
    ledger.reserve("h1", "A", 5, now=6.0, ttl=5)
    assert ledger.available("A", now=6.0) == 0


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_"):
            fn()
```

## The clean review

- Minor: Duplicate-id check ignores expiry. An expired hold stays in the table until swept, so reusing its id is rejected as a duplicate even though the stale hold should not block anything, contrary to the reserved_for comment.

## The blind review

1. Critical in src/reservations.py: A decorator is applied to a module-level dict assignment, which is a syntax/semantic error: @dataclass must decorate a class. The module will fail to import as written. Remove the decorator.
   Quoted: @dataclass _SHARED_HOLDS: dict[str, Hold] = {}
   Judged: ValidExtra. A real defect: @dataclass is applied to a module-level annotated assignment, which is a syntax error, so the module cannot be imported. It is not the key's shared-state issue.
2. Critical in src/reservations.py: StockLedger is not decorated with @dataclass, so the field(...) defaults are never turned into constructor parameters. StockLedger(on_hand={...}) used throughout the tests will raise TypeError. Add @dataclass to the class.
   Quoted: class StockLedger:     on_hand: dict[str, int] = field(default_factory=dict)     holds: dict[str, Hold] = field(default_factory=lambda: _SHARED_HOLDS)
   Judged: ValidExtra. A real defect: StockLedger lacks @dataclass, so the field defaults do not become constructor parameters and StockLedger(on_hand=...) raises TypeError. The key does not hold this problem.
3. Critical in src/reservations.py: Every ledger shares one module-level dict by default, so holds from one instance leak into all others. Reservations in one ledger would reduce availability in unrelated ledgers and tests would interfere with each other. Use default_factory=dict.
   Quoted: holds: dict[str, Hold] = field(default_factory=lambda: _SHARED_HOLDS)
   Judged: Matches { issues: [2] }. Correctly names the cause of issue 1: the shared module-level default dict makes holds leak across ledger instances.
4. Major in src/reservations.py: An expired hold is popped and discarded, and the caller gets 0 with no signal that the commit failed. Callers cannot distinguish 'unknown hold' from 'expired' from 'committed zero'. Consider raising a clear error, which also aligns with the test expecting an exception.
   Quoted:         hold = self.holds.pop(hold_id, None)         if hold is None or hold.expires_at <= now:             return 0
   Judged: Matches { issues: [1] }. Correctly names the cause of issue 3: commit returns 0 on an expired or unknown hold rather than signalling failure.
5. Minor in src/reservations.py: The check ignores the existing hold for hold_id when it is expired but still stored, which is fine, but the duplicate check raises KeyError while the insufficient check raises ValueError, mixing exception types for similar failures. Also commit() reports missing holds by returning 0 instead of raising, so error handling is inconsistent across the API.
   Quoted:         if quantity > self.available(sku, now):             raise ValueError(f"insufficient stock for {sku}")
   Judged: Matches { issues: [1] }. Mainly about mixed exception types, but its remark that commit returns 0 instead of raising for missing holds names issue 3's cause in part.
6. Minor in src/reservations.py: commit can drive on_hand negative if stock was reduced or holds were inconsistent; there is no guard. Stock invariants should be enforced.
   Quoted:         self.on_hand[hold.sku] = self.on_hand.get(hold.sku, 0) - hold.quantity
   Judged: FalsePositive. Asks for a guard against negative on_hand. The clean code has no such guard and is sound without it, so this is a preference rather than a real defect.
7. Major in src/reservations.py: reserved_for treats a hold as live when expires_at >= now, while sweep and commit treat expires_at <= now as expired. At exactly expires_at == now the hold counts as reserved in reserved_for but is removed by sweep, and commit rejects it. The boundary is inconsistent; pick one convention.
   Quoted:         expired = [k for k, h in self.holds.items() if h.expires_at <= now]
   Judged: Matches { issues: [0] }. Correctly identifies the inconsistent expiry boundary between reserved_for and sweep/commit, which is issue 2.
8. Critical in tests/test_reservations.py: The test expects LookupError, but commit() returns 0 for expired holds and never raises, so this test fails. It also reflects the unresolved contract between code and test. Also the test does not verify that the hold was removed.
   Quoted:     try:         ledger.commit("h1", now=5.0)     except LookupError:         pass     else:         raise AssertionError("expected LookupError")
   Judged: Matches { issues: [1] }. Notes that the test expects LookupError but commit returns 0 and never raises, which is the consequence of issue 3's silent-success return.
9. Minor in tests/test_reservations.py: The test pins the boundary at expires_at == now as expired for availability, while reserved_for treats it as live under >=. The test asserts behavior that conflicts with the reservation logic at the boundary is ambiguous; this should be explicit and consistent.
   Quoted:     ledger.reserve("h1", "A", 5, now=0.0, ttl=10)     assert ledger.available("A", now=9.0) == 0     assert ledger.available("A", now=10.0) == 5
   Judged: Matches { issues: [0] }. Points at the boundary conflict between the test and reserved_for, but the cause is vague: it calls the behaviour ambiguous rather than naming the >= bug as the defect.
10. Minor in tests/test_reservations.py: Tests share no fixtures and rely on the module-level shared dict; with the shared default, test order affects results. Tests should isolate state. Also a runner like pytest is preferred over this hand-rolled loop.
   Quoted: if __name__ == "__main__":     for name, fn in list(globals().items()):         if name.startswith("test_"):             fn()
   Judged: Matches { issues: [2] }. Notes that the shared module-level dict makes test results depend on order, which is issue 1's effect. The pytest preference is only a remark.
11. Minor in tests/test_reservations.py: The test never checks that release actually removed h2 from available counts or that h1 is the swept hold; it only checks the sweep count and empty table. Assert availability before and after to prove the behavior.
   Quoted: def test_release_and_sweep():     ledger = StockLedger(on_hand={"A": 4})
   Judged: FalsePositive. Asks for more assertions in the release test. This is a test-coverage preference, not a defect in the code.

- planted-1 (correctness.off_by_one): the review Found { finding: 6 }. Finding 7 identifies the boundary mismatch: reserved_for uses expires_at >= now, so at exact expiry a hold still counts as reserved, while sweep and commit treat it as expired. That is the off-by-one on line 24.
- planted-2 (errors.error_as_success): the review Found { finding: 3 }. Finding 4 names the cause: commit returns 0 for an expired or unknown hold with no signal of failure, so callers cannot tell failure from a zero commit.
- planted-3 (concurrency.unsynchronised_shared_state): the review Found { finding: 2 }. Finding 3 names the cause: the default holds dict is one module-level table shared by every ledger, so holds from one instance leak into the others. That is the unsynchronised shared state the key describes.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
