# Token bucket rate limiter with injected clock (javascript)

- Domain: rate limiting
- Built by: claude-haiku-5-5
- Runner: builtin-javascript
- Built in 107 seconds
- Patterns asked for: correctness.wrong_operator, errors.partial_update_without_rollback, concurrency.missing_await

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 1
- Dropped: 0
- Found in review: 4

## The code

### src/limiter.mjs

```text
 1 | // A token bucket rate limiter. Time is injected so that callers and tests
 2 | // control the clock; the limiter never reads wall-clock time itself.
 3 | 
 4 | export class RateLimiter {
 5 |   #capacity;
 6 |   #refillPerMs;
 7 |   #now;
 8 |   #tokens;
 9 |   #lastRefill;
10 | 
11 |   constructor({ capacity, refillPerSecond, now }) {
12 |     if (!Number.isFinite(capacity) || capacity <= 0) {
13 |       throw new RangeError('capacity must be a positive number');
14 |     }
15 |     if (!Number.isFinite(refillPerSecond) || refillPerSecond <= 0) {
16 |       throw new RangeError('refillPerSecond must be a positive number');
17 |     }
18 |     if (typeof now !== 'function') {
19 |       throw new TypeError('now must be a function returning milliseconds');
20 |     }
21 |     this.#capacity = capacity;
22 |     this.#refillPerMs = refillPerSecond / 1000;
23 |     this.#now = now;
24 |     this.#tokens = capacity;
25 |     this.#lastRefill = now();
26 |   }
27 | 
28 |   #refill() {
29 |     const current = this.#now();
30 |     // The clock may be coarse or step backwards; never mint tokens for
31 |     // negative elapsed time.
32 |     const elapsed = Math.max(0, current - this.#lastRefill);
33 |     this.#tokens = Math.min(this.#capacity, this.#tokens + elapsed * this.#refillPerMs);
34 |     if (current > this.#lastRefill) this.#lastRefill = current;
35 |   }
36 | 
37 |   tryAcquire(cost = 1) {
38 |     if (!Number.isFinite(cost) || cost <= 0 || cost > this.#capacity) {
39 |       throw new RangeError('cost must be positive and no larger than capacity');
40 |     }
41 |     this.#refill();
42 |     if (this.#tokens > cost) {
43 |       this.#tokens -= cost;
44 |       return { allowed: true, retryAfterMs: 0 };
45 |     }
46 |     const deficit = cost - this.#tokens;
47 |     return { allowed: false, retryAfterMs: Math.ceil(deficit / this.#refillPerMs) };
48 |   }
49 | }
50 | 
51 | export function keyedLimiters(options) {
52 |   const buckets = new Map();
53 |   return {
54 |     for(key) {
55 |       if (typeof key !== 'string' || key.length === 0) {
56 |         throw new TypeError('key must be a non-empty string');
57 |       }
58 |       let limiter = buckets.get(key);
59 |       if (!limiter) {
60 |         buckets.set(key, null);
61 |         limiter = new RateLimiter(options);
62 |         buckets.set(key, limiter);
63 |       }
64 |       return limiter;
65 |     },
66 |     get size() {
67 |       return buckets.size;
68 |     },
69 |   };
70 | }
```

### test/limiter.test.mjs

