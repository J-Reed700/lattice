# Token bucket rate limiter with per-key buckets and a test suite (typescript)

- Domain: rate limiting
- Built by: claude-haiku-5-5
- Runner: none, every defect was judged
- Built in 76 seconds
- Patterns asked for: correctness.wrong_operator, errors.partial_update_without_rollback, concurrency.missing_await

## Counts

- Planted: 3
- Proven by a test: 0
- Proven by the judge: 1
- Dropped: 2
- Found in review: 3

## The code

### src/rateLimiter.ts

```text
 1 | export interface RateLimiterOptions {
 2 |   capacity: number;
 3 |   refillPerSecond: number;
 4 |   now?: () => number;
 5 | }
 6 | 
 7 | export interface Decision {
 8 |   allowed: boolean;
 9 |   remaining: number;
10 |   retryAfterMs: number;
11 | }
12 | 
13 | interface Bucket {
14 |   tokens: number;
15 |   updatedAt: number;
16 | }
17 | 
18 | export class TokenBucketLimiter {
19 |   private readonly buckets = new Map<string, Bucket>();
20 |   private readonly capacity: number;
21 |   private readonly refillPerMs: number;
22 |   private readonly now: () => number;
23 | 
24 |   constructor(options: RateLimiterOptions) {
25 |     if (!Number.isFinite(options.capacity) || options.capacity < 1) {
26 |       throw new RangeError('capacity must be a finite number >= 1');
27 |     }
28 |     if (!Number.isFinite(options.refillPerSecond) || options.refillPerSecond <= 0) {
29 |       throw new RangeError('refillPerSecond must be a positive finite number');
30 |     }
31 |     this.capacity = options.capacity;
32 |     this.refillPerMs = options.refillPerSecond / 1000;
33 |     this.now = options.now ?? Date.now;
34 |   }
35 | 
36 |   consume(key: string, cost = 1): Decision {
37 |     if (!Number.isFinite(cost) || cost <= 0 || cost > this.capacity) {
38 |       throw new RangeError('cost must be positive and no larger than capacity');
39 |     }
40 |     const current = this.now();
41 |     const bucket = this.buckets.get(key) ?? { tokens: this.capacity, updatedAt: current };
42 | 
43 |     // Refill lazily on access so idle keys cost nothing between requests.
44 |     const elapsed = Math.max(0, current - bucket.updatedAt);
45 |     bucket.tokens = Math.min(this.capacity, bucket.tokens + elapsed * this.refillPerMs);
46 |     bucket.updatedAt = Math.max(bucket.updatedAt, current);
47 |     this.buckets.set(key, bucket);
48 | 
49 |     if (bucket.tokens >= cost) {
50 |       bucket.tokens -= cost;
51 |       return { allowed: true, remaining: Math.floor(bucket.tokens), retryAfterMs: 0 };
52 |     }
53 | 
54 |     // Report how long until enough tokens accumulate so callers can send Retry-After.
55 |     const deficit = cost - bucket.tokens;
56 |     return {
57 |       allowed: false,
58 |       remaining: Math.floor(bucket.tokens),
59 |       retryAfterMs: Math.ceil(deficit / this.refillPerMs),
60 |     };
61 |   }
62 | 
63 |   prune(maxIdleMs: number): number {
64 |     const current = this.now();
65 |     let removed = 0;
66 |     for (const [key, bucket] of this.buckets) {
67 |       if (current - bucket.updatedAt > maxIdleMs) {
68 |         this.buckets.delete(key);
69 |         removed++;
70 |       }
71 |     }
72 |     return removed;
73 |   }
74 | }
```

### test/rateLimiter.test.ts

