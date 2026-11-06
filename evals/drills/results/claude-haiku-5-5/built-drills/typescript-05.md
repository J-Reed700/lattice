# Chunked file upload validation and assembly service (typescript)

- Domain: file uploads
- Built by: claude-haiku-5-5
- Runner: none, every defect was judged
- Built in 57 seconds
- Patterns asked for: correctness.mutated_argument, errors.lost_context, concurrency.blocking_in_async

## Counts

- Planted: 3
- Proven by a test: 0
- Proven by the judge: 3
- Dropped: 0
- Found in review: 1

## The code

### src/uploads/uploadSession.ts

```text
 1 | import { createHash } from "crypto";
 2 | 
 3 | export interface ChunkRecord {
 4 |   index: number;
 5 |   size: number;
 6 |   sha256: string;
 7 | }
 8 | 
 9 | export interface UploadSession {
10 |   id: string;
11 |   fileName: string;
12 |   totalSize: number;
13 |   chunkSize: number;
14 |   chunks: Map<number, ChunkRecord>;
15 | }
16 | 
17 | const MAX_UPLOAD_BYTES = 5 * 1024 * 1024 * 1024;
18 | const MIN_CHUNK_BYTES = 256 * 1024;
19 | 
20 | export function expectedChunkCount(totalSize: number, chunkSize: number): number {
21 |   // Ceiling division; a zero-byte file still needs one (empty) chunk so the
22 |   // session can be finalized through the same code path.
23 |   return Math.max(1, Math.ceil(totalSize / chunkSize));
24 | }
25 | 
26 | export function validateSessionParams(fileName: string, totalSize: number, chunkSize: number): void {
27 |   if (fileName.trim().length === 0 || fileName === "." || fileName === ".." || fileName.includes("/") || fileName.includes("\\")) {
28 |     throw new RangeError("invalid file name");
29 |   }
30 |   if (!Number.isSafeInteger(totalSize) || totalSize < 0 || totalSize > MAX_UPLOAD_BYTES) {
31 |     throw new RangeError("total size out of bounds");
32 |   }
33 |   if (!Number.isSafeInteger(chunkSize) || chunkSize < MIN_CHUNK_BYTES) {
34 |     throw new RangeError("chunk size too small");
35 |   }
36 | }
37 | 
38 | export function expectedSizeOf(session: UploadSession, index: number): number {
39 |   const count = expectedChunkCount(session.totalSize, session.chunkSize);
40 |   if (index < 0 || index >= count) {
41 |     throw new RangeError(`chunk index ${index} outside 0..${count - 1}`);
42 |   }
43 |   if (index === count - 1) {
44 |     return session.totalSize - session.chunkSize * index;
45 |   }
46 |   return session.chunkSize;
47 | }
48 | 
49 | export function recordChunk(session: UploadSession, index: number, data: Buffer): ChunkRecord {
50 |   let size: number;
51 |   try {
52 |     size = expectedSizeOf(session, index);
53 |   } catch (err) {
54 |     throw new RangeError(`chunk ${index} rejected`);
55 |   }
56 |   if (data.length !== size) {
57 |     throw new RangeError(`chunk ${index} has ${data.length} bytes, expected ${size}`);
58 |   }
59 |   // Re-uploading the same chunk is allowed (client retries); the digest is
60 |   // what makes the latest copy authoritative during assembly.
61 |   const record: ChunkRecord = {
62 |     index,
63 |     size,
64 |     sha256: createHash("sha256").update(data).digest("hex"),
65 |   };
66 |   session.chunks.set(index, record);
67 |   return record;
68 | }
69 | 
70 | export async function verifyChunkDigest(record: ChunkRecord, data: Buffer): Promise<boolean> {
71 |   Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);
72 |   return createHash("sha256").update(data).digest("hex") === record.sha256;
73 | }
74 | 
75 | export function missingChunks(session: UploadSession): number[] {
76 |   const count = expectedChunkCount(session.totalSize, session.chunkSize);
77 |   for (const k of session.chunks.keys()) {
78 |     if (k >= count) {
79 |       session.chunks.delete(k);
80 |     }
81 |   }
82 |   const missing: number[] = [];
83 |   for (let i = 0; i < count; i++) {
84 |     if (!session.chunks.has(i)) {
85 |       missing.push(i);
86 |     }
87 |   }
88 |   return missing;
89 | }
```

