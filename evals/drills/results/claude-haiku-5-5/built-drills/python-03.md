# User session store with expiry and login attempt throttling (python)

- Domain: user sessions and login
- Built by: claude-haiku-5-5
- Runner: builtin-python
- Built in 88 seconds
- Patterns asked for: correctness.boundary_and_empty_input, errors.overly_broad_catch, concurrency.lock_ordering

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 1
- Dropped: 0
- Found in review: 2

## The code

### src/sessions.py

```text
  1 | import secrets
  2 | from dataclasses import dataclass
  3 | 
  4 | 
  5 | @dataclass(frozen=True)
  6 | class Session:
  7 |     token: str
  8 |     user_id: str
  9 |     created_at: float
 10 |     expires_at: float
 11 | 
 12 | 
 13 | class SessionStore:
 14 |     """In-memory session store. Time and token generation are injected so
 15 |     callers can control expiry deterministically."""
 16 | 
 17 |     def __init__(self, clock, ttl_seconds, token_factory=None):
 18 |         if ttl_seconds <= 0:
 19 |             raise ValueError("ttl_seconds must be positive")
 20 |         self._clock = clock
 21 |         self._ttl = ttl_seconds
 22 |         self._token_factory = token_factory or (lambda: secrets.token_urlsafe(32))
 23 |         import threading
 24 |         self._sessions = {}
 25 |         self._lock = threading.Lock()
 26 |         self._index_lock = threading.Lock()
 27 | 
 28 |     def create(self, user_id):
 29 |         if not user_id:
 30 |             raise ValueError("user_id is required")
 31 |         now = self._clock()
 32 |         token = self._token_factory()
 33 |         with self._lock:
 34 |             with self._index_lock:
 35 |                 if token in self._sessions:
 36 |                     # A collision means the factory is broken; refusing to overwrite
 37 |                     # keeps another user's live session from being hijacked.
 38 |                     raise RuntimeError("token collision")
 39 |                 session = Session(token, user_id, now, now + self._ttl)
 40 |                 self._sessions[token] = session
 41 |         return session
 42 | 
 43 |     def get(self, token):
 44 |         session = self._sessions.get(token)
 45 |         if session is None:
 46 |             return None
 47 |         if self._clock() >= session.expires_at:
 48 |             # Expired sessions are dropped lazily on access.
 49 |             del self._sessions[token]
 50 |             return None
 51 |         return session
 52 | 
 53 |     def revoke(self, token):
 54 |         return self._sessions.pop(token, None) is not None
 55 | 
 56 |     def revoke_user(self, user_id):
 57 |         with self._index_lock:
 58 |             with self._lock:
 59 |                 stale = [t for t, s in self._sessions.items() if s.user_id == user_id]
 60 |                 for token in stale:
 61 |                     del self._sessions[token]
 62 |         return len(stale)
 63 | 
 64 | 
 65 | class LoginThrottle:
 66 |     """Locks an account for a cooldown after too many failed attempts."""
 67 | 
 68 |     def __init__(self, clock, max_failures, window_seconds):
 69 |         if max_failures < 1 or window_seconds <= 0:
 70 |             raise ValueError("invalid throttle configuration")
 71 |         self._clock = clock
 72 |         self._max = max_failures
 73 |         self._window = window_seconds
 74 |         self._failures = {}
 75 | 
 76 |     def _recent(self, user_id):
 77 |         cutoff = self._clock() - self._window
 78 |         kept = [t for t in self._failures.get(user_id, []) if t >= cutoff]
 79 |         if kept:
 80 |             self._failures[user_id] = kept
 81 |         else:
 82 |             self._failures.pop(user_id, None)
 83 |         return kept
 84 | 
 85 |     def is_locked(self, user_id):
 86 |         return len(self._recent(user_id)) >= self._max
 87 | 
 88 |     def record_failure(self, user_id):
 89 |         recent = self._recent(user_id)
 90 |         recent.append(self._clock())
 91 |         self._failures[user_id] = recent
 92 | 
 93 |     def record_success(self, user_id):
 94 |         self._failures.pop(user_id, None)
 95 | 
 96 | 
 97 | def login(user_id, password, verify, store, throttle):
 98 |     """Check credentials and open a session. Returns the Session, or None
 99 |     when the password is wrong or the account is temporarily locked."""
100 |     if throttle.is_locked(user_id):
101 |         return None
102 |     try:
103 |         ok = verify(user_id, password)
104 |     except Exception:
105 |         ok = False
106 |     if not ok:
107 |         throttle.record_failure(user_id)
108 |         return None
109 |     throttle.record_success(user_id)
110 |     return store.create(user_id)
```