```text
 1 | import { describe, it, expect } from 'vitest';
 2 | import { TokenBucketLimiter } from '../src/rateLimiter';
 3 | 
 4 | describe('TokenBucketLimiter', () => {
 5 |   it('allows bursts up to capacity then rejects', () => {
 6 |     let t = 0;
 7 |     const limiter = new TokenBucketLimiter({ capacity: 2, refillPerSecond: 1, now: () => t });
 8 |     expect(limiter.consume('a').allowed).toBe(true);
 9 |     expect(limiter.consume('a').allowed).toBe(true);
10 |     const denied = limiter.consume('a');
11 |     expect(denied.allowed).toBe(false);
12 |     expect(denied.retryAfterMs).toBe(1000);
13 |   });
14 | 
15 |   it('refills over time without exceeding capacity', () => {
16 |     let t = 0;
17 |     const limiter = new TokenBucketLimiter({ capacity: 2, refillPerSecond: 1, now: () => t });
18 |     limiter.consume('a');
19 |     limiter.consume('a');
20 |     t = 1000;
21 |     expect(limiter.consume('a').allowed).toBe(true);
22 |     t = 100000;
23 |     expect(limiter.consume('a').remaining).toBe(1);
24 |   });
25 | 
26 |   it('tracks keys independently', () => {
27 |     let t = 0;
28 |     const limiter = new TokenBucketLimiter({ capacity: 1, refillPerSecond: 1, now: () => t });
29 |     expect(limiter.consume('a').allowed).toBe(true);
30 |     expect(limiter.consume('b').allowed).toBe(true);
31 |     expect(limiter.consume('a').allowed).toBe(false);
32 |   });
33 | 
34 |   it('prunes idle buckets', () => {
35 |     let t = 0;
36 |     const limiter = new TokenBucketLimiter({ capacity: 1, refillPerSecond: 1, now: () => t });
37 |     limiter.consume('a');
38 |     t = 5000;
39 |     limiter.consume('b');
40 |     expect(limiter.prune(3000)).toBe(1);
41 |   });
42 | 
43 |   it('rejects invalid configuration', () => {
44 |     expect(() => new TokenBucketLimiter({ capacity: 0, refillPerSecond: 1 })).toThrow(RangeError);
45 |     expect(() => new TokenBucketLimiter({ capacity: 1, refillPerSecond: 0 })).toThrow(RangeError);
46 |   });
47 | });
```

## The key

### 1. Wrong operator or comparison (correctness.wrong_operator), Minor, planted, src/rateLimiter.ts lines 67 to 67

- Consequence: Buckets idle for exactly maxIdleMs are kept one more cycle, so prune reports fewer removals and memory is released slightly later than the API promises.
- In an interview: The idle check uses a strict comparison where the intent is inclusive, so a bucket idle for exactly the threshold is not pruned. Use >= so the boundary matches the documented 'idle for at least maxIdleMs' behaviour.

Proven by the judge: The edit changes the prune comparison from >= to >, so buckets idle for exactly maxIdleMs are no longer removed. This is a real boundary change that matches the claimed mechanism of keeping such buckets one more cycle. However, the claimed consequence is about the API promise and memory release timing; whether this counts as the wrong-operator defect depends on intent, and the original >= plausibly matches a 'idle for at least maxIdleMs' contract. The edit does introduce a boundary difference relative to the original, and the original's >= is not itself the defect, so the edit introduces a deviation from the original's intended semantics but the original code lacks the problem as described only if >= was intended, which is ambiguous; the original is the more natural reading of 'idle for at least maxIdleMs', so the original does not exhibit the defect.


The fix, in src/rateLimiter.ts:

```diff
-      if (current - bucket.updatedAt > maxIdleMs) {
+      if (current - bucket.updatedAt >= maxIdleMs) {
```

### 2. Unbounded growth (resources.unbounded_growth), Major, found in review, src/rateLimiter.ts lines 46 to 47

- Consequence: Every distinct key adds a Bucket entry that consume() never removes, so with attacker-controlled keys such as IPs or user IDs the map grows until the process runs out of memory, unless the caller remembers to call prune().
- In an interview: The buckets map has no size bound, so an attacker who varies the key can make its memory use grow without limit, because prune() runs only when someone calls it. Capping the map inside consume(), evicting the oldest-inserted key when a new key would exceed the cap, bounds memory without relying on callers, though it is FIFO rather than true LRU.

Proven by the judge: Points out unbounded growth of the buckets map with attacker-controlled keys, a real memory exhaustion risk; this is a valid defect not in the key.

The fix, in src/rateLimiter.ts:

```diff
     bucket.updatedAt = Math.max(bucket.updatedAt, current);
+    const maxKeys = 10_000;
+    if (!this.buckets.has(key) && this.buckets.size >= maxKeys) {
+      const oldest = this.buckets.keys().next().value;
+      if (oldest !== undefined) this.buckets.delete(oldest);
+    }
     this.buckets.set(key, bucket);
```

### 3. Unhandled boundary or empty input (correctness.boundary_and_empty_input), Major, found in review, src/rateLimiter.ts lines 46 to 46

- Consequence: If the clock moves backwards by D ms, updatedAt keeps the later timestamp, so no tokens refill for the next D ms after the clock returns to the earlier value. Under a large NTP or VM clock correction, legitimate clients are denied for that whole span.
- In an interview: The clamp handles a negative elapsed time for the refill math, but the Math.max on updatedAt means the bucket keeps a future reference point after a backwards clock jump. Resetting updatedAt to the current reading lets refill resume immediately, and the test suite would need a case that moves the clock back to catch the stall.