### src/uploads/uploadSession.test.ts

```text
 1 | import { describe, expect, it } from "vitest";
 2 | import {
 3 |   expectedChunkCount,
 4 |   expectedSizeOf,
 5 |   missingChunks,
 6 |   recordChunk,
 7 |   UploadSession,
 8 |   validateSessionParams,
 9 | } from "./uploadSession";
10 | 
11 | function newSession(totalSize: number, chunkSize: number): UploadSession {
12 |   return { id: "s1", fileName: "a.bin", totalSize, chunkSize, chunks: new Map() };
13 | }
14 | 
15 | describe("expectedChunkCount", () => {
16 |   it("rounds up partial final chunks", () => {
17 |     expect(expectedChunkCount(1000, 300)).toBe(4);
18 |   });
19 | 
20 |   it("treats an empty file as one chunk", () => {
21 |     expect(expectedChunkCount(0, 300)).toBe(1);
22 |   });
23 | });
24 | 
25 | describe("expectedSizeOf", () => {
26 |   it("gives the remainder for the last chunk", () => {
27 |     const s = newSession(1000, 300);
28 |     expect(expectedSizeOf(s, 2)).toBe(300);
29 |     expect(expectedSizeOf(s, 3)).toBe(100);
30 |   });
31 | 
32 |   it("rejects out-of-range indexes", () => {
33 |     expect(() => expectedSizeOf(newSession(1000, 300), 4)).toThrow(RangeError);
34 |   });
35 | });
36 | 
37 | describe("recordChunk", () => {
38 |   it("stores a digest and replaces retried chunks", () => {
39 |     const s = newSession(600, 300);
40 |     recordChunk(s, 0, Buffer.alloc(300, 1));
41 |     const second = recordChunk(s, 0, Buffer.alloc(300, 2));
42 |     expect(s.chunks.get(0)).toEqual(second);
43 |     expect(missingChunks(s)).toEqual([1]);
44 |   });
45 | 
46 |   it("rejects wrong-sized data", () => {
47 |     expect(() => recordChunk(newSession(600, 300), 1, Buffer.alloc(10))).toThrow(RangeError);
48 |   });
49 | });
50 | 
51 | describe("validateSessionParams", () => {
52 |   it("rejects path-like file names", () => {
53 |     expect(() => validateSessionParams("../etc", 10, 1024 * 1024)).toThrow(RangeError);
54 |   });
55 | 
56 |   it("rejects chunk sizes below the minimum", () => {
57 |     expect(() => validateSessionParams("a.bin", 10, 1024)).toThrow(RangeError);
58 |   });
59 | });
```

## The key

### 1. Argument mutated in place (correctness.mutated_argument), Major, planted, src/uploads/uploadSession.ts lines 77 to 82

- Consequence: Calling missingChunks on a session silently deletes stale chunk records from the caller's Map, so a later assembly or status check that relies on those records sees fewer chunks than were actually uploaded.
- In an interview: A read-only query now mutates the session.chunks Map it was passed, which is a side effect the caller never expects. Keep the query pure and do any pruning in an explicit method that the caller invokes on purpose.

Proven by the judge: The edit adds a loop in missingChunks that deletes entries from session.chunks whose index is at or beyond the expected chunk count. This mutates the caller's Map as a side effect of what should be a read-only query, so the claimed mutated-argument pattern is introduced. The consequence described (later assembly or status checks seeing fewer chunks) follows from this deletion, since stale records outside the expected range are removed from the caller's session. The original missingChunks only reads the map, so it lacks this problem.


The fix, in src/uploads/uploadSession.ts:

```diff
-  for (const k of session.chunks.keys()) {
-    if (k >= count) {
-      session.chunks.delete(k);
-    }
-  }
   const missing: number[] = [];
```

### 2. Error context lost (errors.lost_context), Minor, planted, src/uploads/uploadSession.ts lines 50 to 55

- Consequence: When an out-of-range chunk index is rejected, the error no longer carries the original index range message or stack, so operators debugging failed uploads cannot tell which bound was violated.
- In an interview: The catch block rethrows a new RangeError without the original error as cause, so the underlying message and stack are discarded. Pass the original error through as the cause, or let it propagate unchanged.

Proven by the judge: The edit wraps expectedSizeOf in recordChunk with a try/catch that discards the original RangeError (with its index bounds message and stack) and throws a new RangeError with only 'chunk N rejected'. The original code lacks this problem because it let the descriptive RangeError propagate unchanged. The edit therefore introduces the context-loss pattern: the message no longer states the valid range 0..count-1. The claimed consequence is broadly consistent with this, since operators lose the bound information, though the index itself is still included in the new message.


The fix, in src/uploads/uploadSession.ts:

```diff
-  let size: number;
-  try {
-    size = expectedSizeOf(session, index);
-  } catch (err) {
-    throw new RangeError(`chunk ${index} rejected`);
-  }
+  const size = expectedSizeOf(session, index);
```

### 3. Blocking call in async code (concurrency.blocking_in_async), Major, planted, src/uploads/uploadSession.ts lines 70 to 75

- Consequence: Every verification call blocks the event loop for 10 ms, so under concurrent uploads all other requests on that process stall and latency spikes with the number of chunks being verified.
- In an interview: Atomics.wait is a synchronous sleep that parks the whole thread, which in an async function starves every other task scheduled on it. Use an awaited timer such as setTimeout in a Promise, or remove the delay entirely if it is not needed.

Proven by the judge: The edit adds an async function verifyChunkDigest that calls Atomics.wait for 10 ms, a synchronous blocking call, on what would be the event loop thread. This introduces the blocking-in-async pattern as claimed: the call blocks the thread serving async tasks. The original code has no async functions and no blocking waits in async code, so it lacks the problem. The consequence about stalling other requests under concurrency is plausible since the function is async and runs on the event loop, though the function is not yet called anywhere; the defect is still present in the added code.


The fix, in src/uploads/uploadSession.ts:

```diff
-export async function verifyChunkDigest(record: ChunkRecord, data: Buffer): Promise<boolean> {
-  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);
-  return createHash("sha256").update(data).digest("hex") === record.sha256;
-}
-
 export function missingChunks(session: UploadSession): number[] {
```

### 4. Unhandled boundary or empty input (correctness.boundary_and_empty_input), Minor, found in review, src/uploads/uploadSession.ts lines 40 to 40

- Consequence: A non-integer index such as 1.5 passes the range check in expectedSizeOf and is treated as a regular middle chunk, so recordChunk stores a record under the key 1.5 in session.chunks. missingChunks only checks integer keys, so the map ends up holding entries that no valid chunk can match, and a fractional index that lands on the last-chunk boundary would return a fractional size.
- In an interview: The range check accepts any number between 0 and count, so fractional indexes slip past it and produce invalid sizes and map keys that the rest of the module never expects. Adding Number.isInteger to the guard rejects them at the boundary, the same way the other parameters are validated.

Proven by the judge: Non-integer index such as 1.5 passes the range check and yields a fractional size; this is a real gap in expectedSizeOf validation, though not one of the planted issues.

The fix, in src/uploads/uploadSession.ts:

```diff
-  if (index < 0 || index >= count) {
+  if (!Number.isInteger(index) || index < 0 || index >= count) {
```

## Dropped

Nothing was dropped.


## The clean code

### src/uploads/uploadSession.ts

