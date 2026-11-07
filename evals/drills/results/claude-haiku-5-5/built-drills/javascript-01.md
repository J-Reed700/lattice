# Refund eligibility and ledger for payment refunds (javascript)

- Domain: payments and refunds
- Built by: claude-haiku-5-5
- Runner: builtin-javascript
- Built in 77 seconds
- Patterns asked for: correctness.null_dereference, errors.swallowed_exception, concurrency.check_then_act

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 0
- Dropped: 1
- Found in review: 3

## The code

### src/refunds.mjs

```text
 1 | // Refund handling for captured card payments.
 2 | // Amounts are integer minor units (cents) to avoid floating point drift.
 3 | // The gateway and clock are injected so the policy can be exercised without I/O.
 4 | 
 5 | const REFUND_WINDOW_MS = 180 * 24 * 60 * 60 * 1000;
 6 | 
 7 | export class RefundError extends Error {
 8 |   constructor(code, message) {
 9 |     super(message);
10 |     this.name = 'RefundError';
11 |     this.code = code;
12 |   }
13 | }
14 | 
15 | export function refundableAmount(payment, refunds) {
16 |   const alreadyRefunded = refunds
17 |     .filter((r) => r.status !== 'failed')
18 |     .reduce((sum, r) => sum + r.amount, 0);
19 |   return payment.amount - alreadyRefunded;
20 | }
21 | 
22 | export async function requestRefund({ payment, refunds, amount, reason, gateway, now }) {
23 |   if (!Number.isInteger(amount) || amount <= 0) {
24 |     throw new RefundError('invalid_amount', 'refund amount must be a positive integer');
25 |   }
26 |   if (payment.status !== 'captured') {
27 |     throw new RefundError('not_captured', `payment is ${payment.status}`);
28 |   }
29 |   if (now - payment.capturedAt > REFUND_WINDOW_MS) {
30 |     throw new RefundError('window_closed', 'refund window has passed');
31 |   }
32 |   const available = refundableAmount(payment, refunds);
33 |   if (amount > available) {
34 |     throw new RefundError('over_refund', `only ${available} remaining`);
35 |   }
36 | 
37 |   // Idempotency key ties retries of the same refund to one gateway call.
38 |   const idempotencyKey = `refund:${payment.id}:${refunds.length}:${amount}`;
39 |   let response;
40 |   try {
41 |     response = await gateway.refund({ paymentId: payment.id, amount, idempotencyKey });
42 |   } catch (err) {
43 |     // Transport failures are not known to be declined; surface them as retryable.
44 |     response = { id: null, status: 'processing' };
45 |   }
46 | 
47 |   const settled = response.status === 'succeeded' ? response : null;
48 |   const record = {
49 |     id: settled.id,
50 |     paymentId: payment.id,
51 |     amount,
52 |     reason,
53 |     status: response.status === 'succeeded' ? 'succeeded' : response.status === 'failed' ? 'failed' : 'pending',
54 |     createdAt: now,
55 |   };
56 |   refunds.push(record);
57 |   return record;
58 | }
59 | 
60 | export function ledgerTotals(payment, refunds) {
61 |   const refunded = refunds
62 |     .filter((r) => r.status === 'succeeded')
63 |     .reduce((sum, r) => sum + r.amount, 0);
64 |   return {
65 |     captured: payment.amount,
66 |     refunded,
67 |     net: payment.amount - refunded,
68 |   };
69 | }
```

### test/refunds.test.mjs

