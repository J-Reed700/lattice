# Inventory stock reservation service with expiring holds (typescript)

- Domain: inventory and stock reservations
- Built by: claude-haiku-5-5
- Runner: none, every defect was judged
- Built in 58 seconds
- Patterns asked for: correctness.off_by_one, errors.error_as_success, concurrency.unsynchronised_shared_state

## Counts

- Planted: 3
- Proven by a test: 0
- Proven by the judge: 2
- Dropped: 1
- Found in review: 1

## The code

### src/inventory/reservations.ts

```text
 1 | export type ReservationStatus = "held" | "committed" | "released";
 2 | 
 3 | export interface Reservation {
 4 |   readonly id: string;
 5 |   readonly sku: string;
 6 |   readonly quantity: number;
 7 |   readonly expiresAt: number;
 8 |   status: ReservationStatus;
 9 | }
10 | 
11 | export class InsufficientStockError extends Error {
12 |   constructor(readonly sku: string, readonly requested: number, readonly available: number) {
13 |     super(`insufficient stock for ${sku}: requested ${requested}, available ${available}`);
14 |     this.name = "InsufficientStockError";
15 |   }
16 | }
17 | 
18 | export class ReservationBook {
19 |   private onHand = new Map<string, number>();
20 |   private reservations = new Map<string, Reservation>();
21 |   private nextId = 1;
22 | 
23 |   setOnHand(sku: string, quantity: number): void {
24 |     if (!Number.isInteger(quantity) || quantity < 0) {
25 |       throw new RangeError(`on-hand quantity must be a non-negative integer, got ${quantity}`);
26 |     }
27 |     this.onHand.set(sku, quantity);
28 |   }
29 | 
30 |   // Held units are subtracted lazily: expired holds stop counting against
31 |   // availability, but they are only marked released when they are touched,
32 |   // so a sweep is never required for correctness.
33 |   available(sku: string, now: number): number {
34 |     let held = 0;
35 |     for (const r of this.reservations.values()) {
36 |       if (r.sku === sku && r.status === "held" && r.expiresAt >= now) {
37 |         held += r.quantity;
38 |       }
39 |     }
40 |     return (this.onHand.get(sku) ?? 0) - held;
41 |   }
42 | 
43 |   reserve(sku: string, quantity: number, ttlMs: number, now: number): Reservation {
44 |     if (!Number.isInteger(quantity) || quantity <= 0) {
45 |       throw new RangeError(`reservation quantity must be a positive integer, got ${quantity}`);
46 |     }
47 |     const free = this.available(sku, now);
48 |     if (quantity > free) {
49 |       throw new InsufficientStockError(sku, quantity, Math.max(free, 0));
50 |     }
51 |     const reservation: Reservation = {
52 |       id: `res-${this.nextId++}`,
53 |       sku,
54 |       quantity,
55 |       expiresAt: now + ttlMs,
56 |       status: "held",
57 |     };
58 |     this.reservations.set(reservation.id, reservation);
59 |     return reservation;
60 |   }
61 | 
62 |   commit(id: string, now: number): void {
63 |     const r = this.requireLive(id, now);
64 |     const onHand = this.onHand.get(r.sku) ?? 0;
65 |     if (onHand < r.quantity) {
66 |       return;
67 |     }
68 |     this.onHand.set(r.sku, onHand - r.quantity);
69 |     r.status = "committed";
70 |   }
71 | 
72 |   release(id: string): void {
73 |     const r = this.reservations.get(id);
74 |     if (r && r.status === "held") {
75 |       r.status = "released";
76 |     }
77 |   }
78 | 
79 |   private requireLive(id: string, now: number): Reservation {
80 |     const r = this.reservations.get(id);
81 |     if (!r || r.status !== "held") {
82 |       throw new Error(`reservation ${id} is not held`);
83 |     }
84 |     if (r.expiresAt <= now) {
85 |       throw new Error(`reservation ${id} expired at ${r.expiresAt}`);
86 |     }
87 |     return r;
88 |   }
89 | }
```

