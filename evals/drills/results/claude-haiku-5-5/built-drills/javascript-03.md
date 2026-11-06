# Session store with expiry and login throttling for user sessions (javascript)

- Domain: user sessions and login
- Built by: claude-haiku-5-5
- Runner: builtin-javascript
- Built in 83 seconds
- Patterns asked for: correctness.boundary_and_empty_input, errors.overly_broad_catch, concurrency.lock_ordering

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 0
- Dropped: 1
- Found in review: 2

## The code

### src/session_store.mjs

```text
 1 | const DEFAULT_TTL_MS = 30 * 60 * 1000;
 2 | 
 3 | export class SessionStore {
 4 |   #sessions = new Map();
 5 |   #clock;
 6 |   #ttlMs;
 7 |   #randomId;
 8 | 
 9 |   constructor({ clock, randomId, ttlMs = DEFAULT_TTL_MS }) {
10 |     if (typeof clock !== 'function') throw new TypeError('clock must be a function');
11 |     if (typeof randomId !== 'function') throw new TypeError('randomId must be a function');
12 |     if (!Number.isFinite(ttlMs) || ttlMs <= 0) throw new RangeError('ttlMs must be a positive number');
13 |     this.#clock = clock;
14 |     this.#randomId = randomId;
15 |     this.#ttlMs = ttlMs;
16 |   }
17 | 
18 |   create(userId) {
19 |     if (typeof userId !== 'string' || userId.length === 0) {
20 |       throw new TypeError('userId must be a non-empty string');
21 |     }
22 |     const id = this.#randomId();
23 |     if (this.#sessions.has(id)) throw new Error('session id collision');
24 |     if (typeof id !== 'string' || id.length < 16) {
25 |       throw new Error('randomId must produce at least 16 characters');
26 |     }
27 |     const now = this.#clock();
28 |     this.#sessions.set(id, { userId, expiresAt: now + this.#ttlMs });
29 |     return id;
30 |   }
31 | 
32 |   // Looking a session up also slides its expiry forward, so an active user
33 |   // is not logged out mid-task. Expired entries are dropped on access.
34 |   resolve(id) {
35 |     const session = this.#sessions.get(id);
36 |     if (!session) return null;
37 |     const now = this.#clock();
38 |     if (now > session.expiresAt) {
39 |       this.#sessions.delete(id);
40 |       return null;
41 |     }
42 |     session.expiresAt = now + this.#ttlMs;
43 |     return session.userId;
44 |   }
45 | 
46 |   revoke(id) {
47 |     return this.#sessions.delete(id);
48 |   }
49 | 
50 |   revokeAllForUser(userId) {
51 |     let count = 0;
52 |     for (const [id, session] of this.#sessions) {
53 |       if (session.userId === userId) {
54 |         this.#sessions.delete(id);
55 |         count += 1;
56 |       }
57 |     }
58 |     return count;
59 |   }
60 | }
```

### src/login_throttle.mjs

```text
 1 | export class LoginThrottle {
 2 |   #attempts = new Map();
 3 |   #clock;
 4 |   #maxFailures;
 5 |   #windowMs;
 6 | 
 7 |   constructor({ clock, maxFailures = 5, windowMs = 15 * 60 * 1000 }) {
 8 |     if (typeof clock !== 'function') throw new TypeError('clock must be a function');
 9 |     if (!Number.isInteger(maxFailures) || maxFailures < 1) {
10 |       throw new RangeError('maxFailures must be a positive integer');
11 |     }
12 |     if (!Number.isFinite(windowMs) || windowMs <= 0) {
13 |       throw new RangeError('windowMs must be a positive number');
14 |     }
15 |     this.#clock = clock;
16 |     this.#maxFailures = maxFailures;
17 |     this.#windowMs = windowMs;
18 |   }
19 | 
20 |   // Returns true when the account may attempt another login right now.
21 |   canAttempt(username) {
22 |     const record = this.#attempts.get(username);
23 |     if (!record) return true;
24 |     if (this.#clock() - record.windowStart >= this.#windowMs) {
25 |       this.#attempts.delete(username);
26 |       return true;
27 |     }
28 |     return record.failures < this.#maxFailures;
29 |   }
30 | 
31 |   recordFailure(username) {
32 |     const now = this.#clock();
33 |     const record = this.#attempts.get(username);
34 |     if (!record || now - record.windowStart >= this.#windowMs) {
35 |       this.#attempts.set(username, { windowStart: now, failures: 1 });
36 |       return;
37 |     }
38 |     record.failures += 1;
39 |   }
40 | 
41 |   // A successful login clears the counter so a user who typed a password
42 |   // correctly is not penalised for earlier typos.
43 |   recordSuccess(username) {
44 |     this.#attempts.delete(username);
45 |   }
46 | }
```