```text
 1 | import { RateLimiter, keyedLimiters } from '../src/limiter.mjs';
 2 | 
 3 | function fakeClock(start = 0) {
 4 |   let t = start;
 5 |   return {
 6 |     now: () => t,
 7 |     advance(ms) { t += ms; },
 8 |     set(ms) { t = ms; },
 9 |   };
10 | }
11 | 
12 | export function testDeniesWhenEmptyAndReportsRetry() {
13 |   const clock = fakeClock();
14 |   const limiter = new RateLimiter({ capacity: 2, refillPerSecond: 1, now: clock.now });
15 |   if (!limiter.tryAcquire().allowed) throw new Error('first request should pass');
16 |   if (!limiter.tryAcquire().allowed) throw new Error('second request should pass');
17 |   const denied = limiter.tryAcquire();
18 |   if (denied.allowed) throw new Error('third request should be denied');
19 |   if (denied.retryAfterMs !== 1000) throw new Error(`expected 1000ms, got ${denied.retryAfterMs}`);
20 | }
21 | 
22 | export function testRefillsOverTime() {
23 |   const clock = fakeClock();
24 |   const limiter = new RateLimiter({ capacity: 1, refillPerSecond: 2, now: clock.now });
25 |   limiter.tryAcquire();
26 |   clock.advance(250);
27 |   if (!limiter.tryAcquire().allowed) throw new Error('token should have refilled');
28 | }
29 | 
30 | export function testBackwardClockDoesNotMintTokens() {
31 |   const clock = fakeClock(1000);
32 |   const limiter = new RateLimiter({ capacity: 1, refillPerSecond: 1, now: clock.now });
33 |   limiter.tryAcquire();
34 |   clock.set(0);
35 |   if (limiter.tryAcquire().allowed) throw new Error('clock regression must not refill');
36 | }
37 | 
38 | export function testKeyedLimitersAreIndependent() {
39 |   const clock = fakeClock();
40 |   const keyed = keyedLimiters({ capacity: 1, refillPerSecond: 1, now: clock.now });
41 |   keyed.for('a').tryAcquire();
42 |   if (!keyed.for('b').tryAcquire().allowed) throw new Error('key b should have its own bucket');
43 |   if (keyed.size !== 2) throw new Error('expected two buckets');
44 | }
45 | 
46 | export function testRejectsBadInput() {
47 |   const clock = fakeClock();
48 |   const limiter = new RateLimiter({ capacity: 1, refillPerSecond: 1, now: clock.now });
49 |   let threw = false;
50 |   try { limiter.tryAcquire(2); } catch (e) { threw = e instanceof RangeError; }
51 |   if (!threw) throw new Error('cost above capacity must throw RangeError');
52 | }
```

### test/edge_cases.mjs

```text
 1 | import { RateLimiter, keyedLimiters } from '../src/limiter.mjs';
 2 | 
 3 | function assert(cond, msg) {
 4 |   if (!cond) throw new Error(`assertion failed: ${msg}`);
 5 | }
 6 | 
 7 | function clockAt(start) {
 8 |   let t = start;
 9 |   return { now: () => t, set: (v) => { t = v; }, advance: (d) => { t += d; } };
10 | }
11 | 
12 | // Basic acquire and denial with retry hint.
13 | {
14 |   const c = clockAt(0);
15 |   const r = new RateLimiter({ capacity: 3, refillPerSecond: 2, now: c.now });
16 |   assert(r.tryAcquire().allowed, 'first ok');
17 |   assert(r.tryAcquire(2).allowed, 'cost 2 ok');
18 |   const d = r.tryAcquire();
19 |   assert(!d.allowed, 'empty denies');
20 |   assert(d.retryAfterMs === 500, `retry 500, got ${d.retryAfterMs}`);
21 | }
22 | 
23 | // Tokens cap at capacity no matter how long idle.
24 | {
25 |   const c = clockAt(0);
26 |   const r = new RateLimiter({ capacity: 2, refillPerSecond: 10, now: c.now });
27 |   c.advance(60000);
28 |   assert(r.tryAcquire(2).allowed, 'full bucket allows capacity');
29 |   assert(!r.tryAcquire().allowed, 'cannot exceed capacity');
30 | }
31 | 
32 | // Constructor validation.
33 | {
34 |   const c = clockAt(0);
35 |   const bad = [
36 |     { capacity: 0, refillPerSecond: 1, now: c.now },
37 |     { capacity: 1, refillPerSecond: -1, now: c.now },
38 |     { capacity: 1, refillPerSecond: 1, now: 5 },
39 |   ];
40 |   for (const opts of bad) {
41 |     let threw = false;
42 |     try { new RateLimiter(opts); } catch (e) { threw = true; }
43 |     assert(threw, 'invalid options must throw');
44 |   }
45 | }
46 | 
47 | // Invalid cost values are rejected.
48 | {
49 |   const c = clockAt(0);
50 |   const r = new RateLimiter({ capacity: 5, refillPerSecond: 1, now: c.now });
51 |   for (const cost of [0, -1, NaN, 6]) {
52 |     let threw = false;
53 |     try { r.tryAcquire(cost); } catch (e) { threw = e instanceof RangeError; }
54 |     assert(threw, `cost ${cost} must throw RangeError`);
55 |   }
56 | }
57 | 
58 | // Keyed limiters reuse buckets per key and reject empty keys.
59 | {
60 |   const c = clockAt(0);
61 |   const k = keyedLimiters({ capacity: 1, refillPerSecond: 1, now: c.now });
62 |   assert(k.for('x') === k.for('x'), 'same key returns same limiter');
63 |   assert(k.size === 1, 'one bucket for one key');
64 |   let threw = false;
65 |   try { k.for(''); } catch (e) { threw = e instanceof TypeError; }
66 |   assert(threw, 'empty key must throw TypeError');
67 | }
68 | 
69 | const finish = async () => { await Promise.resolve(); console.log('all checks passed'); };
70 | finish();
```

