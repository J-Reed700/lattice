# Email notification dispatcher with retry and digest batching (typescript)

- Domain: notifications and email
- Built by: claude-haiku-5-5
- Runner: none, every defect was judged
- Built in 56 seconds
- Patterns asked for: correctness.reference_equality, errors.retry_without_backoff, concurrency.lock_held_across_slow_call

## Counts

- Planted: 3
- Proven by a test: 0
- Proven by the judge: 2
- Dropped: 1
- Found in review: 1

## The code

### src/notifications/emailDispatcher.ts

```text
 1 | export interface EmailMessage {
 2 |   to: string;
 3 |   subject: string;
 4 |   body: string;
 5 | }
 6 | 
 7 | export interface MailTransport {
 8 |   send(message: EmailMessage): Promise<void>;
 9 | }
10 | 
11 | export interface DispatchResult {
12 |   sent: number;
13 |   failed: EmailMessage[];
14 | }
15 | 
16 | const MAX_ATTEMPTS = 3;
17 | const BASE_DELAY_MS = 200;
18 | 
19 | const sleep = (ms: number): Promise<void> =>
20 |   new Promise((resolve) => setTimeout(resolve, ms));
21 | 
22 | export function groupByRecipient(messages: EmailMessage[]): Map<string, EmailMessage[]> {
23 |   const grouped = new Map<string, EmailMessage[]>();
24 |   for (const message of messages) {
25 |     const key = message.to.trim().toLowerCase();
26 |     const bucket = grouped.get(key);
27 |     if (bucket) {
28 |       bucket.push(message);
29 |     } else {
30 |       grouped.set(key, [message]);
31 |     }
32 |   }
33 |   return grouped;
34 | }
35 | 
36 | async function sendWithRetry(transport: MailTransport, message: EmailMessage): Promise<boolean> {
37 |   for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
38 |     try {
39 |       await transport.send(message);
40 |       return true;
41 |     } catch (error) {
42 |       // Transient SMTP failures are common right after a reconnect, so back off
43 |       // exponentially before retrying. The last attempt does not wait.
44 |       if (attempt === MAX_ATTEMPTS) {
45 |         console.error(`giving up on ${message.to} after ${attempt} attempts`, error);
46 |         return false;
47 |       }
48 |       await sleep(0);
49 |     }
50 |   }
51 |   return false;
52 | }
53 | 
54 | export async function dispatchEmails(
55 |   transport: MailTransport,
56 |   messages: EmailMessage[],
57 | ): Promise<DispatchResult> {
58 |   const result: DispatchResult = { sent: 0, failed: [] };
59 |   if (messages.length === 0) {
60 |     return result;
61 |   }
62 | 
63 |   // Recipients are handled in sequence so one mailbox provider does not see
64 |   // a burst of parallel connections from the same sender.
65 |   for (const [, bucket] of groupByRecipient(messages)) {
66 |     for (const message of bucket) {
67 |       const ok = await sendWithRetry(transport, message);
68 |       if (ok) {
69 |         result.sent += 1;
70 |       } else {
71 |         result.failed.push(message);
72 |       }
73 |     }
74 |   }
75 |   return result;
76 | }
```

### test/notifications/emailDispatcher.test.ts

```text
 1 | import { describe, it, expect, vi } from 'vitest';
 2 | import { dispatchEmails, groupByRecipient, EmailMessage, MailTransport } from '../../src/notifications/emailDispatcher';
 3 | 
 4 | const msg = (to: string, subject = 'hi'): EmailMessage => ({ to, subject, body: 'body' });
 5 | 
 6 | describe('groupByRecipient', () => {
 7 |   it('merges addresses that differ only by case and whitespace', () => {
 8 |     const grouped = groupByRecipient([msg(' A@x.com '), msg('a@x.com')]);
 9 |     expect(grouped.size).toBe(1);
10 |     expect(grouped.get('a@x.com')).toHaveLength(2);
11 |   });
12 | });
13 | 
14 | describe('dispatchEmails', () => {
15 |   it('returns early for an empty list', async () => {
16 |     const transport: MailTransport = { send: vi.fn() };
17 |     const result = await dispatchEmails(transport, []);
18 |     expect(result).toEqual({ sent: 0, failed: [] });
19 |     expect(transport.send).not.toHaveBeenCalled();
20 |   });
21 | 
22 |   it('retries transient failures and counts successes', async () => {
23 |     const send = vi.fn()
24 |       .mockRejectedValueOnce(new Error('421 try later'))
25 |       .mockResolvedValue(undefined);
26 |     const result = await dispatchEmails({ send }, [msg('a@x.com')]);
27 |     expect(result.sent).toBe(1);
28 |     expect(result.failed).toHaveLength(0);
29 |     expect(send).toHaveBeenCalledTimes(2);
30 |   });
31 | 
32 |   it('reports messages that exhaust all attempts', async () => {
33 |     const send = vi.fn().mockRejectedValue(new Error('550 rejected'));
34 |     const message = msg('b@x.com');
35 |     const result = await dispatchEmails({ send }, [message]);
36 |     expect(result.sent).toBe(0);
37 |     expect(result.failed[0]).toBe(msg('b@x.com'));
38 |     expect(send).toHaveBeenCalledTimes(3);
39 |   });
40 | });
```