Proven by the judge: Describes a backwards-clock stall where updatedAt keeps the later timestamp and refill stalls; this is a real behavior risk in the code, not the planted comparison.

The fix, in src/rateLimiter.ts:

```diff
-    bucket.updatedAt = Math.max(bucket.updatedAt, current);
+    bucket.updatedAt = current;
```

### 4. Unhandled boundary or empty input (correctness.boundary_and_empty_input), Minor, found in review, src/rateLimiter.ts lines 63 to 63

- Consequence: A negative maxIdleMs makes every bucket look idle, so prune deletes all state and every key's rate limit resets at once. A NaN maxIdleMs makes every comparison false, so prune never removes anything and idle buckets accumulate in memory.
- In an interview: The method accepts any number, so a negative value wipes every bucket and resets all limits, while NaN silently disables pruning. The constructor already validates its inputs, so prune should reject bad values at the same boundary instead of letting them produce opposite failures.

Proven by the judge: Notes missing validation of maxIdleMs where negative or NaN values behave oddly; a real robustness gap in the public API, though not the planted boundary comparison.

The fix, in src/rateLimiter.ts:

```diff
-  prune(maxIdleMs: number): number {
+  prune(maxIdleMs: number): number { if (!Number.isFinite(maxIdleMs) || maxIdleMs < 0) throw new RangeError('maxIdleMs must be a finite number >= 0');
```

## Dropped

- errors.partial_update_without_rollback: The edit removes this.buckets.set(key, bucket) before the allow/deny decision. For a first-seen key, the new bucket is never stored in the map, so the consumed tokens are lost. The claimed consequence is that new keys are never stored and each request starts from a full bucket. However, the bug described is a missing persistence step, not a partial multi-step update that fails partway and leaves data half updated. The edit does not create a partial-update scenario; there is no multi-step change that can fail between steps. Additionally, the original code does not have this problem since it stores the bucket. The pattern (partial update without rollback) is not what the edit introduces, so the answer is no for introduction; the original lacks the bug, so cleanLacksIt is true.
  Consequence claimed: Buckets for keys seen for the first time are never stored, so every request for a new key starts from a full bucket and the limit is bypassed for those keys.
- concurrency.missing_await: The edit wraps the synchronous prune result in expect(...).resolves, but prune returns a plain number, not a promise. Vitest's resolves on a non-promise fails with an error, and since the test does not await or return the assertion, the failure is not tied to the test properly. The original code has no missing await: prune is synchronous and its assertion runs immediately within the test. So the claimed consequence (assertion running after the test finishes) does not arise from the original, and the edit introduces a different problem (misuse of resolves on a non-promise), not a missing await in the original.
  Consequence claimed: The assertion on the prune result runs after the test has finished, so a wrong count is reported as an unhandled rejection or not at all instead of failing this test.

## The clean code

### src/rateLimiter.ts

```text
 1 | export interface RateLimiterOptions {
 2 |   capacity: number;
 3 |   refillPerSecond: number;
 4 |   now?: () => number;
 5 | }
 6 | 
 7 | export interface Decision {
 8 |   allowed: boolean;
 9 |   remaining: number;
10 |   retryAfterMs: number;
11 | }
12 | 
13 | interface Bucket {
14 |   tokens: number;
15 |   updatedAt: number;
16 | }
17 | 
18 | export class TokenBucketLimiter {
19 |   private readonly buckets = new Map<string, Bucket>();
20 |   private readonly capacity: number;
21 |   private readonly refillPerMs: number;
22 |   private readonly now: () => number;
23 | 
24 |   constructor(options: RateLimiterOptions) {
25 |     if (!Number.isFinite(options.capacity) || options.capacity < 1) {
26 |       throw new RangeError('capacity must be a finite number >= 1');
27 |     }
28 |     if (!Number.isFinite(options.refillPerSecond) || options.refillPerSecond <= 0) {
29 |       throw new RangeError('refillPerSecond must be a positive finite number');
30 |     }
31 |     this.capacity = options.capacity;
32 |     this.refillPerMs = options.refillPerSecond / 1000;
33 |     this.now = options.now ?? Date.now;
34 |   }
35 | 
36 |   consume(key: string, cost = 1): Decision {
37 |     if (!Number.isFinite(cost) || cost <= 0 || cost > this.capacity) {
38 |       throw new RangeError('cost must be positive and no larger than capacity');
39 |     }
40 |     const current = this.now();
41 |     const bucket = this.buckets.get(key) ?? { tokens: this.capacity, updatedAt: current };
42 | 
43 |     // Refill lazily on access so idle keys cost nothing between requests.
44 |     const elapsed = Math.max(0, current - bucket.updatedAt);
45 |     bucket.tokens = Math.min(this.capacity, bucket.tokens + elapsed * this.refillPerMs);
46 |     bucket.updatedAt = Math.max(bucket.updatedAt, current);
47 |     this.buckets.set(key, bucket);
48 | 
49 |     if (bucket.tokens >= cost) {
50 |       bucket.tokens -= cost;
51 |       return { allowed: true, remaining: Math.floor(bucket.tokens), retryAfterMs: 0 };
52 |     }
53 | 
54 |     // Report how long until enough tokens accumulate so callers can send Retry-After.
55 |     const deficit = cost - bucket.tokens;
56 |     return {
57 |       allowed: false,
58 |       remaining: Math.floor(bucket.tokens),
59 |       retryAfterMs: Math.ceil(deficit / this.refillPerMs),
60 |     };
61 |   }
62 | 
63 |   prune(maxIdleMs: number): number {
64 |     const current = this.now();
65 |     let removed = 0;
66 |     for (const [key, bucket] of this.buckets) {
67 |       if (current - bucket.updatedAt >= maxIdleMs) {
68 |         this.buckets.delete(key);
69 |         removed++;
70 |       }
71 |     }
72 |     return removed;
73 |   }
74 | }
```