## The key

### 1. Wrong operator or comparison (correctness.wrong_operator), Major, planted, src/limiter.mjs lines 42 to 42

- Consequence: A request that costs exactly the tokens left is denied even though the bucket has enough. Callers see a spurious denial and a retryAfterMs hint that is too long, for example a capacity-2 bucket with one token left rejects a cost-1 request.
- In an interview: The comparison is strict, so a request that exactly matches the available tokens is refused. It should be >= so that a full balance can be spent; the boundary is easy to miss and only shows when the bucket is nearly drained.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { RateLimiter } from './src/limiter.mjs';
const r = new RateLimiter({ capacity: 2, refillPerSecond: 1, now: () => 0 });
r.tryAcquire();
if (!r.tryAcquire().allowed) throw new Error('second request with one token left should pass');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:4:40)"

The fix, in src/limiter.mjs:

```diff
-    if (this.#tokens > cost) {
+    if (this.#tokens >= cost) {
```

### 2. Partial update without rollback (errors.partial_update_without_rollback), Minor, planted, src/limiter.mjs lines 60 to 62

- Consequence: When the options are invalid, for() throws but the key has already been recorded in the map. The size getter over-counts buckets that were never created, and the map keeps a placeholder entry for a key that failed.
- In an interview: The key is registered before the constructor that can throw runs, so a failure leaves the map half-updated with no compensation. Create the limiter first and insert it only after construction succeeds, so a failure leaves no trace.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { keyedLimiters } from './src/limiter.mjs';
const k = keyedLimiters({ capacity: 0, refillPerSecond: 1, now: () => 0 });
let threw = false;
try { k.for('x'); } catch (e) { threw = e instanceof RangeError; }
if (!threw) throw new Error('expected RangeError');
if (k.size !== 0) throw new Error(`failed key left in map, size ${k.size}`);
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:6:67)"

The fix, in src/limiter.mjs:

```diff
-        buckets.set(key, null);
         limiter = new RateLimiter(options);
         buckets.set(key, limiter);
```

### 3. Missing await (concurrency.missing_await), Minor, planted, test/edge_cases.mjs lines 69 to 70

- Consequence: The completion step runs after the module has returned, so the final message can be printed late or not at all. Any error thrown inside finish becomes an unhandled rejection that the runner never sees, so a failing suite can look like it passed.
- In an interview: The async function is called without await or a catch, so its promise is dropped and its failures are lost. Await it at the top level or attach a catch handler so completion and errors are observed in order.

Proven by the judge: The edit wraps the final console.log in an async function that awaits a resolved promise, then calls finish() without awaiting or handling its returned promise. This creates the claimed missing-await pattern: the message prints after module return, and any error inside finish would be an unhandled rejection. However, the original code had no asynchronous call at all; the final log was synchronous and ran deterministically. The edit introduces the un-awaited async call, so the defect is introduced by the edit. The claimed consequence (late or missing message, unhandled rejection hiding failure) follows from the edit, though the only thrown error possible is from console.log, which is practically negligible; the consequence is nonetheless the stated pattern's consequence.


The fix, in test/edge_cases.mjs:

```diff
-const finish = async () => { await Promise.resolve(); console.log('all checks passed'); };
-finish();
+console.log('all checks passed');
```

### 4. Unbounded growth (resources.unbounded_growth), Major, found in review, src/limiter.mjs lines 60 to 61

- Consequence: Every distinct key adds a RateLimiter that is never removed, so when keys come from untrusted input such as client IPs or user strings, an attacker can keep sending new keys and grow the Map until the process runs out of memory.
- In an interview: The keyed map only ever grows, and keys often come from clients an attacker controls, so memory use is effectively unbounded. Capping the map and evicting the oldest insertion (Map keeps insertion order) bounds memory, at the cost that an evicted key starts with a fresh full bucket, which is a tradeoff to state explicitly.

Proven by the judge: Unbounded growth of the per-key map is a real memory risk in keyedLimiters, not a planted issue, and the clean code has the same behavior.

The fix, in src/limiter.mjs:

```diff
-buckets.set(key, null);
+if (buckets.size >= (options.maxKeys ?? 10000)) buckets.delete(buckets.keys().next().value);
         limiter = new RateLimiter(options);
```

### 5. Untested branch (testing.untested_branch), Minor, found in review, test/limiter.test.mjs lines 26 to 27

- Consequence: The refill test only checks that a request is allowed after 250ms. A limiter that refills too fast, or that ignores the refill threshold, still passes, so the refill boundary goes untested.
- In an interview: A refill test needs both sides: a request must be denied before enough time has passed and allowed after. Adding a negative assertion at a shorter interval catches a limiter that refills too quickly.

Proven by the judge: The positive-only refill test is a real gap: it never checks that requests are denied before the refill threshold, so a too-fast refill would pass.

The fix, in test/limiter.test.mjs:

```diff
-  clock.advance(250);
+  clock.advance(100);
+  if (limiter.tryAcquire().allowed) throw new Error('token must not refill before the threshold');
+  clock.advance(150);
   if (!limiter.tryAcquire().allowed) throw new Error('token should have refilled');
```

### 6. Untested branch (testing.untested_branch), Major, found in review, test/limiter.test.mjs lines 46 to 47

- Consequence: The test file only exports functions and has no runner, so running it executes nothing and reports success. Also, the zero, negative and NaN cost paths and the constructor validation are never run, so a regression there reaches production unnoticed.
- In an interview: The test covers only one of the rejection branches, and the file exports its tests without any runner, so nothing ever executes them. A candidate would point out that exported test functions need a driver, such as a script that imports and calls each one, to count as tests at all.

Proven by the judge: The test file exports functions with no runner, so these tests may never execute; this is a real gap in the test setup.

The fix, in test/limiter.test.mjs:

```diff
 export function testRejectsBadInput() {
   const clock = fakeClock();
+  const probe = new RateLimiter({ capacity: 5, refillPerSecond: 1, now: clock.now });
+  for (const cost of [0, -1, NaN]) {
+    let threw = false;
+    try { probe.tryAcquire(cost); } catch (e) { threw = e instanceof RangeError; }
+    if (!threw) throw new Error(`cost ${cost} must throw RangeError`);
+  }
```

### 7. Overly broad catch (errors.overly_broad_catch), Minor, found in review, test/edge_cases.mjs lines 42 to 42

- Consequence: The constructor-validation check passes for any exception, including a TypeError or ReferenceError from a bug in the constructor. A regression that throws the wrong error type would still pass, so the test cannot tell a validation failure from a crash.
- In an interview: The catch block sets threw for any exception, so a programming error inside the constructor would satisfy the test just as a deliberate validation error does. Checking for the specific error types the constructor is meant to throw, RangeError or TypeError, keeps the test meaningful.

Proven by the judge: The catch accepts any exception, so a TypeError from a bug would satisfy the check; this is a real weakness in the test.

The fix, in test/edge_cases.mjs:

```diff
-try { new RateLimiter(opts); } catch (e) { threw = true; }
+try { new RateLimiter(opts); } catch (e) { threw = e instanceof RangeError || e instanceof TypeError; }
```

## Dropped

Nothing was dropped.


## The clean code

