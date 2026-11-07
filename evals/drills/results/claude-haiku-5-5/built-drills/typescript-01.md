# Refund eligibility and partial refund allocation for payment captures (typescript)

- Domain: payments and refunds
- Built by: claude-haiku-5-5
- Runner: none, every defect was judged
- Built in 71 seconds
- Patterns asked for: correctness.null_dereference, errors.swallowed_exception, concurrency.check_then_act

## Counts

- Planted: 3
- Proven by a test: 0
- Proven by the judge: 2
- Dropped: 1
- Found in review: 2

## The code

### src/payments/refunds.ts

```text
 1 | export type Currency = "USD" | "EUR" | "GBP";
 2 | 
 3 | export interface Capture {
 4 |   id: string;
 5 |   currency: Currency;
 6 |   amountMinor: number;
 7 |   refundedMinor: number;
 8 |   capturedAt: Date;
 9 | }
10 | 
11 | export interface RefundRequest {
12 |   captureId: string;
13 |   amountMinor: number;
14 |   currency: Currency;
15 | }
16 | 
17 | export type RefundDecision =
18 |   | { ok: true; refundableMinor: number }
19 |   | { ok: false; reason: "unknown_capture" | "currency_mismatch" | "exceeds_balance" | "non_positive_amount" | "window_closed" };
20 | 
21 | // Card networks generally refuse refunds on captures older than this window;
22 | // older captures must be handled through a manual dispute instead.
23 | const REFUND_WINDOW_MS = 180 * 24 * 60 * 60 * 1000;
24 | 
25 | export function remainingRefundable(capture: Capture): number {
26 |   return Math.max(0, capture.amountMinor - capture.refundedMinor);
27 | }
28 | 
29 | export function evaluateRefund(
30 |   request: RefundRequest,
31 |   captures: ReadonlyMap<string, Capture>,
32 |   now: Date,
33 | ): RefundDecision {
34 |   if (!Number.isSafeInteger(request.amountMinor) || request.amountMinor <= 0) {
35 |     return { ok: false, reason: "non_positive_amount" };
36 |   }
37 | 
38 |   const capture = captures.get(request.captureId);
39 |   if (capture.currency !== request.currency) {
40 |     return { ok: false, reason: "currency_mismatch" };
41 |   }
42 | 
43 |   try {
44 |     if (now.getTime() - capture.capturedAt.getTime() > REFUND_WINDOW_MS) {
45 |       return { ok: false, reason: "window_closed" };
46 |     }
47 |   } catch {
48 |   }
49 | 
50 |   const refundableMinor = remainingRefundable(capture);
51 |   if (request.amountMinor > refundableMinor) {
52 |     return { ok: false, reason: "exceeds_balance" };
53 |   }
54 | 
55 |   return { ok: true, refundableMinor: refundableMinor - request.amountMinor };
56 | }
57 | 
58 | // Splits a refund across captures in the order given, drawing each one down
59 | // before moving to the next. Returns the allocations, or null if the total
60 | // cannot be covered.
61 | export function allocateRefund(
62 |   amountMinor: number,
63 |   captures: readonly Capture[],
64 | ): Array<{ captureId: string; amountMinor: number }> | null {
65 |   if (!Number.isSafeInteger(amountMinor) || amountMinor <= 0) {
66 |     return null;
67 |   }
68 | 
69 |   let remaining = amountMinor;
70 |   const allocations: Array<{ captureId: string; amountMinor: number }> = [];
71 | 
72 |   const currency = captures.length > 0 ? captures[0].currency : undefined;
73 |   for (const capture of captures) {
74 |     if (capture.currency !== currency) return null;
75 |     if (remaining === 0) break;
76 |     const take = Math.min(remaining, remainingRefundable(capture));
77 |     if (take > 0) {
78 |       allocations.push({ captureId: capture.id, amountMinor: take });
79 |       remaining -= take;
80 |     }
81 |   }
82 | 
83 |   return remaining === 0 ? allocations : null;
84 | }
```

### test/payments/refunds.test.ts