### test/inventory/reservations.test.ts

```text
 1 | import { describe, it, expect } from "vitest";
 2 | import { ReservationBook, InsufficientStockError } from "../../src/inventory/reservations";
 3 | 
 4 | describe("ReservationBook", () => {
 5 |   it("blocks reservations beyond available stock", () => {
 6 |     const book = new ReservationBook();
 7 |     book.setOnHand("sku-1", 5);
 8 |     book.reserve("sku-1", 3, 1000, 0);
 9 |     expect(() => book.reserve("sku-1", 3, 1000, 0)).toThrow(InsufficientStockError);
10 |   });
11 | 
12 |   it("frees stock once a hold expires", () => {
13 |     const book = new ReservationBook();
14 |     book.setOnHand("sku-1", 5);
15 |     book.reserve("sku-1", 5, 1000, 0);
16 |     expect(book.available("sku-1", 999)).toBe(0);
17 |     expect(book.available("sku-1", 1000)).toBe(5);
18 |   });
19 | 
20 |   it("commit decrements on-hand and rejects expired holds", () => {
21 |     const book = new ReservationBook();
22 |     book.setOnHand("sku-1", 4);
23 |     const r = book.reserve("sku-1", 2, 500, 0);
24 |     book.commit(r.id, 100);
25 |     expect(book.available("sku-1", 100)).toBe(2);
26 |     const late = book.reserve("sku-1", 1, 500, 100);
27 |     expect(() => book.commit(late.id, 600)).toThrow(/expired/);
28 |   });
29 | });
```

## The key

### 1. Off-by-one error (correctness.off_by_one), Major, planted, src/inventory/reservations.ts lines 36 to 36

- Consequence: A hold is still counted against availability at the exact instant it expires, so stock looks unavailable for one extra millisecond and a reserve at that instant is wrongly rejected with InsufficientStockError.
- In an interview: The expiry boundary is now inclusive, so a hold with expiresAt equal to now still blocks stock even though the code treats it as expired elsewhere. The fix is to keep the strict comparison, expiresAt > now, so that expiry is consistent with requireLive and the existing test that expects stock to free at exactly 1000.

Proven by the judge: The edit changes the expiry comparison in available() from strict to non-strict, so a hold whose expiresAt equals now is still counted as held. This makes stock look unavailable at the exact expiry instant, which matches the claimed consequence. However, the original code's requireLive uses expiresAt <= now to treat a hold as expired at that instant, so the original treats expiry at exactly expiresAt as expired, while the edited available() treats it as still held, creating an inconsistency. The claimed consequence (a reserve at that instant wrongly rejected) does occur with the edit. But the original code already had the boundary consistent with the 'frees stock once a hold expires' test (available at 1000 returns 5), and the edit breaks that existing test expectation, which is a different defect (test regression) than merely an off-by-one in the consequence claimed. The problem described as off-by-one is introduced by the edit, and the original lacks it since the original correctly frees stock at expiresAt.


The fix, in src/inventory/reservations.ts:

```diff
-if (r.sku === sku && r.status === "held" && r.expiresAt >= now) {
+if (r.sku === sku && r.status === "held" && r.expiresAt > now) {
```

### 2. Error reported as success (errors.error_as_success), Critical, planted, src/inventory/reservations.ts lines 65 to 67

- Consequence: When on-hand stock has fallen below the hold quantity, commit returns normally without marking the reservation committed or decrementing stock, so the caller records a sale that never happened and the order is fulfilled without inventory.
- In an interview: The insufficient-stock failure is swallowed and reported as a successful commit, so callers cannot tell the commit failed. The fix is to throw InsufficientStockError as the original code did so the caller can reject the order or retry.

Proven by the judge: The edit replaces a thrown InsufficientStockError with a silent return when on-hand stock is below the reserved quantity. The commit then returns normally without marking the reservation committed or decrementing stock, so callers cannot tell the commit failed. This matches the error-as-success pattern: a failure becomes a normal success. The original code threw the error, so the failure was visible and the problem was not present there. The claimed consequence (caller records a sale that never happened) follows from the silent return, since the caller sees a normal result.