### src/limiter.mjs

```text
 1 | // A token bucket rate limiter. Time is injected so that callers and tests
 2 | // control the clock; the limiter never reads wall-clock time itself.
 3 | 
 4 | export class RateLimiter {
 5 |   #capacity;
 6 |   #refillPerMs;
 7 |   #now;
 8 |   #tokens;
 9 |   #lastRefill;
10 | 
11 |   constructor({ capacity, refillPerSecond, now }) {
12 |     if (!Number.isFinite(capacity) || capacity <= 0) {
13 |       throw new RangeError('capacity must be a positive number');
14 |     }
15 |     if (!Number.isFinite(refillPerSecond) || refillPerSecond <= 0) {
16 |       throw new RangeError('refillPerSecond must be a positive number');
17 |     }
18 |     if (typeof now !== 'function') {
19 |       throw new TypeError('now must be a function returning milliseconds');
20 |     }
21 |     this.#capacity = capacity;
22 |     this.#refillPerMs = refillPerSecond / 1000;
23 |     this.#now = now;
24 |     this.#tokens = capacity;
25 |     this.#lastRefill = now();
26 |   }
27 | 
28 |   #refill() {
29 |     const current = this.#now();
30 |     // The clock may be coarse or step backwards; never mint tokens for
31 |     // negative elapsed time.
32 |     const elapsed = Math.max(0, current - this.#lastRefill);
33 |     this.#tokens = Math.min(this.#capacity, this.#tokens + elapsed * this.#refillPerMs);
34 |     if (current > this.#lastRefill) this.#lastRefill = current;
35 |   }
36 | 
37 |   tryAcquire(cost = 1) {
38 |     if (!Number.isFinite(cost) || cost <= 0 || cost > this.#capacity) {
39 |       throw new RangeError('cost must be positive and no larger than capacity');
40 |     }
41 |     this.#refill();
42 |     if (this.#tokens >= cost) {
43 |       this.#tokens -= cost;
44 |       return { allowed: true, retryAfterMs: 0 };
45 |     }
46 |     const deficit = cost - this.#tokens;
47 |     return { allowed: false, retryAfterMs: Math.ceil(deficit / this.#refillPerMs) };
48 |   }
49 | }
50 | 
51 | export function keyedLimiters(options) {
52 |   const buckets = new Map();
53 |   return {
54 |     for(key) {
55 |       if (typeof key !== 'string' || key.length === 0) {
56 |         throw new TypeError('key must be a non-empty string');
57 |       }
58 |       let limiter = buckets.get(key);
59 |       if (!limiter) {
60 |         limiter = new RateLimiter(options);
61 |         buckets.set(key, limiter);
62 |       }
63 |       return limiter;
64 |     },
65 |     get size() {
66 |       return buckets.size;
67 |     },
68 |   };
69 | }
```

### test/limiter.test.mjs