```text
 1 | import { describe, expect, it } from "vitest";
 2 | import { allocateRefund, Capture, evaluateRefund } from "../../src/payments/refunds";
 3 | 
 4 | const now = new Date("2024-06-01T00:00:00Z");
 5 | 
 6 | function capture(overrides: Partial<Capture> = {}): Capture {
 7 |   return {
 8 |     id: "cap_1",
 9 |     currency: "USD",
10 |     amountMinor: 1000,
11 |     refundedMinor: 0,
12 |     capturedAt: new Date("2024-05-01T00:00:00Z"),
13 |     ...overrides,
14 |   };
15 | }
16 | 
17 | describe("evaluateRefund", () => {
18 |   it("accepts a partial refund and reports what remains", () => {
19 |     const captures = new Map([["cap_1", capture()]]);
20 |     const result = evaluateRefund({ captureId: "cap_1", amountMinor: 400, currency: "USD" }, captures, now);
21 |     expect(result).toEqual({ ok: true, refundableMinor: 600 });
22 |   });
23 | 
24 |   it("rejects amounts above the remaining balance", () => {
25 |     const captures = new Map([["cap_1", capture({ refundedMinor: 700 })]]);
26 |     const result = evaluateRefund({ captureId: "cap_1", amountMinor: 400, currency: "USD" }, captures, now);
27 |     expect(result).toEqual({ ok: false, reason: "exceeds_balance" });
28 |   });
29 | 
30 |   it("rejects refunds on captures outside the window", () => {
31 |     const captures = new Map([["cap_1", capture({ capturedAt: new Date("2023-01-01T00:00:00Z") })]]);
32 |     const result = evaluateRefund({ captureId: "cap_1", amountMinor: 100, currency: "USD" }, captures, now);
33 |     expect(result).toEqual({ ok: false, reason: "window_closed" });
34 |   });
35 | });
36 | 
37 | describe("allocateRefund", () => {
38 |   it("spreads a refund across captures in order", () => {
39 |     const captures = [
40 |       capture({ id: "a", amountMinor: 300 }),
41 |       capture({ id: "b", amountMinor: 500 }),
42 |     ];
43 |     expect(allocateRefund(600, captures)).toEqual([
44 |       { captureId: "a", amountMinor: 300 },
45 |       { captureId: "b", amountMinor: 300 },
46 |     ]);
47 |   });
48 | 
49 |   it("returns null when the captures cannot cover the amount", () => {
50 |     expect(allocateRefund(900, [capture({ amountMinor: 300 })])).toBeNull();
51 |   });
52 | });
```

## The key

### 1. Missing null check (correctness.null_dereference), Major, planted, src/payments/refunds.ts lines 39 to 39

- Consequence: A refund request naming a capture id that does not exist throws a TypeError on capture.currency instead of returning unknown_capture, so the request fails with a 500 rather than a clean rejection.
- In an interview: The lookup in captures.get returns undefined for unknown ids, and the guard that handles that case has been removed, so the next property access dereferences undefined. The fix is to restore the early return for a missing capture before any of its fields are read.

Proven by the judge: The edit removes the `if (!capture)` guard that returned unknown_capture. Now `captures.get` returns `Capture | undefined` and `capture.currency` is accessed directly, so an unknown captureId causes a TypeError. The claimed consequence (TypeError on capture.currency for a nonexistent capture id) matches this. The original code had the guard, so it did not have this null dereference. The edit introduces exactly this problem.


The fix, in src/payments/refunds.ts:

```diff
+  if (!capture) {
+    return { ok: false, reason: "unknown_capture" };
+  }
+
   if (capture.currency !== request.currency) {
```

### 2. Swallowed exception (errors.swallowed_exception), Major, planted, src/payments/refunds.ts lines 43 to 48

- Consequence: If the window computation throws, for example on a malformed capturedAt date, the error is discarded and the refund proceeds as if the capture were inside the window, so refunds past the network limit get sent and fail downstream or get accepted wrongly.
- In an interview: The try block catches any exception from the window check and does nothing with it, so a broken timestamp silently passes as in-window. The fix is to let the error propagate or to return an explicit rejection so the failure is visible to the caller.

Proven by the judge: The edit wraps the window check in a try block with an empty catch, so any exception thrown during the window computation is silently discarded and execution continues to the balance check and approval. This matches the swallowed-exception pattern: the error is caught and ignored. The original code had no try/catch, so an exception from a malformed capturedAt would propagate rather than be swallowed. The claimed consequence (refund proceeds past the window after a throw) is plausible in principle: for an invalid Date, getTime() returns NaN, and NaN > REFUND_WINDOW_MS is false, so the check would not throw at all but would simply pass, which means the consequence as described via throwing is not really what happens; still, the edit does introduce a swallowed exception pattern, which is the pattern being judged. The edit introduces exactly the swallowed-exception problem and the original lacks it.


The fix, in src/payments/refunds.ts:

```diff
-  try {
-    if (now.getTime() - capture.capturedAt.getTime() > REFUND_WINDOW_MS) {
-      return { ok: false, reason: "window_closed" };
-    }
-  } catch {
+  if (now.getTime() - capture.capturedAt.getTime() > REFUND_WINDOW_MS) {
+    return { ok: false, reason: "window_closed" };
   }
```

### 3. Error reported as success (errors.error_as_success), Major, found in review, src/payments/refunds.ts lines 74 to 74

- Consequence: When captures mix currencies, allocateRefund returns null, the same value it returns when the captures cannot cover the amount. A caller that treats null as insufficient balance can show the wrong message, retry with a smaller amount, or escalate the wrong issue, and it cannot tell a currency problem from a balance problem.
- In an interview: A failure that is reported as the same null value as a different failure hides which rule was broken from the caller. Raising a distinct error for the currency mismatch, or returning a tagged result, lets callers handle each case correctly.

Proven by the judge: Points at allocateRefund's currency-mismatch null return, which conflates failures, and its missing window check. These are real defects in the code but are not the planted issues.

The fix, in src/payments/refunds.ts:

```diff
-    if (capture.currency !== currency) return null;
+    if (capture.currency !== currency) {
+      throw new Error(`allocateRefund: capture ${capture.id} currency ${capture.currency} does not match ${currency}`);
+    }
```

### 4. Misleading name (readability.misleading_name), Minor, found in review, src/payments/refunds.ts lines 55 to 55

- Consequence: Callers who read refundableMinor as the amount still refundable before this request will treat the value as a cap and may compute wrong limits or display wrong balances after a successful refund.
- In an interview: The field is named as if it held the amount refundable before the request, but it actually holds the balance remaining after applying the refund. Because the name invites that misreading, the fix documents the meaning at the return site, and a full rename to something like remainingAfterMinor would be the stronger long-term change.

Proven by the judge: The field refundableMinor actually holds the balance left after the refund, which contradicts its name and the refund-eligibility meaning. This is a real, if minor, misleading-API problem in the code.

The fix, in src/payments/refunds.ts:

```diff
+  // refundableMinor here is the balance left after this refund, not the amount refundable before it.
   return { ok: true, refundableMinor: refundableMinor - request.amountMinor };
```

## Dropped

- concurrency.check_then_act: The edit adds a module-level inFlight Set keyed by captureId, checked and then added synchronously with no await between them. In JavaScript's single-threaded event loop, the has-check and add cannot interleave with another caller, so two concurrent refund requests cannot both pass the check. The claimed check-then-act race is not possible for this synchronous code. Moreover, the edit never removes captureIds from inFlight, so it would wrongly reject all later refunds on that capture, which is a different defect (a permanent lockout), not the over-refund described. The original code also has no such race; it is purely synchronous and has no shared mutable reservation at all, so it lacks the claimed over-refund consequence only in the sense that the consequence was not reachable via this path either. The edit does not introduce the claimed over-refund problem.
  Consequence claimed: Two concurrent refund requests for the same capture can both pass the in-flight check before either records its reservation, so both are approved against the same remaining balance and the capture is over-refunded.

## The clean code

### src/payments/refunds.ts

