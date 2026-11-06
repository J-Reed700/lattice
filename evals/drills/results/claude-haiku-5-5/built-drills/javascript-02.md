# Inventory stock reservation service with expiring holds (javascript)

- Domain: inventory and stock reservations
- Built by: claude-haiku-5-5
- Runner: builtin-javascript
- Built in 100 seconds
- Patterns asked for: correctness.off_by_one, errors.error_as_success, concurrency.unsynchronised_shared_state

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 0
- Dropped: 1
- Found in review: 5

## The code

### src/reservations.mjs

```text
 1 | // Stock reservations: a customer holds units of a SKU for a limited time.
 2 | // The store and the clock are injected so the rules can be exercised without
 3 | // touching real persistence or wall-clock time.
 4 | 
 5 | export class ReservationError extends Error {
 6 |   constructor(message) {
 7 |     super(message);
 8 |     this.name = 'ReservationError';
 9 |   }
10 | }
11 | 
12 | export class ReservationService {
13 |   #store;
14 |   #now;
15 |   #holdMs;
16 | 
17 |   constructor({ store, now = () => Date.now(), holdMs = 15 * 60 * 1000 }) {
18 |     if (!store) throw new TypeError('store is required');
19 |     this.#store = store;
20 |     this.#now = now;
21 |     this.#holdMs = holdMs;
22 |   }
23 | 
24 |   // Available units are on-hand stock minus unexpired holds. Expired holds
25 |   // are ignored here rather than deleted eagerly, so a late confirm can still
26 |   // detect that the hold lapsed and report it accurately.
27 |   available(sku) {
28 |     const item = this.#store.getItem(sku);
29 |     if (!item) return 0;
30 |     const cutoff = this.#now();
31 |     let held = 0;
32 |     for (const hold of this.#store.holdsFor(sku)) {
33 |       if (hold.expiresAt >= cutoff) held += hold.quantity;
34 |     }
35 |     return item.onHand - held;
36 |   }
37 | 
38 |   reserve(sku, quantity, customerId) {
39 |     if (!Number.isInteger(quantity) || quantity <= 0) {
40 |       throw new ReservationError('quantity must be a positive integer');
41 |     }
42 |     if (this.available(sku) < quantity) {
43 |       throw new ReservationError(`insufficient stock for ${sku}`);
44 |     }
45 |     const hold = {
46 |       id: this.#store.nextId(),
47 |       sku,
48 |       quantity,
49 |       customerId,
50 |       expiresAt: this.#now() + this.#holdMs,
51 |     };
52 |     this.#store.saveHold(hold);
53 |     return hold;
54 |   }
55 | 
56 |   // Confirming consumes the hold: on-hand drops and the hold is removed.
57 |   confirm(holdId) {
58 |     const hold = this.#store.getHold(holdId);
59 |     if (!hold) throw new ReservationError(`no such hold: ${holdId}`);
60 |     if (hold.expiresAt <= this.#now()) {
61 |       this.#store.deleteHold(holdId);
62 |       throw new ReservationError(`hold expired: ${holdId}`);
63 |     }
64 |     this.#store.decrementOnHand(hold.sku, hold.quantity);
65 |     this.#store.deleteHold(holdId);
66 |     return hold;
67 |   }
68 | 
69 |   release(holdId) {
70 |     const hold = this.#store.getHold(holdId);
71 |     if (!hold) return false;
72 |     this.#store.deleteHold(holdId);
73 |     return true;
74 |   }
75 | }
```

### src/memory_store.mjs

