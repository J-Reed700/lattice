# Chunked file upload session manager with size limits and checksum verification (python)

- Domain: file uploads
- Built by: claude-haiku-5-5
- Runner: builtin-python
- Built in 110 seconds
- Patterns asked for: correctness.mutated_argument, errors.lost_context, concurrency.blocking_in_async

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 1
- Dropped: 0
- Found in review: 2

## The code

### src/upload_session.py

```text
 1 | import hashlib
 2 | import re
 3 | 
 4 | 
 5 | _SAFE_NAME = re.compile(r'^[A-Za-z0-9._-]{1,128}\Z')
 6 | _HEX_DIGITS = frozenset('0123456789abcdef')
 7 | 
 8 | 
 9 | class UploadError(Exception):
10 |     """Raised when an upload request is rejected."""
11 | 
12 | 
13 | class UploadSession:
14 |     """Accumulates ordered chunks of one file and verifies the final digest.
15 | 
16 |     Chunks may arrive as they are produced. The byte budget is enforced on
17 |     every write, so a client cannot exceed max_bytes before finishing.
18 |     Chunks are kept until finish() so that a retried chunk can be compared
19 |     byte for byte with the copy already held.
20 |     """
21 | 
22 |     def __init__(self, name, expected_sha256, max_bytes):
23 |         if not _SAFE_NAME.match(name) or name in ('.', '..'):
24 |             raise UploadError(f'unsafe file name: {name!r}')
25 |         if len(expected_sha256) != 64 or not _HEX_DIGITS.issuperset(expected_sha256):
26 |             raise UploadError('expected_sha256 must be 64 lowercase hex characters')
27 |         self.name = name
28 |         self._expected = expected_sha256
29 |         self._max_bytes = max_bytes
30 |         self._received = 0
31 |         self._next_index = 0
32 |         self._digest = hashlib.sha256()
33 |         self._parts = []
34 |         self.complete = False
35 | 
36 |     def add_chunk(self, index, data):
37 |         """Store chunk `index`. Returns False for an identical retry of a held chunk."""
38 |         if self.complete:
39 |             raise UploadError('session already completed')
40 |         if index < 0:
41 |             raise UploadError('chunk index must not be negative')
42 |         if index < self._next_index:
43 |             if bytes(data) != self._parts[index]:
44 |                 raise UploadError(f'chunk {index} was already received with different data')
45 |             return False
46 |         if index != self._next_index:
47 |             raise UploadError(f'expected chunk {self._next_index}, got {index}')
48 |         if self._received + len(data) > self._max_bytes:
49 |             raise UploadError('upload exceeds the size limit')
50 |         self._received += len(data)
51 |         self._digest.update(data)
52 |         self._parts.append(data)
53 |         self._next_index += 1
54 |         return True
55 | 
56 |     def finish(self):
57 |         """Return the assembled bytes once the digest matches the expected value."""
58 |         if self.complete:
59 |             raise UploadError('session already completed')
60 |         if self._digest.hexdigest() != self._expected:
61 |             raise UploadError('checksum mismatch')
62 |         self.complete = True
63 |         return b''.join(self._parts)
```

### src/upload_store.py

```text
 1 | from src.upload_session import UploadError, UploadSession
 2 | 
 3 | 
 4 | class UploadStore:
 5 |     """Tracks open upload sessions by id and caps how many may be open at once."""
 6 | 
 7 |     def __init__(self, max_open, new_id):
 8 |         if max_open < 1:
 9 |             raise UploadError('max_open must be at least 1')
10 |         self._max_open = max_open
11 |         self._new_id = new_id
12 |         self._sessions = {}
13 | 
14 |     def open(self, name, expected_sha256, max_bytes):
15 |         if len(self._sessions) >= self._max_open:
16 |             raise UploadError('too many open uploads')
17 |         session_id = self._new_id()
18 |         if session_id in self._sessions:
19 |             raise UploadError('duplicate session id')
20 |         self._sessions[session_id] = UploadSession(name, expected_sha256, max_bytes)
21 |         return session_id
22 | 
23 |     def get(self, session_id):
24 |         try:
25 |             return self._sessions[session_id]
26 |         except KeyError:
27 |             raise UploadError('upload failed') from None
28 | 
29 |     async def wait_idle(self, session_id):
30 |         while len(self._sessions) >= self._max_open:
31 |             sum(range(10**7))
32 |         return self.get(session_id)
33 | 
34 |     def close(self, session_id):
35 |         session = self.get(session_id)
36 |         if not session.complete:
37 |             raise UploadError('upload not finished')
38 |         del self._sessions[session_id]
```

