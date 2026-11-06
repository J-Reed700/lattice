# CSV report export with streaming rows and injected clock and writer (javascript)

- Domain: reporting and CSV export
- Built by: claude-haiku-5-5
- Runner: builtin-javascript
- Built in 96 seconds
- Patterns asked for: correctness.float_for_exact_values, resources.unbounded_growth, security.sql_injection

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 1
- Dropped: 0
- Found in review: 1

## The code

### src/csv.mjs

```text
 1 | const NEEDS_QUOTING = /[",\r\n]/;
 2 | 
 3 | export function escapeCsvField(value) {
 4 |   if (value === null || value === undefined) {
 5 |     return '';
 6 |   }
 7 |   const text = String(value);
 8 |   if (!NEEDS_QUOTING.test(text)) {
 9 |     return text;
10 |   }
11 |   // Quotes inside a field are doubled per RFC 4180 so spreadsheet readers
12 |   // keep the field boundaries intact.
13 |   return '"' + text.replace(/"/g, '""') + '"';
14 | }
15 | 
16 | export function formatCsvLine(cells) {
17 |   return cells.map(escapeCsvField).join(',') + '\r\n';
18 | }
```

### src/report.mjs

```text
 1 | import { formatCsvLine } from './csv.mjs';
 2 | 
 3 | const DEFAULT_COLUMNS = ['id', 'name', 'amount', 'created_at'];
 4 | 
 5 | export function buildRowsQuery(table, id) {
 6 |   return 'SELECT * FROM ' + table + ' WHERE id = ' + id;
 7 | }
 8 | 
 9 | // Exports rows from an async source into a sink. The clock and the sink are
10 | // parameters so that exports are reproducible and testable without I/O.
11 | export async function exportReport({ rows, sink, clock, columns = DEFAULT_COLUMNS, maxRows = Infinity }) {
12 |   if (typeof sink?.write !== 'function') {
13 |     throw new TypeError('sink must provide a write(text) method');
14 |   }
15 |   const startedAt = clock();
16 |   let header = formatCsvLine(columns);
17 |   (globalThis.exportHistory = globalThis.exportHistory || []).push(header);
18 |   await sink.write(header);
19 | 
20 |   let written = 0;
21 |   let skipped = 0;
22 |   for await (const row of rows) {
23 |     if (written >= maxRows) {
24 |       skipped += 1;
25 |       continue;
26 |     }
27 |     const cells = columns.map((column) => {
28 |       const value = row[column];
29 |       // Dates are rendered as ISO strings so the output does not depend on
30 |       // the locale of the machine running the export.
31 |       if (value instanceof Date) {
32 |         return Number.isNaN(value.getTime()) ? '' : value.toISOString();
33 |       }
34 |       return column === 'amount' ? Math.trunc(value * 100) / 100 : value;
35 |     });
36 |     await sink.write(formatCsvLine(cells));
37 |     written += 1;
38 |   }
39 | 
40 |   const finishedAt = clock();
41 |   return { written, skipped, elapsedMs: finishedAt - startedAt };
42 | }
```

### test/export.check.mjs

