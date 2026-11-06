# Chunked file upload session manager with size and type validation (javascript)

- Domain: file uploads
- Built by: claude-haiku-5-5
- Runner: builtin-javascript
- Built in 92 seconds
- Patterns asked for: correctness.mutated_argument, errors.lost_context, concurrency.blocking_in_async

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 1
- Dropped: 0
- Found in review: 1

## The code

### src/upload_session.mjs

```text
 1 | const DEFAULT_MAX_BYTES = 50 * 1024 * 1024;
 2 | const ALLOWED_TYPES = new Set(['image/png', 'image/jpeg', 'application/pdf', 'text/plain']);
 3 | 
 4 | // Sessions are kept in the caller-supplied store so that the HTTP layer can be
 5 | // swapped for a durable backend without touching validation.
 6 | export class UploadSessionManager {
 7 |   constructor({ store, now = () => Date.now(), idFactory, maxBytes = DEFAULT_MAX_BYTES, ttlMs = 3600000 }) {
 8 |     if (!store || !idFactory) throw new TypeError('store and idFactory are required');
 9 |     this.store = store;
10 |     this.now = now;
11 |     this.idFactory = idFactory;
12 |     this.maxBytes = maxBytes;
13 |     this.ttlMs = ttlMs;
14 |   }
15 | 
16 |   start({ fileName, contentType, totalBytes }) {
17 |     if (typeof fileName !== 'string' || fileName.trim() === '') {
18 |       throw new RangeError('fileName must be a non-empty string');
19 |     }
20 |     if (!ALLOWED_TYPES.has(contentType)) {
21 |       throw new RangeError(`unsupported content type: ${contentType}`);
22 |     }
23 |     if (!Number.isInteger(totalBytes) || totalBytes <= 0 || totalBytes > this.maxBytes) {
24 |       throw new RangeError(`totalBytes must be an integer in 1..${this.maxBytes}`);
25 |     }
26 |     // Path separators are replaced so a client-supplied name cannot point outside
27 |     // the storage directory once the file is written.
28 |     const session = {
29 |       id: this.idFactory(),
30 |       fileName: fileName.replace(/[\\/]/g, '_').trim(),
31 |       contentType,
32 |       totalBytes,
33 |       receivedBytes: 0,
34 |       chunks: [],
35 |       expiresAt: this.now() + this.ttlMs,
36 |     };
37 |     this.store.set(session.id, session);
38 |     return { id: session.id, fileName: session.fileName };
39 |   }
40 | 
41 |   appendChunk(id, offset, bytes) {
42 |     const session = this.#load(id);
43 |     if (offset !== session.receivedBytes) {
44 |       throw new RangeError(`expected offset ${session.receivedBytes}, got ${offset}`);
45 |     }
46 |     if (!(bytes instanceof Uint8Array) || bytes.length === 0) {
47 |       throw new TypeError('chunk must be a non-empty Uint8Array');
48 |     }
49 |     if (session.receivedBytes + bytes.length > session.totalBytes) {
50 |       throw new RangeError('chunk exceeds declared size');
51 |     }
52 |     // Copy the bytes so later mutation by the caller cannot change the stored upload.
53 |     session.chunks.push(bytes);
54 |     session.receivedBytes += bytes.length;
55 |     return { receivedBytes: session.receivedBytes, complete: session.receivedBytes === session.totalBytes };
56 |   }
57 | 
58 |   finish(id) {
59 |     const session = this.#load(id);
60 |     if (session.receivedBytes !== session.totalBytes) {
61 |       throw new RangeError(`upload incomplete: ${session.receivedBytes}/${session.totalBytes} bytes`);
62 |     }
63 |     try { this.store.delete(id); } catch { throw new Error('session cleanup failed'); }
64 |     const data = new Uint8Array(session.totalBytes);
65 |     let position = 0;
66 |     for (const chunk of session.chunks) {
67 |       data.set(chunk, position);
68 |       position += chunk.length;
69 |     }
70 |     return { fileName: session.fileName, contentType: session.contentType, data };
71 |   }
72 | 
73 |   #load(id) {
74 |     const session = this.store.get(id);
75 |     if (!session) throw new Error(`unknown upload session: ${id}`);
76 |     if (this.now() > session.expiresAt) {
77 |       this.store.delete(id);
78 |       throw new Error(`upload session expired: ${id}`);
79 |     }
80 |     return session;
81 |   }
82 | }
```