### tests/test_upload.py

```text
 1 | import hashlib
 2 | 
 3 | from src.upload_session import UploadError, UploadSession
 4 | from src.upload_store import UploadStore
 5 | 
 6 | 
 7 | def digest(data):
 8 |     return hashlib.sha256(data).hexdigest()
 9 | 
10 | 
11 | def expect_error(fn):
12 |     try:
13 |         fn()
14 |     except UploadError:
15 |         return
16 |     raise AssertionError('expected UploadError')
17 | 
18 | 
19 | def test_roundtrip():
20 |     payload = b'hello world, chunked'
21 |     s = UploadSession('a.txt', digest(payload), 100)
22 |     assert s.add_chunk(0, b'hello ') is True
23 |     assert s.add_chunk(1, b'world, ') is True
24 |     assert s.add_chunk(2, b'chunked') is True
25 |     assert s.finish() == payload
26 |     assert s.complete
27 | 
28 | 
29 | def test_retry_identical_chunk_is_noop():
30 |     s = UploadSession('a.txt', digest(b'ab'), 10)
31 |     assert s.add_chunk(0, b'a') is True
32 |     assert s.add_chunk(0, b'a') is False
33 |     expect_error(lambda: s.add_chunk(0, b'z'))
34 |     assert s.add_chunk(1, b'b') is True
35 |     assert s.finish() == b'ab'
36 | 
37 | 
38 | def test_gap_and_size_limit():
39 |     s = UploadSession('a.bin', digest(b'abc'), 3)
40 |     expect_error(lambda: s.add_chunk(1, b'b'))
41 |     expect_error(lambda: s.add_chunk(-1, b'b'))
42 |     s.add_chunk(0, b'ab')
43 |     expect_error(lambda: s.add_chunk(1, b'cd'))
44 | 
45 | 
46 | def test_checksum_mismatch():
47 |     s = UploadSession('a.bin', digest(b'xyz'), 10)
48 |     s.add_chunk(0, b'abc')
49 |     expect_error(s.finish)
50 |     assert not s.complete
51 | 
52 | 
53 | def test_bad_names_and_digests():
54 |     for bad in ['../etc', '..', 'a/b', '', 'x' * 129]:
55 |         expect_error(lambda b=bad: UploadSession(b, digest(b''), 1))
56 |     expect_error(lambda: UploadSession('ok.txt', 'ABC', 1))
57 |     expect_error(lambda: UploadSession('ok.txt', 'G' * 64, 1))
58 | 
59 | 
60 | def test_store_limits_and_close():
61 |     ids = iter(['s1', 's2', 's1'])
62 |     store = UploadStore(1, lambda: next(ids))
63 |     sid = store.open('f.txt', digest(b'q'), 5)
64 |     expect_error(lambda: store.open('g.txt', digest(b'q'), 5))
65 |     expect_error(lambda: store.close(sid))
66 |     store.get(sid).add_chunk(0, b'q')
67 |     store.get(sid).finish()
68 |     store.close(sid)
69 |     expect_error(lambda: store.get(sid))
70 |     store2 = UploadStore(2, lambda: 'same')
71 |     store2.open('a', digest(b''), 1)
72 |     expect_error(lambda: store2.open('b', digest(b''), 1))
73 | 
74 | 
75 | if __name__ == '__main__':
76 |     for name, fn in list(globals().items()):
77 |         if name.startswith('test_'):
78 |             fn()
79 |     print('all checks passed')
```

## The key

### 1. Argument mutated in place (correctness.mutated_argument), Major, planted, src/upload_session.py lines 52 to 52

