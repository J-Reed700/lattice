# User session management and login handling (typescript)

- Domain: user sessions and login
- Built by: claude-haiku-5-5
- Runner: none, every defect was judged
- Built in 114 seconds
- Patterns asked for: correctness.boundary_and_empty_input, errors.overly_broad_catch, concurrency.lock_ordering

## Counts

- Planted: 3
- Proven by a test: 0
- Proven by the judge: 3
- Dropped: 0
- Found in review: 5

## The code

### src/auth/sessionStore.ts

```text
 1 | import { randomBytes, createHash, timingSafeEqual } from 'node:crypto';
 2 | 
 3 | export interface Session {
 4 |   id: string;
 5 |   userId: string;
 6 |   createdAt: number;
 7 |   expiresAt: number;
 8 |   revoked: boolean;
 9 | }
10 | 
11 | const DEFAULT_TTL_MS = 30 * 60 * 1000;
12 | const MAX_SESSIONS_PER_USER = 5;
13 | 
14 | function hashToken(token: string): string {
15 |   return createHash('sha256').update(token).digest('hex');
16 | }
17 | 
18 | export class SessionStore {
19 |   private readonly byHash = new Map<string, Session>();
20 |   private readonly byUser = new Map<string, string[]>();
21 | 
22 |   constructor(private readonly ttlMs: number = DEFAULT_TTL_MS, private readonly now: () => number = Date.now) {}
23 | 
24 |   create(userId: string): { token: string; session: Session } {
25 |     const token = randomBytes(32).toString('base64url');
26 |     const hash = hashToken(token);
27 |     const createdAt = this.now();
28 |     const session: Session = { id: hash.slice(0, 12), userId, createdAt, expiresAt: createdAt + this.ttlMs, revoked: false };
29 | 
30 |     const ids = this.byUser.get(userId) ?? [];
31 |     ids.push(hash);
32 |     // Evict the oldest sessions so a single account cannot accumulate unbounded state.
33 |     while (ids.length > MAX_SESSIONS_PER_USER) {
34 |       const oldest = ids.shift();
35 |       if (oldest !== undefined) this.byHash.delete(oldest);
36 |     }
37 |     this.byUser.set(userId, ids);
38 |     this.byHash.set(hash, session);
39 |     return { token, session };
40 |   }
41 | 
42 |   validate(token: string): Session | null {
43 |     if (token.length === 0) return null;
44 |     const hash = hashToken(token);
45 |     const session = this.byHash.get(hash);
46 |     if (!session || session.revoked) return null;
47 |     if (this.now() > session.expiresAt) {
48 |       this.revokeHash(hash, session);
49 |       return null;
50 |     }
51 |     return session;
52 |   }
53 | 
54 |   revokeAllForUser(userId: string): number {
55 |     const ids = this.byUser.get(userId) ?? [];
56 |     let count = 0;
57 |     for (const hash of ids) {
58 |       const session = this.byHash.get(hash);
59 |       if (session && !session.revoked) {
60 |         session.revoked = true;
61 |         count++;
62 |       }
63 |     }
64 |     this.byUser.delete(userId);
65 |     return count;
66 |   }
67 | 
68 |   private revokeHash(hash: string, session: Session): void {
69 |     session.revoked = true;
70 |     this.byHash.delete(hash);
71 |   }
72 | }
73 | 
74 | export function tokensMatch(a: string, b: string): boolean {
75 |   const left = Buffer.from(a);
76 |   const right = Buffer.from(b);
77 |   if (left.length !== right.length) return false;
78 |   return timingSafeEqual(left, right);
79 | }
```

### src/auth/login.ts

