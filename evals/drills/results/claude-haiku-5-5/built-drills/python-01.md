# Refund eligibility and partial refund ledger for payments (python)

- Domain: payments and refunds
- Built by: claude-haiku-5-5
- Runner: builtin-python
- Built in 78 seconds
- Patterns asked for: correctness.null_dereference, errors.swallowed_exception, concurrency.check_then_act

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 0
- Dropped: 1
- Found in review: 1

## The code

### src/refunds.py

```text
 1 | from dataclasses import dataclass, field
 2 | from decimal import Decimal
 3 | from typing import Callable
 4 | 
 5 | 
 6 | class RefundError(Exception):
 7 |     pass
 8 | 
 9 | 
10 | @dataclass
11 | class Payment:
12 |     payment_id: str
13 |     amount: Decimal
14 |     captured_at: float
15 |     refunded: Decimal = Decimal("0")
16 |     history: list = field(default_factory=list)
17 | 
18 |     @property
19 |     def remaining(self) -> Decimal:
20 |         return self.amount - self.refunded
21 | 
22 | 
23 | class RefundService:
24 |     """Issues partial or full refunds against captured payments.
25 | 
26 |     The gateway call is injected so the ledger logic stays free of I/O.
27 |     Refund windows are measured against the injected clock, which keeps
28 |     the cutoff deterministic.
29 |     """
30 | 
31 |     def __init__(self, gateway: Callable[[str, Decimal], str],
32 |                  clock: Callable[[], float], window_seconds: float = 30 * 24 * 3600):
33 |         self._gateway = gateway
34 |         self._clock = clock
35 |         self._window = window_seconds
36 |         self._payments: dict[str, Payment] = {}
37 | 
38 |     def register(self, payment: Payment) -> None:
39 |         if payment.payment_id in self._payments:
40 |             raise RefundError(f"duplicate payment {payment.payment_id}")
41 |         self._payments[payment.payment_id] = payment
42 | 
43 |     def refund(self, payment_id: str, amount: Decimal) -> str:
44 |         payment = self._payments.get(payment_id)
45 |         if amount <= 0:
46 |             raise RefundError("refund amount must be positive")
47 |         if self._clock() - payment.captured_at > self._window:
48 |             raise RefundError("refund window has closed")
49 |         if amount > payment.remaining:
50 |             raise RefundError(
51 |                 f"refund {amount} exceeds remaining {payment.remaining}")
52 |         # The gateway is called before the ledger is updated so that a
53 |         # declined refund never leaves a phantom debit on the payment.
54 |         try:
55 |             reference = self._gateway(payment_id, amount)
56 |         except Exception:
57 |             reference = 'unconfirmed'
58 |         payment.refunded += amount
59 |         payment.history.append((reference, amount))
60 |         return reference
61 | 
62 |     def status(self, payment_id: str) -> str:
63 |         payment = self._payments.get(payment_id)
64 |         if payment is None:
65 |             raise RefundError(f"unknown payment {payment_id}")
66 |         if payment.remaining == 0:
67 |             return "fully_refunded"
68 |         if payment.refunded > 0:
69 |             return "partially_refunded"
70 |         return "captured"
```

### tests/test_refunds.py