- Consequence: When a caller passes a bytearray and reuses or overwrites that buffer after add_chunk returns, the stored chunk changes too. finish() then returns different bytes from the ones whose digest was verified, so a corrupted file can be written out as if it had passed the checksum.
- In an interview: The session keeps a reference to the caller's mutable buffer instead of an immutable copy, so the caller's later in-place writes alter the session's stored data. Copy the input when storing it (bytes(data)), as the retry comparison already assumes.

Proven by a test that fails on the drill and passes on the clean code:

```text
import hashlib
from src.upload_session import UploadSession
buf = bytearray(b'a')
s = UploadSession('a.txt', hashlib.sha256(b'a').hexdigest(), 10)
s.add_chunk(0, buf)
buf[:] = b'z'
assert s.finish() == b'a'
print('ok')
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/upload_session.py:

```diff
-        self._parts.append(data)
+        self._parts.append(bytes(data))
```

### 2. Error context lost (errors.lost_context), Minor, planted, src/upload_store.py lines 26 to 27

- Consequence: When a caller looks up a session id that does not exist, the error reports a generic 'upload failed' with no mention of the unknown session. Callers and logs cannot tell a bad id from other failures, which slows debugging of client mistakes.
- In an interview: The rewritten error drops the specific message that said which lookup failed, so the handler cannot tell what went wrong. Keep the descriptive message and the chained KeyError so the failure is diagnosable.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.upload_session import UploadError
from src.upload_store import UploadStore
s = UploadStore(1, lambda: 'x')
try:
    s.get('missing')
except UploadError as e:
    assert 'unknown' in str(e), str(e)
else:
    raise AssertionError('expected UploadError')
print('ok')
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/upload_store.py:

```diff
         except KeyError:
-            raise UploadError('upload failed') from None
+            raise UploadError('unknown upload session') from None
```

### 3. Blocking call in async code (concurrency.blocking_in_async), Major, planted, src/upload_store.py lines 29 to 35

- Consequence: When an async server awaits wait_idle, the busy computation runs on the event loop thread. Every other request on that loop stalls for the whole spin, which can make the service look hung under load.
- In an interview: The coroutine performs CPU-heavy work synchronously, so it never yields control to the event loop while it waits. Await asyncio.sleep between checks, or offload the work with asyncio.to_thread, so other tasks keep running.

Proven by the judge: The edit adds an async method wait_idle containing a busy-wait loop that runs sum(range(10**7)) synchronously and never awaits. Once invoked on the event loop, it blocks the loop thread for the whole spin, matching the claimed blocking-in-async consequence. The original code had no async code at all, so it lacks this problem. The edit introduces exactly this pattern (blocking CPU work in a coroutine with no yield point) and does not introduce other defects beyond the busy-wait nature of the same issue.


The fix, in src/upload_store.py:

```diff
-    async def wait_idle(self, session_id):
-        while len(self._sessions) >= self._max_open:
-            sum(range(10**7))
-        return self.get(session_id)
-
     def close(self, session_id):
         session = self.get(session_id)
```

### 4. Leaked handle or connection (resources.leaked_handle), Major, found in review, src/upload_store.py lines 34 to 34

- Consequence: A session that is abandoned or fails its checksum is never removed, so it keeps one of the max_open slots and its buffered chunks in memory. After enough abandoned uploads, open() rejects every new upload with 'too many open uploads' until the process restarts.
- In an interview: A session is acquired in open() but only released on the success path in close(), so every failed or abandoned upload leaks a slot and its buffered chunks. An explicit abort that deletes the session frees the slot on every failure path, and it should also run when an upload is abandoned.

Proven by the judge: Abandoned sessions cannot be removed, so they permanently consume slots. This is a real defect in close() but not the planted issue of the key, which concerns the blocking wait_idle and error context.

The fix, in src/upload_store.py:

```diff
+    def abort(self, session_id):
+        self.get(session_id)
+        del self._sessions[session_id]
+
     def close(self, session_id):