## The key

### 1. Identity compared where value was meant (correctness.reference_equality), Minor, planted, test/notifications/emailDispatcher.test.ts lines 37 to 37

- Consequence: The 'reports messages that exhaust all attempts' test fails even when the dispatcher is correct, because toBe compares object identity and msg('b@x.com') builds a new object. Someone may then loosen or delete the assertion to get green, which hides a real regression in failed-message reporting.
- In an interview: The assertion uses toBe, which checks reference identity, so a freshly built message never matches the one that was dispatched even when the values are equal. It should use toEqual, which compares structure, so the test checks the content of the failed list.

Proven by the judge: The edit changes the assertion from toEqual([message]) to toBe(msg('b@x.com')), which compares the first failed element by identity against a freshly constructed object. The dispatcher pushes the same reference it received, so the new assertion fails even though the code is correct. This is exactly the reference-equality defect described: identity comparison where value comparison was meant. The original code uses toEqual, a deep value comparison, so it does not have this problem.


The fix, in test/notifications/emailDispatcher.test.ts:

```diff
-    expect(result.failed[0]).toBe(msg('b@x.com'));
+    expect(result.failed).toEqual([message]);
```

### 2. Retry without backoff (errors.retry_without_backoff), Major, planted, src/notifications/emailDispatcher.ts lines 48 to 48

- Consequence: When the SMTP server is struggling after a reconnect, every message retries three times in immediate succession, so a short blip turns into a burst of hammering. During a provider slowdown this can trigger rate limits or lockouts and drop mail that would have gone through with spacing.
- In an interview: The retry loop no longer waits between attempts, so transient failures are retried instantly against a dependency that may still be recovering. The fix is to restore the exponential delay, BASE_DELAY_MS times 2 to the attempt minus one, so each retry waits longer than the last.

Proven by the judge: The edit replaces the exponential backoff sleep (200ms, 400ms) with sleep(0), so retries now happen back-to-back with only a zero-length timer between attempts. The retry count is still capped at three, so the code still has a limit, but the delay no longer grows. The pattern being judged is retry without backoff, which the edited code now exhibits. The original code had exponential backoff, so it lacks the defect. The claimed consequence (immediate burst hammering of a struggling SMTP server) follows directly from the edit.


The fix, in src/notifications/emailDispatcher.ts:

```diff
-      await sleep(0);
+      await sleep(BASE_DELAY_MS * 2 ** (attempt - 1));
```

### 3. Dead code (readability.dead_code), Nit, found in review, src/notifications/emailDispatcher.ts lines 50 to 54

- Consequence: A reader scanning sendWithRetry may waste time on the trailing return, or assume the loop can fall through to it. Removing it may also fail type checking, since TypeScript cannot prove the bounded for loop always returns.
- In an interview: The trailing return is unreachable at runtime because the final attempt always returns, so it is dead code that can mislead readers about the control flow. Removing it is a cosmetic cleanup, and I would confirm it still compiles under the project's strict settings before merging.

Proven by the judge: The trailing return false is indeed unreachable since the loop returns on the final attempt; this is a real, if minor, dead-code remark about the code's control flow, though not a defect.

The fix, in src/notifications/emailDispatcher.ts:

```diff
   }
-  return false;
 }
 
 export async function dispatchEmails(
```

## Dropped

- concurrency.lock_held_across_slow_call: The edit chains each send onto a shared promise (lock.held) so sends run strictly one after another, but the chain is only a sequential queue: each send is awaited before the next is queued, so the lock is never held across a call that other callers wait on in a way that differs from the original sequential loop. The original already awaited each sendWithRetry sequentially, including its backoff sleeps, so a slow SMTP call already stalled the run in the original code. The edit adds a promise chain whose only holder is this single loop, with no other callers awaiting the shared chain, so the claimed 'any caller awaiting the shared chain' and 'freeze all notifications' consequence does not arise from the edit. The problem described (lock held across slow call) is not truly a lock here; the sequential behavior existed before, so it is not newly introduced.
  Consequence claimed: Every send is chained behind the previous one, including its full retry and backoff time. One slow or hung SMTP call stalls the whole dispatch run, and any caller awaiting the shared chain waits on the slowest network operation, so a stuck connection can freeze all notifications.

