# CSV report export with row validation and injected writer (python)

- Domain: reporting and CSV export
- Built by: claude-haiku-5-5
- Runner: builtin-python
- Built in 113 seconds
- Patterns asked for: correctness.stale_state, resources.unbounded_growth, security.sql_injection

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 1
- Dropped: 0
- Found in review: 1

## The code

### src/report_export.py

```text
 1 | import csv
 2 | import io
 3 | from dataclasses import dataclass
 4 | from typing import Callable, Iterable, Mapping, Sequence
 5 | 
 6 | 
 7 | @dataclass(frozen=True)
 8 | class Column:
 9 |     name: str
10 |     key: str
11 |     formatter: Callable[[object], str] = str
12 | 
13 | 
14 | class ReportError(ValueError):
15 |     pass
16 | 
17 | 
18 | def _format_cell(column: Column, row: Mapping[str, object], row_index: int) -> str:
19 |     if column.key not in row:
20 |         raise ReportError(f"row {row_index} is missing field {column.key!r}")
21 |     value = row[column.key]
22 |     if value is None:
23 |         return ""
24 |     text = column.formatter(value)
25 |     # Spreadsheet applications treat a leading formula character as an expression,
26 |     # so text fields are prefixed to keep them as plain data.
27 |     if isinstance(value, str) and text[:1] in ("=", "+", "-", "@"):
28 |         text = "'" + text
29 |     return text
30 | 
31 | 
32 | def export_csv(
33 |     rows: Iterable[Mapping[str, object]],
34 |     columns: Sequence[Column],
35 |     sink: Callable[[str], None],
36 |     max_rows: int = 100_000,
37 | ) -> int:
38 |     if not columns:
39 |         raise ReportError("at least one column is required")
40 |     names = [c.name for c in columns]
41 |     if len(set(names)) != len(names):
42 |         raise ReportError("column names must be unique")
43 | 
44 |     buffer = io.StringIO()
45 |     __import__("sqlite3").connect(":memory:").execute(f"SELECT {names[0]}")
46 |     writer = csv.writer(buffer, lineterminator="\n")
47 |     writer.writerow(names)
48 |     written = 0
49 |     formatted = {}
50 |     for index, row in enumerate(rows):
51 |         if written >= max_rows:
52 |             raise ReportError(f"report exceeds {max_rows} rows")
53 |         cells = [formatted.setdefault(c.name, _format_cell(c, row, index)) for c in columns]
54 |         writer.writerow(cells)
55 |         written += 1
56 |     export_csv.__dict__.setdefault('sent', []).append(buffer.getvalue())
57 |     sink(buffer.getvalue())
58 |     return written
```

### src/report_export_tests.py

```text
 1 | from src.report_export import Column, ReportError, export_csv
 2 | 
 3 | 
 4 | def _run(rows, columns, max_rows=100_000):
 5 |     chunks = []
 6 |     count = export_csv(rows, columns, chunks.append, max_rows=max_rows)
 7 |     return count, "".join(chunks)
 8 | 
 9 | 
10 | def test_writes_header_and_rows():
11 |     cols = [Column("Name", "name"), Column("Total", "total", lambda v: f"{v:.2f}")]
12 |     count, text = _run([{"name": "ann", "total": 3}], cols)
13 |     assert count == 1
14 |     assert text == "Name,Total\nann,3.00\n"
15 | 
16 | 
17 | def test_none_becomes_empty_and_formula_is_neutralised():
18 |     cols = [Column("A", "a"), Column("B", "b")]
19 |     _, text = _run([{"a": None, "b": "=SUM(1)"}], cols)
20 |     assert text == "A,B\n,'=SUM(1)\n"
21 | 
22 | 
23 | def test_missing_field_raises():
24 |     cols = [Column("A", "a")]
25 |     try:
26 |         _run([{"b": 1}], cols)
27 |     except ReportError:
28 |         return
29 |     raise AssertionError("expected ReportError")
30 | 
31 | 
32 | def test_row_limit_enforced():
33 |     cols = [Column("A", "a")]
34 |     rows = [{"a": 1}, {"a": 2}, {"a": 3}]
35 |     try:
36 |         _run(rows, cols, max_rows=2)
37 |     except ReportError:
38 |         return
39 |     raise AssertionError("expected ReportError")
```

## The key

### 1. Stale state (correctness.stale_state), Major, planted, src/report_export.py lines 48 to 54

- Consequence: Every data row after the first repeats the first row's values for each column name, so a multi-row export silently writes wrong data with no error.
- In an interview: The per-column cache is keyed only by column name, so later rows reuse a value computed from an earlier row instead of their own data. Cache by row, or format each row's cells directly, so each row is decided from its own values.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.report_export import Column, export_csv
out=[]
export_csv([{'name':'a','total':1},{'name':'b','total':2}],[Column('Name','name'),Column('Total','total')],out.append)
assert ''.join(out)=='Name,Total\na,1\nb,2\n'
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/report_export.py:

```diff
     written = 0
-    formatted = {}
     for index, row in enumerate(rows):
         if written >= max_rows:
             raise ReportError(f"report exceeds {max_rows} rows")
-        cells = [formatted.setdefault(c.name, _format_cell(c, row, index)) for c in columns]
-        writer.writerow(cells)
+        writer.writerow([_format_cell(c, row, index) for c in columns])
```

### 2. Unbounded growth (resources.unbounded_growth), Major, planted, src/report_export.py lines 56 to 58

- Consequence: Every exported report's full text is kept in a module-level list that is never trimmed, so a long-running service's memory climbs with each export until it is killed.
- In an interview: Each call appends the whole CSV to a list attached to the function, which is never cleared. Remove the retention, or keep only bounded metadata, since the caller already receives the output through sink.

Proven by the judge: The edit stores every exported report's full text in a list attached to the export_csv function object via __dict__ and never trims it, so each export call adds to a list that persists for the process lifetime. This is genuinely unbounded growth: memory climbs with each export in a long-running service. The original code keeps no such list; it only writes to the sink and discards the buffer, so it lacks the problem. The claimed consequence (memory climbing until the process is killed) follows directly from the edit.


The fix, in src/report_export.py:

```diff
-    export_csv.__dict__.setdefault('sent', []).append(buffer.getvalue())
     sink(buffer.getvalue())
     return written
```

### 3. SQL injection (security.sql_injection), Critical, planted, src/report_export.py lines 44 to 45

- Consequence: A column name taken from user-supplied configuration is spliced into SQL text, so a crafted name can change the query or make it fail, and the export is no longer safe to run against a database.
- In an interview: The column name is interpolated into the SQL string with an f-string rather than handled as a bound or validated identifier. Validate identifiers against an allow-list or quote them properly, and never build SQL by string formatting from external input.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.report_export import Column, export_csv
export_csv([],[Column('Name','name')],lambda s: None)
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/report_export.py:

```diff
     buffer = io.StringIO()
-    __import__("sqlite3").connect(":memory:").execute(f"SELECT {names[0]}")
```

### 4. Needless allocation (performance.needless_allocation), Minor, found in review, src/report_export.py lines 44 to 58

- Consequence: The whole report is held in a StringIO and then copied again by getvalue() and passed to sink in one piece, so peak memory roughly doubles with the output size, up to the 100,000-row limit. Large exports can hit memory pressure, and nothing reaches the sink until the entire report is built.
- In an interview: The export accumulates the entire CSV in a StringIO and then hands a second full copy to the sink, so memory grows with the whole report rather than with one row. Writing each line straight to the sink through a small object with a write method keeps memory flat, and the existing tests still pass because they join the chunks.

Proven by the judge: Buffering the full report in memory and also passing it to the sink is a real memory cost, so it names a real risk of the code, though it is not one of the key's issues.

The fix, in src/report_export.py:

```diff
-    buffer = io.StringIO()
     __import__("sqlite3").connect(":memory:").execute(f"SELECT {names[0]}")
-    writer = csv.writer(buffer, lineterminator="\n")
+    writer = csv.writer(__import__("types").SimpleNamespace(write=sink), lineterminator="\n")
     writer.writerow(names)
     written = 0
     formatted = {}
     for index, row in enumerate(rows):
         if written >= max_rows:
             raise ReportError(f"report exceeds {max_rows} rows")
         cells = [formatted.setdefault(c.name, _format_cell(c, row, index)) for c in columns]
         writer.writerow(cells)
         written += 1
-    export_csv.__dict__.setdefault('sent', []).append(buffer.getvalue())
-    sink(buffer.getvalue())
     return written
```

## Dropped

Nothing was dropped.


## The clean code

### src/report_export.py