```

### 5. Unhandled boundary or empty input (correctness.boundary_and_empty_input), Major, found in review, src/upload_session.py lines 48 to 48

- Consequence: When a caller passes a memoryview whose items are wider than one byte, len(data) returns the item count rather than the byte count, so the max_bytes budget is undercounted and a client can store more data than the limit allows, up to the item size times the limit. The digest and the stored parts still use the true bytes, so the overrun is not caught later.
- In an interview: The budget check measures the chunk with len(), which counts items rather than bytes for a non-byte memoryview, so the size limit can be bypassed for those inputs. Normalising the chunk to bytes once, before the length check, makes the budget, the digest and the stored copy all describe the same data.

Proven by the judge: The claim about memoryview item sizes is a real concern: len() on a non-byte memoryview counts items, so the budget check can be wrong. The planted code is not the clean code's issue, so it is a valid extra risk.

The fix, in src/upload_session.py:

```diff
+        data = bytes(data)
         if self._received + len(data) > self._max_bytes:
```

## Dropped

Nothing was dropped.


## The clean code

### src/upload_session.py

```text
 1 | import hashlib
 2 | import re
 3 | 
 4 | 
 5 | _SAFE_NAME = re.compile(r'^[A-Za-z0-9._-]{1,128}\Z')
 6 | _HEX_DIGITS = frozenset('0123456789abcdef')
 7 | 
 8 | 
 9 | class UploadError(Exception):
10 |     """Raised when an upload request is rejected."""
11 | 
12 | 
13 | class UploadSession:
14 |     """Accumulates ordered chunks of one file and verifies the final digest.
15 | 
16 |     Chunks may arrive as they are produced. The byte budget is enforced on
17 |     every write, so a client cannot exceed max_bytes before finishing.
18 |     Chunks are kept until finish() so that a retried chunk can be compared
19 |     byte for byte with the copy already held.
20 |     """
21 | 
22 |     def __init__(self, name, expected_sha256, max_bytes):
23 |         if not _SAFE_NAME.match(name) or name in ('.', '..'):
24 |             raise UploadError(f'unsafe file name: {name!r}')
25 |         if len(expected_sha256) != 64 or not _HEX_DIGITS.issuperset(expected_sha256):
26 |             raise UploadError('expected_sha256 must be 64 lowercase hex characters')
27 |         self.name = name
28 |         self._expected = expected_sha256
29 |         self._max_bytes = max_bytes
30 |         self._received = 0
31 |         self._next_index = 0
32 |         self._digest = hashlib.sha256()
33 |         self._parts = []
34 |         self.complete = False
35 | 
36 |     def add_chunk(self, index, data):
37 |         """Store chunk `index`. Returns False for an identical retry of a held chunk."""
38 |         if self.complete:
39 |             raise UploadError('session already completed')
40 |         if index < 0:
41 |             raise UploadError('chunk index must not be negative')
42 |         if index < self._next_index:
43 |             if bytes(data) != self._parts[index]:
44 |                 raise UploadError(f'chunk {index} was already received with different data')
45 |             return False
46 |         if index != self._next_index:
47 |             raise UploadError(f'expected chunk {self._next_index}, got {index}')
48 |         if self._received + len(data) > self._max_bytes:
49 |             raise UploadError('upload exceeds the size limit')
50 |         self._received += len(data)
51 |         self._digest.update(data)
52 |         self._parts.append(bytes(data))
53 |         self._next_index += 1
54 |         return True
55 | 
56 |     def finish(self):
57 |         """Return the assembled bytes once the digest matches the expected value."""
58 |         if self.complete:
59 |             raise UploadError('session already completed')
60 |         if self._digest.hexdigest() != self._expected:
61 |             raise UploadError('checksum mismatch')
62 |         self.complete = True
63 |         return b''.join(self._parts)
```

### src/upload_store.py

```text
 1 | from src.upload_session import UploadError, UploadSession
 2 | 
 3 | 
 4 | class UploadStore:
 5 |     """Tracks open upload sessions by id and caps how many may be open at once."""
 6 | 
 7 |     def __init__(self, max_open, new_id):
 8 |         if max_open < 1:
 9 |             raise UploadError('max_open must be at least 1')