### tests/test_sessions.py

```text
 1 | from src.sessions import SessionStore, LoginThrottle, login
 2 | 
 3 | 
 4 | class FakeClock:
 5 |     def __init__(self, now=1000.0):
 6 |         self.now = now
 7 | 
 8 |     def __call__(self):
 9 |         return self.now
10 | 
11 | 
12 | def test_session_expires():
13 |     clock = FakeClock()
14 |     store = SessionStore(clock, ttl_seconds=60, token_factory=lambda: "t1")
15 |     s = store.create("alice")
16 |     assert store.get("t1") is s
17 |     clock.now += 59.9
18 |     assert store.get("t1") is not None
19 |     clock.now += 0.1
20 |     assert store.get("t1") is None
21 | 
22 | 
23 | def test_login_locks_after_failures():
24 |     clock = FakeClock()
25 |     store = SessionStore(clock, ttl_seconds=60, token_factory=lambda: "tok")
26 |     throttle = LoginThrottle(clock, max_failures=2, window_seconds=300)
27 |     verify = lambda u, p: p == "secret"
28 |     assert login("bob", "bad", verify, store, throttle) is None
29 |     assert login("bob", "bad", verify, store, throttle) is None
30 |     assert login("bob", "secret", verify, store, throttle) is None
31 |     clock.now += 301
32 |     assert login("bob", "secret", verify, store, throttle) is not None
```

## The key

### 1. Unhandled boundary or empty input (correctness.boundary_and_empty_input), Major, planted, src/sessions.py lines 78 to 78

- Consequence: A failure recorded exactly one window ago still counts, so an account stays locked one tick longer than the configured window and the throttle's lockout ends late for every user.
- In an interview: The filter keeps failures at the window boundary, so a failure exactly window_seconds old still counts toward the lockout. Use a strict comparison so the window is exclusive at the boundary, matching how SessionStore.get treats expiry.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.sessions import LoginThrottle

class C:
    now = 0.0
    def __call__(self):
        return self.now

c = C()
t = LoginThrottle(c, max_failures=1, window_seconds=100)
t.record_failure('x')
c.now = 100.0
assert not t.is_locked('x'), 'boundary failure still counted'
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/sessions.py:

```diff
-        kept = [t for t in self._failures.get(user_id, []) if t >= cutoff]
+        kept = [t for t in self._failures.get(user_id, []) if t > cutoff]
```

### 2. Overly broad catch (errors.overly_broad_catch), Major, planted, src/sessions.py lines 102 to 106

- Consequence: When the credential backend fails or has a bug, every login is silently treated as a wrong password, so users are locked out and the outage is hidden instead of surfacing as an error.
- In an interview: The broad except turns any exception from verify into a failed login, including programming errors and backend outages that the login path cannot handle. Catch only the specific expected failure, or let exceptions propagate so they are reported.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.sessions import SessionStore, LoginThrottle, login

class C:
    now = 0.0
    def __call__(self):
        return self.now

c = C()
s = SessionStore(c, 60)
t = LoginThrottle(c, 3, 300)
def boom(u, p):
    raise RuntimeError('backend down')
try:
    login('u', 'pw', boom, s, t)
except RuntimeError:
    pass
else:
    raise AssertionError('verify error swallowed')
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/sessions.py:

```diff
-    try:
-        ok = verify(user_id, password)
-    except Exception:
-        ok = False
-    if not ok:
+    if not verify(user_id, password):
```

### 3. Inconsistent lock ordering (concurrency.lock_ordering), Critical, planted, src/sessions.py lines 23 to 62

- Consequence: If create and revoke_user run concurrently, one holds the session lock while waiting for the index lock and the other holds the index lock while waiting for the session lock, so both threads hang and login or logout stops for the whole service.
- In an interview: create takes the session lock then the index lock, while revoke_user takes them in the opposite order, so the two threads can each hold one lock and wait forever on the other. Both paths must acquire the locks in one agreed global order.