```text
 1 | // In-memory store used by tests and local tooling. It implements the same
 2 | // shape as the production store so the service never knows the difference.
 3 | 
 4 | export class MemoryStore {
 5 |   #items = new Map();
 6 |   #holds = new Map();
 7 |   #seq = 0;
 8 | 
 9 |   constructor(initialItems = []) {
10 |     for (const { sku, onHand } of initialItems) {
11 |       if (!Number.isInteger(onHand) || onHand < 0) {
12 |         throw new RangeError(`invalid on-hand for ${sku}`);
13 |       }
14 |       this.#items.set(sku, { sku, onHand });
15 |     }
16 |   }
17 | 
18 |   getItem(sku) {
19 |     const item = this.#items.get(sku);
20 |     return item ? { ...item } : undefined;
21 |   }
22 | 
23 |   holdsFor(sku) {
24 |     return [...this.#holds.values()].filter((h) => h.sku === sku).map((h) => ({ ...h }));
25 |   }
26 | 
27 |   nextId() {
28 |     this.#seq += 1;
29 |     return `hold-${this.#seq}`;
30 |   }
31 | 
32 |   saveHold(hold) {
33 |     this.#holds.set(hold.id, { ...hold });
34 |   }
35 | 
36 |   getHold(id) {
37 |     const hold = this.#holds.get(id);
38 |     return hold ? { ...hold } : undefined;
39 |   }
40 | 
41 |   deleteHold(id) {
42 |     this.#holds.delete(id);
43 |   }
44 | 
45 |   decrementOnHand(sku, quantity) {
46 |     const item = this.#items.get(sku);
47 |     if (!item || item.onHand < quantity) {
48 |       throw new RangeError(`cannot decrement ${sku} by ${quantity}`);
49 |     }
50 |     item.onHand -= quantity;
51 |   }
52 | }
```

### test/reservations.test.mjs

```text
 1 | import { ReservationService, ReservationError } from '../src/reservations.mjs';
 2 | import { MemoryStore } from '../src/memory_store.mjs';
 3 | 
 4 | function assert(cond, msg) {
 5 |   if (!cond) throw new Error(`assertion failed: ${msg}`);
 6 | }
 7 | 
 8 | function expectThrows(fn, pattern, msg) {
 9 |   try {
10 |     fn();
11 |   } catch (e) {
12 |     assert(e instanceof ReservationError, `${msg}: wrong error type`);
13 |     assert(pattern.test(e.message), `${msg}: message was '${e.message}'`);
14 |     return;
15 |   }
16 |   throw new Error(`assertion failed: ${msg}: no error thrown`);
17 | }
18 | 
19 | function setup(onHand = 10) {
20 |   let clock = 1000;
21 |   const store = new MemoryStore([{ sku: 'A1', onHand }]);
22 |   const service = new ReservationService({
23 |     store,
24 |     now: () => clock,
25 |     holdMs: 100,
26 |   });
27 |   return { store, service, tick: (ms) => { clock += ms; } };
28 | }
29 | 
30 | {
31 |   const { service } = setup(10);
32 |   assert(service.available('A1') === 10, 'initial availability');
33 |   const hold = service.reserve('A1', 4, 'cust-1');
34 |   assert(service.available('A1') === 6, 'hold reduces availability');
35 |   assert(hold.expiresAt === 1100, 'expiry uses injected clock');
36 | }
37 | 
38 | {
39 |   const { service, tick } = setup(10);
40 |   service.reserve('A1', 8, 'cust-1');
41 |   expectThrows(() => service.reserve('A1', 3, 'cust-2'), /insufficient/, 'overbook');
42 |   tick(100);
43 |   assert(service.available('A1') === 10, 'expired hold no longer counts');
44 |   service.reserve('A1', 3, 'cust-2');
45 | }
46 | 
47 | {
48 |   const { service, store } = setup(10);
49 |   const hold = service.reserve('A1', 3, 'cust-1');
50 |   const confirmed = service.confirm(hold.id);
51 |   assert(confirmed.quantity === 3, 'confirm returns hold');
52 |   assert(store.getItem('A1').onHand === 7, 'confirm consumes stock');
53 |   assert(service.available('A1') === 7, 'availability after confirm');
54 |   expectThrows(() => service.confirm(hold.id), /no such hold/, 'double confirm');
55 | }
56 | 
57 | {
58 |   const { service, tick } = setup(10);
59 |   const hold = service.reserve('A1', 2, 'cust-1');
60 |   tick(100);
61 |   expectThrows(() => service.confirm(hold.id), /expired/, 'late confirm');
62 |   assert(service.available('A1') === 10, 'late confirm frees units');
63 | }
64 | 
65 | {
66 |   const { service } = setup(5);
67 |   const hold = service.reserve('A1', 5, 'cust-1');
68 |   assert(service.release(hold.id) === true, 'release existing hold');
69 |   assert(service.release(hold.id) === false, 'release is idempotent');
70 |   assert(service.available('A1') === 5, 'release restores units');
71 |   expectThrows(() => service.reserve('A1', 0, 'cust-1'), /positive integer/, 'zero quantity');
72 |   expectThrows(() => service.reserve('A1', 1.5, 'cust-1'), /positive integer/, 'fractional quantity');
73 |   expectThrows(() => service.available('ZZ'), /unknown sku/, 'unknown sku');
74 | }
75 | 
76 | console.log('all reservation checks passed');
```

## The key

### 1. Off-by-one error (correctness.off_by_one), Major, planted, src/reservations.mjs lines 33 to 33

- Consequence: At the exact expiry instant a lapsed hold still blocks stock, so available() reports fewer units than are really free until the next tick and reserve() can refuse a valid request.
- In an interview: The expiry comparison is off by one at the boundary, so a hold whose expiresAt equals the current time is still counted as active. The fix is to use a strict comparison so a hold stops counting exactly when it expires, matching confirm() which treats expiresAt <= now as expired.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { ReservationService } from './src/reservations.mjs';
import { MemoryStore } from './src/memory_store.mjs';
let clock = 0;
const store = new MemoryStore([{ sku: 'X', onHand: 4 }]);
const s = new ReservationService({ store, now: () => clock, holdMs: 100 });
s.reserve('X', 4, 'c1');
clock = 100;
if (s.available('X') !== 4) throw new Error('hold still counted at expiry boundary');
console.log('ok');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:8:39)"

The fix, in src/reservations.mjs:

```diff
-      if (hold.expiresAt >= cutoff) held += hold.quantity;
+      if (hold.expiresAt > cutoff) held += hold.quantity;
```

### 2. Error reported as success (errors.error_as_success), Major, planted, src/reservations.mjs lines 29 to 29

- Consequence: A typo'd or retired SKU reports zero availability instead of failing, so storefronts show it as sold out and callers cannot distinguish a bad SKU from empty stock.
- In an interview: An unknown SKU is now turned into a normal zero result instead of an error, hiding a caller bug or bad data. The fix is to keep raising ReservationError so callers can tell a failed lookup from genuinely zero stock.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { ReservationService, ReservationError } from './src/reservations.mjs';
import { MemoryStore } from './src/memory_store.mjs';
const s = new ReservationService({ store: new MemoryStore([]), now: () => 0 });
let threw = false;
try { s.available('ZZ'); } catch (e) { threw = e instanceof ReservationError; }
if (!threw) throw new Error('unknown sku did not raise');
console.log('ok');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:6:23)"

The fix, in src/reservations.mjs:

```diff
-    if (!item) return 0;
+    if (!item) throw new ReservationError(`unknown sku: ${sku}`);
```

### 3. Partial update without rollback (errors.partial_update_without_rollback), Major, found in review, src/reservations.mjs lines 64 to 66

- Consequence: If decrementOnHand succeeds and deleteHold then throws, the stock is consumed but the hold remains and still reduces availability, so a retried confirm would decrement on-hand a second time. If decrementOnHand throws, the hold is left in place even though nothing was consumed, and the failure path is not handled.
- In an interview: The two store writes are not atomic, so a failure between them can leave stock consumed while the hold still counts, or leave a hold that a retry would consume twice. Deleting the hold first and restoring it with a compensating saveHold if the decrement fails means a retry can never double-consume, though a true fix needs a transactional store.

Proven by the judge: Non-atomic decrement and delete in confirm() is a real risk, but it is not one of the planted issues and is not about the off-by-one or the silent zero return.

The fix, in src/reservations.mjs:

```diff
-    this.#store.decrementOnHand(hold.sku, hold.quantity);
     this.#store.deleteHold(holdId);