### src/login_service.mjs

```text
 1 | export class LoginService {
 2 |   #users;
 3 |   #throttle;
 4 |   #sessions;
 5 |   #verifyPassword;
 6 | 
 7 |   constructor({ users, throttle, sessions, verifyPassword }) {
 8 |     this.#users = users;
 9 |     this.#throttle = throttle;
10 |     this.#sessions = sessions;
11 |     this.#verifyPassword = verifyPassword;
12 |   }
13 | 
14 |   // Returns a session id on success, or a reason code. The reason is
15 |   // deliberately identical for unknown users and wrong passwords so the
16 |   // response does not reveal which accounts exist.
17 |   login(username, password) {
18 |     if (typeof username !== 'string' || typeof password !== 'string') {
19 |       return { ok: false, reason: 'invalid_input' };
20 |     }
21 |     if (!this.#throttle.canAttempt(username)) {
22 |       return { ok: false, reason: 'throttled' };
23 |     }
24 |     const user = this.#users.find(username);
25 |     let passwordOk = false;
26 |     try {
27 |       passwordOk = this.#verifyPassword(password, user ? user.passwordHash : null);
28 |     } catch (e) {
29 |       passwordOk = false;
30 |     }
31 |     if (!user || !passwordOk) {
32 |       this.#throttle.recordFailure(username);
33 |       return { ok: false, reason: 'bad_credentials' };
34 |     }
35 |     this.#throttle.recordSuccess(username);
36 |     return { ok: true, sessionId: this.#sessions.create(user.id) };
37 |   }
38 | }
```

### tests/session_flow_test.mjs