Proven by the judge: The edit adds two locks to SessionStore. create acquires _lock then _index_lock, while revoke_user acquires _index_lock then _lock. This is a genuine inconsistent lock ordering, so the claimed deadlock pattern is introduced by the edit. The original code had no locks at all, so it cannot have this problem. The claimed consequence (both threads hang when create and revoke_user run concurrently) matches the classic AB-BA deadlock, so the consequence can happen.


The fix, in src/sessions.py:

```diff
-        import threading
         self._sessions = {}
-        self._lock = threading.Lock()
-        self._index_lock = threading.Lock()
 
     def create(self, user_id):
         if not user_id:
             raise ValueError("user_id is required")
         now = self._clock()
         token = self._token_factory()
-        with self._lock:
-            with self._index_lock:
-                if token in self._sessions:
-                    # A collision means the factory is broken; refusing to overwrite
-                    # keeps another user's live session from being hijacked.
-                    raise RuntimeError("token collision")
-                session = Session(token, user_id, now, now + self._ttl)
-                self._sessions[token] = session
+        if token in self._sessions:
+            # A collision means the factory is broken; refusing to overwrite
+            # keeps another user's live session from being hijacked.
+            raise RuntimeError("token collision")
+        session = Session(token, user_id, now, now + self._ttl)
+        self._sessions[token] = session
         return session
 
     def get(self, token):
         session = self._sessions.get(token)
         if session is None:
             return None
         if self._clock() >= session.expires_at:
             # Expired sessions are dropped lazily on access.
             del self._sessions[token]
             return None
         return session
 
     def revoke(self, token):
         return self._sessions.pop(token, None) is not None
 
     def revoke_user(self, user_id):
-        with self._index_lock:
-            with self._lock:
-                stale = [t for t, s in self._sessions.items() if s.user_id == user_id]
-                for token in stale:
-                    del self._sessions[token]
+        stale = [t for t, s in self._sessions.items() if s.user_id == user_id]
+        for token in stale:
+            del self._sessions[token]
         return len(stale)
```

### 4. Unsynchronised shared state (concurrency.unsynchronised_shared_state), Major, found in review, src/sessions.py lines 48 to 49

- Consequence: get() deletes an expired token from the shared dict without self._lock, while create() and revoke_user() mutate or iterate it under that lock. When an expired session is read concurrently with revoke_user() or with another get() on the same token, the del can raise KeyError or the dict can change size during revoke_user's iteration, raising RuntimeError and leaving sessions half-revoked.
- In an interview: The dict is shared by create, get and revoke_user, but get() removes expired entries without taking the lock the other writers use, so the removal can race with a concurrent revoke_user iteration or with another get() on the same token. Taking self._lock around the removal, with pop(token, None) so a token already removed elsewhere is tolerated, makes every mutation of the dict serialise on the same lock.

Proven by the judge: get() deletes from the session dict without holding self._lock while create and revoke_user use it, which is a real unsynchronized mutation race. It is not the lock-order inversion of issue 1, so it is an extra valid defect.

The fix, in src/sessions.py:

```diff
             # Expired sessions are dropped lazily on access.
-            del self._sessions[token]
+            with self._lock:
+                self._sessions.pop(token, None)
```

### 5. Unsynchronised shared state (concurrency.unsynchronised_shared_state), Major, found in review, src/sessions.py lines 88 to 91

- Consequence: Concurrent failed logins can interleave their read-modify-write of _failures, so some failure records are lost. An attacker sending parallel guesses can then exceed max_failures and keep guessing past the lockout. The same dict also grows with every distinct user_id probed, which is a separate resources.unbounded_growth issue the key does not list.
- In an interview: LoginThrottle mutates a shared dict with no lock, so two threads can both read the same failure list and one append is lost, letting an attacker exceed the limit. The fix guards the whole read-modify-write with a lock, which must also be created in __init__ as self._failures_lock = threading.Lock().

Proven by the judge: LoginThrottle read-modify-writes _failures without a lock, a real race, and the dict grows with every probed user_id, a real memory risk. Both are genuine defects not in the key.

The fix, in src/sessions.py:

```diff
     def record_failure(self, user_id):
-        recent = self._recent(user_id)
-        recent.append(self._clock())
-        self._failures[user_id] = recent
+        with self._failures_lock:
+            recent = self._recent(user_id)
+            recent.append(self._clock())
+            self._failures[user_id] = recent
```

## Dropped

Nothing was dropped.


## The clean code

### src/sessions.py