10 |         self._max_open = max_open
11 |         self._new_id = new_id
12 |         self._sessions = {}
13 | 
14 |     def open(self, name, expected_sha256, max_bytes):
15 |         if len(self._sessions) >= self._max_open:
16 |             raise UploadError('too many open uploads')
17 |         session_id = self._new_id()
18 |         if session_id in self._sessions:
19 |             raise UploadError('duplicate session id')
20 |         self._sessions[session_id] = UploadSession(name, expected_sha256, max_bytes)
21 |         return session_id
22 | 
23 |     def get(self, session_id):
24 |         try:
25 |             return self._sessions[session_id]
26 |         except KeyError:
27 |             raise UploadError('unknown upload session') from None
28 | 
29 |     def close(self, session_id):
30 |         session = self.get(session_id)
31 |         if not session.complete:
32 |             raise UploadError('upload not finished')
33 |         del self._sessions[session_id]
```

### tests/test_upload.py

```text
 1 | import hashlib
 2 | 
 3 | from src.upload_session import UploadError, UploadSession
 4 | from src.upload_store import UploadStore
 5 | 
 6 | 
 7 | def digest(data):
 8 |     return hashlib.sha256(data).hexdigest()
 9 | 
10 | 
11 | def expect_error(fn):
12 |     try:
13 |         fn()
14 |     except UploadError:
15 |         return
16 |     raise AssertionError('expected UploadError')
17 | 
18 | 
19 | def test_roundtrip():
20 |     payload = b'hello world, chunked'
21 |     s = UploadSession('a.txt', digest(payload), 100)
22 |     assert s.add_chunk(0, b'hello ') is True
23 |     assert s.add_chunk(1, b'world, ') is True
24 |     assert s.add_chunk(2, b'chunked') is True
25 |     assert s.finish() == payload
26 |     assert s.complete
27 | 
28 | 
29 | def test_retry_identical_chunk_is_noop():
30 |     s = UploadSession('a.txt', digest(b'ab'), 10)
31 |     assert s.add_chunk(0, b'a') is True
32 |     assert s.add_chunk(0, b'a') is False
33 |     expect_error(lambda: s.add_chunk(0, b'z'))
34 |     assert s.add_chunk(1, b'b') is True
35 |     assert s.finish() == b'ab'
36 | 
37 | 
38 | def test_gap_and_size_limit():
39 |     s = UploadSession('a.bin', digest(b'abc'), 3)
40 |     expect_error(lambda: s.add_chunk(1, b'b'))
41 |     expect_error(lambda: s.add_chunk(-1, b'b'))
42 |     s.add_chunk(0, b'ab')
43 |     expect_error(lambda: s.add_chunk(1, b'cd'))
44 | 
45 | 
46 | def test_checksum_mismatch():
47 |     s = UploadSession('a.bin', digest(b'xyz'), 10)
48 |     s.add_chunk(0, b'abc')
49 |     expect_error(s.finish)
50 |     assert not s.complete
51 | 
52 | 
53 | def test_bad_names_and_digests():
54 |     for bad in ['../etc', '..', 'a/b', '', 'x' * 129]:
55 |         expect_error(lambda b=bad: UploadSession(b, digest(b''), 1))
56 |     expect_error(lambda: UploadSession('ok.txt', 'ABC', 1))
57 |     expect_error(lambda: UploadSession('ok.txt', 'G' * 64, 1))
58 | 
59 | 
60 | def test_store_limits_and_close():
61 |     ids = iter(['s1', 's2', 's1'])
62 |     store = UploadStore(1, lambda: next(ids))
63 |     sid = store.open('f.txt', digest(b'q'), 5)
64 |     expect_error(lambda: store.open('g.txt', digest(b'q'), 5))
65 |     expect_error(lambda: store.close(sid))
66 |     store.get(sid).add_chunk(0, b'q')
67 |     store.get(sid).finish()
68 |     store.close(sid)
69 |     expect_error(lambda: store.get(sid))
70 |     store2 = UploadStore(2, lambda: 'same')
71 |     store2.open('a', digest(b''), 1)
72 |     expect_error(lambda: store2.open('b', digest(b''), 1))
73 | 
74 | 
75 | if __name__ == '__main__':
76 |     for name, fn in list(globals().items()):
77 |         if name.startswith('test_'):
78 |             fn()
79 |     print('all checks passed')
```

### The hidden suite

```text
import hashlib

