# Email notification digest builder with injected mailer and clock (javascript)

- Domain: notifications and email
- Built by: claude-haiku-5-5
- Runner: builtin-javascript
- Built in 96 seconds
- Patterns asked for: correctness.reference_equality, errors.retry_without_backoff, concurrency.lock_held_across_slow_call

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 0
- Dropped: 1
- Found in review: 3

## The code

### src/digest.mjs

```text
 1 | // Groups pending notifications per recipient and sends one digest email per
 2 | // recipient, instead of one email per event, so that busy users are not spammed.
 3 | // The mailer and the clock are injected so the scheduler stays testable.
 4 | 
 5 | const MAX_ITEMS_PER_DIGEST = 50;
 6 | 
 7 | export function groupByRecipient(notifications) {
 8 |   const groups = new Map();
 9 |   for (const note of notifications) {
10 |     if (!note || typeof note.recipient !== 'string' || note.recipient.length === 0) {
11 |       throw new TypeError('notification is missing a recipient');
12 |     }
13 |     if (groups.get(note.recipient) !== []) {
14 |       groups.set(note.recipient, []);
15 |     }
16 |     groups.get(note.recipient).push(note);
17 |   }
18 |   return groups;
19 | }
20 | 
21 | function renderBody(items, now) {
22 |   const lines = items.slice(0, MAX_ITEMS_PER_DIGEST).map((n) => `- ${n.summary}`);
23 |   const hidden = items.length - lines.length;
24 |   if (hidden > 0) {
25 |     lines.push(`...and ${hidden} more`);
26 |   }
27 |   return `You have ${items.length} new notification(s) as of ${new Date(now).toISOString()}:\n${lines.join('\n')}`;
28 | }
29 | 
30 | export async function sendDigests({ notifications, mailer, clock }) {
31 |   if (typeof mailer?.send !== 'function') {
32 |     throw new TypeError('mailer.send must be a function');
33 |   }
34 |   if (typeof clock !== 'function') {
35 |     throw new TypeError('clock must be a function');
36 |   }
37 |   const now = clock();
38 |   const groups = groupByRecipient(notifications);
39 |   const report = { sent: 0, failed: [] };
40 | 
41 |   for (const [recipient, items] of groups) {
42 |     // Failures are collected per recipient so one bad address does not block
43 |     // the rest of the batch.
44 |     try {
45 |       await mailer.send({
46 |         to: recipient,
47 |         subject: `${items.length} new notification(s)`,
48 |         body: renderBody(items, now),
49 |       });
50 |       report.sent += 1;
51 |     } catch (err) {
52 |       let retried = false;
53 |       while (!retried) {
54 |         try {
55 |           await mailer.send({
56 |             to: recipient,
57 |             subject: `${items.length} new notification(s)`,
58 |             body: renderBody(items, now),
59 |           });
60 |           report.sent += 1;
61 |           retried = true;
62 |         } catch (retryErr) {
63 |           retried = false;
64 |         }
65 |       }
66 |     }
67 |   }
68 |   return report;
69 | }
```

### test/digest.test.mjs

```text
 1 | import { groupByRecipient, sendDigests } from '../src/digest.mjs';
 2 | 
 3 | function fakeMailer(failFor = new Set()) {
 4 |   const sent = [];
 5 |   return {
 6 |     sent,
 7 |     async send(message) {
 8 |       if (failFor.has(message.to)) {
 9 |         throw new Error('bounced');
10 |       }
11 |       sent.push(message);
12 |     },
13 |   };
14 | }
15 | 
16 | export async function groupsAndSendsOneDigestPerRecipient() {
17 |   const notes = [
18 |     { recipient: 'a@x.test', summary: 'one' },
19 |     { recipient: 'b@x.test', summary: 'two' },
20 |     { recipient: 'a@x.test', summary: 'three' },
21 |   ];
22 |   const mailer = fakeMailer();
23 |   const report = await sendDigests({ notifications: notes, mailer, clock: () => 0 });
24 |   if (report.sent !== 2 || report.failed.length !== 0) {
25 |     throw new Error('expected two digests sent');
26 |   }
27 |   const a = mailer.sent.find((m) => m.to === 'a@x.test');
28 |   if (!a.subject.startsWith('2 ')) {
29 |     throw new Error('digest subject should count items');
30 |   }
31 | }
32 | 
33 | export async function reportsFailuresWithoutStoppingBatch() {
34 |   const notes = [
35 |     { recipient: 'bad@x.test', summary: 'x' },
36 |     { recipient: 'good@x.test', summary: 'y' },
37 |   ];
38 |   const mailer = fakeMailer(new Set(['bad@x.test']));
39 |   const report = await sendDigests({ notifications: notes, mailer, clock: () => 0 });
40 |   if (report.sent !== 1 || report.failed[0]?.recipient !== 'bad@x.test') {
41 |     throw new Error('failure should be reported and batch should continue');
42 |   }
43 | }
44 | 
45 | export function rejectsNotificationWithoutRecipient() {
46 |   let threw = false;
47 |   try {
48 |     groupByRecipient([{ summary: 'orphan' }]);
49 |   } catch (err) {
50 |     threw = err instanceof TypeError;
51 |   }
52 |   if (!threw) {
53 |     throw new Error('missing recipient must raise TypeError');
54 |   }
55 | }
56 | 
57 | export async function truncatesLongDigests() {
58 |   const notes = [];
59 |   for (let i = 0; i < 53; i++) {
60 |     notes.push({ recipient: 'c@x.test', summary: `n${i}` });
61 |   }
62 |   const mailer = fakeMailer();
63 |   await sendDigests({ notifications: notes, mailer, clock: () => 0 });
64 |   if (!mailer.sent[0].body.includes('...and 3 more')) {
65 |     throw new Error('long digest should note hidden items');
66 |   }
67 | }
```