```text
 1 | import secrets
 2 | from dataclasses import dataclass
 3 | 
 4 | 
 5 | @dataclass(frozen=True)
 6 | class Session:
 7 |     token: str
 8 |     user_id: str
 9 |     created_at: float
10 |     expires_at: float
11 | 
12 | 
13 | class SessionStore:
14 |     """In-memory session store. Time and token generation are injected so
15 |     callers can control expiry deterministically."""
16 | 
17 |     def __init__(self, clock, ttl_seconds, token_factory=None):
18 |         if ttl_seconds <= 0:
19 |             raise ValueError("ttl_seconds must be positive")
20 |         self._clock = clock
21 |         self._ttl = ttl_seconds
22 |         self._token_factory = token_factory or (lambda: secrets.token_urlsafe(32))
23 |         self._sessions = {}
24 | 
25 |     def create(self, user_id):
26 |         if not user_id:
27 |             raise ValueError("user_id is required")
28 |         now = self._clock()
29 |         token = self._token_factory()
30 |         if token in self._sessions:
31 |             # A collision means the factory is broken; refusing to overwrite
32 |             # keeps another user's live session from being hijacked.
33 |             raise RuntimeError("token collision")
34 |         session = Session(token, user_id, now, now + self._ttl)
35 |         self._sessions[token] = session
36 |         return session
37 | 
38 |     def get(self, token):
39 |         session = self._sessions.get(token)
40 |         if session is None:
41 |             return None
42 |         if self._clock() >= session.expires_at:
43 |             # Expired sessions are dropped lazily on access.
44 |             del self._sessions[token]
45 |             return None
46 |         return session
47 | 
48 |     def revoke(self, token):
49 |         return self._sessions.pop(token, None) is not None
50 | 
51 |     def revoke_user(self, user_id):
52 |         stale = [t for t, s in self._sessions.items() if s.user_id == user_id]
53 |         for token in stale:
54 |             del self._sessions[token]
55 |         return len(stale)
56 | 
57 | 
58 | class LoginThrottle:
59 |     """Locks an account for a cooldown after too many failed attempts."""
60 | 
61 |     def __init__(self, clock, max_failures, window_seconds):
62 |         if max_failures < 1 or window_seconds <= 0:
63 |             raise ValueError("invalid throttle configuration")
64 |         self._clock = clock
65 |         self._max = max_failures
66 |         self._window = window_seconds
67 |         self._failures = {}
68 | 
69 |     def _recent(self, user_id):
70 |         cutoff = self._clock() - self._window
71 |         kept = [t for t in self._failures.get(user_id, []) if t > cutoff]
72 |         if kept:
73 |             self._failures[user_id] = kept
74 |         else:
75 |             self._failures.pop(user_id, None)
76 |         return kept
77 | 
78 |     def is_locked(self, user_id):
79 |         return len(self._recent(user_id)) >= self._max
80 | 
81 |     def record_failure(self, user_id):
82 |         recent = self._recent(user_id)
83 |         recent.append(self._clock())
84 |         self._failures[user_id] = recent
85 | 
86 |     def record_success(self, user_id):
87 |         self._failures.pop(user_id, None)
88 | 
89 | 
90 | def login(user_id, password, verify, store, throttle):
91 |     """Check credentials and open a session. Returns the Session, or None
92 |     when the password is wrong or the account is temporarily locked."""
93 |     if throttle.is_locked(user_id):
94 |         return None
95 |     if not verify(user_id, password):
96 |         throttle.record_failure(user_id)
97 |         return None
98 |     throttle.record_success(user_id)
99 |     return store.create(user_id)
```

### tests/test_sessions.py

```text
 1 | from src.sessions import SessionStore, LoginThrottle, login
 2 | 
 3 | 
 4 | class FakeClock:
 5 |     def __init__(self, now=1000.0):
 6 |         self.now = now
 7 | 
 8 |     def __call__(self):
 9 |         return self.now
10 | 
11 | 
12 | def test_session_expires():
13 |     clock = FakeClock()
14 |     store = SessionStore(clock, ttl_seconds=60, token_factory=lambda: "t1")
15 |     s = store.create("alice")
16 |     assert store.get("t1") is s
17 |     clock.now += 59.9
18 |     assert store.get("t1") is not None
19 |     clock.now += 0.1
20 |     assert store.get("t1") is None
21 | 
22 | 
23 | def test_login_locks_after_failures():
24 |     clock = FakeClock()
25 |     store = SessionStore(clock, ttl_seconds=60, token_factory=lambda: "tok")
26 |     throttle = LoginThrottle(clock, max_failures=2, window_seconds=300)
27 |     verify = lambda u, p: p == "secret"
28 |     assert login("bob", "bad", verify, store, throttle) is None
29 |     assert login("bob", "bad", verify, store, throttle) is None
30 |     assert login("bob", "secret", verify, store, throttle) is None
31 |     clock.now += 301
32 |     assert login("bob", "secret", verify, store, throttle) is not None
```

