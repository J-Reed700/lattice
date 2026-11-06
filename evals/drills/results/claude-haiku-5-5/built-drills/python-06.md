# Email notification digest builder with per-recipient throttling (python)

- Domain: notifications and email
- Built by: claude-haiku-5-5
- Runner: builtin-python
- Built in 128 seconds
- Patterns asked for: correctness.mutable_default_argument, errors.retry_without_backoff, concurrency.lock_held_across_slow_call

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 0
- Dropped: 1
- Found in review: 0

## The code

### src/digest.py

```text
 1 | from dataclasses import dataclass, field
 2 | from email.message import EmailMessage
 3 | from typing import Callable, Iterable
 4 | 
 5 | 
 6 | @dataclass(frozen=True)
 7 | class Notification:
 8 |     recipient: str
 9 |     subject: str
10 |     body: str
11 |     created_at: float
12 | 
13 | 
14 | @dataclass
15 | class Digest:
16 |     recipient: str
17 |     items: list[Notification] = field(default_factory=list)
18 | 
19 | 
20 | MAX_ITEMS_PER_DIGEST = 50
21 | 
22 | 
23 | def group_by_recipient(notifications: Iterable[Notification], digests: dict[str, Digest] = {}) -> dict[str, Digest]:
24 |     for note in notifications:
25 |         digest = digests.get(note.recipient)
26 |         if digest is None:
27 |             digest = Digest(recipient=note.recipient)
28 |             digests[note.recipient] = digest
29 |         digest.items.append(note)
30 |     for digest in digests.values():
31 |         digest.items.sort(key=lambda n: n.created_at)
32 |     return digests
33 | 
34 | 
35 | def render_message(digest: Digest, sender: str, now: Callable[[], float]) -> EmailMessage:
36 |     if not digest.items:
37 |         raise ValueError("cannot render an empty digest")
38 |     shown = digest.items[:MAX_ITEMS_PER_DIGEST]
39 |     hidden = len(digest.items) - len(shown)
40 | 
41 |     lines = [f"- {n.subject}: {n.body}" for n in shown]
42 |     if hidden:
43 |         # Recipients with a large backlog get a pointer instead of an unbounded mail.
44 |         lines.append(f"... and {hidden} more notification(s) not shown.")
45 | 
46 |     message = EmailMessage()
47 |     message["From"] = sender
48 |     message["To"] = digest.recipient
49 |     message["Subject"] = f"You have {len(digest.items)} new notification(s)"
50 |     message["Date"] = _format_date(now())
51 |     message.set_content("\n".join(lines) + "\n")
52 |     return message
53 | 
54 | 
55 | def _format_date(timestamp: float) -> str:
56 |     from email.utils import formatdate
57 | 
58 |     return formatdate(timestamp, usegmt=True)
59 | 
60 | 
61 | def build_digests(
62 |     notifications: Iterable[Notification],
63 |     sender: str,
64 |     now: Callable[[], float],
65 |     send: Callable[[EmailMessage], None],
66 | ) -> int:
67 |     # Failures for one recipient must not stop the rest of the batch, so each
68 |     # send is isolated and the count reflects only messages that went out.
69 |     sent = 0
70 |     for digest in group_by_recipient(notifications).values():
71 |         if not digest.items:
72 |             continue
73 |         try:
74 |             send(render_message(digest, sender, now))
75 |         except OSError:
76 |             try:
77 |                 send(render_message(digest, sender, now))
78 |             except OSError:
79 |                 continue
80 |         sent += 1
81 |     return sent
```

### tests/test_digest.py