```text
 1 | import { RateLimiter, keyedLimiters } from '../src/limiter.mjs';
 2 | 
 3 | function fakeClock(start = 0) {
 4 |   let t = start;
 5 |   return {
 6 |     now: () => t,
 7 |     advance(ms) { t += ms; },
 8 |     set(ms) { t = ms; },
 9 |   };
10 | }
11 | 
12 | export function testDeniesWhenEmptyAndReportsRetry() {
13 |   const clock = fakeClock();
14 |   const limiter = new RateLimiter({ capacity: 2, refillPerSecond: 1, now: clock.now });
15 |   if (!limiter.tryAcquire().allowed) throw new Error('first request should pass');
16 |   if (!limiter.tryAcquire().allowed) throw new Error('second request should pass');
17 |   const denied = limiter.tryAcquire();
18 |   if (denied.allowed) throw new Error('third request should be denied');
19 |   if (denied.retryAfterMs !== 1000) throw new Error(`expected 1000ms, got ${denied.retryAfterMs}`);
20 | }
21 | 
22 | export function testRefillsOverTime() {
23 |   const clock = fakeClock();
24 |   const limiter = new RateLimiter({ capacity: 1, refillPerSecond: 2, now: clock.now });
25 |   limiter.tryAcquire();
26 |   clock.advance(250);
27 |   if (!limiter.tryAcquire().allowed) throw new Error('token should have refilled');
28 | }
29 | 
30 | export function testBackwardClockDoesNotMintTokens() {
31 |   const clock = fakeClock(1000);
32 |   const limiter = new RateLimiter({ capacity: 1, refillPerSecond: 1, now: clock.now });
33 |   limiter.tryAcquire();
34 |   clock.set(0);
35 |   if (limiter.tryAcquire().allowed) throw new Error('clock regression must not refill');
36 | }
37 | 
38 | export function testKeyedLimitersAreIndependent() {
39 |   const clock = fakeClock();
40 |   const keyed = keyedLimiters({ capacity: 1, refillPerSecond: 1, now: clock.now });
41 |   keyed.for('a').tryAcquire();
42 |   if (!keyed.for('b').tryAcquire().allowed) throw new Error('key b should have its own bucket');
43 |   if (keyed.size !== 2) throw new Error('expected two buckets');
44 | }
45 | 
46 | export function testRejectsBadInput() {
47 |   const clock = fakeClock();
48 |   const limiter = new RateLimiter({ capacity: 1, refillPerSecond: 1, now: clock.now });
49 |   let threw = false;
50 |   try { limiter.tryAcquire(2); } catch (e) { threw = e instanceof RangeError; }
51 |   if (!threw) throw new Error('cost above capacity must throw RangeError');
52 | }
```

### test/edge_cases.mjs

```text
 1 | import { RateLimiter, keyedLimiters } from '../src/limiter.mjs';
 2 | 
 3 | function assert(cond, msg) {
 4 |   if (!cond) throw new Error(`assertion failed: ${msg}`);
 5 | }
 6 | 
 7 | function clockAt(start) {
 8 |   let t = start;
 9 |   return { now: () => t, set: (v) => { t = v; }, advance: (d) => { t += d; } };
10 | }
11 | 
12 | // Basic acquire and denial with retry hint.
13 | {
14 |   const c = clockAt(0);
15 |   const r = new RateLimiter({ capacity: 3, refillPerSecond: 2, now: c.now });
16 |   assert(r.tryAcquire().allowed, 'first ok');
17 |   assert(r.tryAcquire(2).allowed, 'cost 2 ok');
18 |   const d = r.tryAcquire();
19 |   assert(!d.allowed, 'empty denies');
20 |   assert(d.retryAfterMs === 500, `retry 500, got ${d.retryAfterMs}`);
21 | }
22 | 
23 | // Tokens cap at capacity no matter how long idle.
24 | {
25 |   const c = clockAt(0);
26 |   const r = new RateLimiter({ capacity: 2, refillPerSecond: 10, now: c.now });
27 |   c.advance(60000);
28 |   assert(r.tryAcquire(2).allowed, 'full bucket allows capacity');
29 |   assert(!r.tryAcquire().allowed, 'cannot exceed capacity');
30 | }
31 | 
32 | // Constructor validation.
33 | {
34 |   const c = clockAt(0);
35 |   const bad = [
36 |     { capacity: 0, refillPerSecond: 1, now: c.now },
37 |     { capacity: 1, refillPerSecond: -1, now: c.now },
38 |     { capacity: 1, refillPerSecond: 1, now: 5 },
39 |   ];
40 |   for (const opts of bad) {
41 |     let threw = false;
42 |     try { new RateLimiter(opts); } catch (e) { threw = true; }
43 |     assert(threw, 'invalid options must throw');
44 |   }
45 | }
46 | 
47 | // Invalid cost values are rejected.
48 | {
49 |   const c = clockAt(0);
50 |   const r = new RateLimiter({ capacity: 5, refillPerSecond: 1, now: c.now });
51 |   for (const cost of [0, -1, NaN, 6]) {
52 |     let threw = false;
53 |     try { r.tryAcquire(cost); } catch (e) { threw = e instanceof RangeError; }
54 |     assert(threw, `cost ${cost} must throw RangeError`);
55 |   }
56 | }
57 | 
58 | // Keyed limiters reuse buckets per key and reject empty keys.
59 | {
60 |   const c = clockAt(0);
61 |   const k = keyedLimiters({ capacity: 1, refillPerSecond: 1, now: c.now });
62 |   assert(k.for('x') === k.for('x'), 'same key returns same limiter');
63 |   assert(k.size === 1, 'one bucket for one key');
64 |   let threw = false;
65 |   try { k.for(''); } catch (e) { threw = e instanceof TypeError; }
66 |   assert(threw, 'empty key must throw TypeError');
67 | }
68 | 
69 | console.log('all checks passed');
```