from src.upload_session import UploadError, UploadSession
from src.upload_store import UploadStore


def digest(data):
    return hashlib.sha256(data).hexdigest()


def raises(fn):
    try:
        fn()
    except UploadError:
        return True
    return False


def check_roundtrip():
    s = UploadSession('doc.pdf', digest(b'abcdef'), 6)
    assert s.add_chunk(0, b'abc')
    assert s.add_chunk(1, bytearray(b'def'))
    assert s.finish() == b'abcdef'
    assert raises(lambda: s.add_chunk(2, b'x'))
    assert raises(s.finish)


def check_retry_and_gap():
    s = UploadSession('a', digest(b'hi'), 2)
    assert s.add_chunk(0, b'h')
    assert s.add_chunk(0, b'h') is False
    assert raises(lambda: s.add_chunk(0, b'x'))
    assert raises(lambda: s.add_chunk(2, b'i'))
    assert s.add_chunk(1, b'i')
    assert s.finish() == b'hi'


def check_limits():
    s = UploadSession('a.bin', digest(b'abc'), 3)
    assert raises(lambda: s.add_chunk(0, b'abcd'))
    assert s.add_chunk(0, b'abc')
    assert not s.complete


def check_names_and_digests():
    assert raises(lambda: UploadSession('../x', digest(b''), 1))
    assert raises(lambda: UploadSession('', digest(b''), 1))
    assert raises(lambda: UploadSession('ok', 'A' * 64, 1))
    assert raises(lambda: UploadSession('ok', 'abc', 1))


def check_store():
    ids = iter(['one', 'one'])
    store = UploadStore(3, lambda: next(ids))
    sid = store.open('f', digest(b''), 1)
    assert raises(lambda: store.close(sid))
    assert raises(lambda: store.get('missing'))
    assert raises(lambda: store.open('g', digest(b''), 1))
    assert raises(lambda: UploadStore(0, lambda: 'x'))
    store.get(sid).finish()
    store.close(sid)
    assert raises(lambda: store.get(sid))


if __name__ == '__main__':
    check_roundtrip()
    check_retry_and_gap()
    check_limits()
    check_names_and_digests()
    check_store()
    print('all checks passed')