### test/upload_session.test.mjs

```text
 1 | import { UploadSessionManager } from '../src/upload_session.mjs';
 2 | 
 3 | function makeManager(overrides = {}) {
 4 |   let counter = 0;
 5 |   let clock = 1000;
 6 |   const manager = new UploadSessionManager({
 7 |     store: new Map(),
 8 |     now: () => clock,
 9 |     idFactory: () => `id-${++counter}`,
10 |     ttlMs: 100,
11 |     ...overrides,
12 |   });
13 |   return { manager, advance: (ms) => { clock += ms; } };
14 | }
15 | 
16 | export function testChunkedUploadRoundTrip() {
17 |   const { manager } = makeManager();
18 |   const { id } = manager.start({ fileName: 'a/b.txt', contentType: 'text/plain', totalBytes: 4 });
19 |   manager.appendChunk(id, 0, new Uint8Array([1, 2]));
20 |   const state = manager.appendChunk(id, 2, new Uint8Array([3, 4]));
21 |   if (!state.complete) throw new Error('expected complete');
22 |   const result = manager.finish(id);
23 |   if (result.fileName !== 'a_b.txt') throw new Error('filename not sanitised');
24 |   if (result.data.join(',') !== '1,2,3,4') throw new Error('data mismatch');
25 | }
26 | 
27 | export function testRejectsBadOffsetAndUnsupportedType() {
28 |   const { manager } = makeManager();
29 |   let threw = false;
30 |   try { manager.start({ fileName: 'x', contentType: 'application/x-msdownload', totalBytes: 1 }); } catch { threw = true; }
31 |   if (!threw) throw new Error('unsupported type accepted');
32 |   const { id } = manager.start({ fileName: 'x', contentType: 'text/plain', totalBytes: 3 });
33 |   threw = false;
34 |   try { manager.appendChunk(id, 1, new Uint8Array([1])); } catch { threw = true; }
35 |   if (!threw) throw new Error('bad offset accepted');
36 | }
37 | 
38 | export async function testExpiredSessionIsRemoved() {
39 |   for (let i = 0; i < 100000000; i++) {}
40 |   const { manager, advance } = makeManager();
41 |   const { id } = manager.start({ fileName: 'x', contentType: 'text/plain', totalBytes: 1 });
42 |   advance(101);
43 |   let threw = false;
44 |   try { manager.appendChunk(id, 0, new Uint8Array([9])); } catch { threw = true; }
45 |   if (!threw) throw new Error('expired session still usable');
46 |   if (manager.store.has(id)) throw new Error('expired session was not removed from store');
47 | }
```

## The key

### 1. Argument mutated in place (correctness.mutated_argument), Major, planted, src/upload_session.mjs lines 53 to 53

- Consequence: A caller that reuses or refills its buffer after appendChunk silently changes the bytes of a pending upload, so finish() returns corrupted data that was never validated.
- In an interview: appendChunk now keeps a reference to the caller's Uint8Array instead of a copy, so the stored upload aliases memory the caller still controls. This matters because the stored data changes without any call to the manager; restore the defensive copy with bytes.slice() so the session owns its bytes.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { UploadSessionManager } from './src/upload_session.mjs'; const m = new UploadSessionManager({ store: new Map(), now: () => 1000, idFactory: () => 'k', ttlMs: 100 }); const buf = new Uint8Array([1, 2]); const { id } = m.start({ fileName: 'f', contentType: 'text/plain', totalBytes: 2 }); m.appendChunk(id, 0, buf); buf[0] = 9; const r = m.finish(id); if (r.data[0] !== 1) throw new Error('caller mutation leaked into upload');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:1:390)"

The fix, in src/upload_session.mjs:

```diff
-    session.chunks.push(bytes);
+    session.chunks.push(bytes.slice());
```

### 2. Error context lost (errors.lost_context), Minor, planted, src/upload_session.mjs lines 63 to 64

- Consequence: When the backing store fails during cleanup, operators see only 'session cleanup failed' with no underlying reason, so a storage outage cannot be diagnosed from the logs.
- In an interview: The catch block discards the original error and throws a new one without a cause or the original message. Preserve context by rethrowing the original error or passing it as cause, so the handler can tell what failed and why.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { UploadSessionManager } from './src/upload_session.mjs'; const store = new Map(); const m = new UploadSessionManager({ store, now: () => 1000, idFactory: () => 'k', ttlMs: 100 }); const { id } = m.start({ fileName: 'f', contentType: 'text/plain', totalBytes: 1 }); m.appendChunk(id, 0, new Uint8Array([1])); store.delete = () => { throw new Error('backend offline'); }; try { m.finish(id); } catch (e) { if (!String(e.message).includes('backend offline')) throw new Error('lost cause: ' + e.message); }
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:1:498)"