### test/rateLimiter.test.ts

```text
 1 | import { describe, it, expect } from 'vitest';
 2 | import { TokenBucketLimiter } from '../src/rateLimiter';
 3 | 
 4 | describe('TokenBucketLimiter', () => {
 5 |   it('allows bursts up to capacity then rejects', () => {
 6 |     let t = 0;
 7 |     const limiter = new TokenBucketLimiter({ capacity: 2, refillPerSecond: 1, now: () => t });
 8 |     expect(limiter.consume('a').allowed).toBe(true);
 9 |     expect(limiter.consume('a').allowed).toBe(true);
10 |     const denied = limiter.consume('a');
11 |     expect(denied.allowed).toBe(false);
12 |     expect(denied.retryAfterMs).toBe(1000);
13 |   });
14 | 
15 |   it('refills over time without exceeding capacity', () => {
16 |     let t = 0;
17 |     const limiter = new TokenBucketLimiter({ capacity: 2, refillPerSecond: 1, now: () => t });
18 |     limiter.consume('a');
19 |     limiter.consume('a');
20 |     t = 1000;
21 |     expect(limiter.consume('a').allowed).toBe(true);
22 |     t = 100000;
23 |     expect(limiter.consume('a').remaining).toBe(1);
24 |   });
25 | 
26 |   it('tracks keys independently', () => {
27 |     let t = 0;
28 |     const limiter = new TokenBucketLimiter({ capacity: 1, refillPerSecond: 1, now: () => t });
29 |     expect(limiter.consume('a').allowed).toBe(true);
30 |     expect(limiter.consume('b').allowed).toBe(true);
31 |     expect(limiter.consume('a').allowed).toBe(false);
32 |   });
33 | 
34 |   it('prunes idle buckets', () => {
35 |     let t = 0;
36 |     const limiter = new TokenBucketLimiter({ capacity: 1, refillPerSecond: 1, now: () => t });
37 |     limiter.consume('a');
38 |     t = 5000;
39 |     limiter.consume('b');
40 |     expect(limiter.prune(3000)).toBe(1);
41 |   });
42 | 
43 |   it('rejects invalid configuration', () => {
44 |     expect(() => new TokenBucketLimiter({ capacity: 0, refillPerSecond: 1 })).toThrow(RangeError);
45 |     expect(() => new TokenBucketLimiter({ capacity: 1, refillPerSecond: 0 })).toThrow(RangeError);
46 |   });
47 | });
```

## The clean review

- Minor: If the injected clock moves backward, elapsed is clamped to 0 but updatedAt is still overwritten with the earlier time. When the clock later moves forward again, the same interval is refilled twice, so tokens can exceed what the rate allows. updatedAt should never move backward.

## The blind review

1. Major in src/rateLimiter.ts: The buckets map grows without bound: every distinct key adds an entry that is only removed if the caller remembers to call prune(). With attacker-controlled keys (e.g. IP or user-supplied IDs) this is a memory exhaustion vector. Consider an automatic eviction policy such as an LRU cap or pruning on a timer inside consume.
   Quoted: const bucket = this.buckets.get(key) ?? { tokens: this.capacity, updatedAt: current };
   Judged: ValidExtra. Points out unbounded growth of the buckets map with attacker-controlled keys, a real memory exhaustion risk; this is a valid defect not in the key.