```text
 1 | import { SessionStore } from '../src/session_store.mjs';
 2 | import { LoginThrottle } from '../src/login_throttle.mjs';
 3 | import { LoginService } from '../src/login_service.mjs';
 4 | 
 5 | function assert(cond, msg) {
 6 |   if (!cond) throw new Error('assertion failed: ' + msg);
 7 | }
 8 | 
 9 | function makeClock(start = 0) {
10 |   let t = start;
11 |   return { now: () => t, advance: (ms) => { t += ms; } };
12 | }
13 | 
14 | let counter = 0;
15 | const randomId = () => 'session-id-' + String(++counter).padStart(8, '0');
16 | 
17 | // Sessions: create, resolve, slide, expire, revoke
18 | {
19 |   const clock = makeClock(1000);
20 |   const store = new SessionStore({ clock: clock.now, randomId, ttlMs: 100 });
21 |   const id = store.create('alice');
22 |   assert(store.resolve(id) === 'alice', 'resolves live session');
23 |   clock.advance(60);
24 |   assert(store.resolve(id) === 'alice', 'slides expiry on access');
25 |   clock.advance(60);
26 |   assert(store.resolve(id) === 'alice', 'slid session still live');
27 |   clock.advance(100);
28 |   assert(store.resolve(id) === null, 'expires after ttl without access');
29 |   const id2 = store.create('bob');
30 |   assert(store.revoke(id2) === true, 'revoke returns true');
31 |   assert(store.resolve(id2) === null, 'revoked session gone');
32 |   assert(store.revoke(id2) === false, 'second revoke returns false');
33 |   const a = store.create('carol');
34 |   const b = store.create('carol');
35 |   store.create('dave');
36 |   assert(store.revokeAllForUser('carol') === 2, 'revokes all for user');
37 |   assert(store.resolve(a) === null && store.resolve(b) === null, 'user sessions removed');
38 | }
39 | 
40 | // SessionStore rejects short ids and bad input
41 | {
42 |   const clock = makeClock();
43 |   const store = new SessionStore({ clock: clock.now, randomId: () => 'short' });
44 |   let threw = false;
45 |   try { store.create('alice'); } catch (e) { threw = e instanceof Error; }
46 |   assert(threw, 'short id rejected');
47 |   let typeErr = false;
48 |   try { new SessionStore({ clock: clock.now, randomId: 5 }); } catch (e) { typeErr = e instanceof TypeError; }
49 |   assert(typeErr, 'non-function randomId rejected');
50 | }
51 | 
52 | // Throttle: lock after max failures, unlock after window, success resets
53 | {
54 |   const clock = makeClock();
55 |   const throttle = new LoginThrottle({ clock: clock.now, maxFailures: 2, windowMs: 1000 });
56 |   assert(throttle.canAttempt('eve'), 'fresh user may attempt');
57 |   throttle.recordFailure('eve');
58 |   assert(throttle.canAttempt('eve'), 'one failure allowed');
59 |   throttle.recordFailure('eve');
60 |   assert(!throttle.canAttempt('eve'), 'locked after max failures');
61 |   clock.advance(1000);
62 |   assert(throttle.canAttempt('eve'), 'unlocks when window passes');
63 |   throttle.recordFailure('eve');
64 |   throttle.recordSuccess('eve');
65 |   throttle.recordFailure('eve');
66 |   assert(throttle.canAttempt('eve'), 'success clears earlier failures');
67 | }
68 | 
69 | // LoginService: end to end behaviour
70 | {
71 |   const clock = makeClock();
72 |   const sessions = new SessionStore({ clock: clock.now, randomId, ttlMs: 5000 });
73 |   const throttle = new LoginThrottle({ clock: clock.now, maxFailures: 2, windowMs: 10000 });
74 |   const users = {
75 |     find: (name) => (name === 'alice' ? { id: 'u-alice', passwordHash: 'h:pw1' } : undefined),
76 |   };
77 |   const verifyPassword = (pw, hash) => hash === 'h:' + pw;
78 |   const service = new LoginService({ users, throttle, sessions, verifyPassword });
79 | 
80 |   assert(service.login(42, 'x').reason === 'invalid_input', 'non-string input rejected');
81 |   const bad = service.login('mallory', 'nope');
82 |   assert(!bad.ok && bad.reason === 'bad_credentials', 'unknown user is bad credentials');
83 |   assert(service.login('alice', 'wrong').reason === 'bad_credentials', 'wrong password is bad credentials');
84 |   assert(service.login('alice', 'wrong').reason === 'bad_credentials', 'second failure still reported as credentials');
85 |   assert(service.login('alice', 'pw1').reason === 'throttled', 'locked out after repeated failures');
86 |   clock.advance(10000);
87 |   const ok = service.login('alice', 'pw1');
88 |   assert(ok.ok === true, 'login succeeds after window');
89 |   assert(sessions.resolve(ok.sessionId) === 'u-alice', 'session maps to user');
90 | }
91 | 
92 | console.log('all checks passed');
```

## The key

### 1. Unhandled boundary or empty input (correctness.boundary_and_empty_input), Minor, planted, src/session_store.mjs lines 38 to 38

- Consequence: A session is still honoured for one extra clock tick at exactly its expiry time, so a session can resolve after the TTL has fully elapsed.
- In an interview: The expiry check uses a strict comparison, so a session is treated as live at the exact instant it should expire. The boundary should be inclusive (now >= expiresAt), matching the throttle window check, so a session never outlives its TTL.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { SessionStore } from './src/session_store.mjs';
let t = 0;
const s = new SessionStore({ clock: () => t, randomId: () => 'abcdefghijklmnop1', ttlMs: 100 });
const id = s.create('alice');
t = 100;
if (s.resolve(id) !== null) throw new Error('session must expire at exactly ttl');
console.log('ok');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:6:39)"

The fix, in src/session_store.mjs:

```diff
-    if (now > session.expiresAt) {
+    if (now >= session.expiresAt) {
```

### 2. Overly broad catch (errors.overly_broad_catch), Major, planted, src/login_service.mjs lines 25 to 30

- Consequence: When the password hashing backend fails (outage, misconfiguration, a bug in the verifier), every login is reported as bad_credentials instead of surfacing a server error, hiding the incident and misleading users into retrying wrong passwords that trip the throttle.
- In an interview: The catch swallows every exception from the password verifier, including programming errors and infrastructure failures, and turns them into a normal credential failure. It should be removed or narrowed to the specific expected error so genuine faults propagate and get logged.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { SessionStore } from './src/session_store.mjs';
import { LoginThrottle } from './src/login_throttle.mjs';
import { LoginService } from './src/login_service.mjs';
let t = 0;
const users = { find: (u) => (u === 'alice' ? { id: 'a1', passwordHash: 'h' } : undefined) };
const svc = new LoginService({ users, throttle: new LoginThrottle({ clock: () => t }), sessions: new SessionStore({ clock: () => t, randomId: () => 'abcdefghijklmnop1' }), verifyPassword: () => { throw new Error('hash backend down'); } });
let threw = false;
try { svc.login('alice', 'pw'); } catch (e) { threw = true; }
if (!threw) throw new Error('verifier failure must propagate');
console.log('ok');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:9:23)"