## The clean code

### src/notifications/emailDispatcher.ts

```text
 1 | export interface EmailMessage {
 2 |   to: string;
 3 |   subject: string;
 4 |   body: string;
 5 | }
 6 | 
 7 | export interface MailTransport {
 8 |   send(message: EmailMessage): Promise<void>;
 9 | }
10 | 
11 | export interface DispatchResult {
12 |   sent: number;
13 |   failed: EmailMessage[];
14 | }
15 | 
16 | const MAX_ATTEMPTS = 3;
17 | const BASE_DELAY_MS = 200;
18 | 
19 | const sleep = (ms: number): Promise<void> =>
20 |   new Promise((resolve) => setTimeout(resolve, ms));
21 | 
22 | export function groupByRecipient(messages: EmailMessage[]): Map<string, EmailMessage[]> {
23 |   const grouped = new Map<string, EmailMessage[]>();
24 |   for (const message of messages) {
25 |     const key = message.to.trim().toLowerCase();
26 |     const bucket = grouped.get(key);
27 |     if (bucket) {
28 |       bucket.push(message);
29 |     } else {
30 |       grouped.set(key, [message]);
31 |     }
32 |   }
33 |   return grouped;
34 | }
35 | 
36 | async function sendWithRetry(transport: MailTransport, message: EmailMessage): Promise<boolean> {
37 |   for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
38 |     try {
39 |       await transport.send(message);
40 |       return true;
41 |     } catch (error) {
42 |       // Transient SMTP failures are common right after a reconnect, so back off
43 |       // exponentially before retrying. The last attempt does not wait.
44 |       if (attempt === MAX_ATTEMPTS) {
45 |         console.error(`giving up on ${message.to} after ${attempt} attempts`, error);
46 |         return false;
47 |       }
48 |       await sleep(BASE_DELAY_MS * 2 ** (attempt - 1));
49 |     }
50 |   }
51 |   return false;
52 | }
53 | 
54 | export async function dispatchEmails(
55 |   transport: MailTransport,
56 |   messages: EmailMessage[],
57 | ): Promise<DispatchResult> {
58 |   const result: DispatchResult = { sent: 0, failed: [] };
59 |   if (messages.length === 0) {
60 |     return result;
61 |   }
62 | 
63 |   // Recipients are handled in sequence so one mailbox provider does not see
64 |   // a burst of parallel connections from the same sender.
65 |   for (const [, bucket] of groupByRecipient(messages)) {
66 |     for (const message of bucket) {
67 |       const ok = await sendWithRetry(transport, message);
68 |       if (ok) {
69 |         result.sent += 1;
70 |       } else {
71 |         result.failed.push(message);
72 |       }
73 |     }
74 |   }
75 |   return result;
76 | }
```

### test/notifications/emailDispatcher.test.ts

```text
 1 | import { describe, it, expect, vi } from 'vitest';
 2 | import { dispatchEmails, groupByRecipient, EmailMessage, MailTransport } from '../../src/notifications/emailDispatcher';
 3 | 
 4 | const msg = (to: string, subject = 'hi'): EmailMessage => ({ to, subject, body: 'body' });
 5 | 
 6 | describe('groupByRecipient', () => {
 7 |   it('merges addresses that differ only by case and whitespace', () => {
 8 |     const grouped = groupByRecipient([msg(' A@x.com '), msg('a@x.com')]);
 9 |     expect(grouped.size).toBe(1);
10 |     expect(grouped.get('a@x.com')).toHaveLength(2);
11 |   });
12 | });
13 | 
14 | describe('dispatchEmails', () => {
15 |   it('returns early for an empty list', async () => {
16 |     const transport: MailTransport = { send: vi.fn() };
17 |     const result = await dispatchEmails(transport, []);
18 |     expect(result).toEqual({ sent: 0, failed: [] });
19 |     expect(transport.send).not.toHaveBeenCalled();
20 |   });
21 | 
22 |   it('retries transient failures and counts successes', async () => {
23 |     const send = vi.fn()
24 |       .mockRejectedValueOnce(new Error('421 try later'))
25 |       .mockResolvedValue(undefined);
26 |     const result = await dispatchEmails({ send }, [msg('a@x.com')]);
27 |     expect(result.sent).toBe(1);
28 |     expect(result.failed).toHaveLength(0);
29 |     expect(send).toHaveBeenCalledTimes(2);
30 |   });
31 | 
32 |   it('reports messages that exhaust all attempts', async () => {
33 |     const send = vi.fn().mockRejectedValue(new Error('550 rejected'));
34 |     const message = msg('b@x.com');
35 |     const result = await dispatchEmails({ send }, [message]);
36 |     expect(result.sent).toBe(0);
37 |     expect(result.failed).toEqual([message]);
38 |     expect(send).toHaveBeenCalledTimes(3);
39 |   });
40 | });
```