```text
 1 | export type Currency = "USD" | "EUR" | "GBP";
 2 | 
 3 | export interface Capture {
 4 |   id: string;
 5 |   currency: Currency;
 6 |   amountMinor: number;
 7 |   refundedMinor: number;
 8 |   capturedAt: Date;
 9 | }
10 | 
11 | export interface RefundRequest {
12 |   captureId: string;
13 |   amountMinor: number;
14 |   currency: Currency;
15 | }
16 | 
17 | export type RefundDecision =
18 |   | { ok: true; refundableMinor: number }
19 |   | { ok: false; reason: "unknown_capture" | "currency_mismatch" | "exceeds_balance" | "non_positive_amount" | "window_closed" };
20 | 
21 | // Card networks generally refuse refunds on captures older than this window;
22 | // older captures must be handled through a manual dispute instead.
23 | const REFUND_WINDOW_MS = 180 * 24 * 60 * 60 * 1000;
24 | 
25 | export function remainingRefundable(capture: Capture): number {
26 |   return Math.max(0, capture.amountMinor - capture.refundedMinor);
27 | }
28 | 
29 | export function evaluateRefund(
30 |   request: RefundRequest,
31 |   captures: ReadonlyMap<string, Capture>,
32 |   now: Date,
33 | ): RefundDecision {
34 |   if (!Number.isSafeInteger(request.amountMinor) || request.amountMinor <= 0) {
35 |     return { ok: false, reason: "non_positive_amount" };
36 |   }
37 | 
38 |   const capture = captures.get(request.captureId);
39 |   if (!capture) {
40 |     return { ok: false, reason: "unknown_capture" };
41 |   }
42 | 
43 |   if (capture.currency !== request.currency) {
44 |     return { ok: false, reason: "currency_mismatch" };
45 |   }
46 | 
47 |   if (now.getTime() - capture.capturedAt.getTime() > REFUND_WINDOW_MS) {
48 |     return { ok: false, reason: "window_closed" };
49 |   }
50 | 
51 |   const refundableMinor = remainingRefundable(capture);
52 |   if (request.amountMinor > refundableMinor) {
53 |     return { ok: false, reason: "exceeds_balance" };
54 |   }
55 | 
56 |   return { ok: true, refundableMinor: refundableMinor - request.amountMinor };
57 | }
58 | 
59 | // Splits a refund across captures in the order given, drawing each one down
60 | // before moving to the next. Returns the allocations, or null if the total
61 | // cannot be covered.
62 | export function allocateRefund(
63 |   amountMinor: number,
64 |   captures: readonly Capture[],
65 | ): Array<{ captureId: string; amountMinor: number }> | null {
66 |   if (!Number.isSafeInteger(amountMinor) || amountMinor <= 0) {
67 |     return null;
68 |   }
69 | 
70 |   let remaining = amountMinor;
71 |   const allocations: Array<{ captureId: string; amountMinor: number }> = [];
72 | 
73 |   const currency = captures.length > 0 ? captures[0].currency : undefined;
74 |   for (const capture of captures) {
75 |     if (capture.currency !== currency) return null;
76 |     if (remaining === 0) break;
77 |     const take = Math.min(remaining, remainingRefundable(capture));
78 |     if (take > 0) {
79 |       allocations.push({ captureId: capture.id, amountMinor: take });
80 |       remaining -= take;
81 |     }
82 |   }
83 | 
84 |   return remaining === 0 ? allocations : null;
85 | }
```

### test/payments/refunds.test.ts

```text
 1 | import { describe, expect, it } from "vitest";
 2 | import { allocateRefund, Capture, evaluateRefund } from "../../src/payments/refunds";
 3 | 
 4 | const now = new Date("2024-06-01T00:00:00Z");
 5 | 
 6 | function capture(overrides: Partial<Capture> = {}): Capture {
 7 |   return {
 8 |     id: "cap_1",
 9 |     currency: "USD",
10 |     amountMinor: 1000,
11 |     refundedMinor: 0,
12 |     capturedAt: new Date("2024-05-01T00:00:00Z"),
13 |     ...overrides,
14 |   };
15 | }
16 | 
17 | describe("evaluateRefund", () => {
18 |   it("accepts a partial refund and reports what remains", () => {
19 |     const captures = new Map([["cap_1", capture()]]);
20 |     const result = evaluateRefund({ captureId: "cap_1", amountMinor: 400, currency: "USD" }, captures, now);
21 |     expect(result).toEqual({ ok: true, refundableMinor: 600 });
22 |   });
23 | 
24 |   it("rejects amounts above the remaining balance", () => {
25 |     const captures = new Map([["cap_1", capture({ refundedMinor: 700 })]]);
26 |     const result = evaluateRefund({ captureId: "cap_1", amountMinor: 400, currency: "USD" }, captures, now);
27 |     expect(result).toEqual({ ok: false, reason: "exceeds_balance" });
28 |   });
29 | 
30 |   it("rejects refunds on captures outside the window", () => {
31 |     const captures = new Map([["cap_1", capture({ capturedAt: new Date("2023-01-01T00:00:00Z") })]]);
32 |     const result = evaluateRefund({ captureId: "cap_1", amountMinor: 100, currency: "USD" }, captures, now);
33 |     expect(result).toEqual({ ok: false, reason: "window_closed" });
34 |   });
35 | });
36 | 
37 | describe("allocateRefund", () => {
38 |   it("spreads a refund across captures in order", () => {
39 |     const captures = [
40 |       capture({ id: "a", amountMinor: 300 }),
41 |       capture({ id: "b", amountMinor: 500 }),
42 |     ];
43 |     expect(allocateRefund(600, captures)).toEqual([
44 |       { captureId: "a", amountMinor: 300 },
45 |       { captureId: "b", amountMinor: 300 },
46 |     ]);
47 |   });
48 | 
49 |   it("returns null when the captures cannot cover the amount", () => {
50 |     expect(allocateRefund(900, [capture({ amountMinor: 300 })])).toBeNull();
51 |   });
52 | });
```

## The clean review