The fix, in src/login_service.mjs:

```diff
-    let passwordOk = false;
-    try {
-      passwordOk = this.#verifyPassword(password, user ? user.passwordHash : null);
-    } catch (e) {
-      passwordOk = false;
-    }
+    const passwordOk = this.#verifyPassword(password, user ? user.passwordHash : null);
```

### 3. Unbounded growth (resources.unbounded_growth), Major, found in review, src/session_store.mjs lines 27 to 28

- Consequence: Every session created and then abandoned without being resolved again stays in the Map after it expires. In a long-running service, memory climbs with each login until the process is killed or degrades.
- In an interview: Expiry is only enforced lazily in resolve, so sessions that are never touched again are never removed and the Map grows with every login. Sweeping expired entries on create bounds the map to the live sessions, since deleting from a Map while iterating it is safe in JavaScript.

Proven by the judge: Expired sessions that are never resolved again remain in the Map forever, a real unbounded memory growth risk the key does not hold.

The fix, in src/session_store.mjs:

```diff
     const now = this.#clock();
+    for (const [sid, s] of this.#sessions) {
+      if (now > s.expiresAt) this.#sessions.delete(sid);
+    }
     this.#sessions.set(id, { userId, expiresAt: now + this.#ttlMs });
```

### 4. Unbounded growth (resources.unbounded_growth), Major, found in review, src/login_throttle.mjs lines 31 to 32

- Consequence: Every distinct username that fails a login adds an entry that is only removed when that same username is looked up again or succeeds. An attacker who submits failures across many made-up usernames grows the Map without limit, so memory climbs until the process is killed. The window expiry is never applied to entries nobody revisits.
- In an interview: Throttle entries are only removed when their own username is touched again, so a stream of distinct usernames makes the Map grow without bound. Sweeping expired windows whenever a failure is recorded keeps memory proportional to the failures seen within one window, though a periodic timer or a size cap would scale better under heavy attack.

Proven by the judge: Throttle records keyed by username are never swept, so distinct usernames grow the Map without bound. This is a real memory risk.

The fix, in src/login_throttle.mjs:

```diff
   recordFailure(username) {
     const now = this.#clock();
+    for (const [name, rec] of this.#attempts) {
+      if (now - rec.windowStart >= this.#windowMs) this.#attempts.delete(name);
+    }
```

## Dropped

- concurrency.lock_ordering: The edit only reorders two calls within login() so session creation happens before the throttle reset. Neither the session store nor the throttle uses any lock; both are plain synchronous Map-based objects in a single-threaded JavaScript runtime, and no lock acquisition exists in either the original or edited code. Therefore no lock ordering problem is introduced, and the original also has no locks, so the claimed deadlock cannot occur in either version.
  Consequence claimed: If the session store and throttle were guarded by separate locks, a login that takes them in the opposite order to revocation or lockout paths could deadlock under concurrent requests, stalling logins until the process is restarted.

## The clean code

### src/session_store.mjs