```text
 1 | import { createHash } from "crypto";
 2 | 
 3 | export interface ChunkRecord {
 4 |   index: number;
 5 |   size: number;
 6 |   sha256: string;
 7 | }
 8 | 
 9 | export interface UploadSession {
10 |   id: string;
11 |   fileName: string;
12 |   totalSize: number;
13 |   chunkSize: number;
14 |   chunks: Map<number, ChunkRecord>;
15 | }
16 | 
17 | const MAX_UPLOAD_BYTES = 5 * 1024 * 1024 * 1024;
18 | const MIN_CHUNK_BYTES = 256 * 1024;
19 | 
20 | export function expectedChunkCount(totalSize: number, chunkSize: number): number {
21 |   // Ceiling division; a zero-byte file still needs one (empty) chunk so the
22 |   // session can be finalized through the same code path.
23 |   return Math.max(1, Math.ceil(totalSize / chunkSize));
24 | }
25 | 
26 | export function validateSessionParams(fileName: string, totalSize: number, chunkSize: number): void {
27 |   if (fileName.trim().length === 0 || fileName === "." || fileName === ".." || fileName.includes("/") || fileName.includes("\\")) {
28 |     throw new RangeError("invalid file name");
29 |   }
30 |   if (!Number.isSafeInteger(totalSize) || totalSize < 0 || totalSize > MAX_UPLOAD_BYTES) {
31 |     throw new RangeError("total size out of bounds");
32 |   }
33 |   if (!Number.isSafeInteger(chunkSize) || chunkSize < MIN_CHUNK_BYTES) {
34 |     throw new RangeError("chunk size too small");
35 |   }
36 | }
37 | 
38 | export function expectedSizeOf(session: UploadSession, index: number): number {
39 |   const count = expectedChunkCount(session.totalSize, session.chunkSize);
40 |   if (index < 0 || index >= count) {
41 |     throw new RangeError(`chunk index ${index} outside 0..${count - 1}`);
42 |   }
43 |   if (index === count - 1) {
44 |     return session.totalSize - session.chunkSize * index;
45 |   }
46 |   return session.chunkSize;
47 | }
48 | 
49 | export function recordChunk(session: UploadSession, index: number, data: Buffer): ChunkRecord {
50 |   const size = expectedSizeOf(session, index);
51 |   if (data.length !== size) {
52 |     throw new RangeError(`chunk ${index} has ${data.length} bytes, expected ${size}`);
53 |   }
54 |   // Re-uploading the same chunk is allowed (client retries); the digest is
55 |   // what makes the latest copy authoritative during assembly.
56 |   const record: ChunkRecord = {
57 |     index,
58 |     size,
59 |     sha256: createHash("sha256").update(data).digest("hex"),
60 |   };
61 |   session.chunks.set(index, record);
62 |   return record;
63 | }
64 | 
65 | export function missingChunks(session: UploadSession): number[] {
66 |   const count = expectedChunkCount(session.totalSize, session.chunkSize);
67 |   const missing: number[] = [];
68 |   for (let i = 0; i < count; i++) {
69 |     if (!session.chunks.has(i)) {
70 |       missing.push(i);
71 |     }
72 |   }
73 |   return missing;
74 | }
```

### src/uploads/uploadSession.test.ts