- Major: evaluateRefund returns refundableMinor as the balance remaining after this refund, but the test and naming suggest it; however the real defect: the window check uses now minus capturedAt without considering that a capture dated in the future yields a negative difference, which is acceptable. The actual logic error is that the window check is performed before currency/balance validation order is irrelevant; the substantive issue is that a refund request for a capture with refundedMinor already exceeding amountMinor is fine. Focusing on confirmed bug: allocateRefund ignores currency, so captures in mixed currencies can be combined into one refund without any check.

## The blind review

1. Critical in src/payments/refunds.ts: captures.get returns undefined for an unknown captureId, so capture.currency throws a TypeError instead of returning the declared reason "unknown_capture". The RefundDecision type has that reason but it is never produced. Add a missing-capture check that returns { ok: false, reason: "unknown_capture" }.
   Quoted:   const capture = captures.get(request.captureId);   if (capture.currency !== request.currency) {
   Judged: Matches { issues: [0] }. Correctly identifies the missing-capture null dereference and the unreachable unknown_capture reason.
2. Major in src/payments/refunds.ts: The try/catch is meaningless: the arithmetic on Date values does not throw in practice, and the empty catch silently swallows any error and lets the refund proceed. Remove the try/catch and let real errors surface; if a fail-closed path is wanted, return window_closed explicitly rather than ignoring the error.
   Quoted:   try {     if (now.getTime() - capture.capturedAt.getTime() > REFUND_WINDOW_MS) {       return { ok: false, reason: "window_closed" };     }   } catch {   }
   Judged: Matches { issues: [1] }. Correctly identifies the empty catch silently swallowing errors and letting refunds proceed.
3. Major in src/payments/refunds.ts: Mixed-currency captures are rejected with null, which is the same signal as insufficient balance, so callers cannot tell the two failures apart. Also, the window check is absent here, so allocateRefund can refund captures outside the 180-day window that evaluateRefund would reject. Return a distinct error or enforce the same rules in both paths.
   Quoted:   const currency = captures.length > 0 ? captures[0].currency : undefined;   for (const capture of captures) {     if (capture.currency !== currency) return null;
   Judged: ValidExtra. Points at allocateRefund's currency-mismatch null return, which conflates failures, and its missing window check. These are real defects in the code but are not the planted issues.
4. Major in src/payments/refunds.ts: allocateRefund never checks capture.capturedAt against REFUND_WINDOW_MS or the time passed in, so the window rule is bypassed on the multi-capture path. This is a business-rule defect that should be fixed by passing now and applying the same window check.
   Quoted:     const take = Math.min(remaining, remainingRefundable(capture));
   Judged: ValidExtra. Real defect: allocateRefund never applies the 180-day window check, so the business rule is bypassed on the multi-capture path.
5. Minor in src/payments/refunds.ts: The field is named refundableMinor, but it returns the balance left after this refund. The name suggests the amount still refundable before this request. Rename it (for example remainingAfterMinor) or document it, since callers can easily misread it.
   Quoted:   return { ok: true, refundableMinor: refundableMinor - request.amountMinor };
   Judged: ValidExtra. The field refundableMinor actually holds the balance left after the refund, which contradicts its name and the refund-eligibility meaning. This is a real, if minor, misleading-API problem in the code.
6. Major in test/payments/refunds.test.ts: The test covers only the window on evaluateRefund. There is no test for an unknown captureId (which currently crashes), for currency mismatch, for non-positive or non-integer amounts, or for the window on allocateRefund. Those gaps leave the defects above undetected.
   Quoted:   it("rejects refunds on captures outside the window", () => {
   Judged: FalsePositive. A remark about test coverage gaps, not a defect in the code under review.
7. Minor in test/payments/refunds.test.ts: This only checks that null is returned and does not verify that the reason is distinct from currency mismatch or invalid input. It also has no test for empty captures or for captures spanning two currencies, so the currency branch is untested.
   Quoted:   it("returns null when the captures cannot cover the amount", () => {     expect(allocateRefund(900, [capture({ amountMinor: 300 })])).toBeNull();
   Judged: FalsePositive. A remark about test assertions and missing test cases, not a defect in the code itself.

- planted-1 (correctness.null_dereference): the review Found { finding: 0 }. Finding 1 names the cause: captures.get returns undefined for an unknown captureId, and capture.currency then throws a TypeError instead of returning unknown_capture.
- planted-2 (errors.swallowed_exception): the review Found { finding: 1 }. Finding 2 names the cause: the empty catch silently discards errors and lets the refund proceed as if the capture were inside the window.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- judge: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