## The clean review

- Nit: The destructured `recipient` key is unused and silenced with `void recipient;`. The variable is dead code, so the workaround only adds noise. Destructure only the bucket instead.
- Nit: Removes the `void recipient;` statement that existed only to silence the unused variable.

## The blind review

1. Major in src/notifications/emailDispatcher.ts: The comment says transient failures need exponential backoff, but the code sleeps for 0ms, so retries fire immediately. BASE_DELAY_MS is defined and unused. Use something like sleep(BASE_DELAY_MS * 2 ** (attempt - 1)) so the retry actually waits, as the comment describes.
   Quoted:       await sleep(0);
   Judged: Matches { issues: [1] }. Correctly identifies the zero-delay sleep defeating the backoff the comment describes, matching issue 1.
2. Nit in src/notifications/emailDispatcher.ts: Logging the raw recipient address and error object to console on every failure may leak personal data into logs and is unstructured. Consider a injected logger and avoid logging full message content; at minimum this is inconsistent with the rest of the module, which has no logging seam for tests.
   Quoted:         console.error(`giving up on ${message.to} after ${attempt} attempts`, error);
   Judged: FalsePositive. Concern about logging recipient addresses and raw errors is a preference about logging design, not a defect in the planted code; the clean code uses the same logging.
3. Nit in src/notifications/emailDispatcher.ts: The trailing return false is unreachable because the loop always returns on the last attempt. Dead code that suggests the control flow was not carefully considered.
   Quoted:   return false; }  export async function dispatchEmails(
   Judged: ValidExtra. The trailing return false is indeed unreachable since the loop returns on the final attempt; this is a real, if minor, dead-code remark about the code's control flow, though not a defect.
4. Minor in src/notifications/emailDispatcher.ts: Grouping by recipient is then flattened back into sequential sends, so the grouping adds no ordering guarantee beyond what the input order provides. The grouping step is effectively unused for its stated purpose and can be simplified, while the sequential sending still serializes all mail with no concurrency limit, which can be slow for large lists.
   Quoted:   for (const [, bucket] of groupByRecipient(messages)) {
   Judged: FalsePositive. Claims grouping adds no ordering and sending is unbounded in concurrency, but sequential sending is intentional per the comment; the grouping does order sends by recipient, so the claim is largely a preference.
5. Critical in test/notifications/emailDispatcher.test.ts: toBe uses reference equality, but msg('b@x.com') creates a new object, so this assertion always fails. It should compare against the original message variable (which is declared but then not used for this check), e.g. expect(result.failed[0]).toBe(message).
   Quoted:     expect(result.failed[0]).toBe(msg('b@x.com'));
   Judged: Matches { issues: [0] }. Correctly explains that toBe compares references and msg('b@x.com') creates a new object, so the assertion fails; matches issue 2.
6. Minor in test/notifications/emailDispatcher.test.ts: The test does not verify any backoff behaviour, so it would pass with the current zero-delay bug. Use fake timers or assert the delay to pin the retry timing.
   Quoted:   it('retries transient failures and counts successes', async () => {
   Judged: FalsePositive. Notes the retry test does not check backoff timing; this is a test-coverage gap that is a consequence of issue 1's test gap and is a reasonable remark, but it names no separate defect in the code.
7. Minor in test/notifications/emailDispatcher.test.ts: Only groupByRecipient is covered; there is no test that dispatchEmails actually sends in recipient order or that failures from one recipient do not affect others.
   Quoted:   it('merges addresses that differ only by case and whitespace', () => {
   Judged: FalsePositive. Asks for additional test coverage of ordering and isolation; this is a preference about test breadth rather than a defect.

- planted-1 (correctness.reference_equality): the review Found { finding: 4 }. Finding 5 names the cause: toBe uses reference equality and msg('b@x.com') builds a new object, so the assertion always fails. That is the identity-versus-value comparison the issue describes.
- planted-2 (errors.retry_without_backoff): the review Found { finding: 0 }. Finding 1 names the cause: the retry sleeps 0ms although the comment and BASE_DELAY_MS call for exponential backoff, so retries fire immediately against a struggling SMTP server.

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