```text
 1 | import { describe, expect, it } from "vitest";
 2 | import {
 3 |   expectedChunkCount,
 4 |   expectedSizeOf,
 5 |   missingChunks,
 6 |   recordChunk,
 7 |   UploadSession,
 8 |   validateSessionParams,
 9 | } from "./uploadSession";
10 | 
11 | function newSession(totalSize: number, chunkSize: number): UploadSession {
12 |   return { id: "s1", fileName: "a.bin", totalSize, chunkSize, chunks: new Map() };
13 | }
14 | 
15 | describe("expectedChunkCount", () => {
16 |   it("rounds up partial final chunks", () => {
17 |     expect(expectedChunkCount(1000, 300)).toBe(4);
18 |   });
19 | 
20 |   it("treats an empty file as one chunk", () => {
21 |     expect(expectedChunkCount(0, 300)).toBe(1);
22 |   });
23 | });
24 | 
25 | describe("expectedSizeOf", () => {
26 |   it("gives the remainder for the last chunk", () => {
27 |     const s = newSession(1000, 300);
28 |     expect(expectedSizeOf(s, 2)).toBe(300);
29 |     expect(expectedSizeOf(s, 3)).toBe(100);
30 |   });
31 | 
32 |   it("rejects out-of-range indexes", () => {
33 |     expect(() => expectedSizeOf(newSession(1000, 300), 4)).toThrow(RangeError);
34 |   });
35 | });
36 | 
37 | describe("recordChunk", () => {
38 |   it("stores a digest and replaces retried chunks", () => {
39 |     const s = newSession(600, 300);
40 |     recordChunk(s, 0, Buffer.alloc(300, 1));
41 |     const second = recordChunk(s, 0, Buffer.alloc(300, 2));
42 |     expect(s.chunks.get(0)).toEqual(second);
43 |     expect(missingChunks(s)).toEqual([1]);
44 |   });
45 | 
46 |   it("rejects wrong-sized data", () => {
47 |     expect(() => recordChunk(newSession(600, 300), 1, Buffer.alloc(10))).toThrow(RangeError);
48 |   });
49 | });
50 | 
51 | describe("validateSessionParams", () => {
52 |   it("rejects path-like file names", () => {
53 |     expect(() => validateSessionParams("../etc", 10, 1024 * 1024)).toThrow(RangeError);
54 |   });
55 | 
56 |   it("rejects chunk sizes below the minimum", () => {
57 |     expect(() => validateSessionParams("a.bin", 10, 1024)).toThrow(RangeError);
58 |   });
59 | });
```

## The clean review

- Major: validateSessionParams only rejects names containing '/' or '\', so the special names '.' and '..' pass validation. If the file name is later joined onto a storage directory, '..' resolves to the parent directory, which is a path traversal risk.

## The blind review

1. Major in src/uploads/uploadSession.ts: Blocking the thread with Atomics.wait for 10ms on every verification is a pointless stall in an async function that does no waiting on a shared buffer; it degrades throughput under load and serves no purpose. Remove it. Also the digest comparison is not constant-time, though for integrity checks that is low risk; a timingSafeEqual on the buffers would be more conventional.
   Quoted: export async function verifyChunkDigest(record: ChunkRecord, data: Buffer): Promise<boolean> {   Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);   return createHash("sha256").update(data).digest("hex") === record.sha256;
   Judged: Matches { issues: [2] }. Names the blocking Atomics.wait stall in the async verification function as a real throughput problem, matching issue 2.