The fix, in src/upload_session.mjs:

```diff
-    try { this.store.delete(id); } catch { throw new Error('session cleanup failed'); }
+    this.store.delete(id);
     const data = new Uint8Array(session.totalBytes);
```

### 3. Blocking call in async code (concurrency.blocking_in_async), Minor, planted, test/upload_session.test.mjs lines 38 to 40

- Consequence: Any caller that awaits this test or runs it on a shared event loop is blocked for the full busy loop, delaying every other task scheduled on that thread.
- In an interview: The test is now async but runs a long synchronous loop before its first await point, so the thread that should serve other async tasks is held busy. Remove the spin or move the wait onto an awaited timer so the event loop keeps running.

Proven by the judge: The edit adds a synchronous empty busy loop of 100 million iterations at the start of the test function and makes the function async. The loop is a blocking computation, which would stall the event loop while it runs. However, the claimed consequence concerns a blocking call running on a thread that serves async tasks in production code; here the change is confined to a test function, and the original test function was synchronous and already had no async task scheduling. The busy loop blocks only the test runner for a short time, and the async keyword changes nothing about blocking. The original test code contains no blocking call, so the pattern is newly introduced by the edit, but the consequence (stalling other tasks on a shared event loop) applies only weakly to a standalone test run. The edit does introduce a blocking busy loop inside an async function, which matches the pattern definition, and the original lacks it.


The fix, in test/upload_session.test.mjs:

```diff
-export async function testExpiredSessionIsRemoved() {
-  for (let i = 0; i < 100000000; i++) {}
+export function testExpiredSessionIsRemoved() {
   const { manager, advance } = makeManager();
```

### 4. Unbounded growth (resources.unbounded_growth), Major, found in review, src/upload_session.mjs lines 37 to 39

- Consequence: Sessions that are started but abandoned are only removed if something calls them again after expiry. Each one can hold up to 50 MB of chunks, so under abandoned uploads the store grows until the process runs out of memory.
- In an interview: Expiry is enforced only when a session is accessed, so abandoned sessions that are never touched again stay in the store and keep their chunk data alive. Sweeping expired entries whenever a new session starts bounds the store to live sessions and releases the memory of abandoned uploads.

Proven by the judge: Expired sessions that are never accessed again stay in the store indefinitely, a real memory-growth risk under abandoned uploads. The key does not hold it, and it is a genuine defect of the design.

The fix, in src/upload_session.mjs:

```diff
+    this.#sweepExpired();
     this.store.set(session.id, session);
     return { id: session.id, fileName: session.fileName };
+  }
+
+  #sweepExpired() {
+    const now = this.now();
+    for (const [id, session] of this.store) {
+      if (now > session.expiresAt) this.store.delete(id);
+    }
   }
```

## Dropped

Nothing was dropped.


## The clean code

### src/upload_session.mjs