+    try {
+      this.#store.decrementOnHand(hold.sku, hold.quantity);
+    } catch (err) {
+      this.#store.saveHold(hold);
+      throw err;
+    }
     return hold;
```

### 4. Check-then-act race (concurrency.check_then_act), Major, found in review, src/reservations.mjs lines 42 to 52

- Consequence: Two reserve() calls for the same SKU can both run the availability check before either hold is saved, so both succeed and the total held quantity exceeds on-hand stock. This happens when the store is shared across concurrent tasks or processes, or is async, so the check and the save can interleave. The result is an oversold SKU that confirm() later cannot fulfil.
- In an interview: The check reads availability before the write, so any interleaving between the two lets concurrent callers both pass. Saving first and then re-checking availability, rolling back the hold if it went negative, means an oversell is always detected, though it can reject both racing callers rather than making the operation atomic.

Proven by the judge: Check-then-write race in reserve() allowing oversell under concurrency is a real risk, not one of the planted issues.

The fix, in src/reservations.mjs:

```diff
-    if (this.available(sku) < quantity) {
-      throw new ReservationError(`insufficient stock for ${sku}`);
-    }
     const hold = {
       id: this.#store.nextId(),
       sku,
       quantity,
       customerId,
       expiresAt: this.#now() + this.#holdMs,
     };
     this.#store.saveHold(hold);