```text
 1 | const DEFAULT_TTL_MS = 30 * 60 * 1000;
 2 | 
 3 | export class SessionStore {
 4 |   #sessions = new Map();
 5 |   #clock;
 6 |   #ttlMs;
 7 |   #randomId;
 8 | 
 9 |   constructor({ clock, randomId, ttlMs = DEFAULT_TTL_MS }) {
10 |     if (typeof clock !== 'function') throw new TypeError('clock must be a function');
11 |     if (typeof randomId !== 'function') throw new TypeError('randomId must be a function');
12 |     if (!Number.isFinite(ttlMs) || ttlMs <= 0) throw new RangeError('ttlMs must be a positive number');
13 |     this.#clock = clock;
14 |     this.#randomId = randomId;
15 |     this.#ttlMs = ttlMs;
16 |   }
17 | 
18 |   create(userId) {
19 |     if (typeof userId !== 'string' || userId.length === 0) {
20 |       throw new TypeError('userId must be a non-empty string');
21 |     }
22 |     const id = this.#randomId();
23 |     if (this.#sessions.has(id)) throw new Error('session id collision');
24 |     if (typeof id !== 'string' || id.length < 16) {
25 |       throw new Error('randomId must produce at least 16 characters');
26 |     }
27 |     const now = this.#clock();
28 |     this.#sessions.set(id, { userId, expiresAt: now + this.#ttlMs });
29 |     return id;
30 |   }
31 | 
32 |   // Looking a session up also slides its expiry forward, so an active user
33 |   // is not logged out mid-task. Expired entries are dropped on access.
34 |   resolve(id) {
35 |     const session = this.#sessions.get(id);
36 |     if (!session) return null;
37 |     const now = this.#clock();
38 |     if (now >= session.expiresAt) {
39 |       this.#sessions.delete(id);
40 |       return null;
41 |     }
42 |     session.expiresAt = now + this.#ttlMs;
43 |     return session.userId;
44 |   }
45 | 
46 |   revoke(id) {
47 |     return this.#sessions.delete(id);
48 |   }
49 | 
50 |   revokeAllForUser(userId) {
51 |     let count = 0;
52 |     for (const [id, session] of this.#sessions) {
53 |       if (session.userId === userId) {
54 |         this.#sessions.delete(id);
55 |         count += 1;
56 |       }
57 |     }
58 |     return count;
59 |   }
60 | }
```

### src/login_throttle.mjs

```text
 1 | export class LoginThrottle {
 2 |   #attempts = new Map();
 3 |   #clock;
 4 |   #maxFailures;
 5 |   #windowMs;
 6 | 
 7 |   constructor({ clock, maxFailures = 5, windowMs = 15 * 60 * 1000 }) {
 8 |     if (typeof clock !== 'function') throw new TypeError('clock must be a function');
 9 |     if (!Number.isInteger(maxFailures) || maxFailures < 1) {
10 |       throw new RangeError('maxFailures must be a positive integer');
11 |     }
12 |     if (!Number.isFinite(windowMs) || windowMs <= 0) {
13 |       throw new RangeError('windowMs must be a positive number');
14 |     }
15 |     this.#clock = clock;
16 |     this.#maxFailures = maxFailures;
17 |     this.#windowMs = windowMs;
18 |   }
19 | 
20 |   // Returns true when the account may attempt another login right now.
21 |   canAttempt(username) {
22 |     const record = this.#attempts.get(username);
23 |     if (!record) return true;
24 |     if (this.#clock() - record.windowStart >= this.#windowMs) {
25 |       this.#attempts.delete(username);
26 |       return true;
27 |     }
28 |     return record.failures < this.#maxFailures;
29 |   }
30 | 
31 |   recordFailure(username) {
32 |     const now = this.#clock();
33 |     const record = this.#attempts.get(username);
34 |     if (!record || now - record.windowStart >= this.#windowMs) {
35 |       this.#attempts.set(username, { windowStart: now, failures: 1 });
36 |       return;
37 |     }
38 |     record.failures += 1;
39 |   }
40 | 
41 |   // A successful login clears the counter so a user who typed a password
42 |   // correctly is not penalised for earlier typos.
43 |   recordSuccess(username) {
44 |     this.#attempts.delete(username);
45 |   }
46 | }
```

### src/login_service.mjs

```text
 1 | export class LoginService {
 2 |   #users;
 3 |   #throttle;
 4 |   #sessions;
 5 |   #verifyPassword;
 6 | 
 7 |   constructor({ users, throttle, sessions, verifyPassword }) {
 8 |     this.#users = users;
 9 |     this.#throttle = throttle;
10 |     this.#sessions = sessions;
11 |     this.#verifyPassword = verifyPassword;
12 |   }
13 | 
14 |   // Returns a session id on success, or a reason code. The reason is
15 |   // deliberately identical for unknown users and wrong passwords so the
16 |   // response does not reveal which accounts exist.
17 |   login(username, password) {
18 |     if (typeof username !== 'string' || typeof password !== 'string') {
19 |       return { ok: false, reason: 'invalid_input' };
20 |     }
21 |     if (!this.#throttle.canAttempt(username)) {
22 |       return { ok: false, reason: 'throttled' };
23 |     }
24 |     const user = this.#users.find(username);
25 |     const passwordOk = this.#verifyPassword(password, user ? user.passwordHash : null);
26 |     if (!user || !passwordOk) {
27 |       this.#throttle.recordFailure(username);
28 |       return { ok: false, reason: 'bad_credentials' };
29 |     }
30 |     this.#throttle.recordSuccess(username);
31 |     return { ok: true, sessionId: this.#sessions.create(user.id) };
32 |   }
33 | }
```