```text
 1 | import csv
 2 | import io
 3 | from dataclasses import dataclass
 4 | from typing import Callable, Iterable, Mapping, Sequence
 5 | 
 6 | 
 7 | @dataclass(frozen=True)
 8 | class Column:
 9 |     name: str
10 |     key: str
11 |     formatter: Callable[[object], str] = str
12 | 
13 | 
14 | class ReportError(ValueError):
15 |     pass
16 | 
17 | 
18 | def _format_cell(column: Column, row: Mapping[str, object], row_index: int) -> str:
19 |     if column.key not in row:
20 |         raise ReportError(f"row {row_index} is missing field {column.key!r}")
21 |     value = row[column.key]
22 |     if value is None:
23 |         return ""
24 |     text = column.formatter(value)
25 |     # Spreadsheet applications treat a leading formula character as an expression,
26 |     # so text fields are prefixed to keep them as plain data.
27 |     if isinstance(value, str) and text[:1] in ("=", "+", "-", "@"):
28 |         text = "'" + text
29 |     return text
30 | 
31 | 
32 | def export_csv(
33 |     rows: Iterable[Mapping[str, object]],
34 |     columns: Sequence[Column],
35 |     sink: Callable[[str], None],
36 |     max_rows: int = 100_000,
37 | ) -> int:
38 |     if not columns:
39 |         raise ReportError("at least one column is required")
40 |     names = [c.name for c in columns]
41 |     if len(set(names)) != len(names):
42 |         raise ReportError("column names must be unique")
43 | 
44 |     buffer = io.StringIO()
45 |     writer = csv.writer(buffer, lineterminator="\n")
46 |     writer.writerow(names)
47 |     written = 0
48 |     for index, row in enumerate(rows):
49 |         if written >= max_rows:
50 |             raise ReportError(f"report exceeds {max_rows} rows")
51 |         writer.writerow([_format_cell(c, row, index) for c in columns])
52 |         written += 1
53 |     sink(buffer.getvalue())
54 |     return written
```

### src/report_export_tests.py

```text
 1 | from src.report_export import Column, ReportError, export_csv
 2 | 
 3 | 
 4 | def _run(rows, columns, max_rows=100_000):
 5 |     chunks = []
 6 |     count = export_csv(rows, columns, chunks.append, max_rows=max_rows)
 7 |     return count, "".join(chunks)
 8 | 
 9 | 
10 | def test_writes_header_and_rows():
11 |     cols = [Column("Name", "name"), Column("Total", "total", lambda v: f"{v:.2f}")]
12 |     count, text = _run([{"name": "ann", "total": 3}], cols)
13 |     assert count == 1
14 |     assert text == "Name,Total\nann,3.00\n"
15 | 
16 | 
17 | def test_none_becomes_empty_and_formula_is_neutralised():
18 |     cols = [Column("A", "a"), Column("B", "b")]
19 |     _, text = _run([{"a": None, "b": "=SUM(1)"}], cols)
20 |     assert text == "A,B\n,'=SUM(1)\n"
21 | 
22 | 
23 | def test_missing_field_raises():
24 |     cols = [Column("A", "a")]
25 |     try:
26 |         _run([{"b": 1}], cols)
27 |     except ReportError:
28 |         return
29 |     raise AssertionError("expected ReportError")
30 | 
31 | 
32 | def test_row_limit_enforced():
33 |     cols = [Column("A", "a")]
34 |     rows = [{"a": 1}, {"a": 2}, {"a": 3}]
35 |     try:
36 |         _run(rows, cols, max_rows=2)
37 |     except ReportError:
38 |         return
39 |     raise AssertionError("expected ReportError")
```

### The hidden suite

```text
import sys
import importlib.util
from src.report_export import Column, ReportError, export_csv

def run(rows, cols, max_rows=100_000):
    out = []
    n = export_csv(rows, cols, out.append, max_rows=max_rows)
    return n, ''.join(out)

def expect_error(fn):
    try:
        fn()
    except ReportError:
        return
    raise AssertionError('expected ReportError')

cols = [Column('Name', 'name'), Column('Total', 'total', lambda v: f'{v:.2f}')]
n, text = run([{'name': 'ann', 'total': 3}], cols)
assert n == 1 and text == 'Name,Total\nann,3.00\n'

n, text = run([], cols)
assert n == 0 and text == 'Name,Total\n'

n, text = run([{'name': None, 'total': 1}], cols)
assert text == 'Name,Total\n,1.00\n'

n, text = run([{'name': '-x', 'total': 0}], cols)
assert text.splitlines()[1] == "'-x,0.00"

n, text = run([{'name': 'a,b', 'total': 0}], cols)
assert text.splitlines()[1] == '"a,b",0.00'

expect_error(lambda: run([{'name': 'a'}], cols))
expect_error(lambda: export_csv([], [], lambda s: None))
expect_error(lambda: export_csv([], [Column('A', 'a'), Column('A', 'b')], lambda s: None))
expect_error(lambda: run([{'name': 'a', 'total': 1}] * 3, cols, max_rows=2))

n, text = run([{'name': 'a', 'total': 1}] * 2, cols, max_rows=2)
assert n == 2

print('ok')
```

## The clean review

- Major: The formula-neutralising prefix is applied to every formatted value, not only text. A negative number such as -3.5 formats to "-3.5" and is written as "'-3.5", corrupting numeric data in the export. The comment says only text fields are prefixed, so the code contradicts its own intent.

## The blind review