+    if (this.available(sku) < 0) {
+      this.#store.deleteHold(hold.id);
+      throw new ReservationError(`insufficient stock for ${sku}`);
+    }
```

### 5. Missing authorisation check (security.missing_authorisation), Major, found in review, src/reservations.mjs lines 69 to 71

- Consequence: Any caller who knows or guesses a hold id can release another customer's hold. Hold ids come from a sequential counter (hold-1, hold-2, ...), so they are easy to guess. The released units reappear as available, letting others take stock that was reserved for someone else, and the original customer loses their reservation without being told.
- In an interview: release() deletes any hold whose id is supplied, with no check that the caller is the customer who created it, even though reserve() records customerId on the hold. Binding the release to the owning customer and rejecting mismatches closes the gap, though the existing tests that call release(hold.id) would need to pass the customer id too.

Proven by the judge: Release without ownership check is a real authorization concern, not a planted issue. The claim that customerId is 'never stored-checked' is loosely stated but the authorization gap is real.

The fix, in src/reservations.mjs:

```diff
-  release(holdId) {
+  release(holdId, customerId) {
     const hold = this.#store.getHold(holdId);
     if (!hold) return false;
+    if (hold.customerId !== customerId) {
+      throw new ReservationError(`not the owner of hold: ${holdId}`);
+    }
```

### 6. Leaky abstraction (design.leaky_abstraction), Minor, found in review, src/reservations.mjs lines 64 to 64

- Consequence: confirm() can throw the store's RangeError instead of ReservationError when decrementOnHand fails, so a caller that catches only ReservationError lets it escape as an unexpected crash.
- In an interview: The service's public error contract is ReservationError, but confirm() lets the storage layer's RangeError leak through, so callers must know about storage internals. Translating the failure at the service boundary keeps the error type consistent and preserves the original cause for debugging.

Proven by the judge: Inconsistent error type from decrementOnHand for a missing SKU is a real, if minor, inconsistency in error handling, not one of the planted issues.

The fix, in src/reservations.mjs:

```diff
-    this.#store.decrementOnHand(hold.sku, hold.quantity);
+    try {
+      this.#store.decrementOnHand(hold.sku, hold.quantity);
+    } catch (e) {
+      throw new ReservationError(`cannot consume hold ${holdId}: ${e.message}`, { cause: e });
+    }
```

### 7. Unhandled boundary or empty input (correctness.boundary_and_empty_input), Minor, found in review, src/memory_store.mjs lines 14 to 14

- Consequence: If initialItems lists the same SKU twice, the second entry silently replaces the first, so the store is built with a different on-hand count than the configuration intended and nothing reports the mistake.
- In an interview: The constructor accepts duplicate SKUs without checking, so a later entry quietly overwrites an earlier one and hides a configuration error. Rejecting duplicates at construction time, the same way invalid on-hand values are already rejected, makes the mistake fail loudly where it is made.

Proven by the judge: Duplicate SKUs silently overwriting in the constructor is a real configuration-hiding risk, though minor; not a planted issue.

The fix, in src/memory_store.mjs:

```diff
+      if (this.#items.has(sku)) {
+        throw new RangeError(`duplicate sku ${sku}`);
+      }
       this.#items.set(sku, { sku, onHand });
```

## Dropped

- concurrency.unsynchronised_shared_state: The edit moves the hold sequence counter from a per-instance private field to a global variable on globalThis, so all store instances share it. The read-then-write pattern (read globalThis.holdSeq, add one, write back) is unsynchronised, which is the pattern in the abstract. However, the claimed consequence is about duplicate IDs causing overwrites that silently lose a reservation. In JavaScript, nextId() runs synchronously with no await between read and write, so on a single thread interleavings cannot occur; the consequence as claimed cannot happen in this runtime. The original code, with a per-instance counter, has no shared mutable state at all across instances, so it lacks the problem. The edit does introduce shared mutable global state, which is a legitimate concern in general, but the specific claimed consequence (lost reservations via interleaved read-write) is not reachable in single-threaded JavaScript, and the stated scenario is not what the edit exactly produces. Given strictness, the edit does introduce unsynchronised shared state, but the claimed consequence is not achievable, so answering yes for the edit introducing exactly this problem is questionable; I judge it as not exactly the claimed problem.
  Consequence claimed: When several store instances or workers share the global counter, read-then-write interleavings can hand two holds the same id, so one hold overwrites the other and a customer's reservation silently disappears.

## The clean code

### src/reservations.mjs

```text
 1 | // Stock reservations: a customer holds units of a SKU for a limited time.
 2 | // The store and the clock are injected so the rules can be exercised without
 3 | // touching real persistence or wall-clock time.
 4 | 
 5 | export class ReservationError extends Error {
 6 |   constructor(message) {
 7 |     super(message);
 8 |     this.name = 'ReservationError';
 9 |   }
10 | }
11 | 
12 | export class ReservationService {
13 |   #store;
14 |   #now;
15 |   #holdMs;
16 | 
17 |   constructor({ store, now = () => Date.now(), holdMs = 15 * 60 * 1000 }) {
18 |     if (!store) throw new TypeError('store is required');
19 |     this.#store = store;
20 |     this.#now = now;
21 |     this.#holdMs = holdMs;
22 |   }
23 | 
24 |   // Available units are on-hand stock minus unexpired holds. Expired holds
25 |   // are ignored here rather than deleted eagerly, so a late confirm can still
26 |   // detect that the hold lapsed and report it accurately.
27 |   available(sku) {
28 |     const item = this.#store.getItem(sku);
29 |     if (!item) throw new ReservationError(`unknown sku: ${sku}`);
30 |     const cutoff = this.#now();
31 |     let held = 0;
32 |     for (const hold of this.#store.holdsFor(sku)) {
33 |       if (hold.expiresAt > cutoff) held += hold.quantity;
34 |     }
35 |     return item.onHand - held;
36 |   }
37 | 
38 |   reserve(sku, quantity, customerId) {
39 |     if (!Number.isInteger(quantity) || quantity <= 0) {
40 |       throw new ReservationError('quantity must be a positive integer');
41 |     }
42 |     if (this.available(sku) < quantity) {
43 |       throw new ReservationError(`insufficient stock for ${sku}`);
44 |     }
45 |     const hold = {
46 |       id: this.#store.nextId(),
47 |       sku,
48 |       quantity,
49 |       customerId,
50 |       expiresAt: this.#now() + this.#holdMs,
51 |     };
52 |     this.#store.saveHold(hold);
53 |     return hold;
54 |   }
55 | 
56 |   // Confirming consumes the hold: on-hand drops and the hold is removed.
57 |   confirm(holdId) {
58 |     const hold = this.#store.getHold(holdId);
59 |     if (!hold) throw new ReservationError(`no such hold: ${holdId}`);
60 |     if (hold.expiresAt <= this.#now()) {
61 |       this.#store.deleteHold(holdId);
62 |       throw new ReservationError(`hold expired: ${holdId}`);
63 |     }
64 |     this.#store.decrementOnHand(hold.sku, hold.quantity);
65 |     this.#store.deleteHold(holdId);
66 |     return hold;
67 |   }
68 | 
69 |   release(holdId) {
70 |     const hold = this.#store.getHold(holdId);
71 |     if (!hold) return false;
72 |     this.#store.deleteHold(holdId);
73 |     return true;
74 |   }
75 | }
```

### src/memory_store.mjs

```text
 1 | // In-memory store used by tests and local tooling. It implements the same
 2 | // shape as the production store so the service never knows the difference.
 3 | 
 4 | export class MemoryStore {
 5 |   #items = new Map();
 6 |   #holds = new Map();
 7 |   #seq = 0;
 8 | 
 9 |   constructor(initialItems = []) {
10 |     for (const { sku, onHand } of initialItems) {
11 |       if (!Number.isInteger(onHand) || onHand < 0) {
12 |         throw new RangeError(`invalid on-hand for ${sku}`);
13 |       }
14 |       this.#items.set(sku, { sku, onHand });
15 |     }
16 |   }
17 | 
18 |   getItem(sku) {
19 |     const item = this.#items.get(sku);
20 |     return item ? { ...item } : undefined;
21 |   }
22 | 
23 |   holdsFor(sku) {
24 |     return [...this.#holds.values()].filter((h) => h.sku === sku).map((h) => ({ ...h }));
25 |   }
26 | 
27 |   nextId() {
28 |     this.#seq += 1;
29 |     return `hold-${this.#seq}`;
30 |   }
31 | 
32 |   saveHold(hold) {
33 |     this.#holds.set(hold.id, { ...hold });
34 |   }
35 | 
36 |   getHold(id) {
37 |     const hold = this.#holds.get(id);
38 |     return hold ? { ...hold } : undefined;
39 |   }
40 | 
41 |   deleteHold(id) {
42 |     this.#holds.delete(id);
43 |   }
44 | 
45 |   decrementOnHand(sku, quantity) {
46 |     const item = this.#items.get(sku);
47 |     if (!item || item.onHand < quantity) {
48 |       throw new RangeError(`cannot decrement ${sku} by ${quantity}`);
49 |     }
50 |     item.onHand -= quantity;
51 |   }
52 | }
```

### test/reservations.test.mjs

```text
 1 | import { ReservationService, ReservationError } from '../src/reservations.mjs';
 2 | import { MemoryStore } from '../src/memory_store.mjs';
 3 | 
 4 | function assert(cond, msg) {
 5 |   if (!cond) throw new Error(`assertion failed: ${msg}`);
 6 | }
 7 | 
 8 | function expectThrows(fn, pattern, msg) {
 9 |   try {
10 |     fn();
11 |   } catch (e) {
12 |     assert(e instanceof ReservationError, `${msg}: wrong error type`);
13 |     assert(pattern.test(e.message), `${msg}: message was '${e.message}'`);
14 |     return;
15 |   }
16 |   throw new Error(`assertion failed: ${msg}: no error thrown`);
17 | }
18 | 
19 | function setup(onHand = 10) {
20 |   let clock = 1000;
21 |   const store = new MemoryStore([{ sku: 'A1', onHand }]);
22 |   const service = new ReservationService({
23 |     store,
24 |     now: () => clock,
25 |     holdMs: 100,
26 |   });
27 |   return { store, service, tick: (ms) => { clock += ms; } };
28 | }
29 | 
30 | {
31 |   const { service } = setup(10);
32 |   assert(service.available('A1') === 10, 'initial availability');
33 |   const hold = service.reserve('A1', 4, 'cust-1');
34 |   assert(service.available('A1') === 6, 'hold reduces availability');
35 |   assert(hold.expiresAt === 1100, 'expiry uses injected clock');
36 | }
37 | 
38 | {
39 |   const { service, tick } = setup(10);
40 |   service.reserve('A1', 8, 'cust-1');
41 |   expectThrows(() => service.reserve('A1', 3, 'cust-2'), /insufficient/, 'overbook');
42 |   tick(100);
43 |   assert(service.available('A1') === 10, 'expired hold no longer counts');
44 |   service.reserve('A1', 3, 'cust-2');
45 | }
46 | 
47 | {
48 |   const { service, store } = setup(10);
49 |   const hold = service.reserve('A1', 3, 'cust-1');
50 |   const confirmed = service.confirm(hold.id);
51 |   assert(confirmed.quantity === 3, 'confirm returns hold');
52 |   assert(store.getItem('A1').onHand === 7, 'confirm consumes stock');
53 |   assert(service.available('A1') === 7, 'availability after confirm');
54 |   expectThrows(() => service.confirm(hold.id), /no such hold/, 'double confirm');
55 | }
56 | 
57 | {
58 |   const { service, tick } = setup(10);
59 |   const hold = service.reserve('A1', 2, 'cust-1');
60 |   tick(100);
61 |   expectThrows(() => service.confirm(hold.id), /expired/, 'late confirm');
62 |   assert(service.available('A1') === 10, 'late confirm frees units');
63 | }
64 | 
65 | {
66 |   const { service } = setup(5);
67 |   const hold = service.reserve('A1', 5, 'cust-1');
68 |   assert(service.release(hold.id) === true, 'release existing hold');
69 |   assert(service.release(hold.id) === false, 'release is idempotent');
70 |   assert(service.available('A1') === 5, 'release restores units');
71 |   expectThrows(() => service.reserve('A1', 0, 'cust-1'), /positive integer/, 'zero quantity');
72 |   expectThrows(() => service.reserve('A1', 1.5, 'cust-1'), /positive integer/, 'fractional quantity');
73 |   expectThrows(() => service.available('ZZ'), /unknown sku/, 'unknown sku');
74 | }
75 | 
76 | console.log('all reservation checks passed');
```

### The hidden suite

```text
import { ReservationService, ReservationError } from './src/reservations.mjs';
import { MemoryStore } from './src/memory_store.mjs';

function assert(cond, msg) {
  if (!cond) throw new Error(`assertion failed: ${msg}`);
}

function makeService(onHand, holdMs = 100) {
  let clock = 0;
  const store = new MemoryStore([{ sku: 'X', onHand }]);
  const service = new ReservationService({ store, now: () => clock, holdMs });
  return { store, service, advance: (ms) => { clock += ms; }, setClock: (t) => { clock = t; } };
}

function throwsReservation(fn, pattern, msg) {
  let caught;
  try {
    fn();
  } catch (e) {
    caught = e;
  }
  assert(caught !== undefined, `${msg}: expected throw`);
  assert(caught instanceof ReservationError, `${msg}: wrong type`);
  assert(pattern.test(caught.message), `${msg}: message '${caught.message}'`);
}

{
  const { service } = makeService(10);
  const hold = service.reserve('X', 3, 'c1');
  assert(service.available('X') === 7, 'reserve reduces available');
  assert(hold.customerId === 'c1', 'hold keeps customer');
  assert(typeof hold.id === 'string', 'hold has id');
}

{
  const { service } = makeService(5);
  service.reserve('X', 5, 'c1');
  throwsReservation(() => service.reserve('X', 1, 'c2'), /insufficient/, 'no oversell');
}

{
  const { service, advance } = makeService(4);
  service.reserve('X', 4, 'c1');
  advance(99);
  assert(service.available('X') === 0, 'hold still active before expiry');
  advance(1);
  assert(service.available('X') === 4, 'hold expires at boundary');
}

{
  const { service, store } = makeService(9);
  const hold = service.reserve('X', 2, 'c1');
  service.confirm(hold.id);
  assert(store.getItem('X').onHand === 7, 'confirm decrements on-hand');
  throwsReservation(() => service.confirm(hold.id), /no such hold/, 'confirm once');
}

{
  const { service, advance } = makeService(3);
  const hold = service.reserve('X', 3, 'c1');
  advance(500);
  throwsReservation(() => service.confirm(hold.id), /expired/, 'expired confirm');
  assert(service.available('X') === 3, 'expired confirm frees stock');
}

{
  const { service } = makeService(6);
  const hold = service.reserve('X', 6, 'c1');
  assert(service.release(hold.id) === true, 'release returns true');
  assert(service.release(hold.id) === false, 'release idempotent');
  assert(service.available('X') === 6, 'release restores stock');
  throwsReservation(() => service.reserve('X', -1, 'c1'), /positive integer/, 'negative qty');
  throwsReservation(() => service.available('nope'), /unknown sku/, 'unknown sku');
}

console.log('checks ok');
```

## The clean review

It found nothing to repair.


## The blind review

1. Critical in test/reservations.test.mjs: available() returns 0 for an unknown SKU and never throws, so this assertion can never pass: expectThrows reports 'no error thrown' and the test suite fails. Either change available() to throw a ReservationError for unknown SKUs (and decide that behavior deliberately) or remove/fix this check.
   Quoted: expectThrows(() => service.available('ZZ'), /unknown sku/, 'unknown sku');
   Judged: Matches { issues: [1] }. The test at line 73 expects available('ZZ') to throw, and the code returns 0. The test failure is real, but the defect is in the test versus code mismatch, which stems from the planted silent-zero change at line 29. It points at the same error-as-success problem, so it matches issue 2 as the failing test exposes it.
2. Major in src/reservations.mjs: Returning 0 for an unknown SKU silently conflates 'no such product' with 'sold out', and reserve() then reports 'insufficient stock' for a typo'd SKU. Callers cannot distinguish the cases; throwing for unknown SKUs would be more accurate.
   Quoted:     if (!item) return 0;
   Judged: Matches { issues: [1] }. Names the cause precisely: returning 0 for an unknown SKU hides a bad SKU as sold out.
3. Major in src/reservations.mjs: confirm() decrements stock and deletes the hold as two separate store calls with no transaction. If the decrement succeeds and deleteHold fails, or the decrement throws, the state is inconsistent (hold still counted, or stock lost). Make the operation atomic in the store or order/compensate the writes.
   Quoted:     this.#store.decrementOnHand(hold.sku, hold.quantity);     this.#store.deleteHold(holdId);
   Judged: ValidExtra. Non-atomic decrement and delete in confirm() is a real risk, but it is not one of the planted issues and is not about the off-by-one or the silent zero return.
4. Minor in src/reservations.mjs: The boundary differs from available(), which treats a hold with expiresAt === cutoff as still active (`>=`). At exactly expiresAt, available() counts the hold while confirm() rejects it as expired. Use one consistent comparison for both.
   Quoted:     if (hold.expiresAt <= this.#now()) {       this.#store.deleteHold(holdId);
   Judged: Matches { issues: [0] }. Identifies the inconsistent expiry boundary between available() and confirm() at lines 60-61 and asks for one comparison. This points at issue 1 at the exact-expiry instant, though it does not name the cause at line 33 explicitly.
5. Major in src/reservations.mjs: The availability check and the saveHold call are not atomic. Two concurrent reserve() calls can both pass the check and oversell stock. Needs a store-level conditional write or a lock.
   Quoted:     if (this.available(sku) < quantity) {       throw new ReservationError(`insufficient stock for ${sku}`);     }     const hold = {
   Judged: ValidExtra. Check-then-write race in reserve() allowing oversell under concurrency is a real risk, not one of the planted issues.
6. Minor in src/reservations.mjs: release() does not check whether the hold is expired or belongs to the caller; any holder id can release any hold. There is no ownership check on customerId, which is accepted in reserve() but never stored-checked or used for authorization.
   Quoted:     if (!hold) return false;     this.#store.deleteHold(holdId);     return true;
   Judged: ValidExtra. Release without ownership check is a real authorization concern, not a planted issue. The claim that customerId is 'never stored-checked' is loosely stated but the authorization gap is real.
7. Nit in src/reservations.mjs: customerId is accepted without validation and no check is made that it is present. Missing or non-string values are stored unchecked in the hold.
   Quoted:   reserve(sku, quantity, customerId) {
   Judged: FalsePositive. Preference-level remark about validating customerId; a nit, not a real defect with consequences.
8. Minor in src/memory_store.mjs: holdsFor scans every hold in the store on each call, and available() calls it on every reserve, making reservation O(total holds). Acceptable for a test store, but the production store should index holds by SKU.
   Quoted:     return [...this.#holds.values()].filter((h) => h.sku === sku).map((h) => ({ ...h }));
   Judged: FalsePositive. Performance remark about full scan in a test store; a preference for a production concern, not a defect in this code.
9. Minor in src/memory_store.mjs: decrementOnHand throws a RangeError for a missing SKU, which the service does not translate into a ReservationError, so callers see an inconsistent error type from confirm().
   Quoted:     if (!item || item.onHand < quantity) {
   Judged: ValidExtra. Inconsistent error type from decrementOnHand for a missing SKU is a real, if minor, inconsistency in error handling, not one of the planted issues.
10. Nit in src/memory_store.mjs: Duplicate SKUs in initialItems silently overwrite the earlier entry instead of being rejected, which can hide configuration mistakes.
   Quoted:     this.#items.set(sku, { sku, onHand });
   Judged: ValidExtra. Duplicate SKUs silently overwriting in the constructor is a real configuration-hiding risk, though minor; not a planted issue.
11. Minor in test/reservations.test.mjs: Tests cover the expiry, overbook, confirm and release paths but never test concurrent reservations, the expiry boundary (expiresAt equal to now), or a reserve for a quantity exceeding stock with a live hold of zero. The boundary behavior that was found inconsistent is untested.
   Quoted:   expectThrows(() => service.reserve('A1', 0, 'cust-1'), /positive integer/, 'zero quantity');
   Judged: FalsePositive. Says the expiry boundary is untested, which is true of the test suite, but names no cause for issue 1 and mainly lists missing test coverage in general.

- planted-1 (correctness.off_by_one): the review HalfFound { finding: 3 }. Finding 4 notes the expiry boundary is inconsistent between available() (>=, counts hold at expiresAt===cutoff) and confirm() (<=, treats it as expired). It identifies the inconsistency at lines 60-61 and the boundary mismatch, but does not name the off-by-one at line 33 as the cause; it calls for one consistent comparison without saying the >= in available() keeps a lapsed hold blocking stock. Finding 11 mentions the boundary is untested but names no cause. The boundary mismatch is pointed at, so this is half_found.
- planted-2 (errors.error_as_success): the review Found { finding: 1 }. Finding 2 names the cause: returning 0 for an unknown SKU conflates 'no such product' with 'sold out', so callers cannot distinguish a typo'd SKU from empty stock, which matches the consequence in the key.

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
- key entry: claude-haiku-5-5