```text
 1 | from decimal import Decimal
 2 | 
 3 | from src.refunds import Payment, RefundError, RefundService
 4 | 
 5 | 
 6 | def make_service(now=1000.0):
 7 |     calls = []
 8 | 
 9 |     def gateway(payment_id, amount):
10 |         calls.append((payment_id, amount))
11 |         return f"ref-{len(calls)}"
12 | 
13 |     clock = {"t": now}
14 |     service = RefundService(gateway, lambda: clock["t"], window_seconds=100)
15 |     service.register(Payment("p1", Decimal("50.00"), captured_at=now))
16 |     return service, calls, clock
17 | 
18 | 
19 | def test_partial_then_full_refund():
20 |     service, calls, _ = make_service()
21 |     assert service.refund("p1", Decimal("20.00")) == "ref-1"
22 |     assert service.status("p1") == "partially_refunded"
23 |     service.refund("p1", Decimal("30.00"))
24 |     assert service.status("p1") == "fully_refunded"
25 |     assert len(calls) == 2
26 | 
27 | 
28 | def test_over_refund_rejected_without_gateway_call():
29 |     service, calls, _ = make_service()
30 |     try:
31 |         service.refund("p1", Decimal("50.01"))
32 |     except RefundError:
33 |         pass
34 |     else:
35 |         raise AssertionError("expected RefundError")
36 |     assert calls == []
37 | 
38 | 
39 | def test_window_closed_and_unknown_payment():
40 |     service, _, clock = make_service()
41 |     clock["t"] = 1000.0 + 101
42 |     for bad in (lambda: service.refund("p1", Decimal("1")),
43 |                 lambda: service.refund("missing", Decimal("1"))):
44 |         try:
45 |             bad()
46 |         except RefundError:
47 |             continue
48 |         raise AssertionError("expected RefundError")
49 | 
50 | 
51 | def test_zero_amount_rejected():
52 |     service, calls, _ = make_service()
53 |     try:
54 |         service.refund("p1", Decimal("0"))
55 |     except RefundError:
56 |         pass
57 |     else:
58 |         raise AssertionError("expected RefundError")
59 |     assert calls == []
```

## The key

### 1. Missing null check (correctness.null_dereference), Major, planted, src/refunds.py lines 45 to 45

- Consequence: Refunding an unknown payment_id no longer raises RefundError; the lookup returns None and the attribute access raises AttributeError instead. Callers that handle RefundError for unknown payments crash with an unexpected exception.
- In an interview: The payment lookup can return None, and the code now dereferences it without checking. The fix is to restore the None check so an unknown payment raises the domain RefundError before any attribute is read.

Proven by a test that fails on the drill and passes on the clean code:

```text
import sys,os
sys.path.insert(0,os.getcwd())
from decimal import Decimal
from src.refunds import Payment, RefundService, RefundError
svc=RefundService(lambda p,a:'r',lambda:0.0)
try:
    svc.refund('nope', Decimal('1'))
except RefundError:
    pass
except AttributeError:
    raise AssertionError('null deref')
else:
    raise AssertionError('expected RefundError')
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/refunds.py:

```diff
+        if payment is None:
+            raise RefundError(f"unknown payment {payment_id}")
         if amount <= 0:
```

### 2. Swallowed exception (errors.swallowed_exception), Major, planted, src/refunds.py lines 54 to 57

- Consequence: When the gateway declines or fails, the exception is swallowed and the ledger is debited anyway, so the payment shows as partially or fully refunded with no actual refund issued. Customers are shorted and the books no longer match the gateway.
- In an interview: The gateway exception is caught and replaced with a placeholder reference, so the code proceeds as though the refund succeeded. The fix is to let the exception propagate so the ledger update is skipped, which is what the original design intends.

Proven by a test that fails on the drill and passes on the clean code:

```text
import sys,os
sys.path.insert(0,os.getcwd())
from decimal import Decimal
from src.refunds import Payment, RefundService
def bad(p,a): raise RuntimeError('declined')
svc=RefundService(bad,lambda:0.0)
svc.register(Payment('q',Decimal('5'),captured_at=0.0))
try:
    svc.refund('q',Decimal('5'))
except RuntimeError:
    pass
assert svc.status('q')=='captured', svc.status('q')
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/refunds.py:

```diff
-        try:
-            reference = self._gateway(payment_id, amount)
-        except Exception:
-            reference = 'unconfirmed'
+        reference = self._gateway(payment_id, amount)
```

### 3. Check-then-act race (concurrency.check_then_act), Major, found in review, src/refunds.py lines 58 to 59

- Consequence: Two concurrent refunds on the same payment can both pass the remaining-amount check before either updates the ledger, so both reach the gateway and the payment is refunded beyond its captured amount.
- In an interview: The check and the update are separate steps, so another thread can change the remaining amount between them, which is a classic check-then-act race. Re-checking immediately before the update narrows the window, but a real fix needs a per-payment lock held across the check, gateway call and update.