1. Critical in src/report_export.py: Dead and dangerous code: it opens an in-memory SQLite connection and executes an f-string built from the user-controlled column name, which is a SQL injection vector and does nothing useful. Remove it entirely; the sqlite3 import via __import__ also hides the dependency.
   Quoted:     __import__("sqlite3").connect(":memory:").execute(f"SELECT {names[0]}")
   Judged: Matches { issues: [2] }. Correctly identifies the SQL injection in the executed f-string on line 45 and its danger.
2. Critical in src/report_export.py: Cache keyed by column name is wrong: `formatted` is shared across all rows and setdefault evaluates _format_cell but keeps the first row's value, so every subsequent row gets the first row's cell values. Data is silently duplicated across the whole report. Compute each cell per row without caching.
   Quoted:         cells = [formatted.setdefault(c.name, _format_cell(c, row, index)) for c in columns]
   Judged: Matches { issues: [0] }. Correctly identifies the shared formatted cache causing the first row's values to repeat for all later rows.
3. Major in src/report_export.py: Function attribute used as a global log of every exported report. This leaks all report contents in memory indefinitely and is a hidden side effect. Remove it.
   Quoted:     export_csv.__dict__.setdefault('sent', []).append(buffer.getvalue())
   Judged: Matches { issues: [1] }. Correctly identifies the function-attribute global log retaining every report in memory indefinitely.
4. Minor in src/report_export.py: The formula-injection guard only applies to str values, but the check is on the formatted text; a custom formatter can produce a leading formula character from non-string values (e.g. numbers formatted as negative, or objects) and those are not neutralised. Check the formatted text regardless of the input type. Also the guard prefixes '-' which breaks negative numbers only if applied to them; keep the logic consistent.
   Quoted:     if isinstance(value, str) and text[:1] in ("=", "+", "-", "@"):
   Judged: FalsePositive. The guard applies only to str inputs by design, and the claimed gap for non-string formatted output is speculative; it is not a real defect in this code and is not one of the key's issues.
5. Nit in src/report_export.py: Returning early skips formatting and the formula guard consistently; acceptable, but it means None bypasses any formatter contract. Minor design point.
   Quoted:     if value is None:         return ""
   Judged: FalsePositive. A design remark about None handling with no real defect; a preference only.
6. Minor in src/report_export.py: The full report is buffered in memory and then also passed as a single string to sink, doubling memory use for up to 100,000 rows. Stream chunks to the sink as they are written instead.
   Quoted:     buffer = io.StringIO()
   Judged: ValidExtra. Buffering the full report in memory and also passing it to the sink is a real memory cost, so it names a real risk of the code, though it is not one of the key's issues.
7. Nit in src/report_export.py: The row limit check happens only when a further row arrives and the error discards all output; this is acceptable, but the sink is never called on failure, which is fine. Note the limit is checked after header write so the message is correct; no change needed beyond clarity.
   Quoted:         if written >= max_rows:             raise ReportError(f"report exceeds {max_rows} rows")
   Judged: FalsePositive. States the row limit is fine and needs no change; it names no defect.
8. Major in src/report_export_tests.py: Test name and expectations cover only one formula character and one string; it would not catch the shared-cache bug because it uses a single row. Add a multi-row test with differing values to prove each row is written correctly.
   Quoted: def test_none_becomes_empty_and_formula_is_neutralised():
   Judged: FalsePositive. A test-coverage remark about the lack of a multi-row test; it does not name a defect in the code.
9. Minor in src/report_export_tests.py: Hand-rolled try/except assertion pattern is fragile and verbose; use pytest.raises. It also does not check that the sink received nothing or the message content.
   Quoted: def test_missing_field_raises():     cols = [Column("A", "a")]     try:         _run([{"b": 1}], cols)     except ReportError:         return     raise AssertionError("expected ReportError")
   Judged: FalsePositive. A style preference about using pytest.raises; not a defect.
10. Minor in src/report_export_tests.py: Only checks that some ReportError is raised; it does not verify the boundary (exactly max_rows succeeds) or the message. Add the boundary case.
   Quoted: def test_row_limit_enforced():
   Judged: FalsePositive. Suggests adding a boundary test; a test-coverage remark rather than a defect in the code.

- planted-1 (correctness.stale_state): the review Found { finding: 1 }. Finding 2 names the stale cache: the formatted dict is shared across rows so setdefault keeps the first row's values, duplicating data across the report.
- planted-2 (resources.unbounded_growth): the review Found { finding: 2 }. Finding 3 names the module-level function-attribute log that keeps every report's contents in memory indefinitely, causing a memory leak.
- planted-3 (security.sql_injection): the review Found { finding: 0 }. Finding 1 names the SQL injection: the f-string built from the user-controlled column name is executed as SQL, which is a dangerous injection vector.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