```text
 1 | from datetime import datetime, timezone
 2 | from email.utils import parsedate_to_datetime
 3 | 
 4 | from src.digest import Notification, build_digests, render_message, group_by_recipient, Digest, MAX_ITEMS_PER_DIGEST
 5 | 
 6 | 
 7 | def note(recipient, subject, created_at, body="b"):
 8 |     return Notification(recipient, subject, body, created_at)
 9 | 
10 | 
11 | FIXED_NOW = 1_700_000_000.0
12 | 
13 | 
14 | def test_grouping_sorts_by_time():
15 |     groups = group_by_recipient([note("a@x", "s2", 5.0), note("b@x", "s1", 1.0), note("a@x", "s1", 2.0)])
16 |     assert [n.subject for n in groups["a@x"].items] == ["s1", "s2"]
17 |     assert list(groups["b@x"].items)[0].subject == "s1"
18 | 
19 | 
20 | def test_render_truncates_large_digest():
21 |     items = [note("a@x", f"s{i}", float(i)) for i in range(MAX_ITEMS_PER_DIGEST + 3)]
22 |     msg = render_message(Digest("a@x", items), "noreply@x", lambda: FIXED_NOW)
23 |     body = msg.get_content()
24 |     assert "and 3 more notification(s) not shown." in body
25 |     assert msg["Subject"] == f"You have {MAX_ITEMS_PER_DIGEST + 3} new notification(s)"
26 |     assert msg["To"] == "a@x"
27 | 
28 | 
29 | def test_render_rejects_empty():
30 |     try:
31 |         render_message(Digest("a@x"), "n@x", lambda: FIXED_NOW)
32 |     except ValueError:
33 |         return
34 |     raise AssertionError("expected ValueError")
35 | 
36 | 
37 | def test_build_continues_after_oserror():
38 |     sent_to = []
39 | 
40 |     def send(msg):
41 |         if msg["To"] == "bad@x":
42 |             raise OSError("smtp down")
43 |         sent_to.append(msg["To"])
44 | 
45 |     count = build_digests(
46 |         [note("bad@x", "s", 1.0), note("good@x", "s", 2.0)],
47 |         "n@x", lambda: FIXED_NOW, send,
48 |     )
49 |     assert count == 1
50 |     assert sent_to == ["good@x"]
51 | 
52 | 
53 | def test_build_empty_input():
54 |     calls = []
55 |     assert build_digests([], "n@x", lambda: FIXED_NOW, calls.append) == 0
56 |     assert calls == []
57 | 
58 | 
59 | def test_date_header_matches_timestamp():
60 |     msg = render_message(Digest("a@x", [note("a@x", "s", 1.0)]), "n@x", lambda: FIXED_NOW)
61 |     parsed = parsedate_to_datetime(str(msg["Date"]))
62 |     assert parsed == datetime.fromtimestamp(FIXED_NOW, timezone.utc)
63 | 
64 | 
65 | if __name__ == "__main__":
66 |     for name, fn in list(globals().items()):
67 |         if name.startswith("test_") and callable(fn):
68 |             fn()
```

## The key

### 1. Mutable default argument (correctness.mutable_default_argument), Major, planted, src/digest.py lines 23 to 23

- Consequence: Every call that omits the argument reuses the same dict, so digests and their items from earlier batches leak into later results, and a later batch can re-send old notifications to recipients from prior runs.
- In an interview: The default {} is evaluated once at definition time, so all calls without the argument share one mutable dict. Use a None default and create a fresh dict inside the function when it is None.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.digest import Notification, group_by_recipient

first = group_by_recipient([Notification('a@x', 's', 'b', 1.0)])
second = group_by_recipient([Notification('b@x', 's', 'b', 2.0)])
assert set(second) == {'b@x'}, set(second)
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/digest.py:

```diff
-def group_by_recipient(notifications: Iterable[Notification], digests: dict[str, Digest] = {}) -> dict[str, Digest]:
+def group_by_recipient(notifications: Iterable[Notification]) -> dict[str, Digest]:
+    digests: dict[str, Digest] = {}
```

### 2. Retry without backoff (errors.retry_without_backoff), Minor, planted, src/digest.py lines 76 to 80

- Consequence: When the SMTP server is failing, every recipient gets a second send attempt straight away, doubling the load on a dependency that is already struggling.
- In an interview: The failed send is retried at once with no delay, so a transient failure gets hammered before it can recover. Retry only with a bounded attempt count and an exponential backoff between attempts, or defer the failed recipient to a later run.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.digest import Notification, build_digests

calls = []

def send(msg):
    calls.append(msg['To'])
    raise OSError('down')

count = build_digests([Notification('bad@x', 's', 'b', 1.0)], 'n@x', lambda: 1.0, send)
assert count == 0
assert calls == ['bad@x'], calls
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/digest.py:

```diff
-            try:
-                send(render_message(digest, sender, now))
-            except OSError:
-                continue
+            continue
         sent += 1
```

## Dropped