## The key

### 1. Identity compared where value was meant (correctness.reference_equality), Major, planted, src/digest.mjs lines 13 to 13

- Consequence: Every notification resets its recipient's group to a fresh empty array, because a new array literal is never identical to the stored one. Only the last notification per recipient survives, so digests silently undercount and drop items whenever a user has several pending notifications.
- In an interview: The code compares an array to a freshly created array literal with !==, which tests identity, so the check is always true and the group is recreated on every event. The fix is to test for the key's presence with groups.has or to compare with undefined, not to compare against a new object, since a value-equal array is still a different reference.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { groupByRecipient } from './src/digest.mjs';
const g = groupByRecipient([{ recipient: 'a', summary: '1' }, { recipient: 'a', summary: '2' }]);
if (g.get('a').length !== 2) throw new Error('group lost items');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:3:40)"

The fix, in src/digest.mjs:

```diff
-    if (groups.get(note.recipient) !== []) {
+    if (!groups.has(note.recipient)) {
```

### 2. Retry without backoff (errors.retry_without_backoff), Major, planted, src/digest.mjs lines 52 to 65

- Consequence: When the mail provider rejects a send, the code calls it again immediately in a tight loop with no delay and no attempt limit. A provider outage or rate limit gets hammered until it recovers, and a bounced address keeps the sender spinning forever, stalling the whole batch.
- In an interview: A failed send is retried at once with no delay and no cap on attempts, so the retry loop adds load exactly when the dependency is struggling. The fix is a bounded number of attempts with exponential backoff and jitter, after which the failure is recorded in report.failed.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { sendDigests } from './src/digest.mjs';
let calls = 0;
const mailer = { async send(m) { calls++; if (calls <= 2) throw new Error('blip'); } };
const report = await sendDigests({ notifications: [{ recipient: 'a', summary: 'x' }], mailer, clock: () => 0 });
if (calls !== 1 || report.failed.length !== 1) throw new Error('failed send must not be retried');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:5:58)"

The fix, in src/digest.mjs:

```diff
-      let retried = false;
-      while (!retried) {
-        try {
-          await mailer.send({
-            to: recipient,
-            subject: `${items.length} new notification(s)`,
-            body: renderBody(items, now),
-          });
-          report.sent += 1;
-          retried = true;
-        } catch (retryErr) {
-          retried = false;
-        }
-      }
+      report.failed.push({ recipient, reason: err instanceof Error ? err.message : String(err) });
```

### 3. Swallowed exception (errors.swallowed_exception), Major, found in review, src/digest.mjs lines 63 to 64

- Consequence: When a recipient keeps failing, the retry loop spins forever while discarding every error, so no operator can see why delivery failed or that the batch is stuck on one address.
- In an interview: The inner catch discards retryErr, so the real cause of each failed retry is lost. Keeping the most recent error on the report gives operators the diagnostic detail they need without changing the retry flow.

Proven by the judge: Points at the retry block and notes that the retry error is swallowed, which is a real defect in that code. It is about lost diagnostics, not the lack of backoff, so it does not name the cause of issue 2. It is not a planted issue of the key, so it is a valid extra.

The fix, in src/digest.mjs:

```diff
 retried = false;
+          report.lastError = retryErr;
         }
```

### 4. Swallowed exception (errors.swallowed_exception), Major, found in review, src/digest.mjs lines 51 to 52