Proven by the judge: The non-atomic check-then-update allowing concurrent over-refunds is a real risk in this code, but it is not the planted issue; it is a valid extra concern.

The fix, in src/refunds.py:

```diff
+        if amount > payment.remaining:
+            raise RefundError(
+                f"refund {amount} exceeds remaining {payment.remaining}")
         payment.refunded += amount
         payment.history.append((reference, amount))
```

## Dropped

- concurrency.check_then_act: The edit replaces a direct membership check on self._payments with a check against a copy taken just before it. The check and the subsequent insert are still not atomic: no lock is held, and the copy is taken at the same point as the check, so two concurrent callers can still both pass the duplicate check before either writes. The race existed in the original code (check-then-insert with no lock), so the edit does not introduce it. The claimed consequence (concurrent duplicate registration silently overwriting a payment) was already possible in the original. The snapshot adds an O(n) copy but no new concurrency hazard beyond the pre-existing one.
  Consequence claimed: When two registrations of the same payment_id run concurrently, both can pass the duplicate check against a stale snapshot and the second write silently replaces the first, so one payment's refund ledger is lost. This only shows under concurrent callers.

## The clean code

### src/refunds.py

```text
 1 | from dataclasses import dataclass, field
 2 | from decimal import Decimal
 3 | from typing import Callable
 4 | 
 5 | 
 6 | class RefundError(Exception):
 7 |     pass
 8 | 
 9 | 
10 | @dataclass
11 | class Payment:
12 |     payment_id: str
13 |     amount: Decimal
14 |     captured_at: float
15 |     refunded: Decimal = Decimal("0")
16 |     history: list = field(default_factory=list)
17 | 
18 |     @property
19 |     def remaining(self) -> Decimal:
20 |         return self.amount - self.refunded
21 | 
22 | 
23 | class RefundService:
24 |     """Issues partial or full refunds against captured payments.
25 | 
26 |     The gateway call is injected so the ledger logic stays free of I/O.
27 |     Refund windows are measured against the injected clock, which keeps
28 |     the cutoff deterministic.
29 |     """
30 | 
31 |     def __init__(self, gateway: Callable[[str, Decimal], str],
32 |                  clock: Callable[[], float], window_seconds: float = 30 * 24 * 3600):
33 |         self._gateway = gateway
34 |         self._clock = clock
35 |         self._window = window_seconds
36 |         self._payments: dict[str, Payment] = {}
37 | 
38 |     def register(self, payment: Payment) -> None:
39 |         if payment.payment_id in self._payments:
40 |             raise RefundError(f"duplicate payment {payment.payment_id}")
41 |         self._payments[payment.payment_id] = payment
42 | 
43 |     def refund(self, payment_id: str, amount: Decimal) -> str:
44 |         payment = self._payments.get(payment_id)
45 |         if payment is None:
46 |             raise RefundError(f"unknown payment {payment_id}")
47 |         if amount <= 0:
48 |             raise RefundError("refund amount must be positive")
49 |         if self._clock() - payment.captured_at > self._window:
50 |             raise RefundError("refund window has closed")
51 |         if amount > payment.remaining:
52 |             raise RefundError(
53 |                 f"refund {amount} exceeds remaining {payment.remaining}")
54 |         # The gateway is called before the ledger is updated so that a
55 |         # declined refund never leaves a phantom debit on the payment.
56 |         reference = self._gateway(payment_id, amount)
57 |         payment.refunded += amount
58 |         payment.history.append((reference, amount))
59 |         return reference
60 | 
61 |     def status(self, payment_id: str) -> str:
62 |         payment = self._payments.get(payment_id)
63 |         if payment is None:
64 |             raise RefundError(f"unknown payment {payment_id}")
65 |         if payment.remaining == 0:
66 |             return "fully_refunded"
67 |         if payment.refunded > 0:
68 |             return "partially_refunded"
69 |         return "captured"
```

### tests/test_refunds.py