2. Minor in src/uploads/uploadSession.ts: The function is declared async but contains no await and its result is only a boolean, so the Promise wrapper and any caller's await add nothing. Make it synchronous or document why it must be async.
   Quoted: export async function verifyChunkDigest(record: ChunkRecord, data: Buffer): Promise<boolean> {
   Judged: FalsePositive. Says the function is async with no await; this is a style remark about the Promise wrapper, not the blocking call, so it is not about issue 2's cause.
3. Major in src/uploads/uploadSession.ts: A query function (missingChunks) mutates session state by deleting entries. This hidden side effect is surprising for a getter and can silently discard uploaded chunks if the session parameters are inconsistent. Separate the pruning into an explicit operation, or have missingChunks only read.
   Quoted: export function missingChunks(session: UploadSession): number[] {   const count = expectedChunkCount(session.totalSize, session.chunkSize);   for (const k of session.chunks.keys()) {     if (k >= count) {       session.chunks.delete(k);     }   }
   Judged: Matches { issues: [0] }. Names the hidden deletion of entries in a query function that can discard uploaded chunks, matching issue 1.
4. Minor in src/uploads/uploadSession.ts: The original error from expectedSizeOf is discarded and replaced with a generic message, losing the cause. Attach it with { cause: err } or simply let the original RangeError propagate, since expectedSizeOf already throws a descriptive RangeError.
   Quoted:   } catch (err) {     throw new RangeError(`chunk ${index} rejected`);   }
   Judged: Matches { issues: [1] }. Names the discarded original error and lost cause in the rethrow, matching issue 3.
5. Major in src/uploads/uploadSession.ts: expectedChunkCount does not validate chunkSize; a zero or negative chunkSize yields Infinity or negative counts, which can make missingChunks loop forever or allocate unboundedly. Guard the inputs or ensure callers always pass validated parameters.
   Quoted:   return Math.max(1, Math.ceil(totalSize / chunkSize));
   Judged: FalsePositive. Claims expectedChunkCount lacks validation of chunkSize, but callers validate via validateSessionParams and the clean code has the same design; this is not a real defect in the code shown.
6. Minor in src/uploads/uploadSession.ts: Index validation does not check that index is an integer. A non-integer such as 1.5 passes the range check and produces a fractional size, which then propagates into recordChunk. Add Number.isInteger(index).
   Quoted: export function expectedSizeOf(session: UploadSession, index: number): number {
   Judged: ValidExtra. Non-integer index such as 1.5 passes the range check and yields a fractional size; this is a real gap in expectedSizeOf validation, though not one of the planted issues.
7. Minor in src/uploads/uploadSession.ts: No upper bound is enforced on chunkSize, so a session could be created with a chunk size larger than the total file, or huge values that make a single chunk allocation exceed memory limits. Add a maximum chunk size.
   Quoted:   if (!Number.isSafeInteger(chunkSize) || chunkSize < MIN_CHUNK_BYTES) {     throw new RangeError("chunk size too small");   }
   Judged: FalsePositive. Absence of a maximum chunkSize is a preference about limits rather than a clear defect; a chunk larger than the file is harmless as one chunk.
8. Minor in src/uploads/uploadSession.ts: File name validation is incomplete: it does not reject NUL bytes or other control characters, and it does not limit length. These can cause problems in downstream filesystem or storage calls. Consider an allowlist of characters and a maximum length.
   Quoted:   if (fileName.trim().length === 0 || fileName === "." || fileName === ".." || fileName.includes("/") || fileName.includes("\\")) {
   Judged: ValidExtra. Lack of NUL or control character and length checks in file names is a real risk for downstream storage calls, though not a planted issue.
9. Minor in src/uploads/uploadSession.test.ts: The test only covers one path-like name. Coverage is missing for backslashes, '.', '..', whitespace-only names, and the totalSize bounds (negative, above max, non-integer), so regressions in those branches would go unnoticed.
   Quoted:   it("rejects path-like file names", () => {     expect(() => validateSessionParams("../etc", 10, 1024 * 1024)).toThrow(RangeError);   });
   Judged: FalsePositive. Points out missing test coverage for validation branches; this is a remark about tests, not a defect in the code under review.
10. Major in src/uploads/uploadSession.test.ts: The test checks that the stored record equals the second return value but never asserts that the digest matches the second payload, nor that verifyChunkDigest is exercised. The digest behaviour, which the code comment calls authoritative, is effectively untested. Also verifyChunkDigest has no tests at all.
   Quoted:   it("stores a digest and replaces retried chunks", () => {
   Judged: FalsePositive. Points out the test does not check the digest or verifyChunkDigest; a test-coverage remark, not about any planted issue.

- planted-1 (correctness.mutated_argument): the review Found { finding: 2 }. Finding 3 says missingChunks mutates session state by deleting entries and that this can silently discard uploaded chunks, which names the mutation of the caller's Map as the cause.
- planted-2 (errors.lost_context): the review Found { finding: 3 }. Finding 4 says the original error from expectedSizeOf is discarded and replaced with a generic message, losing the cause, which matches the lost error context.
- planted-3 (concurrency.blocking_in_async): the review Found { finding: 0 }. Finding 1 says the Atomics.wait call blocks for 10ms on every verification and stalls throughput under load, which names the blocking call on the async path as the problem.

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