```text
 1 | import { requestRefund, refundableAmount, ledgerTotals, RefundError } from '../src/refunds.mjs';
 2 | 
 3 | const DAY = 24 * 60 * 60 * 1000;
 4 | const payment = { id: 'pay_1', status: 'captured', amount: 5000, capturedAt: 0 };
 5 | 
 6 | function fakeGateway(status = 'succeeded') {
 7 |   const calls = [];
 8 |   return {
 9 |     calls,
10 |     async refund(req) {
11 |       calls.push(req);
12 |       return { id: `re_${calls.length}`, status };
13 |     },
14 |   };
15 | }
16 | 
17 | async function expectError(promise, code) {
18 |   try {
19 |     await promise;
20 |   } catch (err) {
21 |     if (!(err instanceof RefundError) || err.code !== code) {
22 |       throw new Error(`expected ${code}, got ${err.code || err.message}`);
23 |     }
24 |     return;
25 |   }
26 |   throw new Error(`expected ${code}, but call succeeded`);
27 | }
28 | 
29 | export async function runTests() {
30 |   const refunds = [];
31 |   const gateway = fakeGateway();
32 |   const record = await requestRefund({ payment, refunds, amount: 2000, reason: 'damaged', gateway, now: DAY });
33 |   if (record.status !== 'succeeded') throw new Error('expected succeeded');
34 |   if (refundableAmount(payment, refunds) !== 3000) throw new Error('remaining amount wrong');
35 | 
36 |   await expectError(
37 |     requestRefund({ payment, refunds, amount: 3001, reason: 'x', gateway, now: DAY }),
38 |     'over_refund',
39 |   );
40 |   await expectError(
41 |     requestRefund({ payment, refunds, amount: 1.5, reason: 'x', gateway, now: DAY }),
42 |     'invalid_amount',
43 |   );
44 |   await expectError(
45 |     requestRefund({ payment, refunds, amount: 100, reason: 'x', gateway, now: 200 * DAY }),
46 |     'window_closed',
47 |   );
48 |   await expectError(
49 |     requestRefund({ payment: { ...payment, status: 'authorized' }, refunds, amount: 100, reason: 'x', gateway, now: DAY }),
50 |     'not_captured',
51 |   );
52 | 
53 |   const failing = { async refund() { throw new Error('socket reset'); } };
54 |   await expectError(
55 |     requestRefund({ payment, refunds: [], amount: 100, reason: 'x', gateway: failing, now: DAY }),
56 |     'gateway_unavailable',
57 |   );
58 | 
59 |   const totals = ledgerTotals(payment, refunds);
60 |   if (totals.refunded !== 2000 || totals.net !== 3000) throw new Error('ledger totals wrong');
61 | }
```

## The key

### 1. Missing null check (correctness.null_dereference), Major, planted, src/refunds.mjs lines 47 to 49

- Consequence: Any gateway response that is not 'succeeded' (for example 'processing' or 'failed') leaves settled null, so requestRefund throws a raw TypeError instead of recording a pending or failed refund. Callers see an unexpected crash on every non-immediate refund, and the refund may already be executed at the gateway.
- In an interview: The new variable is null for every response except a succeeded one, and the record is then built by reading a property of it without a check. The fix is to read the id from response directly, which is always present, and to keep the status mapping separate.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { requestRefund } from './src/refunds.mjs';
const DAY = 24 * 60 * 60 * 1000;
const payment = { id: 'pay_k', status: 'captured', amount: 1000, capturedAt: 0 };
const gw = { async refund() { return { id: 're_k', status: 'processing' }; } };
const rec = await requestRefund({ payment, refunds: [], amount: 100, reason: 'x', gateway: gw, now: DAY });
if (rec.status !== 'pending') throw new Error('expected pending, got ' + rec.status);
console.log('checks passed');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at requestRefund (src/refunds.mjs:49:9)"

The fix, in src/refunds.mjs:

```diff
-  const settled = response.status === 'succeeded' ? response : null;
   const record = {
-    id: settled.id,
+    id: response.id,
```

### 2. Swallowed exception (errors.swallowed_exception), Major, planted, src/refunds.mjs lines 44 to 44

- Consequence: When the gateway call fails with a transport error, the refund is recorded as pending with a null id and reported to the caller as a normal result. The customer is never told the refund failed, no retry is triggered, and the ledger holds a phantom pending refund that reserves amount.
- In an interview: The catch block now swallows the gateway error and substitutes a fabricated response, so the failure goes unnoticed. The fix is to keep rethrowing a retryable RefundError so the caller knows the refund was not attempted.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { requestRefund, RefundError } from './src/refunds.mjs';
const DAY = 24 * 60 * 60 * 1000;
const payment = { id: 'pay_k', status: 'captured', amount: 1000, capturedAt: 0 };
const failing = { async refund() { throw new Error('socket reset'); } };
let threw = false;
try {
  await requestRefund({ payment, refunds: [], amount: 100, reason: 'x', gateway: failing, now: DAY });
} catch (err) {
  threw = err instanceof RefundError && err.code === 'gateway_unavailable';
}
if (!threw) throw new Error('expected gateway_unavailable');
console.log('checks passed');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:11:23)"

The fix, in src/refunds.mjs:

```diff
-    response = { id: null, status: 'processing' };
+    throw new RefundError('gateway_unavailable', String(err && err.message));
```