```text
 1 | from decimal import Decimal
 2 | 
 3 | from src.refunds import Payment, RefundError, RefundService
 4 | 
 5 | 
 6 | def make_service(now=1000.0):
 7 |     calls = []
 8 | 
 9 |     def gateway(payment_id, amount):
10 |         calls.append((payment_id, amount))
11 |         return f"ref-{len(calls)}"
12 | 
13 |     clock = {"t": now}
14 |     service = RefundService(gateway, lambda: clock["t"], window_seconds=100)
15 |     service.register(Payment("p1", Decimal("50.00"), captured_at=now))
16 |     return service, calls, clock
17 | 
18 | 
19 | def test_partial_then_full_refund():
20 |     service, calls, _ = make_service()
21 |     assert service.refund("p1", Decimal("20.00")) == "ref-1"
22 |     assert service.status("p1") == "partially_refunded"
23 |     service.refund("p1", Decimal("30.00"))
24 |     assert service.status("p1") == "fully_refunded"
25 |     assert len(calls) == 2
26 | 
27 | 
28 | def test_over_refund_rejected_without_gateway_call():
29 |     service, calls, _ = make_service()
30 |     try:
31 |         service.refund("p1", Decimal("50.01"))
32 |     except RefundError:
33 |         pass
34 |     else:
35 |         raise AssertionError("expected RefundError")
36 |     assert calls == []
37 | 
38 | 
39 | def test_window_closed_and_unknown_payment():
40 |     service, _, clock = make_service()
41 |     clock["t"] = 1000.0 + 101
42 |     for bad in (lambda: service.refund("p1", Decimal("1")),
43 |                 lambda: service.refund("missing", Decimal("1"))):
44 |         try:
45 |             bad()
46 |         except RefundError:
47 |             continue
48 |         raise AssertionError("expected RefundError")
49 | 
50 | 
51 | def test_zero_amount_rejected():
52 |     service, calls, _ = make_service()
53 |     try:
54 |         service.refund("p1", Decimal("0"))
55 |     except RefundError:
56 |         pass
57 |     else:
58 |         raise AssertionError("expected RefundError")
59 |     assert calls == []
```

### The hidden suite

```text
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from decimal import Decimal
from src.refunds import Payment, RefundError, RefundService


def make(now=0.0, window=100):
    calls = []
    def gw(pid, amt):
        calls.append((pid, amt))
        return f"r{len(calls)}"
    clock = [now]
    svc = RefundService(gw, lambda: clock[0], window_seconds=window)
    svc.register(Payment("p", Decimal("10"), captured_at=now))
    return svc, calls, clock


def raises(fn):
    try:
        fn()
    except RefundError:
        return True
    return False


def main():
    svc, calls, clock = make()
    assert svc.status("p") == "captured"
    assert svc.refund("p", Decimal("4")) == "r1"
    assert svc.status("p") == "partially_refunded"
    assert svc.refund("p", Decimal("6")) == "r2"
    assert svc.status("p") == "fully_refunded"
    assert calls == [("p", Decimal("4")), ("p", Decimal("6"))]

    svc, calls, _ = make()
    assert raises(lambda: svc.refund("p", Decimal("10.01")))
    assert raises(lambda: svc.refund("p", Decimal("-1")))
    assert raises(lambda: svc.refund("nope", Decimal("1")))
    assert calls == []

    svc, calls, clock = make(now=0.0, window=100)
    clock[0] = 100.0
    assert svc.refund("p", Decimal("1")) == "r1"
    clock[0] = 100.5
    assert raises(lambda: svc.refund("p", Decimal("1")))
    assert len(calls) == 1

    def failing(pid, amt):
        raise RuntimeError("declined")
    svc = RefundService(failing, lambda: 0.0)
    svc.register(Payment("q", Decimal("5"), captured_at=0.0))
    try:
        svc.refund("q", Decimal("5"))
    except RuntimeError:
        pass
    assert svc.status("q") == "captured"

    svc, _, _ = make()
    dup = False
    try:
        svc.register(Payment("p", Decimal("1"), captured_at=0.0))
    except RefundError:
        dup = True
    assert dup


main()
```