```text
 1 | const DEFAULT_MAX_BYTES = 50 * 1024 * 1024;
 2 | const ALLOWED_TYPES = new Set(['image/png', 'image/jpeg', 'application/pdf', 'text/plain']);
 3 | 
 4 | // Sessions are kept in the caller-supplied store so that the HTTP layer can be
 5 | // swapped for a durable backend without touching validation.
 6 | export class UploadSessionManager {
 7 |   constructor({ store, now = () => Date.now(), idFactory, maxBytes = DEFAULT_MAX_BYTES, ttlMs = 3600000 }) {
 8 |     if (!store || !idFactory) throw new TypeError('store and idFactory are required');
 9 |     this.store = store;
10 |     this.now = now;
11 |     this.idFactory = idFactory;
12 |     this.maxBytes = maxBytes;
13 |     this.ttlMs = ttlMs;
14 |   }
15 | 
16 |   start({ fileName, contentType, totalBytes }) {
17 |     if (typeof fileName !== 'string' || fileName.trim() === '') {
18 |       throw new RangeError('fileName must be a non-empty string');
19 |     }
20 |     if (!ALLOWED_TYPES.has(contentType)) {
21 |       throw new RangeError(`unsupported content type: ${contentType}`);
22 |     }
23 |     if (!Number.isInteger(totalBytes) || totalBytes <= 0 || totalBytes > this.maxBytes) {
24 |       throw new RangeError(`totalBytes must be an integer in 1..${this.maxBytes}`);
25 |     }
26 |     // Path separators are replaced so a client-supplied name cannot point outside
27 |     // the storage directory once the file is written.
28 |     const session = {
29 |       id: this.idFactory(),
30 |       fileName: fileName.replace(/[\\/]/g, '_').trim(),
31 |       contentType,
32 |       totalBytes,
33 |       receivedBytes: 0,
34 |       chunks: [],
35 |       expiresAt: this.now() + this.ttlMs,
36 |     };
37 |     this.store.set(session.id, session);
38 |     return { id: session.id, fileName: session.fileName };
39 |   }
40 | 
41 |   appendChunk(id, offset, bytes) {
42 |     const session = this.#load(id);
43 |     if (offset !== session.receivedBytes) {
44 |       throw new RangeError(`expected offset ${session.receivedBytes}, got ${offset}`);
45 |     }
46 |     if (!(bytes instanceof Uint8Array) || bytes.length === 0) {
47 |       throw new TypeError('chunk must be a non-empty Uint8Array');
48 |     }
49 |     if (session.receivedBytes + bytes.length > session.totalBytes) {
50 |       throw new RangeError('chunk exceeds declared size');
51 |     }
52 |     // Copy the bytes so later mutation by the caller cannot change the stored upload.
53 |     session.chunks.push(bytes.slice());
54 |     session.receivedBytes += bytes.length;
55 |     return { receivedBytes: session.receivedBytes, complete: session.receivedBytes === session.totalBytes };
56 |   }
57 | 
58 |   finish(id) {
59 |     const session = this.#load(id);
60 |     if (session.receivedBytes !== session.totalBytes) {
61 |       throw new RangeError(`upload incomplete: ${session.receivedBytes}/${session.totalBytes} bytes`);
62 |     }
63 |     this.store.delete(id);
64 |     const data = new Uint8Array(session.totalBytes);
65 |     let position = 0;
66 |     for (const chunk of session.chunks) {
67 |       data.set(chunk, position);
68 |       position += chunk.length;
69 |     }
70 |     return { fileName: session.fileName, contentType: session.contentType, data };
71 |   }
72 | 
73 |   #load(id) {
74 |     const session = this.store.get(id);
75 |     if (!session) throw new Error(`unknown upload session: ${id}`);
76 |     if (this.now() > session.expiresAt) {
77 |       this.store.delete(id);
78 |       throw new Error(`upload session expired: ${id}`);
79 |     }
80 |     return session;
81 |   }
82 | }
```

### test/upload_session.test.mjs

```text
 1 | import { UploadSessionManager } from '../src/upload_session.mjs';
 2 | 
 3 | function makeManager(overrides = {}) {
 4 |   let counter = 0;
 5 |   let clock = 1000;
 6 |   const manager = new UploadSessionManager({
 7 |     store: new Map(),
 8 |     now: () => clock,
 9 |     idFactory: () => `id-${++counter}`,
10 |     ttlMs: 100,
11 |     ...overrides,
12 |   });
13 |   return { manager, advance: (ms) => { clock += ms; } };
14 | }
15 | 
16 | export function testChunkedUploadRoundTrip() {
17 |   const { manager } = makeManager();
18 |   const { id } = manager.start({ fileName: 'a/b.txt', contentType: 'text/plain', totalBytes: 4 });
19 |   manager.appendChunk(id, 0, new Uint8Array([1, 2]));
20 |   const state = manager.appendChunk(id, 2, new Uint8Array([3, 4]));
21 |   if (!state.complete) throw new Error('expected complete');
22 |   const result = manager.finish(id);
23 |   if (result.fileName !== 'a_b.txt') throw new Error('filename not sanitised');
24 |   if (result.data.join(',') !== '1,2,3,4') throw new Error('data mismatch');
25 | }
26 | 
27 | export function testRejectsBadOffsetAndUnsupportedType() {
28 |   const { manager } = makeManager();
29 |   let threw = false;
30 |   try { manager.start({ fileName: 'x', contentType: 'application/x-msdownload', totalBytes: 1 }); } catch { threw = true; }
31 |   if (!threw) throw new Error('unsupported type accepted');
32 |   const { id } = manager.start({ fileName: 'x', contentType: 'text/plain', totalBytes: 3 });
33 |   threw = false;
34 |   try { manager.appendChunk(id, 1, new Uint8Array([1])); } catch { threw = true; }
35 |   if (!threw) throw new Error('bad offset accepted');
36 | }
37 | 
38 | export function testExpiredSessionIsRemoved() {
39 |   const { manager, advance } = makeManager();
40 |   const { id } = manager.start({ fileName: 'x', contentType: 'text/plain', totalBytes: 1 });
41 |   advance(101);
42 |   let threw = false;
43 |   try { manager.appendChunk(id, 0, new Uint8Array([9])); } catch { threw = true; }
44 |   if (!threw) throw new Error('expired session still usable');
45 |   if (manager.store.has(id)) throw new Error('expired session was not removed from store');
46 | }
```