### The hidden suite

```text
import { RateLimiter, keyedLimiters } from './src/limiter.mjs';

function assert(cond, msg) {
  if (!cond) throw new Error(`assertion failed: ${msg}`);
}

function clockAt(start) {
  let t = start;
  return { now: () => t, set: (v) => { t = v; }, advance: (d) => { t += d; } };
}

{
  const c = clockAt(0);
  const r = new RateLimiter({ capacity: 2, refillPerSecond: 1, now: c.now });
  assert(r.tryAcquire().allowed, 'first allowed');
  assert(r.tryAcquire().allowed, 'second allowed');
  const d = r.tryAcquire();
  assert(!d.allowed && d.retryAfterMs === 1000, 'third denied with 1000ms hint');
  c.advance(1000);
  assert(r.tryAcquire().allowed, 'allowed after refill');
}

{
  const c = clockAt(1000);
  const r = new RateLimiter({ capacity: 1, refillPerSecond: 1, now: c.now });
  r.tryAcquire();
  c.set(0);
  assert(!r.tryAcquire().allowed, 'backward clock does not refill');
}

{
  const c = clockAt(0);
  const k = keyedLimiters({ capacity: 1, refillPerSecond: 1, now: c.now });
  k.for('a').tryAcquire();
  assert(k.for('b').tryAcquire().allowed, 'keys are independent');
  assert(k.size === 2, 'two buckets');
}

{
  const c = clockAt(0);
  const r = new RateLimiter({ capacity: 1, refillPerSecond: 1, now: c.now });
  let threw = false;
  try { r.tryAcquire(2); } catch (e) { threw = e instanceof RangeError; }
  assert(threw, 'cost above capacity throws RangeError');
}

console.log('suite passed');
```

## The clean review

- Major: When the clock steps backwards, #refill() sets #lastRefill to the regressed time. Time between the regressed value and the old high-water mark is then credited again when the clock moves forward, so tokens are minted twice for the same interval. Only advance #lastRefill when time moves forward.

## The blind review