```text
 1 | import { escapeCsvField, formatCsvLine } from '../src/csv.mjs';
 2 | import { exportReport } from '../src/report.mjs';
 3 | 
 4 | function assertEqual(actual, expected, label) {
 5 |   if (actual !== expected) {
 6 |     throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
 7 |   }
 8 | }
 9 | 
10 | assertEqual(escapeCsvField(null), '', 'null cell');
11 | assertEqual(escapeCsvField('plain'), 'plain', 'plain cell');
12 | assertEqual(escapeCsvField('a,b'), '"a,b"', 'comma cell');
13 | assertEqual(escapeCsvField('say "hi"'), '"say ""hi"""', 'quote cell');
14 | assertEqual(formatCsvLine(['x', 'y']), 'x,y\r\n', 'line');
15 | 
16 | async function* sampleRows() {
17 |   yield { id: 1, name: 'Ada, L.', amount: 10, created_at: new Date(Date.UTC(2024, 0, 2)) };
18 |   yield { id: 2, name: 'Bob', amount: 5, created_at: new Date(Date.UTC(2024, 1, 3)) };
19 |   yield { id: 3, name: 'Cy', amount: 7, created_at: new Date(Date.UTC(2024, 2, 4)) };
20 | }
21 | 
22 | const lines = [];
23 | const sink = { write: async (text) => { lines.push(text); } };
24 | let tick = 100;
25 | const clock = () => (tick += 5);
26 | 
27 | const result = await exportReport({ rows: sampleRows(), sink, clock });
28 | assertEqual(result.written, 3, 'written count');
29 | assertEqual(result.skipped, 0, 'skipped count');
30 | assertEqual(result.elapsedMs, 5, 'elapsed');
31 | assertEqual(lines[0], 'id,name,amount,created_at\r\n', 'header');
32 | assertEqual(lines[1], '1,"Ada, L.",10,2024-01-02T00:00:00.000Z\r\n', 'first row');
33 | 
34 | const limited = [];
35 | const limitedSink = { write: async (text) => { limited.push(text); } };
36 | const capped = await exportReport({
37 |   rows: sampleRows(),
38 |   sink: limitedSink,
39 |   clock,
40 |   columns: ['id'],
41 |   maxRows: 2,
42 | });
43 | assertEqual(capped.written, 2, 'capped written');
44 | assertEqual(capped.skipped, 1, 'capped skipped');
45 | assertEqual(limited.length, 3, 'header plus two rows');
46 | 
47 | let rejected = false;
48 | try {
49 |   await exportReport({ rows: sampleRows(), sink: {}, clock });
50 | } catch (error) {
51 |   rejected = error instanceof TypeError;
52 | }
53 | assertEqual(rejected, true, 'missing sink rejected');
```

## The key

### 1. Floating point for exact values (correctness.float_for_exact_values), Major, planted, src/report.mjs lines 34 to 34

- Consequence: Amounts with fractional cents are exported wrong: 1.15 is written as 1.14 because 1.15 * 100 is stored as 114.99999999999999 in binary floating point. Every export that contains such amounts under-reports money, and the error appears silently on each run.
- In an interview: The export scales amounts to cents with float multiplication and truncates, which loses a cent whenever the binary value falls just below the decimal. Keep money in integer cents end to end, or use a decimal type, and round rather than truncate when a conversion is unavoidable.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { exportReport } from './src/report.mjs';
async function* r() { yield { id: 1, name: 'x', amount: 1.15, created_at: new Date(0) }; }
const out = [];
await exportReport({ rows: r(), sink: { write: async (s) => { out.push(s); } }, clock: () => 0 });
if (out[1] !== '1,x,1.15,1970-01-01T00:00:00.000Z\r\n') throw new Error('amount exported as ' + JSON.stringify(out[1]));
console.log('ok');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:5:112)"

The fix, in src/report.mjs:

```diff
-      return column === 'amount' ? Math.trunc(value * 100) / 100 : value;
+      return value;
```

### 2. Unbounded growth (resources.unbounded_growth), Major, planted, src/report.mjs lines 16 to 17

- Consequence: Every export call appends its header to a module-wide history that is never trimmed. A long-running service that exports on each request keeps growing its heap until the process is killed out of memory.
- In an interview: The header line is pushed onto a global array on every export, so the array grows with each request and nothing removes entries. Keep history local to the call, or bound it with a fixed-size ring buffer or an explicit eviction policy.

Proven by the judge: The edit pushes each export's header onto globalThis.exportHistory, an array that is never trimmed or cleared, so every export call adds an entry and memory grows with each call. This is exactly the unbounded growth pattern with the claimed consequence. The original code keeps no module-wide history; each call's header is local and garbage-collected, so the original lacks the problem.


The fix, in src/report.mjs:

```diff
   let header = formatCsvLine(columns);
-  (globalThis.exportHistory = globalThis.exportHistory || []).push(header);
```

### 3. SQL injection (security.sql_injection), Critical, planted, src/report.mjs lines 3 to 7

- Consequence: When a caller passes a user-supplied id, an attacker can append SQL such as 1 OR 1=1 and read every row, or inject statements that modify data. The query text changes meaning instead of being treated as a value.
- In an interview: The helper concatenates the id straight into the SQL text, so input becomes part of the statement. Pass the value as a bound parameter, for example a placeholder with a separate params array, so the driver never parses it as SQL.