The fix, in src/inventory/reservations.ts:

```diff
     if (onHand < r.quantity) {
-      return;
+      throw new InsufficientStockError(r.sku, r.quantity, onHand);
     }
```

### 3. Untested branch (testing.untested_branch), Minor, found in review, test/inventory/reservations.test.ts lines 27 to 27

- Consequence: The silent-return branch in commit() when on-hand stock is below the held quantity is never executed by any test, so it can leave a reservation held while reporting success, and the expiry boundary (now equal to expiresAt) is not pinned down, so available() and commit() can disagree there without any test failing.
- In an interview: A test that reaches the insufficient-on-hand path would expose that commit() returns silently instead of failing, so untested branches hide real defects. Boundary cases such as now equal to expiresAt should each have an explicit assertion so that the two methods' expiry rules are checked against each other.

Proven by the judge: Test-coverage remark noting missing tests for the silent-return branch and the expiry boundary. It does not name the cause of either defect in the code beyond pointing at the critical bug, so it is a valid observation about tests rather than a finding of a code defect; it points at issue 1's branch and the boundary of issue 2 without stating their causes.

The fix, in test/inventory/reservations.test.ts:

```diff
     expect(() => book.commit(late.id, 600)).toThrow(/expired/);
+    const tight = book.reserve("sku-1", 1, 500, 100);
+    book.setOnHand("sku-1", 0);
+    expect(() => book.commit(tight.id, 100)).toThrow(InsufficientStockError);
```

## Dropped

- concurrency.unsynchronised_shared_state: The edit hoists the onHand map to a module-level singleton shared by all ReservationBook instances, so one book's setOnHand and commit changes another's availability. This is cross-instance state leakage (a shared-state/isolation bug), not an unsynchronised concurrent read-modify-write problem. JavaScript is single-threaded and commit has no await, so interleaving lost updates within the event loop cannot occur from this edit. The original code has per-instance maps, so the sharing problem is not present there, and the claimed lost-update-by-interleaving consequence is not what the edit introduces.
  Consequence claimed: Every ReservationBook in the process now shares one stock map, so concurrent requests reading and then writing on-hand counts for the same SKU can interleave and lose updates, and one book's setOnHand changes another's availability.

## The clean code

### src/inventory/reservations.ts