```

## The clean review

- Major: The file-name check uses `$` in `_SAFE_NAME`, which also matches just before a trailing newline. A name such as 'ok.txt\n' passes validation even though it is not a safe name. Use `\Z` so the whole string must match.

## The blind review

1. Critical in src/upload_store.py: This is a busy-wait that burns CPU for the full duration of the wait and blocks the event loop because it never awaits. Nothing ever frees a slot while the loop spins, so it can hang the process forever. Use asyncio.Event or Condition signaled from close(), or remove the method if it is not needed.
   Quoted:     async def wait_idle(self, session_id):         while len(self._sessions) >= self._max_open:             sum(range(10**7))
   Judged: Matches { issues: [2] }. Correctly identifies the non-awaiting busy-wait that blocks the event loop, matching issue 2.
2. Major in src/upload_store.py: close() only removes sessions that finished, so an abandoned or failed upload stays in the map forever and permanently consumes one of the max_open slots. Repeated abandoned uploads will exhaust capacity and deny service. Provide an abort/discard path that frees the slot and the buffered chunks.
   Quoted:     def close(self, session_id):         session = self.get(session_id)         if not session.complete:             raise UploadError('upload not finished')         del self._sessions[session_id]
   Judged: ValidExtra. Abandoned sessions cannot be removed, so they permanently consume slots. This is a real defect in close() but not the planted issue of the key, which concerns the blocking wait_idle and error context.
3. Minor in src/upload_store.py: The duplicate check raises instead of retrying, so a collision from the id generator fails the request. The tests even exercise this with a constant generator. Also the id source is injected with no guarantee of unpredictability; session ids act as bearer capabilities and should come from a secrets-based generator, with collisions handled by regenerating.
   Quoted:         session_id = self._new_id()         if session_id in self._sessions:             raise UploadError('duplicate session id')
   Judged: FalsePositive. Duplicate-id handling and id unpredictability are a preference or a hardening remark; not a defect the key holds, and not clearly a real defect in the clean code.
4. Minor in src/upload_session.py: Comparing bytes(data) copies the whole chunk on every retry, and the code keeps every chunk in memory until finish(). For large uploads this doubles memory pressure and CPU cost. Storing a per-chunk digest and comparing hashes would bound memory growth or at least avoid the copy.
   Quoted:         if index < self._next_index:             if bytes(data) != self._parts[index]:
   Judged: FalsePositive. Copying bytes on retry is a performance remark about a deliberate design choice; it is not a real defect in the planted sense.
5. Major in src/upload_session.py: The chunk is stored by reference without copying, so a mutable buffer (bytearray or memoryview) passed by the caller can be changed after acceptance, which would silently corrupt the assembled payload and make the digest disagree with the stored bytes. Copy with bytes(data) at ingestion.
   Quoted:         self._received += len(data)         self._digest.update(data)         self._parts.append(data)
   Judged: Matches { issues: [0] }. Names the aliasing cause: storing a caller's mutable buffer by reference lets later changes corrupt the payload and digest mismatch. This matches issue 1.
6. Minor in src/upload_session.py: len(data) is used as a byte count, which is wrong for memoryview with non-byte item sizes or other non-bytes buffers, and the size check is applied only after the index check. Normalize data to bytes first so the budget and digest use the same view of the data.
   Quoted:         if self._received + len(data) > self._max_bytes:
   Judged: ValidExtra. The claim about memoryview item sizes is a real concern: len() on a non-byte memoryview counts items, so the budget check can be wrong. The planted code is not the clean code's issue, so it is a valid extra risk.
7. Minor in src/upload_session.py: The session keeps all chunks and the full payload after finish() returns, and finish() does not release _parts. The buffered data is retained for as long as the session stays in the store, so memory is not released after the result is consumed. Clear _parts once the bytes are returned.
   Quoted:     def finish(self):         """Return the assembled bytes once the digest matches the expected value."""         if self.complete:
   Judged: FalsePositive. Retaining chunks after finish is a memory-retention remark; a preference about lifetime rather than a defect of the key.
8. Minor in tests/test_upload.py: The test never covers wait_idle, the async method, so the busy-wait defect goes undetected. It also never checks that a failed or abandoned session can be removed, which hides the slot-leak problem. Add tests for both behaviors.
   Quoted: def test_store_limits_and_close():
   Judged: FalsePositive. Points at missing tests for wait_idle and abandoned sessions; it is a test-coverage remark, not a named cause of the key's issues.
9. Nit in tests/test_upload.py: The bad-name test loop does not check the 128-character boundary for valid names or the unicode/null-byte cases, and the mutable-buffer aliasing bug is not tested. Test that a caller mutating its bytearray after add_chunk does not change the result of finish().
   Quoted:     expect_error(lambda: UploadSession('ok.txt', 'G' * 64, 1))
   Judged: FalsePositive. Test-coverage nit about boundary and aliasing tests; it is a remark rather than a defect in the code.

- planted-1 (correctness.mutated_argument): the review Found { finding: 4 }. Finding 5 names the cause: the chunk is stored by reference, so a caller's mutable buffer changed after acceptance corrupts the payload and the digest disagrees with the stored bytes. This is the aliasing problem of the issue.
- planted-2 (errors.lost_context): the review Missed. No finding mentions the lost error context in the KeyError re-raise with a generic message.
- planted-3 (concurrency.blocking_in_async): the review Found { finding: 0 }. Finding 1 names the cause: the loop never awaits, blocks the event loop and spins CPU, so the event loop stalls. It matches the blocking-call pattern.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean code (asked again): claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