```text
 1 | import { SessionStore } from './sessionStore';
 2 | 
 3 | export interface UserRecord {
 4 |   id: string;
 5 |   email: string;
 6 |   passwordHash: string;
 7 | }
 8 | 
 9 | export interface PasswordVerifier {
10 |   verify(password: string, hash: string): Promise<boolean>;
11 | }
12 | 
13 | export type LoginResult =
14 |   | { ok: true; token: string; userId: string }
15 |   | { ok: false; reason: 'invalid_input' | 'bad_credentials' | 'rate_limited' };
16 | 
17 | const MAX_FAILURES = 5;
18 | const LOCKOUT_MS = 15 * 60 * 1000;
19 | 
20 | export class LoginService {
21 |   private readonly failures = new Map<string, { count: number; lockedUntil: number }>();
22 | 
23 |   constructor(
24 |     private readonly users: { findByEmail(email: string): Promise<UserRecord | null> },
25 |     private readonly verifier: PasswordVerifier,
26 |     private readonly sessions: SessionStore,
27 |     private readonly now: () => number = Date.now,
28 |   ) {}
29 | 
30 |   async login(email: string, password: string): Promise<LoginResult> {
31 |     const normalized = email.trim().toLowerCase();
32 |     if (!normalized.includes('@') || password.length === 0) {
33 |       return { ok: false, reason: 'invalid_input' };
34 |     }
35 | 
36 |     const state = this.failures.get(normalized);
37 |     if (state && state.lockedUntil > this.now()) {
38 |       return { ok: false, reason: 'rate_limited' };
39 |     }
40 | 
41 |     let user: UserRecord | null = null;
42 |     try {
43 |       user = await this.users.findByEmail(normalized);
44 |     } catch {
45 |       return { ok: false, reason: 'bad_credentials' };
46 |     }
47 |     // Verify against a placeholder hash for unknown users so response timing
48 |     // does not reveal which emails are registered.
49 |     const matches = await this.verifier.verify(password, user?.passwordHash ?? DUMMY_HASH);
50 |     if (!user || !matches) {
51 |       await failureLock.run(() => sessionLock.run(async () => this.recordFailure(normalized)));
52 |       return { ok: false, reason: 'bad_credentials' };
53 |     }
54 | 
55 |     await sessionLock.run(() => failureLock.run(async () => this.failures.delete(normalized)));
56 |     const { token } = this.sessions.create(user.id);
57 |     return { ok: true, token, userId: user.id };
58 |   }
59 | 
60 |   private recordFailure(email: string): void {
61 |     const current = this.failures.get(email) ?? { count: 0, lockedUntil: 0 };
62 |     const count = current.count + 1;
63 |     const lockedUntil = count >= MAX_FAILURES ? this.now() + LOCKOUT_MS : 0;
64 |     this.failures.set(email, { count: lockedUntil ? 0 : count, lockedUntil });
65 |   }
66 | }
67 | 
68 | const DUMMY_HASH = '$2b$12$CwTycUXWue0Thq9StjUM0uJ8fB1s0ZQp5Yz0y8G2rQm0c5x6Jg4mK';
```

### test/auth/login.test.ts

```text
 1 | import { describe, it, expect } from 'vitest';
 2 | import { LoginService } from '../../src/auth/login';
 3 | import { SessionStore } from '../../src/auth/sessionStore';
 4 | 
 5 | const users = {
 6 |   async findByEmail(email: string) {
 7 |     return email === 'ada@example.com' ? { id: 'u1', email, passwordHash: 'h' } : null;
 8 |   },
 9 | };
10 | const verifier = {
11 |   async verify(password: string, hash: string) {
12 |     return password === 'correct' && hash === 'h';
13 |   },
14 | };
15 | 
16 | describe('LoginService', () => {
17 |   it('returns a session token for valid credentials', async () => {
18 |     const service = new LoginService(users, verifier, new SessionStore());
19 |     const result = await service.login(' Ada@Example.com ', 'correct');
20 |     expect(result.ok).toBe(true);
21 |     expect(result.ok && result.token.length > 0).toBe(true);
22 |   });
23 | 
24 |   it('rejects malformed email without touching the store', async () => {
25 |     const service = new LoginService(users, verifier, new SessionStore());
26 |     expect(await service.login('nope', 'correct')).toEqual({ ok: false, reason: 'invalid_input' });
27 |   });
28 | 
29 |   it('locks the account after repeated failures', async () => {
30 |     let clock = 0;
31 |     const service = new LoginService(users, verifier, new SessionStore(), () => clock);
32 |     for (let i = 0; i < 5; i++) await service.login('ada@example.com', 'wrong');
33 |     expect(await service.login('ada@example.com', 'correct')).toEqual({ ok: false, reason: 'rate_limited' });
34 |     clock += 16 * 60 * 1000;
35 |     expect((await service.login('ada@example.com', 'correct')).ok).toBe(true);
36 |   });
37 | });
```