### The hidden suite

```text
import sys
from src.sessions import SessionStore, LoginThrottle, login, Session


class FakeClock:
    def __init__(self, now=0.0):
        self.now = now

    def __call__(self):
        return self.now


def counter_tokens():
    n = [0]
    def make():
        n[0] += 1
        return f"tok{n[0]}"
    return make


def check_create_and_get():
    clock = FakeClock(100.0)
    store = SessionStore(clock, 60, counter_tokens())
    s = store.create("u1")
    assert isinstance(s, Session)
    assert s.created_at == 100.0 and s.expires_at == 160.0
    assert store.get(s.token).user_id == "u1"
    assert store.get("missing") is None


def check_expiry_boundary_is_exclusive():
    clock = FakeClock(0.0)
    store = SessionStore(clock, 10, counter_tokens())
    s = store.create("u")
    clock.now = 10.0
    assert store.get(s.token) is None


def check_revoke():
    clock = FakeClock()
    store = SessionStore(clock, 60, counter_tokens())
    a = store.create("u1")
    b = store.create("u1")
    c = store.create("u2")
    assert store.revoke(a.token) is True
    assert store.revoke(a.token) is False
    assert store.revoke_user("u1") == 1
    assert store.get(b.token) is None
    assert store.get(c.token) is not None


def check_token_collision_rejected():
    clock = FakeClock()
    store = SessionStore(clock, 60, lambda: "same")
    store.create("u1")
    try:
        store.create("u2")
    except RuntimeError:
        pass
    else:
        raise AssertionError("collision not detected")


def check_invalid_config():
    clock = FakeClock()
    for args in [(0, 1), (-5, 1)]:
        try:
            SessionStore(clock, args[0])
        except ValueError:
            pass
        else:
            raise AssertionError("bad ttl accepted")
    try:
        LoginThrottle(clock, 0, 10)
    except ValueError:
        pass
    else:
        raise AssertionError("bad max_failures accepted")


def check_throttle_window_slides():
    clock = FakeClock(0.0)
    t = LoginThrottle(clock, max_failures=2, window_seconds=100)
    t.record_failure("x")
    clock.now = 50.0
    t.record_failure("x")
    assert t.is_locked("x")
    clock.now = 100.0
    assert not t.is_locked("x")


def check_login_flow():
    clock = FakeClock()
    store = SessionStore(clock, 60, counter_tokens())
    t = LoginThrottle(clock, 3, 300)
    verify = lambda u, p: p == "pw"
    assert login("u", "no", verify, store, t) is None
    assert login("u", "no", verify, store, t) is None
    s = login("u", "pw", verify, store, t)
    assert s is not None and store.get(s.token) is not None
    assert login("u", "no", verify, store, t) is None
    assert t._failures.get("u") is None or len(t._failures["u"]) == 1


if __name__ == "__main__":
    checks = [v for k, v in list(globals().items()) if k.startswith("check_")]
    for check in checks:
        check()
    print(f"{len(checks)} checks passed")
    sys.exit(0)
```

## The clean review

It found nothing to repair.


## The blind review

1. Major in src/sessions.py: get() mutates the dict without holding self._lock, while create() and revoke_user() mutate it under the lock. Concurrent access can race (e.g. del on a token already removed by revoke_user raises KeyError, and dict iteration in revoke_user can fail if get() deletes concurrently). Take the lock here, or use pop(token, None).
   Quoted:         session = self._sessions.get(token)         if session is None:             return None         if self._clock() >= session.expires_at:             # Expired sessions are dropped lazily on access.             del self._sessions[token]
   Judged: ValidExtra. get() deletes from the session dict without holding self._lock while create and revoke_user use it, which is a real unsynchronized mutation race. It is not the lock-order inversion of issue 1, so it is an extra valid defect.