### tests/session_flow_test.mjs

```text
 1 | import { SessionStore } from '../src/session_store.mjs';
 2 | import { LoginThrottle } from '../src/login_throttle.mjs';
 3 | import { LoginService } from '../src/login_service.mjs';
 4 | 
 5 | function assert(cond, msg) {
 6 |   if (!cond) throw new Error('assertion failed: ' + msg);
 7 | }
 8 | 
 9 | function makeClock(start = 0) {
10 |   let t = start;
11 |   return { now: () => t, advance: (ms) => { t += ms; } };
12 | }
13 | 
14 | let counter = 0;
15 | const randomId = () => 'session-id-' + String(++counter).padStart(8, '0');
16 | 
17 | // Sessions: create, resolve, slide, expire, revoke
18 | {
19 |   const clock = makeClock(1000);
20 |   const store = new SessionStore({ clock: clock.now, randomId, ttlMs: 100 });
21 |   const id = store.create('alice');
22 |   assert(store.resolve(id) === 'alice', 'resolves live session');
23 |   clock.advance(60);
24 |   assert(store.resolve(id) === 'alice', 'slides expiry on access');
25 |   clock.advance(60);
26 |   assert(store.resolve(id) === 'alice', 'slid session still live');
27 |   clock.advance(100);
28 |   assert(store.resolve(id) === null, 'expires after ttl without access');
29 |   const id2 = store.create('bob');
30 |   assert(store.revoke(id2) === true, 'revoke returns true');
31 |   assert(store.resolve(id2) === null, 'revoked session gone');
32 |   assert(store.revoke(id2) === false, 'second revoke returns false');
33 |   const a = store.create('carol');
34 |   const b = store.create('carol');
35 |   store.create('dave');
36 |   assert(store.revokeAllForUser('carol') === 2, 'revokes all for user');
37 |   assert(store.resolve(a) === null && store.resolve(b) === null, 'user sessions removed');
38 | }
39 | 
40 | // SessionStore rejects short ids and bad input
41 | {
42 |   const clock = makeClock();
43 |   const store = new SessionStore({ clock: clock.now, randomId: () => 'short' });
44 |   let threw = false;
45 |   try { store.create('alice'); } catch (e) { threw = e instanceof Error; }
46 |   assert(threw, 'short id rejected');
47 |   let typeErr = false;
48 |   try { new SessionStore({ clock: clock.now, randomId: 5 }); } catch (e) { typeErr = e instanceof TypeError; }
49 |   assert(typeErr, 'non-function randomId rejected');
50 | }
51 | 
52 | // Throttle: lock after max failures, unlock after window, success resets
53 | {
54 |   const clock = makeClock();
55 |   const throttle = new LoginThrottle({ clock: clock.now, maxFailures: 2, windowMs: 1000 });
56 |   assert(throttle.canAttempt('eve'), 'fresh user may attempt');
57 |   throttle.recordFailure('eve');
58 |   assert(throttle.canAttempt('eve'), 'one failure allowed');
59 |   throttle.recordFailure('eve');
60 |   assert(!throttle.canAttempt('eve'), 'locked after max failures');
61 |   clock.advance(1000);
62 |   assert(throttle.canAttempt('eve'), 'unlocks when window passes');
63 |   throttle.recordFailure('eve');
64 |   throttle.recordSuccess('eve');
65 |   throttle.recordFailure('eve');
66 |   assert(throttle.canAttempt('eve'), 'success clears earlier failures');
67 | }
68 | 
69 | // LoginService: end to end behaviour
70 | {
71 |   const clock = makeClock();
72 |   const sessions = new SessionStore({ clock: clock.now, randomId, ttlMs: 5000 });
73 |   const throttle = new LoginThrottle({ clock: clock.now, maxFailures: 2, windowMs: 10000 });
74 |   const users = {
75 |     find: (name) => (name === 'alice' ? { id: 'u-alice', passwordHash: 'h:pw1' } : undefined),
76 |   };
77 |   const verifyPassword = (pw, hash) => hash === 'h:' + pw;
78 |   const service = new LoginService({ users, throttle, sessions, verifyPassword });
79 | 
80 |   assert(service.login(42, 'x').reason === 'invalid_input', 'non-string input rejected');
81 |   const bad = service.login('mallory', 'nope');
82 |   assert(!bad.ok && bad.reason === 'bad_credentials', 'unknown user is bad credentials');
83 |   assert(service.login('alice', 'wrong').reason === 'bad_credentials', 'wrong password is bad credentials');
84 |   assert(service.login('alice', 'wrong').reason === 'bad_credentials', 'second failure still reported as credentials');
85 |   assert(service.login('alice', 'pw1').reason === 'throttled', 'locked out after repeated failures');
86 |   clock.advance(10000);
87 |   const ok = service.login('alice', 'pw1');
88 |   assert(ok.ok === true, 'login succeeds after window');
89 |   assert(sessions.resolve(ok.sessionId) === 'u-alice', 'session maps to user');
90 | }
91 | 
92 | console.log('all checks passed');
```