## The key

### 1. Unhandled boundary or empty input (correctness.boundary_and_empty_input), Minor, planted, src/auth/sessionStore.ts lines 47 to 47

- Consequence: A session is still accepted at the exact millisecond it expires, so a token can remain valid one tick past its TTL. The window is tiny, but it breaks the documented expiry boundary.
- In an interview: The expiry check uses a strict comparison, so the boundary instant is off by one from the intended 'expired at expiresAt' rule. It matters because TTL boundaries are part of the security contract; use >= so a session is rejected at exactly expiresAt.

Proven by the judge: The edit changes the expiry comparison from >= to >, so a session is now still accepted at exactly expiresAt, which is the boundary the claimed consequence describes. However, the original code already has the one-tick-late behavior only if you consider the claim; the original rejects at exactly expiresAt (>=), so the original lacks the claimed problem. The edited code does exhibit the claimed one-tick-past-TTL acceptance at the exact expiry millisecond. The claimed consequence is a real boundary off-by-one, matching the boundary pattern: at the exact boundary value the behavior is wrong relative to documented expiry semantics. The edit therefore introduces the boundary defect and the original does not have it.


The fix, in src/auth/sessionStore.ts:

```diff
-    if (this.now() > session.expiresAt) {
+    if (this.now() >= session.expiresAt) {
```

### 2. Overly broad catch (errors.overly_broad_catch), Major, planted, src/auth/login.ts lines 41 to 46

- Consequence: When the user store is down or throws a programming error, every login is reported as bad credentials, so users are told their password is wrong during an outage and the real failure is hidden from logs and alerts.
- In an interview: The catch-all turns infrastructure failures and bugs into a credential error, swallowing exceptions it cannot handle. Catch only the expected not-found case and let other errors propagate so they surface as 5xx responses and get logged.

Proven by the judge: The edit wraps the user store lookup in a try/catch that catches every exception with no type check and returns bad_credentials. This matches the claimed consequence: a store outage or programming error during findByEmail is reported to the user as wrong credentials and the real error is swallowed. The original code had no try/catch, so store failures propagated as thrown errors and were not disguised as bad credentials, so the problem is newly introduced by this edit.


The fix, in src/auth/login.ts:

```diff
-    let user: UserRecord | null = null;
-    try {
-      user = await this.users.findByEmail(normalized);
-    } catch {
-      return { ok: false, reason: 'bad_credentials' };
-    }
+    const user = await this.users.findByEmail(normalized);
```

### 3. Inconsistent lock ordering (concurrency.lock_ordering), Major, planted, src/auth/login.ts lines 50 to 56

- Consequence: When a failed and a successful login for the same account run concurrently, one request can hold the failure lock while waiting for the session lock while the other holds the session lock waiting for the failure lock, so both logins hang indefinitely.
- In an interview: The failure path acquires failureLock then sessionLock, while the success path acquires sessionLock then failureLock, which is an inconsistent lock order that permits deadlock. Acquire the locks in one fixed global order (for example failureLock then sessionLock) on every path.

Proven by the judge: The edit makes the failure path acquire failureLock then sessionLock, while the success path acquires sessionLock then failureLock, which is an inconsistent lock ordering that can deadlock under concurrency as claimed. However, the edit references failureLock and sessionLock, which are not defined anywhere in the original or edited code, so they are unresolved identifiers; the original code has no locks at all, so the original lacks the problem. The edit introduces the inverted acquisition order, matching the pattern, so the edit introduces the pattern while the clean code lacks it.


The fix, in src/auth/login.ts:

```diff
     if (!user || !matches) {
-      await failureLock.run(() => sessionLock.run(async () => this.recordFailure(normalized)));
+      this.recordFailure(normalized);
       return { ok: false, reason: 'bad_credentials' };
     }
 
-    await sessionLock.run(() => failureLock.run(async () => this.failures.delete(normalized)));
+    this.failures.delete(normalized);
     const { token } = this.sessions.create(user.id);
```