2. Minor in src/sessions.py: revoke() ignores self._lock while create() and revoke_user() hold it, so it is not synchronized with the rest of the store. Wrap it in the same lock for consistency.
   Quoted:     def revoke(self, token):         return self._sessions.pop(token, None) is not None
   Judged: FalsePositive. dict.pop is atomic and revoke is a simple single operation; the lack of the lock is a style choice rather than a real defect.
3. Minor in src/sessions.py: Two locks (_lock and _index_lock) guard the same dict with no clear invariant, and _index_lock is never used on its own in a meaningful way. This adds complexity and deadlock risk without benefit; a single lock suffices.
   Quoted:                 if token in self._sessions:                     # A collision means the factory is broken; refusing to overwrite
   Judged: Matches { issues: [2] }. Points at the two-lock design and its deadlock risk, which is the area of issue 1, though it does not identify the inconsistent ordering.
4. Nit in src/sessions.py: threading is imported inside __init__ rather than at module top. This is non-idiomatic and adds per-instance import overhead; move the import to module level.
   Quoted:         import threading         self._sessions = {}
   Judged: FalsePositive. Moving the threading import inside __init__ is a style preference with negligible impact.
5. Major in src/sessions.py: Any exception from verify() is silently treated as a failed login, hiding backend outages or bugs as ordinary wrong-password attempts that also count toward lockout. Log the exception or let infrastructure errors propagate.
   Quoted:     except Exception:         ok = False
   Judged: Matches { issues: [1] }. Correctly identifies that the broad exception handler hides real errors as wrong-password attempts.
6. Nit in src/sessions.py: Locked accounts return None without calling verify, which is fine for timing, but the caller cannot distinguish locked from wrong password. Document or return a distinct result so the UI can explain the lockout appropriately; currently the docstring hides this.
   Quoted:     if throttle.is_locked(user_id):         return None
   Judged: FalsePositive. Locked accounts returning None is documented design; the request for a distinct result is a preference, not a defect.
7. Major in src/sessions.py: LoginThrottle is not thread-safe: _failures is read-modify-written without a lock, so concurrent failed attempts can lose records and let an attacker exceed the limit. Also _failures grows unboundedly with every distinct user_id probed, since entries are only pruned when that user is accessed again.
   Quoted:     def record_failure(self, user_id):         recent = self._recent(user_id)
   Judged: ValidExtra. LoginThrottle read-modify-writes _failures without a lock, a real race, and the dict grows with every probed user_id, a real memory risk. Both are genuine defects not in the key.
8. Nit in src/sessions.py: SessionStore accepts an arbitrary token_factory with no check; the default is secure but a caller-supplied predictable factory is not guarded. Acceptable for injection, but the test-only factory path should not be used in production; consider a clearer name or documentation.
   Quoted:     def __init__(self, clock, ttl_seconds, token_factory=None):
   Judged: FalsePositive. Caller-supplied token factories are an intended injection point; the comment is a preference about naming and documentation.
9. Minor in tests/test_sessions.py: The test does not verify that the lockout actually blocks a correct password while locked in a way distinct from a wrong password, and it never checks that the window expiry is the reason for success. Also no coverage for revoke, revoke_user, token collision, invalid config, or expiry during get; the suite proves little about the store's behavior.
   Quoted:     assert login("bob", "secret", verify, store, throttle) is None     clock.now += 301
   Judged: FalsePositive. Comments on test coverage gaps rather than a code defect.
10. Minor in tests/test_sessions.py: Only the happy expiry path is tested. There are no tests for concurrency, the token collision RuntimeError, empty user_id rejection, or revoke_user, so the locking and error paths are unverified.
   Quoted: def test_session_expires():
   Judged: FalsePositive. Comments on missing tests rather than a code defect.

- planted-1 (correctness.boundary_and_empty_input): the review Missed. No finding mentions the boundary comparison in _recent, where failures exactly one window old are kept because of the >= cutoff.
- planted-2 (errors.overly_broad_catch): the review Found { finding: 4 }. Finding 5 names the overly broad except Exception around verify(), explaining that backend outages or bugs are silently treated as wrong-password attempts, which is the cause of this issue.
- planted-3 (concurrency.lock_ordering): the review HalfFound { finding: 2 }. Finding 3 says the two locks guarding the same dict add deadlock risk, which points at the lock design. It never identifies the inverted acquisition order between create (_lock then _index_lock) and revoke_user (_index_lock then _lock), so the cause is only partly named.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