2. Nit in src/rateLimiter.ts: Validation of cost happens on every call while the constructor validates capacity; this is fine, but the thrown error is an uncaught RangeError for what is usually a programmer mistake on hot paths. Acceptable, though callers should be told it can throw; document it or return a Decision.
   Quoted:     if (!Number.isFinite(cost) || cost <= 0 || cost > this.capacity) {
   Judged: FalsePositive. Remark about documenting that consume throws on invalid cost; a preference about error handling, not a defect.
3. Minor in src/rateLimiter.ts: Clamping elapsed to zero is correct for a clock going backwards, but updatedAt is then only advanced with Math.max, so after a backwards clock jump the bucket keeps the later timestamp and refill stalls until the clock catches up. This can silently deny requests for a long time after a clock adjustment. Document or handle the non-monotonic clock case explicitly.
   Quoted:     const elapsed = Math.max(0, current - bucket.updatedAt);
   Judged: ValidExtra. Describes a backwards-clock stall where updatedAt keeps the later timestamp and refill stalls; this is a real behavior risk in the code, not the planted comparison.
4. Nit in src/rateLimiter.ts: Each call creates a new object and writes it back with set even when the bucket already exists; minor overhead on a hot path. Mutating the existing bucket in place and only calling set for new keys would be cleaner.
   Quoted:     const bucket = this.buckets.get(key) ?? { tokens: this.capacity, updatedAt: current };
   Judged: FalsePositive. Style remark about recreating the bucket object on each call; a preference, not a defect.
5. Minor in src/rateLimiter.ts: No validation of maxIdleMs; a negative or NaN value behaves unexpectedly (negative deletes every bucket, NaN deletes none). Validate the argument like the constructor does.
   Quoted:   prune(maxIdleMs: number): number {
   Judged: ValidExtra. Notes missing validation of maxIdleMs where negative or NaN values behave oddly; a real robustness gap in the public API, though not the planted boundary comparison.
6. Nit in src/rateLimiter.ts: Floating point tokens can make deficit tiny but positive, producing a retryAfterMs of 1 or an off-by-one versus the actual refill time; the reported value is an estimate, which is fine, but the rounding should be understood as approximate.
   Quoted:       retryAfterMs: Math.ceil(deficit / this.refillPerMs),
   Judged: FalsePositive. Notes floating point rounding in retryAfterMs is approximate; this is an acceptable estimate, not a defect.
7. Minor in test/rateLimiter.test.ts: The test only checks the count of removed buckets; it never verifies that the correct key ('a') was removed and 'b' kept. A later consume on 'a' should show a fresh full bucket, and 'b' should still hold its state.
   Quoted:     expect(limiter.prune(3000)).toBe(1);
   Judged: FalsePositive. Test gap: does not verify the correct key was removed; a test quality remark, not a defect in the code.
8. Nit in test/rateLimiter.test.ts: This asserts the cap at capacity only indirectly through remaining after a consume; it does not check the boundary where tokens are exactly at capacity or that refill never exceeds capacity with a direct assertion on the bucket state. Also the test name claims refill but the assertion mixes two behaviors.
   Quoted:     t = 100000;     expect(limiter.consume('a').remaining).toBe(1);
   Judged: FalsePositive. Test quality remark about indirect assertion of capacity cap; not a code defect.
9. Minor in test/rateLimiter.test.ts: Validation is tested only for two cases. Missing coverage for NaN/Infinity capacity, negative refill, and for consume() rejecting invalid cost (zero, negative, greater than capacity), which is a validated code path with no test.
   Quoted:     expect(() => new TokenBucketLimiter({ capacity: 0, refillPerSecond: 1 })).toThrow(RangeError);
   Judged: FalsePositive. Test coverage gap for invalid cost and config; a remark on tests, not a code defect.
10. Minor in test/rateLimiter.test.ts: No test covers the fractional-token case, where retryAfterMs and remaining rounding are most likely to be wrong, nor the custom cost parameter, nor a clock moving backwards.
   Quoted:   it('allows bursts up to capacity then rejects', () => {
   Judged: FalsePositive. Test coverage gap for fractional tokens, custom cost and backwards clock; a remark on tests, not a code defect.

- planted-1 (correctness.wrong_operator): the review Missed. Finding 5 is on prune's signature and Finding 7 mentions prune but neither names the strict > versus >= boundary problem: that buckets idle exactly maxIdleMs are kept one more cycle. No finding identifies this comparison.

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