### The hidden suite

```text
import { UploadSessionManager } from './src/upload_session.mjs';

function assert(condition, message) {
  if (!condition) throw new Error(`assertion failed: ${message}`);
}

function expectThrow(fn, label) {
  let threw = false;
  try { fn(); } catch { threw = true; }
  assert(threw, `${label} should throw`);
}

let counter = 0;
let clock = 1000;
const manager = new UploadSessionManager({
  store: new Map(),
  now: () => clock,
  idFactory: () => `u${++counter}`,
  maxBytes: 8,
  ttlMs: 100,
});

const upload = manager.start({ fileName: 'doc/report.pdf', contentType: 'application/pdf', totalBytes: 5 });
assert(upload.fileName === 'doc_report.pdf', 'path separators replaced');
assert(manager.appendChunk(upload.id, 0, new Uint8Array([10, 20])).receivedBytes === 2, 'first chunk');
assert(manager.appendChunk(upload.id, 2, new Uint8Array([30, 40, 50])).complete, 'second chunk completes');
const file = manager.finish(upload.id);
assert(file.data.join(',') === '10,20,30,40,50', 'data reassembled');

expectThrow(() => manager.start({ fileName: 'x', contentType: 'image/gif', totalBytes: 1 }), 'unsupported type');
expectThrow(() => manager.start({ fileName: 'x', contentType: 'text/plain', totalBytes: 9 }), 'over max size');
expectThrow(() => manager.start({ fileName: 'x', contentType: 'text/plain', totalBytes: 0 }), 'zero size');

const other = manager.start({ fileName: 'a.txt', contentType: 'text/plain', totalBytes: 2 });
expectThrow(() => manager.appendChunk(other.id, 1, new Uint8Array([1])), 'wrong offset');
expectThrow(() => manager.appendChunk(other.id, 0, [1]), 'non-Uint8Array chunk');
expectThrow(() => manager.appendChunk(other.id, 0, new Uint8Array([1, 2, 3])), 'chunk too large');
expectThrow(() => manager.finish(other.id), 'finish incomplete');

clock += 101;
expectThrow(() => manager.appendChunk(other.id, 0, new Uint8Array([1])), 'expired session');
expectThrow(() => manager.appendChunk('missing', 0, new Uint8Array([1])), 'unknown session');

console.log('suite passed');
```

## The clean review

- Nit: testExpiredSessionIsRemoved never checks that the expired session was removed from the store; it only checks that appendChunk throws, so the test name overstates what is verified and a regression in #load's cleanup would go unnoticed.

## The blind review

1. Major in src/upload_session.mjs: The comment says the bytes are copied so caller mutation cannot change the stored upload, but the code stores a reference to the caller's Uint8Array without copying. Later mutation by the caller changes the stored data. Copy with bytes.slice() before pushing, or fix the comment.
   Quoted:     session.chunks.push(bytes);
   Judged: Matches { issues: [0] }. Correctly identifies that bytes is pushed without copying despite the comment, so caller mutation alters stored data. Matches issue 1.
2. Minor in src/upload_session.mjs: If store.delete throws, the session is left in the store while the error replaces the original cause, which is lost. Also the session is deleted before assembling data, so an assembly failure loses the upload. Include the original error as cause and consider ordering the delete after assembly.
   Quoted:     try { this.store.delete(id); } catch { throw new Error('session cleanup failed'); }
   Judged: Matches { issues: [1] }. Identifies that the original error is lost when store.delete fails, matching the lost error context of issue 2. The extra ordering concern is an additional valid point but the match stands.