### 3. Retry of a non-idempotent operation (errors.retry_not_idempotent), Critical, found in review, src/refunds.mjs lines 38 to 38

- Consequence: The key changes whenever refunds.length changes. After a transport failure or a pending response, the record is still pushed, so a retry computes a different key and the gateway can issue a second refund for the same request, returning money twice.
- In an interview: The idempotency key is built from refunds.length, which grows when a pending record is stored, so a retry of the same refund sends a different key and the gateway can refund twice. Deriving the key from a stable, client-generated request id that the caller reuses on every retry makes the gateway treat all attempts as one operation.

Proven by the judge: The idempotency key depends on refunds.length, which changes between retries, so a retry may double refund. This is a real defect not planted in the key lines, so it is a valid extra concern.

The fix, in src/refunds.mjs:

```diff
-  const idempotencyKey = `refund:${payment.id}:${refunds.length}:${amount}`;
+  const idempotencyKey = `refund:${payment.id}:${requestId}`;
```

### 4. Check-then-act race (concurrency.check_then_act), Major, found in review, src/refunds.mjs lines 38 to 56

- Consequence: Two requests for the same payment can both call refundableAmount before either pushes its record, so both pass the over_refund check and the gateway refunds more than the captured amount. This happens whenever refund requests for one payment overlap in time.
- In an interview: The availability check and the push are separated by an await, so concurrent callers can both see the same remaining amount and both succeed. Reserving the amount by pushing a pending record before the await closes that window, and refundableAmount then counts it for every later caller.

Proven by the judge: Claims concurrent requests can over-refund because the amount is not reserved before awaiting. This is a real race in the code since refunds are pushed only after the await, so it is a valid extra.

The fix, in src/refunds.mjs:

```diff
   const idempotencyKey = `refund:${payment.id}:${refunds.length}:${amount}`;
+  const record = {
+    id: null,
+    paymentId: payment.id,
+    amount,
+    reason,
+    status: 'pending',
+    createdAt: now,
+  };
+  refunds.push(record);
   let response;
   try {
     response = await gateway.refund({ paymentId: payment.id, amount, idempotencyKey });
   } catch (err) {
     // Transport failures are not known to be declined; surface them as retryable.
     response = { id: null, status: 'processing' };
   }
 
-  const settled = response.status === 'succeeded' ? response : null;
-  const record = {
-    id: settled.id,
-    paymentId: payment.id,
-    amount,
-    reason,
-    status: response.status === 'succeeded' ? 'succeeded' : response.status === 'failed' ? 'failed' : 'pending',
-    createdAt: now,
-  };
-  refunds.push(record);
+  record.id = response.id;
+  record.status = response.status === 'succeeded' ? 'succeeded' : response.status === 'failed' ? 'failed' : 'pending';
+  return record;
```

### 5. Missing null check (correctness.null_dereference), Major, found in review, src/refunds.mjs lines 49 to 49

- Consequence: When the gateway call fails with a transport error, the catch sets response to a processing status, so settled is null and reading settled.id throws a TypeError. Any refund that is not immediately succeeded (processing or failed) crashes instead of returning a pending record, and the gateway_unavailable test fails with a TypeError rather than a RefundError.
- In an interview: The code dereferences settled, which is null whenever the gateway response is not succeeded, so the record construction throws a TypeError for pending and failed refunds. Reading the id from response, which always exists, removes the null dereference and lets the pending record be returned as the comment expects.

Proven by the judge: The test expects gateway_unavailable which the source no longer throws; this is a real test mismatch caused by the planted removal of the throw, but it does not name the swallowed exception as the cause of issue 2, and it is about the test expectation rather than the swallowing.

The fix, in src/refunds.mjs:

```diff
-    id: settled.id,
+    id: response.id,
```

## Dropped

- concurrency.check_then_act: The edit wraps the synchronous refundableAmount call in await Promise.resolve(...), adding a microtask yield between reading the remaining amount and the over-refund check. This does create a window where another concurrent requestRefund could run, but the original code already had the same check-then-act gap: the check reads refunds, then awaits gateway.refund before pushing the record, so two concurrent callers could both pass the check in the original too. The claimed consequence (double over-refund) therefore already existed before the edit; the edit does not introduce this pattern anew, it merely widens the timing slightly without changing the nature of the defect.
  Consequence claimed: Two refund requests for the same payment issued concurrently can both read the same remaining amount before either records its refund, so together they refund more than was captured. The over-refund is only visible later in reconciliation.