### 4. Leaky abstraction (design.leaky_abstraction), Minor, found in review, src/auth/sessionStore.ts lines 28 to 28

- Consequence: The public Session object carries an id derived from the SHA-256 of the bearer token, so any code that logs, returns, or lists sessions emits token-derived material. The 48-bit prefix does not practically reveal the 256-bit random token, so the risk is moderate rather than a direct token compromise.
- In an interview: The Session type exposes an implementation detail, namely that its identifier is derived from the hash of the secret bearer token, which callers can then depend on or leak. Generating the id independently with randomBytes keeps the public identifier separate from the credential material.

Proven by the judge: Using a truncated hash of the bearer token as the exposed session id leaks token-derived material; this is a real, if moderate, security concern about the code shown, not described by the key.

The fix, in src/auth/sessionStore.ts:

```diff
-const session: Session = { id: hash.slice(0, 12), userId, createdAt, expiresAt: createdAt + this.ttlMs, revoked: false };
+const session: Session = { id: randomBytes(9).toString('base64url'), userId, createdAt, expiresAt: createdAt + this.ttlMs, revoked: false };
```

### 5. Unbounded growth (resources.unbounded_growth), Major, found in review, src/auth/sessionStore.ts lines 27 to 27

- Consequence: Every login adds an entry to byHash, and an entry is only deleted if its token is presented again after expiry. Sessions that are abandoned stay in the map for the life of the process, so memory climbs steadily under normal traffic until the process is restarted or killed.
- In an interview: Expired sessions are removed only when someone revalidates them, so any session nobody returns with stays in byHash forever. Sweeping expired entries on each create bounds the map to the sessions that are still live, since deleting from a Map while iterating it is safe in JavaScript.

Proven by the judge: Expired sessions are only removed on revalidation, so unvisited sessions accumulate in byHash. This is a real memory growth defect, but it is not the exact-expiry boundary issue.

The fix, in src/auth/sessionStore.ts:

```diff
     const createdAt = this.now();
+    for (const [key, s] of this.byHash) {
+      if (createdAt > s.expiresAt) this.byHash.delete(key);
+    }
```

### 6. Unbounded growth (resources.unbounded_growth), Major, found in review, src/auth/sessionStore.ts lines 59 to 62

- Consequence: Each call to revokeAllForUser marks sessions revoked but leaves their entries in byHash, and the byUser index that pointed to them is dropped, so nothing ever removes them except a later validate call on that exact token. Over time, logouts and password resets accumulate dead sessions in memory until the process runs out of memory.
- In an interview: revokeAllForUser drops the per-user index but keeps the revoked sessions in the hash map, and nothing else will reclaim them unless each token is validated again. Deleting each revoked hash from byHash at the moment it is revoked keeps the map bounded by the live sessions.

Proven by the judge: revokeAllForUser leaves revoked sessions in byHash, a real memory leak, but not the expiry boundary problem.

The fix, in src/auth/sessionStore.ts:

```diff
       if (session && !session.revoked) {
         session.revoked = true;
+        this.byHash.delete(hash);
         count++;
       }
```

### 7. Check-then-act race (concurrency.check_then_act), Major, found in review, src/auth/login.ts lines 36 to 51

- Consequence: Concurrent login requests for the same email all read the failure state before any failure is recorded, since verification and the async user lookup happen between the check and the recordFailure call. An attacker can send a burst of parallel guesses that far exceeds MAX_FAILURES before the lockout takes effect.
- In an interview: The lockout check and the failure record are separated by awaits, so the check-then-act sequence is not atomic and parallel requests all pass it. Reserving the attempt synchronously before any await closes the window, and the successful path already clears the counter.

Proven by the judge: Concurrent requests can pass the failure check before any failure is recorded, a real rate-limit bypass, but not the issue's concern.

The fix, in src/auth/login.ts:

```diff
     const state = this.failures.get(normalized);
     if (state && state.lockedUntil > this.now()) {
       return { ok: false, reason: 'rate_limited' };
     }
+    // Reserve the attempt synchronously, before any await, so concurrent requests see it; success clears it.
+    this.recordFailure(normalized);
 
     let user: UserRecord | null = null;
     try {
       user = await this.users.findByEmail(normalized);
     } catch {
       return { ok: false, reason: 'bad_credentials' };
     }
     // Verify against a placeholder hash for unknown users so response timing
     // does not reveal which emails are registered.
     const matches = await this.verifier.verify(password, user?.passwordHash ?? DUMMY_HASH);
     if (!user || !matches) {
-      await failureLock.run(() => sessionLock.run(async () => this.recordFailure(normalized)));
+      return { ok: false, reason: 'bad_credentials' };
```

### 8. Unbounded growth (resources.unbounded_growth), Major, found in review, src/auth/login.ts lines 64 to 64

- Consequence: Every distinct email that fails a login gets an entry in the failures map, and entries are only removed on a successful login, so an attacker who submits many made-up addresses grows process memory without limit until the service is killed.
- In an interview: The failures map is keyed by attacker-supplied email addresses and never trims entries for addresses that fail once and never return, so its size tracks the number of distinct guesses rather than active lockouts. Capping the map and evicting the oldest insertion when the cap is reached bounds memory, at the cost that a very old lockout record could be forgotten under heavy attack.

Proven by the judge: Unbounded growth of the failures map for one-time failing emails is a real memory risk, not the issue.

The fix, in src/auth/login.ts:

```diff
+    const MAX_TRACKED_EMAILS = 10_000;
+    if (!this.failures.has(email) && this.failures.size >= MAX_TRACKED_EMAILS) {
+      const oldest = this.failures.keys().next().value;
+      if (oldest !== undefined) this.failures.delete(oldest);
+    }
     this.failures.set(email, { count: lockedUntil ? 0 : count, lockedUntil });
```

## Dropped

Nothing was dropped.


## The clean code

### src/auth/sessionStore.ts

```text
 1 | import { randomBytes, createHash, timingSafeEqual } from 'node:crypto';
 2 | 
 3 | export interface Session {
 4 |   id: string;
 5 |   userId: string;
 6 |   createdAt: number;
 7 |   expiresAt: number;
 8 |   revoked: boolean;
 9 | }
10 | 
11 | const DEFAULT_TTL_MS = 30 * 60 * 1000;
12 | const MAX_SESSIONS_PER_USER = 5;
13 | 
14 | function hashToken(token: string): string {
15 |   return createHash('sha256').update(token).digest('hex');
16 | }
17 | 
18 | export class SessionStore {
19 |   private readonly byHash = new Map<string, Session>();
20 |   private readonly byUser = new Map<string, string[]>();
21 | 
22 |   constructor(private readonly ttlMs: number = DEFAULT_TTL_MS, private readonly now: () => number = Date.now) {}
23 | 
24 |   create(userId: string): { token: string; session: Session } {
25 |     const token = randomBytes(32).toString('base64url');
26 |     const hash = hashToken(token);
27 |     const createdAt = this.now();
28 |     const session: Session = { id: hash.slice(0, 12), userId, createdAt, expiresAt: createdAt + this.ttlMs, revoked: false };
29 | 
30 |     const ids = this.byUser.get(userId) ?? [];
31 |     ids.push(hash);
32 |     // Evict the oldest sessions so a single account cannot accumulate unbounded state.
33 |     while (ids.length > MAX_SESSIONS_PER_USER) {
34 |       const oldest = ids.shift();
35 |       if (oldest !== undefined) this.byHash.delete(oldest);
36 |     }
37 |     this.byUser.set(userId, ids);
38 |     this.byHash.set(hash, session);
39 |     return { token, session };
40 |   }
41 | 
42 |   validate(token: string): Session | null {
43 |     if (token.length === 0) return null;
44 |     const hash = hashToken(token);
45 |     const session = this.byHash.get(hash);
46 |     if (!session || session.revoked) return null;
47 |     if (this.now() >= session.expiresAt) {
48 |       this.revokeHash(hash, session);
49 |       return null;
50 |     }
51 |     return session;
52 |   }
53 | 
54 |   revokeAllForUser(userId: string): number {
55 |     const ids = this.byUser.get(userId) ?? [];
56 |     let count = 0;
57 |     for (const hash of ids) {
58 |       const session = this.byHash.get(hash);
59 |       if (session && !session.revoked) {
60 |         session.revoked = true;
61 |         count++;
62 |       }
63 |     }
64 |     this.byUser.delete(userId);
65 |     return count;
66 |   }
67 | 
68 |   private revokeHash(hash: string, session: Session): void {
69 |     session.revoked = true;
70 |     this.byHash.delete(hash);
71 |   }
72 | }
73 | 
74 | export function tokensMatch(a: string, b: string): boolean {
75 |   const left = Buffer.from(a);
76 |   const right = Buffer.from(b);
77 |   if (left.length !== right.length) return false;
78 |   return timingSafeEqual(left, right);
79 | }
```