- Consequence: When a recipient's send fails, the error is bound to err and then discarded, and report.failed stays empty, so callers cannot learn which digests were not delivered.
- In an interview: The catch block receives the error but never records it, so report.failed, which callers rely on, is never filled in. Recording the recipient and error at the point of failure makes the report reflect what actually went wrong, though the unbounded retry loop beneath it still needs its own fix.

Proven by the judge: report.failed is never populated, so failed recipients are not reported; this is a real defect in the code as shown, though not a planted issue of the key.

The fix, in src/digest.mjs:

```diff
     } catch (err) {
+      report.failed.push({ recipient, error: err });
       let retried = false;
```

### 5. Retry without backoff (errors.retry_without_backoff), Critical, found in review, src/digest.mjs lines 51 to 66

- Consequence: The retry loop has no attempt limit and no delay. A permanently bouncing address (the test's 'bad@x.test') makes sendDigests spin forever, so the whole batch never finishes and the test hangs. The failure is also never recorded in report.failed, so the test's expectation of a failed entry could never pass even if the loop ended.
- In an interview: A retry loop with no cap and no backoff turns one permanently failing recipient into an infinite spin that blocks every later recipient in the batch. The fix records the failure once and moves on, which also gives report.failed the recipient-bearing entries the test expects.

Proven by the judge: Notes that the test expects failed entries the implementation never adds, and that the unbounded retry would hang the test; this is a real consequence in the code, though it concerns the test rather than a planted issue.

The fix, in src/digest.mjs:

```diff
     } catch (err) {
-      let retried = false;
-      while (!retried) {
-        try {
-          await mailer.send({
-            to: recipient,
-            subject: `${items.length} new notification(s)`,
-            body: renderBody(items, now),
-          });
-          report.sent += 1;
-          retried = true;
-        } catch (retryErr) {
-          retried = false;
-        }
-      }
+      report.failed.push({ recipient, error: err });
     }
```

## Dropped

- concurrency.lock_held_across_slow_call: The original loop already awaits each mailer.send in sequence, so a hung or slow SMTP call already stalls every later recipient; the original holds no lock at all. The edit wraps each send in a shared promise chain (report.lock), which serializes sends the same way the await already did, so it does not add a new stall or deadlock. It does add a different defect: once one send rejects, report.lock stays rejected and every later .then is skipped, so all subsequent recipients fail with the earlier error instead of being attempted. That cascade is not the claimed lock-held-across-slow-call consequence.
  Consequence claimed: Each send is chained onto a promise that is held until the mail provider answers. A slow or hung SMTP call stalls every later recipient in the batch, and a provider that never resolves blocks the chain indefinitely, so the digest job appears deadlocked.

## The clean code

### src/digest.mjs

```text
 1 | // Groups pending notifications per recipient and sends one digest email per
 2 | // recipient, instead of one email per event, so that busy users are not spammed.
 3 | // The mailer and the clock are injected so the scheduler stays testable.
 4 | 
 5 | const MAX_ITEMS_PER_DIGEST = 50;
 6 | 
 7 | export function groupByRecipient(notifications) {
 8 |   const groups = new Map();
 9 |   for (const note of notifications) {
10 |     if (!note || typeof note.recipient !== 'string' || note.recipient.length === 0) {
11 |       throw new TypeError('notification is missing a recipient');
12 |     }
13 |     if (!groups.has(note.recipient)) {
14 |       groups.set(note.recipient, []);
15 |     }
16 |     groups.get(note.recipient).push(note);
17 |   }
18 |   return groups;
19 | }
20 | 
21 | function renderBody(items, now) {
22 |   const lines = items.slice(0, MAX_ITEMS_PER_DIGEST).map((n) => `- ${n.summary}`);
23 |   const hidden = items.length - lines.length;
24 |   if (hidden > 0) {
25 |     lines.push(`...and ${hidden} more`);
26 |   }
27 |   return `You have ${items.length} new notification(s) as of ${new Date(now).toISOString()}:\n${lines.join('\n')}`;
28 | }
29 | 
30 | export async function sendDigests({ notifications, mailer, clock }) {
31 |   if (typeof mailer?.send !== 'function') {
32 |     throw new TypeError('mailer.send must be a function');
33 |   }
34 |   if (typeof clock !== 'function') {
35 |     throw new TypeError('clock must be a function');
36 |   }
37 |   const now = clock();
38 |   const groups = groupByRecipient(notifications);
39 |   const report = { sent: 0, failed: [] };
40 | 
41 |   for (const [recipient, items] of groups) {
42 |     // Failures are collected per recipient so one bad address does not block
43 |     // the rest of the batch.
44 |     try {
45 |       await mailer.send({
46 |         to: recipient,
47 |         subject: `${items.length} new notification(s)`,
48 |         body: renderBody(items, now),
49 |       });
50 |       report.sent += 1;
51 |     } catch (err) {
52 |       report.failed.push({ recipient, reason: err instanceof Error ? err.message : String(err) });
53 |     }
54 |   }
55 |   return report;
56 | }
```

### test/digest.test.mjs

```text
 1 | import { groupByRecipient, sendDigests } from '../src/digest.mjs';
 2 | 
 3 | function fakeMailer(failFor = new Set()) {
 4 |   const sent = [];
 5 |   return {
 6 |     sent,
 7 |     async send(message) {
 8 |       if (failFor.has(message.to)) {
 9 |         throw new Error('bounced');
10 |       }
11 |       sent.push(message);
12 |     },
13 |   };
14 | }
15 | 
16 | export async function groupsAndSendsOneDigestPerRecipient() {
17 |   const notes = [
18 |     { recipient: 'a@x.test', summary: 'one' },
19 |     { recipient: 'b@x.test', summary: 'two' },
20 |     { recipient: 'a@x.test', summary: 'three' },
21 |   ];
22 |   const mailer = fakeMailer();
23 |   const report = await sendDigests({ notifications: notes, mailer, clock: () => 0 });
24 |   if (report.sent !== 2 || report.failed.length !== 0) {
25 |     throw new Error('expected two digests sent');
26 |   }
27 |   const a = mailer.sent.find((m) => m.to === 'a@x.test');
28 |   if (!a.subject.startsWith('2 ')) {
29 |     throw new Error('digest subject should count items');
30 |   }
31 | }
32 | 
33 | export async function reportsFailuresWithoutStoppingBatch() {
34 |   const notes = [
35 |     { recipient: 'bad@x.test', summary: 'x' },
36 |     { recipient: 'good@x.test', summary: 'y' },
37 |   ];
38 |   const mailer = fakeMailer(new Set(['bad@x.test']));
39 |   const report = await sendDigests({ notifications: notes, mailer, clock: () => 0 });
40 |   if (report.sent !== 1 || report.failed[0]?.recipient !== 'bad@x.test') {
41 |     throw new Error('failure should be reported and batch should continue');
42 |   }
43 | }
44 | 
45 | export function rejectsNotificationWithoutRecipient() {
46 |   let threw = false;
47 |   try {
48 |     groupByRecipient([{ summary: 'orphan' }]);
49 |   } catch (err) {
50 |     threw = err instanceof TypeError;
51 |   }
52 |   if (!threw) {
53 |     throw new Error('missing recipient must raise TypeError');
54 |   }
55 | }
56 | 
57 | export async function truncatesLongDigests() {
58 |   const notes = [];
59 |   for (let i = 0; i < 53; i++) {
60 |     notes.push({ recipient: 'c@x.test', summary: `n${i}` });
61 |   }
62 |   const mailer = fakeMailer();
63 |   await sendDigests({ notifications: notes, mailer, clock: () => 0 });
64 |   if (!mailer.sent[0].body.includes('...and 3 more')) {
65 |     throw new Error('long digest should note hidden items');
66 |   }
67 | }
```

### The hidden suite

```text
import { groupByRecipient, sendDigests } from './src/digest.mjs';

function assert(cond, msg) {
  if (!cond) throw new Error('assertion failed: ' + msg);
}

function fakeMailer(failFor = new Set()) {
  const sent = [];
  return {
    sent,
    async send(message) {
      if (failFor.has(message.to)) throw new Error('bounced');
      sent.push(message);
    },
  };
}

const groups = groupByRecipient([
  { recipient: 'a', summary: '1' },
  { recipient: 'a', summary: '2' },
  { recipient: 'b', summary: '3' },
]);
assert(groups.get('a').length === 2, 'groups per recipient');
assert(groups.get('b').length === 1, 'second recipient kept');

let typeErr = false;
try { groupByRecipient([{ summary: 'x' }]); } catch (e) { typeErr = e instanceof TypeError; }
assert(typeErr, 'missing recipient throws TypeError');

const mailer = fakeMailer(new Set(['bad']));
const report = await sendDigests({
  notifications: [
    { recipient: 'ok', summary: 'hello' },
    { recipient: 'bad', summary: 'nope' },
  ],
  mailer,
  clock: () => Date.UTC(2024, 0, 2),
});
assert(report.sent === 1, 'one sent');
assert(report.failed.length === 1 && report.failed[0].recipient === 'bad', 'failure reported');
assert(mailer.sent[0].body.includes('2024-01-02T00:00:00.000Z'), 'clock used in body');

let badMailer = false;
try { await sendDigests({ notifications: [], mailer: {}, clock: () => 0 }); } catch (e) { badMailer = e instanceof TypeError; }
assert(badMailer, 'mailer without send rejected');

console.log('all checks passed');
```

## The clean review

- Minor: The failure reason is built with String(err && err.message), so when a thrown value has no message (for example a thrown string or null) the reason becomes the string 'undefined' or 'null' or drops the actual error; more importantly a falsy err like 0 or '' loses information. Using a proper fallback keeps the failure report accurate.
- Nit: The clock is called and the digest timestamp is derived before validating the mailer's presence in a way that matters less, but the real defect is that sendDigests never validates that clock is a function; calling clock() with a missing clock throws an unhelpful TypeError after mailer validation. Validating the injected dependency up front matches the mailer check.

## The blind review

1. Critical in src/digest.mjs: This condition is always true because a fresh array literal is never identical to an existing one, so every notification resets the recipient's group and discards earlier items. The intent is to create the group only when absent; use `if (!groups.has(note.recipient))`. Current tests pass only because they check counts from a single note or a different path, and the truncation test with 53 items of one recipient would actually produce just one item and fail to show hidden items.
   Quoted:     if (groups.get(note.recipient) !== []) {       groups.set(note.recipient, []);     }
   Judged: Matches { issues: [0] }. Correctly identifies the identity comparison against a new array literal that resets groups and drops items; this matches issue 1.
2. Critical in src/digest.mjs: The retry loop is an unbounded busy retry with no delay: if the mailer keeps failing for a recipient (e.g. a permanent bounce), this spins forever and blocks the whole batch, defeating the stated per-recipient isolation. The failure is also never recorded in report.failed. Use a bounded number of attempts with backoff, then push the recipient into report.failed.
   Quoted:     } catch (err) {       let retried = false;       while (!retried) {
   Judged: Matches { issues: [1] }. Correctly identifies the unbounded immediate retry with no delay that can spin forever; matches issue 2.
3. Major in src/digest.mjs: The retry error is silently swallowed and the cause is lost, so operators cannot diagnose why delivery failed. Record the error in the report and keep the last error.
   Quoted:         } catch (retryErr) {           retried = false;         }
   Judged: ValidExtra. Points at the retry block and notes that the retry error is swallowed, which is a real defect in that code. It is about lost diagnostics, not the lack of backoff, so it does not name the cause of issue 2. It is not a planted issue of the key, so it is a valid extra.
4. Minor in src/digest.mjs: The message construction is duplicated between the first send and the retry, and the retry logic is the only thing that differs. Build the message once and factor out a single send-with-retry helper.
   Quoted:     try {       await mailer.send({         to: recipient,
   Judged: FalsePositive. Duplicated message construction is a maintainability preference, not a defect the key holds or a real risk; it is only a style remark.
5. Major in src/digest.mjs: report.failed is documented by the tests as holding objects with a recipient field, but it is never populated anywhere, so failures are invisible to callers and the reporting contract is broken.
   Quoted:   return report; }
   Judged: ValidExtra. report.failed is never populated, so failed recipients are not reported; this is a real defect in the code as shown, though not a planted issue of the key.
6. Major in test/digest.test.mjs: This test requires failed entries to be objects with a recipient, but the implementation never adds any, so the test would fail; and with the current unbounded retry the bad recipient would hang the test forever rather than fail. The test is meaningful only once the implementation is fixed.
   Quoted:   if (report.sent !== 1 || report.failed[0]?.recipient !== 'bad@x.test') {
   Judged: ValidExtra. Notes that the test expects failed entries the implementation never adds, and that the unbounded retry would hang the test; this is a real consequence in the code, though it concerns the test rather than a planted issue.
7. Nit in test/digest.test.mjs: The test is synchronous and checks only the thrown type; it does not check the message, and the test runner convention (other tests are async) is inconsistent. Minor, but it proves little about the error content.
   Quoted: export function rejectsNotificationWithoutRecipient() {
   Judged: FalsePositive. The remark about the test's synchronous style and the unchecked error message is only a preference, not a real defect.

- planted-1 (correctness.reference_equality): the review Found { finding: 0 }. Finding 1 names the cause: the comparison groups.get(...) !== [] is always true because a fresh array literal is never identical to the stored array, so each notification resets the group and discards earlier items. It also gives the correct fix.
- planted-2 (errors.retry_without_backoff): the review Found { finding: 1 }. Finding 2 names the cause: the retry loop is an unbounded busy retry with no delay, so a persistently failing recipient spins forever and blocks the batch. It proposes bounded attempts with backoff.

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