## The clean code

### src/refunds.mjs

```text
 1 | // Refund handling for captured card payments.
 2 | // Amounts are integer minor units (cents) to avoid floating point drift.
 3 | // The gateway and clock are injected so the policy can be exercised without I/O.
 4 | 
 5 | const REFUND_WINDOW_MS = 180 * 24 * 60 * 60 * 1000;
 6 | 
 7 | export class RefundError extends Error {
 8 |   constructor(code, message) {
 9 |     super(message);
10 |     this.name = 'RefundError';
11 |     this.code = code;
12 |   }
13 | }
14 | 
15 | export function refundableAmount(payment, refunds) {
16 |   const alreadyRefunded = refunds
17 |     .filter((r) => r.status !== 'failed')
18 |     .reduce((sum, r) => sum + r.amount, 0);
19 |   return payment.amount - alreadyRefunded;
20 | }
21 | 
22 | export async function requestRefund({ payment, refunds, amount, reason, gateway, now }) {
23 |   if (!Number.isInteger(amount) || amount <= 0) {
24 |     throw new RefundError('invalid_amount', 'refund amount must be a positive integer');
25 |   }
26 |   if (payment.status !== 'captured') {
27 |     throw new RefundError('not_captured', `payment is ${payment.status}`);
28 |   }
29 |   if (now - payment.capturedAt > REFUND_WINDOW_MS) {
30 |     throw new RefundError('window_closed', 'refund window has passed');
31 |   }
32 |   const available = refundableAmount(payment, refunds);
33 |   if (amount > available) {
34 |     throw new RefundError('over_refund', `only ${available} remaining`);
35 |   }
36 | 
37 |   // Idempotency key ties retries of the same refund to one gateway call.
38 |   const idempotencyKey = `refund:${payment.id}:${refunds.length}:${amount}`;
39 |   let response;
40 |   try {
41 |     response = await gateway.refund({ paymentId: payment.id, amount, idempotencyKey });
42 |   } catch (err) {
43 |     // Transport failures are not known to be declined; surface them as retryable.
44 |     throw new RefundError('gateway_unavailable', String(err && err.message));
45 |   }
46 | 
47 |   const record = {
48 |     id: response.id,
49 |     paymentId: payment.id,
50 |     amount,
51 |     reason,
52 |     status: response.status === 'succeeded' ? 'succeeded' : response.status === 'failed' ? 'failed' : 'pending',
53 |     createdAt: now,
54 |   };
55 |   refunds.push(record);
56 |   return record;
57 | }
58 | 
59 | export function ledgerTotals(payment, refunds) {
60 |   const refunded = refunds
61 |     .filter((r) => r.status === 'succeeded')
62 |     .reduce((sum, r) => sum + r.amount, 0);
63 |   return {
64 |     captured: payment.amount,
65 |     refunded,
66 |     net: payment.amount - refunded,
67 |   };
68 | }
```

### test/refunds.test.mjs

```text
 1 | import { requestRefund, refundableAmount, ledgerTotals, RefundError } from '../src/refunds.mjs';
 2 | 
 3 | const DAY = 24 * 60 * 60 * 1000;
 4 | const payment = { id: 'pay_1', status: 'captured', amount: 5000, capturedAt: 0 };
 5 | 
 6 | function fakeGateway(status = 'succeeded') {
 7 |   const calls = [];
 8 |   return {
 9 |     calls,
10 |     async refund(req) {
11 |       calls.push(req);
12 |       return { id: `re_${calls.length}`, status };
13 |     },
14 |   };
15 | }
16 | 
17 | async function expectError(promise, code) {
18 |   try {
19 |     await promise;
20 |   } catch (err) {
21 |     if (!(err instanceof RefundError) || err.code !== code) {
22 |       throw new Error(`expected ${code}, got ${err.code || err.message}`);
23 |     }
24 |     return;
25 |   }
26 |   throw new Error(`expected ${code}, but call succeeded`);
27 | }
28 | 
29 | export async function runTests() {
30 |   const refunds = [];
31 |   const gateway = fakeGateway();
32 |   const record = await requestRefund({ payment, refunds, amount: 2000, reason: 'damaged', gateway, now: DAY });
33 |   if (record.status !== 'succeeded') throw new Error('expected succeeded');
34 |   if (refundableAmount(payment, refunds) !== 3000) throw new Error('remaining amount wrong');
35 | 
36 |   await expectError(
37 |     requestRefund({ payment, refunds, amount: 3001, reason: 'x', gateway, now: DAY }),
38 |     'over_refund',
39 |   );
40 |   await expectError(
41 |     requestRefund({ payment, refunds, amount: 1.5, reason: 'x', gateway, now: DAY }),
42 |     'invalid_amount',
43 |   );
44 |   await expectError(
45 |     requestRefund({ payment, refunds, amount: 100, reason: 'x', gateway, now: 200 * DAY }),
46 |     'window_closed',
47 |   );
48 |   await expectError(
49 |     requestRefund({ payment: { ...payment, status: 'authorized' }, refunds, amount: 100, reason: 'x', gateway, now: DAY }),
50 |     'not_captured',
51 |   );
52 | 
53 |   const failing = { async refund() { throw new Error('socket reset'); } };
54 |   await expectError(
55 |     requestRefund({ payment, refunds: [], amount: 100, reason: 'x', gateway: failing, now: DAY }),
56 |     'gateway_unavailable',
57 |   );
58 | 
59 |   const totals = ledgerTotals(payment, refunds);
60 |   if (totals.refunded !== 2000 || totals.net !== 3000) throw new Error('ledger totals wrong');
61 | }
```