## The clean review

- Minor: The test name says it checks an invalid amount, but the body only exercises a closed refund window and an unknown payment ID. The name misleads readers about coverage.

## The blind review

1. Major in src/refunds.py: If payment_id is unknown, payment is None and the next line raises AttributeError on payment.captured_at instead of a RefundError. status() handles this case explicitly; refund() should check for None and raise RefundError(f"unknown payment {payment_id}") before using the payment.
   Quoted:         payment = self._payments.get(payment_id)         if amount <= 0:
   Judged: Matches { issues: [0] }. Correctly identifies that an unknown payment_id yields None and an AttributeError instead of RefundError, which matches issue 1.
2. Critical in src/refunds.py: All gateway exceptions are swallowed and the refund is recorded as successful, debiting the ledger even though the gateway may have declined or failed. This contradicts the comment above it about declined refunds never leaving a phantom debit. Either propagate the error, or record a pending state explicitly, and log the exception. Catching Exception broadly also hides programming errors.
   Quoted:         except Exception:             reference = 'unconfirmed'         payment.refunded += amount
   Judged: Matches { issues: [1] }. Correctly identifies the swallowed gateway exception that debits the ledger despite the gateway failing, which matches issue 2.
3. Major in src/refunds.py: The gateway call is outside any lock and the check-then-update sequence is not atomic. Concurrent refunds on the same payment can both pass the remaining-amount check and over-refund. Guard the check and update with a lock per payment.
   Quoted:         payment.refunded += amount         payment.history.append((reference, amount))
   Judged: ValidExtra. The non-atomic check-then-update allowing concurrent over-refunds is a real risk in this code, but it is not the planted issue; it is a valid extra concern.
4. Nit in src/refunds.py: history is untyped and holds (reference, amount) tuples; use list[tuple[str, Decimal]] for clarity.
   Quoted:     refunded: Decimal = Decimal("0")     history: list = field(default_factory=list)
   Judged: FalsePositive. Suggesting a more specific type annotation for history is a style preference, not a defect.
5. Minor in src/refunds.py: Amount validation runs after the payment lookup; order is fine but the amount is never checked for type. A float passed in would be mixed with Decimal arithmetic. Consider rejecting non-Decimal input.
   Quoted:         if amount <= 0:             raise RefundError("refund amount must be positive")
   Judged: FalsePositive. Asks for runtime type checking of amount; this is a preference and not a real defect in the code, so it is only a remark.
6. Minor in tests/test_refunds.py: The test combines two distinct behaviors in one loop and the unknown-payment case currently passes only by accident (AttributeError is not RefundError, so it would fail; the test would actually expose the bug). The test also never verifies the gateway was not called for the closed window. Split into separate tests with explicit assertions that the gateway is not called.
   Quoted: def test_window_closed_and_unknown_payment():
   Judged: FalsePositive. Complains about how the test file combines cases; this is a remark about test structure, not a real defect in the code under review.
7. Major in tests/test_refunds.py: No test covers gateway failure. Given the swallowed exception behavior, a test asserting that a failing gateway leaves refunded unchanged (or raises) is needed, and currently none exists.
   Quoted: def test_partial_then_full_refund():     service, calls, _ = make_service()     assert service.refund("p1", Decimal("20.00")) == "ref-1"
   Judged: Matches { issues: [1] }. Points at the missing gateway-failure test, tied to the swallowed exception behaviour of issue 2. It points at issue 2 and so matches it, though finding 2 earns the credit.

- planted-1 (correctness.null_dereference): the review Found { finding: 0 }. Finding 1 names the null-payment problem: payment is None, so the next line raises AttributeError instead of RefundError, and it asks for a None check that raises RefundError. That is the cause of issue 1.
- planted-2 (errors.swallowed_exception): the review Found { finding: 1 }. Finding 2 says all gateway exceptions are swallowed and the refund is recorded as successful, debiting the ledger although the gateway may have failed. That names the swallowed-exception cause and its consequence. Finding 7 also points at this behaviour, but finding 2 names the cause best.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