```text
 1 | export type ReservationStatus = "held" | "committed" | "released";
 2 | 
 3 | export interface Reservation {
 4 |   readonly id: string;
 5 |   readonly sku: string;
 6 |   readonly quantity: number;
 7 |   readonly expiresAt: number;
 8 |   status: ReservationStatus;
 9 | }
10 | 
11 | export class InsufficientStockError extends Error {
12 |   constructor(readonly sku: string, readonly requested: number, readonly available: number) {
13 |     super(`insufficient stock for ${sku}: requested ${requested}, available ${available}`);
14 |     this.name = "InsufficientStockError";
15 |   }
16 | }
17 | 
18 | export class ReservationBook {
19 |   private onHand = new Map<string, number>();
20 |   private reservations = new Map<string, Reservation>();
21 |   private nextId = 1;
22 | 
23 |   setOnHand(sku: string, quantity: number): void {
24 |     if (!Number.isInteger(quantity) || quantity < 0) {
25 |       throw new RangeError(`on-hand quantity must be a non-negative integer, got ${quantity}`);
26 |     }
27 |     this.onHand.set(sku, quantity);
28 |   }
29 | 
30 |   // Held units are subtracted lazily: expired holds stop counting against
31 |   // availability, but they are only marked released when they are touched,
32 |   // so a sweep is never required for correctness.
33 |   available(sku: string, now: number): number {
34 |     let held = 0;
35 |     for (const r of this.reservations.values()) {
36 |       if (r.sku === sku && r.status === "held" && r.expiresAt > now) {
37 |         held += r.quantity;
38 |       }
39 |     }
40 |     return (this.onHand.get(sku) ?? 0) - held;
41 |   }
42 | 
43 |   reserve(sku: string, quantity: number, ttlMs: number, now: number): Reservation {
44 |     if (!Number.isInteger(quantity) || quantity <= 0) {
45 |       throw new RangeError(`reservation quantity must be a positive integer, got ${quantity}`);
46 |     }
47 |     const free = this.available(sku, now);
48 |     if (quantity > free) {
49 |       throw new InsufficientStockError(sku, quantity, Math.max(free, 0));
50 |     }
51 |     const reservation: Reservation = {
52 |       id: `res-${this.nextId++}`,
53 |       sku,
54 |       quantity,
55 |       expiresAt: now + ttlMs,
56 |       status: "held",
57 |     };
58 |     this.reservations.set(reservation.id, reservation);
59 |     return reservation;
60 |   }
61 | 
62 |   commit(id: string, now: number): void {
63 |     const r = this.requireLive(id, now);
64 |     const onHand = this.onHand.get(r.sku) ?? 0;
65 |     if (onHand < r.quantity) {
66 |       throw new InsufficientStockError(r.sku, r.quantity, onHand);
67 |     }
68 |     this.onHand.set(r.sku, onHand - r.quantity);
69 |     r.status = "committed";
70 |   }
71 | 
72 |   release(id: string): void {
73 |     const r = this.reservations.get(id);
74 |     if (r && r.status === "held") {
75 |       r.status = "released";
76 |     }
77 |   }
78 | 
79 |   private requireLive(id: string, now: number): Reservation {
80 |     const r = this.reservations.get(id);
81 |     if (!r || r.status !== "held") {
82 |       throw new Error(`reservation ${id} is not held`);
83 |     }
84 |     if (r.expiresAt <= now) {
85 |       throw new Error(`reservation ${id} expired at ${r.expiresAt}`);
86 |     }
87 |     return r;
88 |   }
89 | }
```

### test/inventory/reservations.test.ts

```text
 1 | import { describe, it, expect } from "vitest";
 2 | import { ReservationBook, InsufficientStockError } from "../../src/inventory/reservations";
 3 | 
 4 | describe("ReservationBook", () => {
 5 |   it("blocks reservations beyond available stock", () => {
 6 |     const book = new ReservationBook();
 7 |     book.setOnHand("sku-1", 5);
 8 |     book.reserve("sku-1", 3, 1000, 0);
 9 |     expect(() => book.reserve("sku-1", 3, 1000, 0)).toThrow(InsufficientStockError);
10 |   });
11 | 
12 |   it("frees stock once a hold expires", () => {
13 |     const book = new ReservationBook();
14 |     book.setOnHand("sku-1", 5);
15 |     book.reserve("sku-1", 5, 1000, 0);
16 |     expect(book.available("sku-1", 999)).toBe(0);
17 |     expect(book.available("sku-1", 1000)).toBe(5);
18 |   });
19 | 
20 |   it("commit decrements on-hand and rejects expired holds", () => {
21 |     const book = new ReservationBook();
22 |     book.setOnHand("sku-1", 4);
23 |     const r = book.reserve("sku-1", 2, 500, 0);
24 |     book.commit(r.id, 100);
25 |     expect(book.available("sku-1", 100)).toBe(2);
26 |     const late = book.reserve("sku-1", 1, 500, 100);
27 |     expect(() => book.commit(late.id, 600)).toThrow(/expired/);
28 |   });
29 | });
```

## The clean review

- Major: commit() does not verify that on-hand stock covers the reservation. setOnHand() can lower on-hand below units already held, so committing can drive on-hand negative without any error.

## The blind review