### The hidden suite

```text
import { requestRefund, refundableAmount, ledgerTotals, RefundError } from './src/refunds.mjs';

function assert(cond, msg) {
  if (!cond) throw new Error('assertion failed: ' + msg);
}

async function expectCode(promise, code) {
  try {
    await promise;
  } catch (err) {
    assert(err instanceof RefundError, 'error is RefundError');
    assert(err.code === code, `code ${code}, got ${err.code}`);
    return;
  }
  throw new Error('expected ' + code);
}

const DAY = 24 * 60 * 60 * 1000;
const payment = { id: 'pay_9', status: 'captured', amount: 1000, capturedAt: 0 };

const gateway = {
  calls: [],
  async refund(req) {
    this.calls.push(req);
    return { id: 're_' + this.calls.length, status: 'succeeded' };
  },
};

const refunds = [];
const first = await requestRefund({ payment, refunds, amount: 400, reason: 'a', gateway, now: DAY });
assert(first.status === 'succeeded', 'first refund succeeded');
assert(gateway.calls.length === 1, 'one gateway call');
assert(gateway.calls[0].paymentId === 'pay_9', 'payment id forwarded');
assert(refundableAmount(payment, refunds) === 600, 'remaining 600');

const pendingGateway = { async refund() { return { id: 're_p', status: 'processing' }; } };
const pending = await requestRefund({ payment, refunds, amount: 100, reason: 'b', gateway: pendingGateway, now: DAY });
assert(pending.status === 'pending', 'processing maps to pending');
assert(refundableAmount(payment, refunds) === 500, 'pending still reserves amount');

refunds.push({ id: 'x', amount: 300, status: 'failed' });
assert(refundableAmount(payment, refunds) === 500, 'failed refunds do not count');

await expectCode(requestRefund({ payment, refunds, amount: 0, reason: 'c', gateway, now: DAY }), 'invalid_amount');
await expectCode(requestRefund({ payment, refunds, amount: -5, reason: 'c', gateway, now: DAY }), 'invalid_amount');
await expectCode(requestRefund({ payment, refunds, amount: 501, reason: 'c', gateway, now: DAY }), 'over_refund');
await expectCode(requestRefund({ payment, refunds, amount: 10, reason: 'c', gateway, now: 181 * DAY }), 'window_closed');

const totals = ledgerTotals(payment, refunds);
assert(totals.captured === 1000, 'captured total');
assert(totals.refunded === 400, 'only succeeded refunds in ledger');
assert(totals.net === 600, 'net total');

console.log('checks passed');
```

## The clean review

- Major: A refund the gateway reports as failed is recorded with status 'pending'. Because refundableAmount only excludes 'failed' records, the declined amount stays counted against the payment and blocks later refunds up to that amount.

## The blind review