### The hidden suite

```text
import { SessionStore } from './src/session_store.mjs';
import { LoginThrottle } from './src/login_throttle.mjs';
import { LoginService } from './src/login_service.mjs';

function assert(cond, msg) {
  if (!cond) throw new Error('assertion failed: ' + msg);
}

let t = 0;
const clock = () => t;
let n = 0;
const randomId = () => 'abcdefghijklmnop' + String(++n);

const store = new SessionStore({ clock, randomId, ttlMs: 100 });
const sid = store.create('alice');
assert(store.resolve(sid) === 'alice', 'live session resolves');
t = 100;
assert(store.resolve(sid) === null, 'expired session is null');
assert(store.resolve('missing') === null, 'unknown id is null');

let rejected = false;
try { store.create(''); } catch (e) { rejected = e instanceof TypeError; }
assert(rejected, 'empty userId rejected');

t = 0;
const throttle = new LoginThrottle({ clock, maxFailures: 3, windowMs: 50 });
throttle.recordFailure('bob');
throttle.recordFailure('bob');
throttle.recordFailure('bob');
assert(!throttle.canAttempt('bob'), 'throttle locks at max');
t = 50;
assert(throttle.canAttempt('bob'), 'throttle window resets');

const users = { find: (u) => (u === 'carl' ? { id: 'c1', passwordHash: 'ok' } : undefined) };
const svc = new LoginService({
  users,
  throttle: new LoginThrottle({ clock, maxFailures: 5, windowMs: 1000 }),
  sessions: new SessionStore({ clock, randomId, ttlMs: 1000 }),
  verifyPassword: (p, h) => p === h,
});
assert(svc.login('carl', 'nope').reason === 'bad_credentials', 'bad password rejected');
const res = svc.login('carl', 'ok');
assert(res.ok === true && typeof res.sessionId === 'string', 'good login returns session');
assert(svc.login(null, 'ok').reason === 'invalid_input', 'invalid input rejected');
console.log('suite passed');
```

## The clean review

- Minor: Login timing side channel: for an unknown username the code skips verifyPassword entirely, so the response returns much faster than for a known user with a wrong password. This leaks account existence, which the comment says the design avoids. Always run the password check, using a null hash for unknown users.
- Minor: create() does not check whether the generated id is already in use. A collision silently overwrites another user's live session, which could hand one user's session to another.

## The blind review

1. Minor in src/session_store.mjs: The collision check runs before the type and length check, so a non-string id reaches Map.has and the validation order is muddled. Validate the id first, then check for collisions. Also, a collision throws instead of retrying, which is an odd contract for a random id source; either document it or regenerate.
   Quoted:     const id = this.#randomId();     if (this.#sessions.has(id)) throw new Error('session id collision');     if (typeof id !== 'string' || id.length < 16) {
   Judged: FalsePositive. The id validation order complaint and the collision-throws contract are design or style remarks, not a real defect in the sound code.