1. Critical in src/inventory/reservations.ts: commit silently returns without error when on-hand is insufficient, leaving the reservation in 'held' state while the caller believes the commit succeeded. The failure is swallowed; it should throw (e.g. InsufficientStockError) or return a result the caller must handle.
   Quoted:     if (onHand < r.quantity) {       return;     }
   Judged: Matches { issues: [1] }. Correctly identifies the silent return on insufficient on-hand in commit as a swallowed failure that should throw.
2. Minor in src/inventory/reservations.ts: available() scans every reservation ever made on each call, and reserve() calls it, so cost grows without bound with history. Consider indexing reservations by sku or pruning settled/expired entries.
   Quoted:   available(sku: string, now: number): number {     let held = 0;     for (const r of this.reservations.values()) {
   Judged: FalsePositive. Performance remark about scanning all reservations; not a planted defect and not a real correctness problem in this code, only a preference about efficiency.
3. Minor in src/inventory/reservations.ts: Releasing an unknown id is silently ignored, and releasing an already-committed reservation is also a silent no-op. Callers cannot detect mistakes such as double release or wrong ids; at least document this or throw for unknown ids.
   Quoted:   release(id: string): void {     const r = this.reservations.get(id);     if (r && r.status === "held") {
   Judged: FalsePositive. Silent no-op on release of unknown ids is a design remark about API feedback; the code is consistent and idempotent, so this is a preference rather than a defect.
4. Major in src/inventory/reservations.ts: Expiry is checked with <= in requireLive but available() counts holds with expiresAt >= now, so the boundary is inconsistent: a hold with expiresAt === now is still counted as blocking stock in available() yet cannot be committed. Align the boundary semantics.
   Quoted:     if (r.expiresAt <= now) {       throw new Error(`reservation ${id} expired at ${r.expiresAt}`);
   Judged: Matches { issues: [0] }. Identifies the boundary inconsistency between available() (>=) and requireLive (<=) at expiresAt === now, which is the off-by-one planted in available().
5. Nit in src/inventory/reservations.ts: Reservation ids are a per-instance counter producing predictable 'res-N' strings; this is fine in-memory but ids are not unique across ReservationBook instances or restarts, which may matter if they are persisted or sent to other services.
   Quoted:   private nextId = 1;
   Judged: FalsePositive. Per-instance predictable ids are a design note on uniqueness across instances; the in-memory counter is intended here and this is not a real defect in the code as written.
6. Major in test/inventory/reservations.test.ts: The test asserts the expired-commit path but never checks that a commit with insufficient on-hand fails, which is the critical bug above; no test covers the silent-return branch. Also the test name claims rejection of expired holds but only covers one case, and there is no test for the expiresAt boundary (now === expiresAt) that is inconsistent between available() and commit().
   Quoted:     const late = book.reserve("sku-1", 1, 500, 100);     expect(() => book.commit(late.id, 600)).toThrow(/expired/);
   Judged: ValidExtra. Test-coverage remark noting missing tests for the silent-return branch and the expiry boundary. It does not name the cause of either defect in the code beyond pointing at the critical bug, so it is a valid observation about tests rather than a finding of a code defect; it points at issue 1's branch and the boundary of issue 2 without stating their causes.
7. Nit in test/inventory/reservations.test.ts: The test checks only the error type, not the reported available quantity or that state is unchanged after the failed reserve. Asserting the error fields and that no reservation was recorded would prove more.
   Quoted:   it("blocks reservations beyond available stock", () => {
   Judged: FalsePositive. Suggests asserting error fields and state after a failed reserve; a test-style preference, not a real defect.

- planted-1 (correctness.off_by_one): the review Found { finding: 3 }. Finding 4 identifies the inconsistent boundary: available() counts holds with expiresAt >= now while requireLive uses <=, so a hold at expiresAt === now still blocks stock yet cannot be committed. This is the same boundary defect as the off-by-one at line 36, named by its cause.
- planted-2 (errors.error_as_success): the review Found { finding: 0 }. Finding 1 names the swallowed failure in commit: it returns silently when on-hand is insufficient, leaving the reservation held while the caller believes the commit succeeded, and says it should throw. This states the cause and its effect.

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