- concurrency.lock_held_across_slow_call: The edit wraps the send call in a with-lock block, but the lock is a freshly created local threading.Lock() inside build_digests, so no other caller can ever contend for it; it does not make other callers wait on the SMTP round trip. The lock is held during send, which is technically a slow call under the lock, yet the claimed consequence (other callers waiting on the send and a stalled server freezing the batch pipeline) cannot happen because no other thread can acquire this lock. The original code has no lock at all, so the pattern is not present there. The edit introduces a lock held across a slow call in a literal sense, but the claimed consequence cannot occur, so the answer is no for introduction of the exact problem as claimed.
  Consequence claimed: The lock is held for the whole SMTP round trip, so any other caller waits on the slowest network send and a stalled server can freeze the whole batch pipeline.

## The clean code

### src/digest.py

```text
 1 | from dataclasses import dataclass, field
 2 | from email.message import EmailMessage
 3 | from typing import Callable, Iterable
 4 | 
 5 | 
 6 | @dataclass(frozen=True)
 7 | class Notification:
 8 |     recipient: str
 9 |     subject: str
10 |     body: str
11 |     created_at: float
12 | 
13 | 
14 | @dataclass
15 | class Digest:
16 |     recipient: str
17 |     items: list[Notification] = field(default_factory=list)
18 | 
19 | 
20 | MAX_ITEMS_PER_DIGEST = 50
21 | 
22 | 
23 | def group_by_recipient(notifications: Iterable[Notification]) -> dict[str, Digest]:
24 |     digests: dict[str, Digest] = {}
25 |     for note in notifications:
26 |         digest = digests.get(note.recipient)
27 |         if digest is None:
28 |             digest = Digest(recipient=note.recipient)
29 |             digests[note.recipient] = digest
30 |         digest.items.append(note)
31 |     for digest in digests.values():
32 |         digest.items.sort(key=lambda n: n.created_at)
33 |     return digests
34 | 
35 | 
36 | def render_message(digest: Digest, sender: str, now: Callable[[], float]) -> EmailMessage:
37 |     if not digest.items:
38 |         raise ValueError("cannot render an empty digest")
39 |     shown = digest.items[:MAX_ITEMS_PER_DIGEST]
40 |     hidden = len(digest.items) - len(shown)
41 | 
42 |     lines = [f"- {n.subject}: {n.body}" for n in shown]
43 |     if hidden:
44 |         # Recipients with a large backlog get a pointer instead of an unbounded mail.
45 |         lines.append(f"... and {hidden} more notification(s) not shown.")
46 | 
47 |     message = EmailMessage()
48 |     message["From"] = sender
49 |     message["To"] = digest.recipient
50 |     message["Subject"] = f"You have {len(digest.items)} new notification(s)"
51 |     message["Date"] = _format_date(now())
52 |     message.set_content("\n".join(lines) + "\n")
53 |     return message
54 | 
55 | 
56 | def _format_date(timestamp: float) -> str:
57 |     from email.utils import formatdate
58 | 
59 |     return formatdate(timestamp, usegmt=True)
60 | 
61 | 
62 | def build_digests(
63 |     notifications: Iterable[Notification],
64 |     sender: str,
65 |     now: Callable[[], float],
66 |     send: Callable[[EmailMessage], None],
67 | ) -> int:
68 |     # Failures for one recipient must not stop the rest of the batch, so each
69 |     # send is isolated and the count reflects only messages that went out.
70 |     sent = 0
71 |     for digest in group_by_recipient(notifications).values():
72 |         if not digest.items:
73 |             continue
74 |         try:
75 |             send(render_message(digest, sender, now))
76 |         except OSError:
77 |             continue
78 |         sent += 1
79 |     return sent
```

### tests/test_digest.py