2. Major in src/session_store.mjs: Expired sessions are only removed when they are accessed. Sessions that are never resolved again stay in the Map forever, so memory grows without bound. Add a periodic sweep or prune on create.
   Quoted:   resolve(id) {     const session = this.#sessions.get(id);
   Judged: ValidExtra. Expired sessions that are never resolved again remain in the Map forever, a real unbounded memory growth risk the key does not hold.
3. Minor in src/session_store.mjs: This scans every session in the store on each call, which is O(n) per revocation. Keep a per-user index of session ids if this runs on hot paths such as password changes.
   Quoted:   revokeAllForUser(userId) {     let count = 0;     for (const [id, session] of this.#sessions) {
   Judged: FalsePositive. O(n) scan per revocation is a performance preference, not a defect in the code as written.
4. Major in src/login_throttle.mjs: Throttle state is keyed only by username and never evicted except on access or success. Attempts against many distinct usernames grow the Map without bound, which is a memory-exhaustion risk. Add expiry sweeping.
   Quoted:   canAttempt(username) {     const record = this.#attempts.get(username);
   Judged: ValidExtra. Throttle records keyed by username are never swept, so distinct usernames grow the Map without bound. This is a real memory risk.
5. Nit in src/login_throttle.mjs: The window is measured from the first failure and never resets while locked, which is intended, but canAttempt and recordFailure use different comparisons with the clock. Keep the window logic in one helper so the two cannot drift apart.
   Quoted:     if (this.#clock() - record.windowStart >= this.#windowMs) {
   Judged: FalsePositive. Suggesting a shared helper for the window comparison is a style remark; the two comparisons are equivalent.
6. Major in src/login_service.mjs: The catch silently swallows every error from the password verifier, including infrastructure failures such as a broken hash backend, so they are reported as bad credentials and hidden from operators. Log or rethrow unexpected errors, and keep only the expected mismatch path.
   Quoted:     const user = this.#users.find(username);     let passwordOk = false;     try {       passwordOk = this.#verifyPassword(password, user ? user.passwordHash : null);     } catch (e) {       passwordOk = false;     }
   Judged: Matches { issues: [1] }. Names the swallowed verifier errors and their hiding from operators, which is the cause of issue 1.
7. Major in src/login_service.mjs: When the user does not exist, verifyPassword is called with null, which may return quickly. That creates a timing difference between unknown users and wrong passwords, which undermines the stated goal of not revealing which accounts exist. Run a dummy hash comparison for unknown users.
   Quoted:     if (!user || !passwordOk) {       this.#throttle.recordFailure(username);
   Judged: FalsePositive. Timing difference for unknown users is a plausible enumeration concern, but the code calls the verifier with null for unknown users; this is a speculative remark and not the planted issue.
8. Nit in src/login_service.mjs: Session creation is not guarded and no session-fixation or rotation step is shown. Acceptable here, but the throttle state and session creation are not atomic with respect to concurrent logins. Document the assumption.
   Quoted:     return { ok: true, sessionId: this.#sessions.create(user.id) };
   Judged: FalsePositive. Concurrency and session fixation remarks are a documentation preference, not a defect in this single-threaded code.
9. Major in tests/session_flow_test.mjs: This test only covers the lockout path through a fake verifier. It never checks that an unknown user and a wrong password produce identical responses and timing, so the enumeration guarantee the comment promises is untested. It also never checks that a verifier exception is handled as intended.
   Quoted:   assert(service.login('alice', 'pw1').reason === 'throttled', 'locked out after repeated failures');
   Judged: FalsePositive. Test coverage gaps for enumeration timing and verifier exceptions are a remark about tests, not a defect in the code under review.
10. Minor in tests/session_flow_test.mjs: The test asserts that a short id throws, but the check is the generic 'threw' flag with no message check, so any unrelated error would pass. Assert the specific error type and message.
   Quoted:   const store = new SessionStore({ clock: clock.now, randomId: () => 'short' });
   Judged: FalsePositive. Test assertion strictness is a test-quality preference, not a code defect.
11. Nit in tests/session_flow_test.mjs: The module-level counter is shared across all test blocks and is never reset, so tests depend on execution order. Create a fresh generator per block so each test is self-contained.
   Quoted: const randomId = () => 'session-id-' + String(++counter).padStart(8, '0');
   Judged: FalsePositive. Shared module-level counter across test blocks is a test style preference and does not cause failures here.

- planted-1 (correctness.boundary_and_empty_input): the review Missed. The issue is the off-by-one at the expiry boundary (now > expiresAt instead of >=). No finding mentions the exact-expiry tick; finding 2 concerns unbounded memory from never-resolved sessions, and finding 10 and others are unrelated.
- planted-2 (errors.overly_broad_catch): the review Found { finding: 5 }. Finding 6 names the catch swallowing every verifier error, including infrastructure failures, reported as bad credentials and hidden from operators. That is the cause the issue describes.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