### src/auth/login.ts

```text
 1 | import { SessionStore } from './sessionStore';
 2 | 
 3 | export interface UserRecord {
 4 |   id: string;
 5 |   email: string;
 6 |   passwordHash: string;
 7 | }
 8 | 
 9 | export interface PasswordVerifier {
10 |   verify(password: string, hash: string): Promise<boolean>;
11 | }
12 | 
13 | export type LoginResult =
14 |   | { ok: true; token: string; userId: string }
15 |   | { ok: false; reason: 'invalid_input' | 'bad_credentials' | 'rate_limited' };
16 | 
17 | const MAX_FAILURES = 5;
18 | const LOCKOUT_MS = 15 * 60 * 1000;
19 | 
20 | export class LoginService {
21 |   private readonly failures = new Map<string, { count: number; lockedUntil: number }>();
22 | 
23 |   constructor(
24 |     private readonly users: { findByEmail(email: string): Promise<UserRecord | null> },
25 |     private readonly verifier: PasswordVerifier,
26 |     private readonly sessions: SessionStore,
27 |     private readonly now: () => number = Date.now,
28 |   ) {}
29 | 
30 |   async login(email: string, password: string): Promise<LoginResult> {
31 |     const normalized = email.trim().toLowerCase();
32 |     if (!normalized.includes('@') || password.length === 0) {
33 |       return { ok: false, reason: 'invalid_input' };
34 |     }
35 | 
36 |     const state = this.failures.get(normalized);
37 |     if (state && state.lockedUntil > this.now()) {
38 |       return { ok: false, reason: 'rate_limited' };
39 |     }
40 | 
41 |     const user = await this.users.findByEmail(normalized);
42 |     // Verify against a placeholder hash for unknown users so response timing
43 |     // does not reveal which emails are registered.
44 |     const matches = await this.verifier.verify(password, user?.passwordHash ?? DUMMY_HASH);
45 |     if (!user || !matches) {
46 |       this.recordFailure(normalized);
47 |       return { ok: false, reason: 'bad_credentials' };
48 |     }
49 | 
50 |     this.failures.delete(normalized);
51 |     const { token } = this.sessions.create(user.id);
52 |     return { ok: true, token, userId: user.id };
53 |   }
54 | 
55 |   private recordFailure(email: string): void {
56 |     const current = this.failures.get(email) ?? { count: 0, lockedUntil: 0 };
57 |     const count = current.count + 1;
58 |     const lockedUntil = count >= MAX_FAILURES ? this.now() + LOCKOUT_MS : 0;
59 |     this.failures.set(email, { count: lockedUntil ? 0 : count, lockedUntil });
60 |   }
61 | }
62 | 
63 | const DUMMY_HASH = '$2b$12$CwTycUXWue0Thq9StjUM0uJ8fB1s0ZQp5Yz0y8G2rQm0c5x6Jg4mK';
```

### test/auth/login.test.ts