```text
 1 | from datetime import datetime, timezone
 2 | from email.utils import parsedate_to_datetime
 3 | 
 4 | from src.digest import Notification, build_digests, render_message, group_by_recipient, Digest, MAX_ITEMS_PER_DIGEST
 5 | 
 6 | 
 7 | def note(recipient, subject, created_at, body="b"):
 8 |     return Notification(recipient, subject, body, created_at)
 9 | 
10 | 
11 | FIXED_NOW = 1_700_000_000.0
12 | 
13 | 
14 | def test_grouping_sorts_by_time():
15 |     groups = group_by_recipient([note("a@x", "s2", 5.0), note("b@x", "s1", 1.0), note("a@x", "s1", 2.0)])
16 |     assert [n.subject for n in groups["a@x"].items] == ["s1", "s2"]
17 |     assert list(groups["b@x"].items)[0].subject == "s1"
18 | 
19 | 
20 | def test_render_truncates_large_digest():
21 |     items = [note("a@x", f"s{i}", float(i)) for i in range(MAX_ITEMS_PER_DIGEST + 3)]
22 |     msg = render_message(Digest("a@x", items), "noreply@x", lambda: FIXED_NOW)
23 |     body = msg.get_content()
24 |     assert "and 3 more notification(s) not shown." in body
25 |     assert msg["Subject"] == f"You have {MAX_ITEMS_PER_DIGEST + 3} new notification(s)"
26 |     assert msg["To"] == "a@x"
27 | 
28 | 
29 | def test_render_rejects_empty():
30 |     try:
31 |         render_message(Digest("a@x"), "n@x", lambda: FIXED_NOW)
32 |     except ValueError:
33 |         return
34 |     raise AssertionError("expected ValueError")
35 | 
36 | 
37 | def test_build_continues_after_oserror():
38 |     sent_to = []
39 | 
40 |     def send(msg):
41 |         if msg["To"] == "bad@x":
42 |             raise OSError("smtp down")
43 |         sent_to.append(msg["To"])
44 | 
45 |     count = build_digests(
46 |         [note("bad@x", "s", 1.0), note("good@x", "s", 2.0)],
47 |         "n@x", lambda: FIXED_NOW, send,
48 |     )
49 |     assert count == 1
50 |     assert sent_to == ["good@x"]
51 | 
52 | 
53 | def test_build_empty_input():
54 |     calls = []
55 |     assert build_digests([], "n@x", lambda: FIXED_NOW, calls.append) == 0
56 |     assert calls == []
57 | 
58 | 
59 | def test_date_header_matches_timestamp():
60 |     msg = render_message(Digest("a@x", [note("a@x", "s", 1.0)]), "n@x", lambda: FIXED_NOW)
61 |     parsed = parsedate_to_datetime(str(msg["Date"]))
62 |     assert parsed == datetime.fromtimestamp(FIXED_NOW, timezone.utc)
63 | 
64 | 
65 | if __name__ == "__main__":
66 |     for name, fn in list(globals().items()):
67 |         if name.startswith("test_") and callable(fn):
68 |             fn()
```

### The hidden suite

```text
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime

from src.digest import Notification, build_digests, render_message, group_by_recipient, Digest, MAX_ITEMS_PER_DIGEST

NOW = 1_700_000_000.0


def note(recipient, subject, created_at, body="b"):
    return Notification(recipient, subject, body, created_at)


def check_grouping_sorts_by_time():
    groups = group_by_recipient([note("a@x", "s2", 5.0), note("b@x", "s1", 1.0), note("a@x", "s1", 2.0)])
    assert [n.subject for n in groups["a@x"].items] == ["s1", "s2"]
    assert set(groups) == {"a@x", "b@x"}


def check_grouping_empty():
    assert group_by_recipient([]) == {}


def check_render_truncates():
    items = [note("a@x", f"s{i}", float(i)) for i in range(MAX_ITEMS_PER_DIGEST + 3)]
    msg = render_message(Digest("a@x", items), "n@x", lambda: NOW)
    assert "and 3 more notification(s) not shown." in msg.get_content()
    assert msg["Subject"] == f"You have {MAX_ITEMS_PER_DIGEST + 3} new notification(s)"


def check_render_no_truncation_note_when_small():
    msg = render_message(Digest("a@x", [note("a@x", "s", 1.0)]), "n@x", lambda: NOW)
    assert "not shown" not in msg.get_content()
    assert msg["From"] == "n@x"


def check_render_rejects_empty():
    try:
        render_message(Digest("a@x"), "n@x", lambda: NOW)
    except ValueError:
        return
    raise AssertionError("expected ValueError")


def check_build_continues_after_oserror():
    sent_to = []

    def send(msg):
        if msg["To"] == "bad@x":
            raise OSError("down")
        sent_to.append(msg["To"])

    count = build_digests([note("bad@x", "s", 1.0), note("good@x", "s", 2.0)], "n@x", lambda: NOW, send)
    assert count == 1
    assert sent_to == ["good@x"]


def check_build_empty_input():
    calls = []
    assert build_digests([], "n@x", lambda: NOW, calls.append) == 0
    assert calls == []


def check_date_header_matches_timestamp():
    msg = render_message(Digest("a@x", [note("a@x", "s", 1.0)]), "n@x", lambda: NOW)
    parsed = parsedate_to_datetime(str(msg["Date"]))
    assert parsed == datetime.fromtimestamp(NOW, timezone.utc)


for _name, _fn in list(globals().items()):
    if _name.startswith("check_") and callable(_fn):
        _fn()
```