Proven by a test that fails on the drill and passes on the clean code:

```text
import * as r from './src/report.mjs';
if (typeof r.buildRowsQuery === 'function') {
  const q = r.buildRowsQuery('rows', '1 OR 1=1');
  if (String(q).includes('1 OR 1=1')) throw new Error('raw input interpolated into SQL: ' + q);
}
console.log('ok');
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:4:93)"

The fix, in src/report.mjs:

```diff
 const DEFAULT_COLUMNS = ['id', 'name', 'amount', 'created_at'];
-
-export function buildRowsQuery(table, id) {
-  return 'SELECT * FROM ' + table + ' WHERE id = ' + id;
-}
```

### 4. Missing null check (correctness.null_dereference), Major, found in review, src/report.mjs lines 27 to 28

- Consequence: When a null or undefined row arrives in the rows stream, row[column] throws a TypeError partway through the export. The header and earlier rows have already been written to the sink, so the destination holds a truncated CSV with no record of which row failed.
- In an interview: Dereferencing a row that may be null without a check means the failure happens after the header and some rows are already written, leaving a silently partial file. Validating each row before use turns this into a clear error that names the offending row, though the export still fails after partial output, so a rollback or temp-file strategy would be needed to avoid a half-written file entirely.

Proven by the judge: A null row throwing mid-export is a real robustness concern in the clean code too, but it is not one of the planted issues and is a valid observation about the code.

The fix, in src/report.mjs:

```diff
+    if (row === null || typeof row !== 'object') {
+      throw new TypeError(`row ${written + skipped} is not an object`);
+    }
     const cells = columns.map((column) => {
       const value = row[column];
```

## Dropped

Nothing was dropped.


## The clean code

### src/csv.mjs

```text
 1 | const NEEDS_QUOTING = /[",\r\n]/;
 2 | 
 3 | export function escapeCsvField(value) {
 4 |   if (value === null || value === undefined) {
 5 |     return '';
 6 |   }
 7 |   const text = String(value);
 8 |   if (!NEEDS_QUOTING.test(text)) {
 9 |     return text;
10 |   }
11 |   // Quotes inside a field are doubled per RFC 4180 so spreadsheet readers
12 |   // keep the field boundaries intact.
13 |   return '"' + text.replace(/"/g, '""') + '"';
14 | }
15 | 
16 | export function formatCsvLine(cells) {
17 |   return cells.map(escapeCsvField).join(',') + '\r\n';
18 | }
```

### src/report.mjs

```text
 1 | import { formatCsvLine } from './csv.mjs';
 2 | 
 3 | const DEFAULT_COLUMNS = ['id', 'name', 'amount', 'created_at'];
 4 | 
 5 | // Exports rows from an async source into a sink. The clock and the sink are
 6 | // parameters so that exports are reproducible and testable without I/O.
 7 | export async function exportReport({ rows, sink, clock, columns = DEFAULT_COLUMNS, maxRows = Infinity }) {
 8 |   if (typeof sink?.write !== 'function') {
 9 |     throw new TypeError('sink must provide a write(text) method');
10 |   }
11 |   const startedAt = clock();
12 |   let header = formatCsvLine(columns);
13 |   await sink.write(header);
14 | 
15 |   let written = 0;
16 |   let skipped = 0;
17 |   for await (const row of rows) {
18 |     if (written >= maxRows) {
19 |       skipped += 1;
20 |       continue;
21 |     }
22 |     const cells = columns.map((column) => {
23 |       const value = row[column];
24 |       // Dates are rendered as ISO strings so the output does not depend on
25 |       // the locale of the machine running the export.
26 |       if (value instanceof Date) {
27 |         return Number.isNaN(value.getTime()) ? '' : value.toISOString();
28 |       }
29 |       return value;
30 |     });
31 |     await sink.write(formatCsvLine(cells));
32 |     written += 1;
33 |   }
34 | 
35 |   const finishedAt = clock();
36 |   return { written, skipped, elapsedMs: finishedAt - startedAt };
37 | }
```

### test/export.check.mjs

```text
 1 | import { escapeCsvField, formatCsvLine } from '../src/csv.mjs';
 2 | import { exportReport } from '../src/report.mjs';
 3 | 
 4 | function assertEqual(actual, expected, label) {
 5 |   if (actual !== expected) {
 6 |     throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
 7 |   }
 8 | }
 9 | 
10 | assertEqual(escapeCsvField(null), '', 'null cell');
11 | assertEqual(escapeCsvField('plain'), 'plain', 'plain cell');
12 | assertEqual(escapeCsvField('a,b'), '"a,b"', 'comma cell');
13 | assertEqual(escapeCsvField('say "hi"'), '"say ""hi"""', 'quote cell');
14 | assertEqual(formatCsvLine(['x', 'y']), 'x,y\r\n', 'line');
15 | 
16 | async function* sampleRows() {
17 |   yield { id: 1, name: 'Ada, L.', amount: 10, created_at: new Date(Date.UTC(2024, 0, 2)) };
18 |   yield { id: 2, name: 'Bob', amount: 5, created_at: new Date(Date.UTC(2024, 1, 3)) };
19 |   yield { id: 3, name: 'Cy', amount: 7, created_at: new Date(Date.UTC(2024, 2, 4)) };
20 | }
21 | 
22 | const lines = [];
23 | const sink = { write: async (text) => { lines.push(text); } };
24 | let tick = 100;
25 | const clock = () => (tick += 5);
26 | 
27 | const result = await exportReport({ rows: sampleRows(), sink, clock });
28 | assertEqual(result.written, 3, 'written count');
29 | assertEqual(result.skipped, 0, 'skipped count');
30 | assertEqual(result.elapsedMs, 5, 'elapsed');
31 | assertEqual(lines[0], 'id,name,amount,created_at\r\n', 'header');
32 | assertEqual(lines[1], '1,"Ada, L.",10,2024-01-02T00:00:00.000Z\r\n', 'first row');
33 | 
34 | const limited = [];
35 | const limitedSink = { write: async (text) => { limited.push(text); } };
36 | const capped = await exportReport({
37 |   rows: sampleRows(),
38 |   sink: limitedSink,
39 |   clock,
40 |   columns: ['id'],
41 |   maxRows: 2,
42 | });
43 | assertEqual(capped.written, 2, 'capped written');
44 | assertEqual(capped.skipped, 1, 'capped skipped');
45 | assertEqual(limited.length, 3, 'header plus two rows');
46 | 
47 | let rejected = false;
48 | try {
49 |   await exportReport({ rows: sampleRows(), sink: {}, clock });
50 | } catch (error) {
51 |   rejected = error instanceof TypeError;
52 | }
53 | assertEqual(rejected, true, 'missing sink rejected');
```

### The hidden suite

```text
import { escapeCsvField, formatCsvLine } from './src/csv.mjs';
import { exportReport } from './src/report.mjs';

function check(condition, label) {
  if (!condition) {
    throw new Error('check failed: ' + label);
  }
}

check(escapeCsvField(null) === '', 'null becomes empty');
check(escapeCsvField(undefined) === '', 'undefined becomes empty');
check(escapeCsvField(42) === '42', 'number stringified');
check(escapeCsvField('a\nb') === '"a\nb"', 'newline quoted');
check(formatCsvLine(['a', 'b c']) === 'a,b c\r\n', 'line terminator');

async function* rows() {
  yield { id: 1, name: 'x,y', amount: 3, created_at: new Date(0) };
  yield { id: 2, name: 'z', amount: 4, created_at: new Date(1000) };
}

const out = [];
let t = 0;
const result = await exportReport({
  rows: rows(),
  sink: { write: async (s) => { out.push(s); } },
  clock: () => (t += 1),
});
check(result.written === 2, 'two rows written');
check(result.skipped === 0, 'nothing skipped');
check(result.elapsedMs === 1, 'elapsed from clock');
check(out.length === 3, 'header and two rows');
check(out[1] === '1,"x,y",3,1970-01-01T00:00:00.000Z\r\n', 'quoted row with date');

let threw = false;
try {
  await exportReport({ rows: rows(), sink: null, clock: () => 0 });
} catch (e) {
  threw = e instanceof TypeError;
}
check(threw, 'invalid sink throws TypeError');

console.log('all checks passed');
```

## The clean review

- Minor: An invalid Date (NaN time) makes toISOString() throw a RangeError partway through the export, after the header and earlier rows have already been written to the sink. The export then fails with a partial file. Invalid dates should be rendered as an empty cell instead.

## The blind review

1. Major in src/report.mjs: Side effect on a global: every export appends its header to a process-wide array that is never cleared, so memory grows without bound in long-running processes and the function is no longer self-contained or reproducible, contrary to the stated design. Remove it or return the history to the caller.
   Quoted: (globalThis.exportHistory = globalThis.exportHistory || []).push(header);
   Judged: Matches { issues: [1] }. Correctly identifies the unbounded global history array that grows with every export call.
2. Critical in src/report.mjs: SQL built by string concatenation with unvalidated table and id: SQL injection. Use parameterized queries for id and an allowlist for the table identifier.
   Quoted: export function buildRowsQuery(table, id) {   return 'SELECT * FROM ' + table + ' WHERE id = ' + id; }
   Judged: Matches { issues: [2] }. Correctly identifies SQL injection from string concatenation of table and id.
3. Major in src/report.mjs: Truncating amount with Math.trunc(value * 100) is lossy and float-unsafe (e.g. 1.005 * 100 is 100.49999 so it becomes 1.00), and null/undefined amounts become NaN or 0 silently. Use integer minor units or toFixed on a validated number and handle missing values explicitly.
   Quoted: return column === 'amount' ? Math.trunc(value * 100) / 100 : value;
   Judged: Matches { issues: [0] }. Correctly identifies the float-unsafe amount truncation with a concrete example.
4. Minor in src/report.mjs: If row is null or undefined this throws a TypeError mid-export after the header was already written, leaving a partial file with no indication of which row failed. Validate rows or wrap the loop with error context.
   Quoted:     const cells = columns.map((column) => {       const value = row[column];
   Judged: ValidExtra. A null row throwing mid-export is a real robustness concern in the clean code too, but it is not one of the planted issues and is a valid observation about the code.
5. Minor in src/report.mjs: After the cap is reached the loop keeps consuming the whole source just to count skipped rows, which can be very slow for large or streaming queries. Break out and count remaining rows only if needed, or document the cost.
   Quoted:     if (written >= maxRows) {       skipped += 1;       continue;     }
   Judged: ValidExtra. Draining the whole source after the cap only to count skipped rows is a real performance cost of the clean code's design, a valid extra observation.
6. Minor in src/csv.mjs: Fields that are not strings are passed through as text but formula-like values beginning with =, +, -, or @ are not neutralized, so exported spreadsheets can execute injected formulas (CSV injection). Consider prefixing such cells.
   Quoted: const NEEDS_QUOTING = /[",\r\n]/;
   Judged: ValidExtra. CSV formula injection is a real risk for spreadsheet exports, though the key does not hold it; the regex line does not neutralize formula-leading cells, making it a valid extra.
7. Minor in test/export.check.mjs: The test never checks the skipped third row's content or that the sink receives no extra writes, and the global exportHistory side effect is untested. Coverage of amount truncation, null dates, and invalid dates is missing, so the tests prove little about the riskier logic.
   Quoted: assertEqual(limited.length, 3, 'header plus two rows');
   Judged: FalsePositive. Points at test coverage gaps, which is a remark about tests rather than a defect in the code's logic; it does not name a real defect of the code, so it is a false positive for this grading.

- planted-1 (correctness.float_for_exact_values): the review Found { finding: 2 }. Finding 3 says the Math.trunc(value * 100) amount handling is lossy and float-unsafe, giving the example 1.005 * 100 becoming 100.49999 so the value is wrong. This names the floating-point cause of the wrong amounts.
- planted-2 (resources.unbounded_growth): the review Found { finding: 0 }. Finding 1 names the process-wide globalThis.exportHistory array that is appended to on every export and never cleared, so memory grows without bound in long-running processes. This is the unbounded growth cause.
- planted-3 (security.sql_injection): the review Found { finding: 1 }. Finding 2 names the string-concatenated SQL in buildRowsQuery as SQL injection and says to use parameterized queries, which is the cause of issue 1.

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