```text
 1 | import { describe, it, expect } from 'vitest';
 2 | import { LoginService } from '../../src/auth/login';
 3 | import { SessionStore } from '../../src/auth/sessionStore';
 4 | 
 5 | const users = {
 6 |   async findByEmail(email: string) {
 7 |     return email === 'ada@example.com' ? { id: 'u1', email, passwordHash: 'h' } : null;
 8 |   },
 9 | };
10 | const verifier = {
11 |   async verify(password: string, hash: string) {
12 |     return password === 'correct' && hash === 'h';
13 |   },
14 | };
15 | 
16 | describe('LoginService', () => {
17 |   it('returns a session token for valid credentials', async () => {
18 |     const service = new LoginService(users, verifier, new SessionStore());
19 |     const result = await service.login(' Ada@Example.com ', 'correct');
20 |     expect(result.ok).toBe(true);
21 |     expect(result.ok && result.token.length > 0).toBe(true);
22 |   });
23 | 
24 |   it('rejects malformed email without touching the store', async () => {
25 |     const service = new LoginService(users, verifier, new SessionStore());
26 |     expect(await service.login('nope', 'correct')).toEqual({ ok: false, reason: 'invalid_input' });
27 |   });
28 | 
29 |   it('locks the account after repeated failures', async () => {
30 |     let clock = 0;
31 |     const service = new LoginService(users, verifier, new SessionStore(), () => clock);
32 |     for (let i = 0; i < 5; i++) await service.login('ada@example.com', 'wrong');
33 |     expect(await service.login('ada@example.com', 'correct')).toEqual({ ok: false, reason: 'rate_limited' });
34 |     clock += 16 * 60 * 1000;
35 |     expect((await service.login('ada@example.com', 'correct')).ok).toBe(true);
36 |   });
37 | });
```

## The clean review

- Minor: The first test is named 'returns a session token for valid credentials' but only asserts `ok`. It never checks that a token was returned, so the test would pass even if the token were missing or empty.

## The blind review

1. Major in src/auth/sessionStore.ts: The session id is a truncated SHA-256 of the bearer token, and it is exposed on the Session object. Any code path that logs, returns, or lists sessions leaks a 48-bit prefix of the token hash. Use a separate random identifier rather than deriving it from the secret-derived hash.
   Quoted: const session: Session = { id: hash.slice(0, 12), userId, createdAt, expiresAt: createdAt + this.ttlMs, revoked: false };
   Judged: ValidExtra. Using a truncated hash of the bearer token as the exposed session id leaks token-derived material; this is a real, if moderate, security concern about the code shown, not described by the key.