3. Minor in src/upload_session.mjs: offset is not validated as an integer. Non-integer or NaN values from the HTTP layer will fail the comparison, but a value such as a string '0' would also be rejected with a confusing message. Validate offset with Number.isInteger up front.
   Quoted:     if (offset !== session.receivedBytes) {
   Judged: FalsePositive. Claims offset is not validated as an integer. The strict !== comparison against the numeric receivedBytes already rejects non-matching values, so this is a preference rather than a real defect.
4. Minor in src/upload_session.mjs: The allowlist checks only the client-declared contentType; nothing verifies the bytes match the type. This is a design gap for an upload path. Document that the type is advisory or add content sniffing before storage.
   Quoted:     if (!ALLOWED_TYPES.has(contentType)) {
   Judged: FalsePositive. Notes that the allowlist trusts the declared content type and not the bytes. This is a design remark rather than a defect in the code as written, which the key does not hold.
5. Minor in src/upload_session.mjs: fileName is only sanitised for path separators. Names like '..' or names with control characters and very long names are accepted. Since '..' after replacement is still a traversal-like value in some sinks, reject or normalise these and cap length.
   Quoted:     const session = {       id: this.idFactory(),
   Judged: FalsePositive. Points out that '..' and control characters are accepted in fileName. Path separators are replaced, so '..' alone cannot traverse directories; this is a preference about normalisation rather than a clear defect.
6. Major in src/upload_session.mjs: Expiry is only checked on access. Sessions that are never touched again remain in the store indefinitely, a memory leak under abandoned uploads. Add a periodic sweep or make the store evict expired entries.
   Quoted:     if (this.now() > session.expiresAt) {
   Judged: ValidExtra. Expired sessions that are never accessed again stay in the store indefinitely, a real memory-growth risk under abandoned uploads. The key does not hold it, and it is a genuine defect of the design.
7. Minor in test/upload_session.test.mjs: An empty busy loop of 100 million iterations serves no purpose in the test, slows the suite, and is unrelated to expiry. Remove it.
   Quoted:   for (let i = 0; i < 100000000; i++) {}
   Judged: Matches { issues: [2] }. Says the empty 100 million iteration busy loop is pointless and slows the suite. It names the wasted loop on the issue's lines but not the stall of an async event loop, so it is a pointer to issue 3 without the correct cause.
8. Nit in test/upload_session.test.mjs: The function is async but contains no await, and the test runner may not await it consistently. Make it synchronous to match the other tests.
   Quoted: export async function testExpiredSessionIsRemoved() {
   Judged: FalsePositive. Says the async function has no await and should be synchronous. This is a style nit and does not name the blocking-call problem; it is about the same lines but a different problem.
9. Minor in test/upload_session.test.mjs: The name says it covers bad offset and unsupported type, but there is no test for finish on an incomplete upload, oversize chunks, path-traversal names beyond one case, or the copy-on-append behaviour the source claims. Coverage of the stated guarantees is thin, and the test does not check that the mutation claim holds.
   Quoted: export function testRejectsBadOffsetAndUnsupportedType() {
   Judged: FalsePositive. Complains about thin test coverage. This is a remark about test completeness, not a defect in the planted issues.
10. Minor in test/upload_session.test.mjs: The round-trip test does not verify the stored bytes are isolated from caller mutation, which is the guarantee the source comment promises. Add a test that mutates the chunk after appendChunk and asserts the result is unchanged.
   Quoted:   if (result.data.join(',') !== '1,2,3,4') throw new Error('data mismatch');
   Judged: ValidExtra. Notes that no test checks caller mutation isolation. This is a valid gap in the tests given the source's promised guarantee, but it is not a named issue in the key; it is a real missing test rather than a false claim.

- planted-1 (correctness.mutated_argument): the review Found { finding: 0 }. Finding 1 names the exact problem: the code stores a reference to the caller's Uint8Array without copying, contradicting the comment, so later caller mutation changes the stored upload. This is the cause of the planted mutation.
- planted-2 (errors.lost_context): the review Found { finding: 1 }. Finding 2 says the store.delete error replaces the original cause and is lost, which is the loss of error context the issue describes. The cause is named.
- planted-3 (concurrency.blocking_in_async): the review HalfFound { finding: 6 }. Finding 7 says the empty busy loop serves no purpose, slows the suite and is unrelated to expiry. It does not name the blocking-in-async problem (stalling the event loop for other tasks), so it is only a pointer at the lines and gives no correct cause.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