1. Critical in src/limiter.mjs: Strict inequality denies a request when tokens exactly equal cost, so a full bucket with cost equal to capacity can never be acquired (capacity 1, cost 1 is denied forever). Should be >= cost. Also the retry computation assumes this boundary; use >= consistently.
   Quoted: if (this.#tokens > cost) {
   Judged: Matches { issues: [0] }. Correctly identifies the strict comparison at line 42 as the cause of denying exact-cost requests.
2. Major in src/limiter.mjs: With the strict comparison, a request whose deficit is zero yields retryAfterMs 0 while still denied, which is inconsistent. Fixing the comparison to >= resolves this; the retry hint should otherwise be computed from the same threshold.
   Quoted: const deficit = cost - this.#tokens;
   Judged: Matches { issues: [0] }. The zero-deficit inconsistency at line 46 follows from the same strict comparison boundary; it restates the consequence of issue 1.
3. Major in src/limiter.mjs: keyedLimiters grows without bound: one bucket is kept per distinct key forever with no eviction, so untrusted keys (e.g. client IPs or user-supplied strings) cause unbounded memory growth. Add an eviction policy or a maximum size.
   Quoted: for(key) {
   Judged: ValidExtra. Unbounded growth of the per-key map is a real memory risk in keyedLimiters, not a planted issue, and the clean code has the same behavior.
4. Major in src/limiter.mjs: Placeholder null is stored before the constructor runs; if the constructor throws (invalid options), the map is left with a null entry, so later calls return null and size is corrupted. Construct first, then set the map entry. Also options are validated only lazily on first use, so a bad configuration surfaces late.
   Quoted: buckets.set(key, null);         limiter = new RateLimiter(options);
   Judged: Matches { issues: [1] }. Correctly identifies that the null placeholder persists when the constructor throws, corrupting later lookups and size.
5. Nit in src/limiter.mjs: When the clock steps backwards, lastRefill is not moved back, so the bucket only resumes refilling once the clock passes the previous high-water mark. That is intended, but the stored value is never updated for the backward step; the behavior is correct only because of the guard below. Worth a comment or a test documenting that refill resumes from the high-water mark.
   Quoted: const elapsed = Math.max(0, current - this.#lastRefill);
   Judged: FalsePositive. The backward-clock behavior is intended and correctly guarded; the finding only asks for a comment or test, which is a remark rather than a defect.
6. Minor in src/limiter.mjs: Destructuring a missing or null argument throws a raw TypeError instead of a clear message; default the parameter or validate the options object.
   Quoted: constructor({ capacity, refillPerSecond, now }) {
   Judged: FalsePositive. Destructuring a null argument throws a TypeError that is reasonably clear; this is a style preference, not a real defect.
7. Minor in test/limiter.test.mjs: The test only checks the positive case. It never verifies that a request is denied before the refill threshold, so an implementation that refills too fast would still pass. Add a negative assertion at a shorter interval.
   Quoted: if (!limiter.tryAcquire().allowed) throw new Error('token should have refilled');
   Judged: ValidExtra. The positive-only refill test is a real gap: it never checks that requests are denied before the refill threshold, so a too-fast refill would pass.
8. Minor in test/limiter.test.mjs: Only the cost-above-capacity case is tested; the constructor validation and the zero/NaN cost paths are not covered here. Also the test file exports functions but nothing runs them, so these tests may never execute.
   Quoted: export function testRejectsBadInput() {
   Judged: ValidExtra. The test file exports functions with no runner, so these tests may never execute; this is a real gap in the test setup.
9. Nit in test/edge_cases.mjs: The success message prints unconditionally after asynchronous no-op; failures thrown synchronously above would already crash, but the async wrapper adds nothing and the message can mislead. Simplify to a plain synchronous log.
   Quoted: const finish = async () => { await Promise.resolve(); console.log('all checks passed'); }; finish();
   Judged: Matches { issues: [2] }. It points at the async finish wrapper and its unconditional success message, which is the code of issue 3, but it does not identify the missing await or the lost error as the problem.
10. Major in test/edge_cases.mjs: With the strict comparison in the limiter, a full bucket of capacity 2 allowing cost 2 would be denied, so this assertion documents the wrong behavior or passes only by accident of timing; it should be verified against the intended >= semantics.
   Quoted: assert(r.tryAcquire(2).allowed, 'full bucket allows capacity');
   Judged: Matches { issues: [0] }. A full bucket with cost equal to capacity is denied because of the strict comparison, so the assertion at line 28 exposes the planted boundary error of issue 1.
11. Minor in test/edge_cases.mjs: The catch accepts any exception, so a TypeError from a bug would satisfy the check. Assert e instanceof RangeError or TypeError specifically, as the other blocks do.
   Quoted: try { new RateLimiter(opts); } catch (e) { threw = true; }
   Judged: ValidExtra. The catch accepts any exception, so a TypeError from a bug would satisfy the check; this is a real weakness in the test.

- planted-1 (correctness.wrong_operator): the review Found { finding: 0 }. Finding 1 names the strict comparison at line 42 as denying a request when tokens exactly equal cost, so a request the bucket can cover is refused, and says it should be >=. This states the cause of the planted operator error.
- planted-2 (errors.partial_update_without_rollback): the review Found { finding: 3 }. Finding 4 says the placeholder null is stored before the constructor runs and remains in the map when the constructor throws, so later calls return null and size is corrupted. This is the partial-update problem the issue describes.
- planted-3 (concurrency.missing_await): the review HalfFound { finding: 8 }. Finding 9 points at the async wrapper and the unconditional success message but never states that finish() is called without await, so the lost error is not named as the cause.

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
- key entry: claude-haiku-5-5