2. Major in src/auth/sessionStore.ts: Expired sessions are only removed when revalidated. Sessions that are never presented again stay in byHash indefinitely, so memory grows without bound. Add periodic sweeping or evict on create.
   Quoted:     if (this.now() > session.expiresAt) {       this.revokeHash(hash, session);
   Judged: ValidExtra. Expired sessions are only removed on revalidation, so unvisited sessions accumulate in byHash. This is a real memory growth defect, but it is not the exact-expiry boundary issue.
3. Minor in src/auth/sessionStore.ts: revokeAllForUser marks sessions revoked but leaves them in byHash, and it drops the byUser index. Revoked entries are never removed, so the map leaks. Delete the hashes from byHash as well.
   Quoted:     this.byUser.delete(userId);     return count;
   Judged: ValidExtra. revokeAllForUser leaves revoked sessions in byHash, a real memory leak, but not the expiry boundary problem.
4. Nit in src/auth/sessionStore.ts: tokensMatch is exported and appears unused by the session path, which looks up tokens by hash in a Map. If it is used for comparing secrets, the length check leaks length and the Map lookup itself is not constant time. Remove it if unused, or document its intended use.
   Quoted: export function tokensMatch(a: string, b: string): boolean {
   Judged: FalsePositive. tokensMatch is unused and a preference-level remark about an exported helper; not a real defect in the session path.
5. Minor in src/auth/login.ts: The verifier is called with the DUMMY_HASH for unknown users, but the lookup failure path returns early without verifying, so timing still differs when findByEmail throws. Also the dummy hash is a bcrypt string while the test verifier expects 'h'; ensure the real verifier cost matches. Consider always performing the same work on every path.
   Quoted:     const matches = await this.verifier.verify(password, user?.passwordHash ?? DUMMY_HASH);
   Judged: FalsePositive. Claims timing differs when findByEmail throws, but the failure path returns early by design; the remark about bcrypt cost and test verifier is a preference or test artifact, not the broad catch cause.
6. Critical in src/auth/login.ts: failureLock and sessionLock are referenced but never declared or imported in this file, so the module will not compile or will throw ReferenceError at runtime. Either define these locks or remove the wrapping.
   Quoted:     if (!user || !matches) {       await failureLock.run(() => sessionLock.run(async () => this.recordFailure(normalized)));
   Judged: ValidExtra. Claims failureLock and sessionLock are undeclared. The locks are referenced without declaration in the shown code, so this is a real compile/runtime defect, but it is not the lock-ordering cause named by issue 2.
7. Major in src/auth/login.ts: Lock acquisition order is reversed relative to the failure path (sessionLock then failureLock here, failureLock then sessionLock above), which risks deadlock under concurrency. Use one consistent lock order.
   Quoted:     await sessionLock.run(() => failureLock.run(async () => this.failures.delete(normalized)));     const { token } = this.sessions.create(user.id);
   Judged: Matches { issues: [2] }. Names the reversed lock order and the deadlock risk under concurrency, matching issue 2.
8. Major in src/auth/login.ts: Rate limiting is keyed only on email and the failure counter is checked before verification, so concurrent requests can all pass the check before any failure is recorded, allowing well beyond MAX_FAILURES guesses. Reserve attempts atomically.
   Quoted:     const state = this.failures.get(normalized);     if (state && state.lockedUntil > this.now()) {
   Judged: ValidExtra. Concurrent requests can pass the failure check before any failure is recorded, a real rate-limit bypass, but not the issue's concern.
9. Minor in src/auth/login.ts: After lockout the count is reset to 0 while lockedUntil is set, so once the lock expires the user gets a fresh 5 attempts; that is intended, but the failures map also never evicts entries for emails that fail once and never return, causing unbounded growth. Add eviction.
   Quoted:     this.failures.set(email, { count: lockedUntil ? 0 : count, lockedUntil });
   Judged: ValidExtra. Unbounded growth of the failures map for one-time failing emails is a real memory risk, not the issue.
10. Major in src/auth/login.ts: Input is not length-bounded before hashing and DB lookup. Very large email or password strings are processed by the verifier, enabling denial of service. Enforce maximum lengths.
   Quoted:     const normalized = email.trim().toLowerCase();
   Judged: ValidExtra. Missing input length bounds enabling DoS is a real hardening concern in the code shown.
11. Nit in src/auth/login.ts: Session creation is not guarded: if sessions.create throws, the error propagates as an unhandled rejection rather than a LoginResult. Consider handling it explicitly.
   Quoted:     const { token } = this.sessions.create(user.id);     return { ok: true, token, userId: user.id };
   Judged: FalsePositive. Unguarded sessions.create throwing is a preference-level remark; create is synchronous and errors are not a defined defect here.
12. Minor in test/auth/login.test.ts: The assertion is weak: it only checks token length, not that the token validates against the session store or maps to user u1. Assert the userId and that sessions.validate returns the session.
   Quoted:     expect(result.ok && result.token.length > 0).toBe(true);
   Judged: FalsePositive. Test assertion is weak, a remark about test quality rather than a defect in the code under test.
13. Minor in test/auth/login.test.ts: The test does not cover concurrent attempts, the bypass that the rate limiter most needs to prevent, nor the unknown-user path. It also never checks that an unknown email gets the same response as a wrong password.
   Quoted:     for (let i = 0; i < 5; i++) await service.login('ada@example.com', 'wrong');
   Judged: FalsePositive. Test coverage gaps for concurrency and unknown users are a remark about tests, not a defect.

- planted-1 (correctness.boundary_and_empty_input): the review Missed. Finding 2 concerns expired sessions accumulating in memory, not the strict comparison at the expiry boundary. It sits on the overlapping line only by proximity and names a different problem, so it is not about the boundary issue.
- planted-2 (errors.overly_broad_catch): the review Missed. Finding 1 concerns session id derivation, not the catch. Finding 5 mentions the findByEmail throw path but attributes the problem to timing, not to the broad catch reporting outages as bad credentials. Finding 11 is about sessions.create. No finding names that the catch hides store failures as bad_credentials.
- planted-3 (concurrency.lock_ordering): the review Found { finding: 6 }. Finding 7 names the reversed lock order between the failure path (failureLock then sessionLock) and the success path (sessionLock then failureLock), and states the deadlock risk under concurrency, which is the cause of the issue.

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
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