## The clean review

It found nothing to repair.


## The blind review

1. Critical in src/digest.py: Mutable default argument: the same dict is shared across every call without an explicit argument, so notifications from earlier batches leak into later calls and get re-sent. Use digests: dict[str, Digest] | None = None and create a fresh dict when None.
   Quoted: def group_by_recipient(notifications: Iterable[Notification], digests: dict[str, Digest] = {}) -> dict[str, Digest]:
   Judged: Matches { issues: [0] }. Correctly identifies the shared mutable default argument and its cross-call leakage and re-sending of notifications.
2. Major in src/digest.py: The second failed attempt is silently swallowed with no logging or record of which recipient was dropped, so the caller only sees a lower count. Log the failure with the recipient and error, or collect failed recipients and return or raise them. The retry also only covers OSError and retries immediately with no backoff.
   Quoted:         try:             send(render_message(digest, sender, now))         except OSError:             try:                 send(render_message(digest, sender, now))             except OSError:                 continue         sent += 1
   Judged: Matches { issues: [1] }. Mainly complains about silently swallowed failures, which is a different problem; it does mention the immediate retry with no backoff, which is issue 2's pattern, but without the load-doubling cause.
3. Minor in src/digest.py: Sorting is applied to every digest in the shared dict, including ones from previous calls, and the function mutates and returns caller-owned state. Combined with the default argument this is surprising; sort only the digests touched in this call or document the mutation.
   Quoted:     for digest in digests.values():         digest.items.sort(key=lambda n: n.created_at)
   Judged: FalsePositive. Sorting the shared dict's digests is a side effect of the same mutable default; it is a consequence of issue 1 rather than a separate defect, and it does not name the leak cause as its own problem, so it is only a remark on the shared state.
4. Major in src/digest.py: Header values built from notification-derived data are not sanitized, and subjects/bodies are placed in the body without checks. Newlines in recipient or sender strings could cause header injection; validate or strip CR/LF from values placed in headers.
   Quoted:     message["Subject"] = f"You have {len(digest.items)} new notification(s)"
   Judged: FalsePositive. Header injection from CR/LF in recipient or sender is a speculative concern; the email library rejects or folds such values and the code is not a real defect here, so it is not a real problem of this code.
5. Minor in src/digest.py: The subject and body of each notification are concatenated into the plain-text body without any length limit, so a very large body multiplies the 50-item cap into an unbounded mail. Consider truncating individual fields.
   Quoted:     lines = [f"- {n.subject}: {n.body}" for n in shown]
   Judged: FalsePositive. Per-field length limits are a preference about output size; the 50-item cap already bounds the mail and no real defect is named.
6. Minor in tests/test_digest.py: The test only covers a permanent failure of one recipient and does not check the retry path, i.e. that a transient failure succeeds on the second attempt and is counted. It also does not check that the shared default dict does not leak across calls; add those cases.
   Quoted: def test_build_continues_after_oserror():
   Judged: FalsePositive. Test coverage suggestion about transient retry and shared-state leakage is a remark on tests, not a real defect in the code.
7. Nit in tests/test_digest.py: Single-item assertion is weaker than the rest and uses a redundant list() wrap; compare the full list of subjects so the test proves the grouping for b@x exactly.
   Quoted:     assert list(groups["b@x"].items)[0].subject == "s1"
   Judged: FalsePositive. A style nit about assertion strength and a redundant list() call; it is only a preference.

- planted-1 (correctness.mutable_default_argument): the review Found { finding: 0 }. Finding 1 names the mutable default dict shared across calls, so notifications from earlier batches leak into later calls and get re-sent, which is the stated cause.
- planted-2 (errors.retry_without_backoff): the review HalfFound { finding: 1 }. Finding 2 says the retry is immediate and covers only OSError, and mentions no backoff risk to load; it complains about the swallowed failure instead. It points at the retry lines and mentions immediate retry with no backoff, but does not state the cause that a struggling dependency gets hit harder.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean code (asked again): claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