1. Critical in src/refunds.mjs: When the gateway call throws, response is { id: null, status: 'processing' }, so settled is null and settled.id throws a TypeError. The transport-failure path therefore crashes instead of recording a pending refund, and the caller never learns the outcome. Use response.id directly, or drop the settled variable entirely.
   Quoted:   const settled = response.status === 'succeeded' ? response : null;   const record = {     id: settled.id,
   Judged: Matches { issues: [0] }. Correctly identifies that settled is null for non-succeeded responses and settled.id throws a TypeError, matching issue 1.
2. Major in src/refunds.mjs: The error is discarded and the record gets id null, so the pending refund cannot be reconciled or retried against the gateway. Keep the error (log it or attach it to the record) and make the retry path use the same idempotency key so a retry cannot double refund.
   Quoted:   } catch (err) {     // Transport failures are not known to be declined; surface them as retryable.     response = { id: null, status: 'processing' };
   Judged: Matches { issues: [1] }. Identifies the discarded error in the catch block and the null id consequence, matching issue 2.
3. Major in src/refunds.mjs: The key depends on refunds.length, which changes whenever any refund is recorded. A retry after a failed attempt that did push a record gets a different key, so the gateway may refund twice. The key should be derived from a stable client-generated refund request id.
   Quoted:   const idempotencyKey = `refund:${payment.id}:${refunds.length}:${amount}`;
   Judged: ValidExtra. The idempotency key depends on refunds.length, which changes between retries, so a retry may double refund. This is a real defect not planted in the key lines, so it is a valid extra concern.
4. Minor in src/refunds.mjs: Only the upper bound is checked. A now earlier than capturedAt yields a negative difference and passes. Also the window check runs after the amount check only in order; consider validating inputs (now, capturedAt numeric) explicitly.
   Quoted:   if (now - payment.capturedAt > REFUND_WINDOW_MS) {
   Judged: FalsePositive. The window check ignores a now earlier than capturedAt, but that is a minor robustness remark rather than a real planted defect; it is a preference-level concern with weak impact.
5. Major in src/refunds.mjs: The check ignores 'pending' refunds in-flight only if they are excluded; refundableAmount excludes only failed, so pending ones count, which is fine, but concurrent requests both read the same refunds array before either pushes, allowing over-refund races. Reserve the amount synchronously before awaiting the gateway.
   Quoted:   const available = refundableAmount(payment, refunds);   if (amount > available) {
   Judged: ValidExtra. Claims concurrent requests can over-refund because the amount is not reserved before awaiting. This is a real race in the code since refunds are pushed only after the await, so it is a valid extra.
6. Nit in src/refunds.mjs: Nested ternary is hard to read and silently maps any unknown gateway status to 'pending'. Use a explicit mapping with a switch so unexpected statuses are visible.
   Quoted:     status: response.status === 'succeeded' ? 'succeeded' : response.status === 'failed' ? 'failed' : 'pending',
   Judged: FalsePositive. Nested ternary readability and mapping unknown statuses to pending is a style preference, not a real defect beyond opinion.
7. Critical in test/refunds.test.mjs: The test expects a 'gateway_unavailable' error code, but the source never throws that; it returns a pending record. Because of the TypeError bug the test would fail with a non-RefundError, and the intended behaviour is not specified. The test should assert the pending record instead and verify it does not throw.
   Quoted:   const failing = { async refund() { throw new Error('socket reset'); } };   await expectError(     requestRefund({ payment, refunds: [], amount: 100, reason: 'x', gateway: failing, now: DAY }),     'gateway_unavailable',   );
   Judged: ValidExtra. The test expects gateway_unavailable which the source no longer throws; this is a real test mismatch caused by the planted removal of the throw, but it does not name the swallowed exception as the cause of issue 2, and it is about the test expectation rather than the swallowing.
8. Minor in test/refunds.test.mjs: Only the happy path is covered. Nothing verifies the idempotency key, that failed refunds are excluded from totals, the pending state, or that the refund window boundary is correct. The test also never checks gateway.calls, so the 'one gateway call' property is unverified.
   Quoted:   const totals = ledgerTotals(payment, refunds);   if (totals.refunded !== 2000 || totals.net !== 3000) throw new Error('ledger totals wrong');
   Judged: FalsePositive. Notes missing test coverage for idempotency, failed exclusion, and gateway calls. This is a test-coverage remark, not a defect in the code.

- planted-1 (correctness.null_dereference): the review Found { finding: 0 }. Finding 1 names the null-dereference: when the gateway call throws, response is a processing stub, settled is null, and settled.id throws a TypeError; it says the transport path crashes and suggests using response.id. This states the cause that the value is null and dereferenced, matching the issue.
- planted-2 (errors.swallowed_exception): the review Found { finding: 1 }. Finding 2 says the error is discarded and the record gets id null, so the pending refund cannot be reconciled or retried. It names the swallowed error as the problem and why it matters.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
