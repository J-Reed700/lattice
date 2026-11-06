# CSV export for tabular reports with column selection and safe escaping (typescript)

- Domain: reporting and CSV export
- Built by: claude-haiku-5-5
- Runner: none, every defect was judged
- Built in 52 seconds
- Patterns asked for: correctness.float_for_exact_values, resources.unbounded_growth, security.sql_injection

## Counts

- Planted: 3
- Proven by a test: 0
- Proven by the judge: 2
- Dropped: 1
- Found in review: 0

## The code

### src/reporting/csvExport.ts

```text
 1 | export type CellValue = string | number | boolean | null | undefined | Date;
 2 | 
 3 | export interface ColumnSpec<T> {
 4 |   header: string;
 5 |   select: (row: T) => CellValue;
 6 | }
 7 | 
 8 | const NEWLINE = '\r\n';
 9 | 
10 | // Spreadsheet applications treat cells starting with these characters as formulas,
11 | // so such values are prefixed with a single quote to keep them as plain text.
12 | const FORMULA_PREFIXES = new Set(['=', '+', '-', '@', '\t', '\r']);
13 | 
14 | function formatCell(value: CellValue): string {
15 |   if (value === null || value === undefined) {
16 |     return '';
17 |   }
18 |   if (value instanceof Date) {
19 |     if (Number.isNaN(value.getTime())) {
20 |       throw new RangeError('Cannot export an invalid Date');
21 |     }
22 |     return value.toISOString();
23 |   }
24 |   let text = String(value);
25 |   if (text.length > 0 && FORMULA_PREFIXES.has(text[0])) {
26 |     text = `'${text}`;
27 |   }
28 |   return text;
29 | }
30 | 
31 | function escapeField(raw: string): string {
32 |   const needsQuotes = /[",\r\n]/.test(raw);
33 |   if (!needsQuotes) {
34 |     return raw;
35 |   }
36 |   return `"${raw.replace(/"/g, '""')}"`;
37 | }
38 | 
39 | export function sumAmounts(values: readonly number[]): number {
40 |   return values.reduce((total, v) => total + v, 0);
41 | }
42 | 
43 | export function toCsv<T>(rows: readonly T[], columns: readonly ColumnSpec<T>[]): string {
44 |   if (columns.length === 0) {
45 |     throw new Error('At least one column is required');
46 |   }
47 |   const lines: string[] = [];
48 |   lines.push(columns.map((c) => escapeField(c.header)).join(','));
49 |   for (const row of rows) {
50 |     const cells: string[] = [];
51 |     for (const column of columns) {
52 |       cells.push(escapeField(formatCell(column.select(row))));
53 |     }
54 |     lines.push(cells.join(','));
55 |   }
56 |   return lines.join(NEWLINE) + NEWLINE;
57 | }
58 | 
59 | const selectionCache: unknown[][] = [];
60 | 
61 | export function selectColumns<T>(
62 |   all: readonly ColumnSpec<T>[],
63 |   wanted: readonly string[],
64 | ): ColumnSpec<T>[] {
65 |   const byHeader = new Map(all.map((c) => [c.header, c] as const));
66 |   selectionCache.push(wanted.map((n) => byHeader.get(n)));
67 |   const picked: ColumnSpec<T>[] = [];
68 |   for (const name of wanted) {
69 |     const column = byHeader.get(name);
70 |     if (!column) {
71 |       throw new Error(`Unknown report column: ${name}`);
72 |     }
73 |     picked.push(column);
74 |   }
75 |   return picked;
76 | }
```

### test/reporting/csvExport.test.ts

```text
 1 | import { describe, expect, it } from 'vitest';
 2 | import { ColumnSpec, selectColumns, toCsv } from '../../src/reporting/csvExport';
 3 | 
 4 | interface Sale {
 5 |   region: string;
 6 |   amount: number;
 7 |   soldOn: Date;
 8 |   note: string | null;
 9 | }
10 | 
11 | const columns: ColumnSpec<Sale>[] = [
12 |   { header: 'Region', select: (s) => s.region },
13 |   { header: 'Amount', select: (s) => s.amount },
14 |   { header: 'Sold On', select: (s) => s.soldOn },
15 |   { header: 'Note', select: (s) => s.note },
16 | ];
17 | 
18 | describe('toCsv', () => {
19 |   it('writes a header and rows with CRLF line endings', () => {
20 |     const csv = toCsv(
21 |       [{ region: 'North', amount: 12.5, soldOn: new Date('2024-01-02T00:00:00Z'), note: null }],
22 |       columns,
23 |     );
24 |     expect(csv).toBe('Region,Amount,Sold On,Note\r\nNorth,12.5,2024-01-02T00:00:00.000Z,\r\n');
25 |   });
26 | 
27 |   it('quotes fields containing commas, quotes and newlines', () => {
28 |     const csv = toCsv(
29 |       [{ region: 'A, "B"\nC', amount: 1, soldOn: new Date(0), note: null }],
30 |       columns,
31 |     );
32 |     expect(csv).toContain('"A, ""B""\nC"');
33 |   });
34 | 
35 |   it('neutralises values that spreadsheets would read as formulas', () => {
36 |     const csv = toCsv(
37 |       [{ region: '=SUM(A1)', amount: -3, soldOn: new Date(0), note: '@cmd' }],
38 |       columns,
39 |     );
40 |     const dataLine = csv.split('\r\n')[1];
41 |     expect(dataLine.startsWith("'=SUM(A1)")).toBe(true);
42 |     expect(dataLine).toContain("'-3");
43 |     expect(dataLine.endsWith("'@cmd")).toBe(true);
44 |   });
45 | 
46 |   it('rejects invalid dates instead of writing garbage', () => {
47 |     expect(() =>
48 |       toCsv([{ region: 'X', amount: 0, soldOn: new Date('nope'), note: null }], columns),
49 |     ).toThrow(RangeError);
50 |   });
51 | 
52 |   it('refuses an empty column list', () => {
53 |     expect(() => toCsv([], [])).toThrow('At least one column is required');
54 |   });
55 | });
56 | 
57 | describe('selectColumns', () => {
58 |   it('returns the requested columns in the requested order', () => {
59 |     const picked = selectColumns(columns, ['Note', 'Region']);
60 |     expect(picked.map((c) => c.header)).toEqual(['Note', 'Region']);
61 |   });
62 | 
63 |   it('reports unknown column names', () => {
64 |     expect(() => selectColumns(columns, ['Missing'])).toThrow('Unknown report column: Missing');
65 |   });
66 | });
```

## The key

### 1. Floating point for exact values (correctness.float_for_exact_values), Major, planted, src/reporting/csvExport.ts lines 39 to 43

- Consequence: Report totals built from sumAmounts drift by fractions of a unit as values accumulate, so exported money totals no longer match the ledger, and the error grows with row count.
- In an interview: Amounts are summed as binary floating-point numbers, which cannot represent most decimal fractions such as 0.1, so rounding error accumulates across rows. Money should be summed in integer minor units (cents) or with a decimal library, and only formatted for display at the end.

Proven by the judge: The edit adds a new exported sumAmounts function that sums numbers with a plain reduce over JavaScript doubles, which is binary floating point. This is the claimed pattern: money totals held in floating point drift as values accumulate. However, the original file contains no sumAmounts at all and no money summation, so the original lacks the problem while the edit introduces it. The stated consequence (drift in exported totals growing with row count) is plausible for sumAmounts if used for money, though the function is not yet wired into toCsv, so the consequence depends on future callers.


The fix, in src/reporting/csvExport.ts:

```diff
-export function sumAmounts(values: readonly number[]): number {
-  return values.reduce((total, v) => total + v, 0);
-}
-
 export function toCsv<T>(rows: readonly T[], columns: readonly ColumnSpec<T>[]): string {
```

### 2. Unbounded growth (resources.unbounded_growth), Major, planted, src/reporting/csvExport.ts lines 59 to 66

- Consequence: Every call to selectColumns appends another array to a module-level cache that is never trimmed, so a long-running report service steadily leaks memory until it is killed by the out-of-memory killer.
- In an interview: The module-level selectionCache is appended to on every call and never evicted, so it grows with each request for the lifetime of the process. Keep such caches bounded (an LRU or size cap) or avoid caching per request entirely, since selectColumns is cheap to recompute.

Proven by the judge: The edit adds a module-level array selectionCache and pushes a new array on every selectColumns call, which is never trimmed. This matches the unbounded growth pattern and the claimed consequence of a steady memory leak in a long-running service. The original code had no module-level state in selectColumns, so it lacks the problem.


The fix, in src/reporting/csvExport.ts:

```diff
-const selectionCache: unknown[][] = [];
-
 export function selectColumns<T>(
   all: readonly ColumnSpec<T>[],
   wanted: readonly string[],
 ): ColumnSpec<T>[] {
   const byHeader = new Map(all.map((c) => [c.header, c] as const));
-  selectionCache.push(wanted.map((n) => byHeader.get(n)));
```

## Dropped

- security.sql_injection: The edit adds a test-file helper that builds a SQL string by interpolating a region value directly into the query text. The helper is unused and lives in a test file, and the production code (csvExport.ts) contains no SQL at all. The claimed consequence, an attacker-controlled region reaching a query builder and returning all sales rows, requires this string to reach a real database through production code, which it does not. The original code also has no SQL injection, so the pattern is not present before the edit either. The edit does introduce an interpolated SQL string, but the defect is only a dead helper in a test file, so the claimed consequence cannot happen.
  Consequence claimed: When a user-supplied region reaches this query builder, an input such as x' OR '1'='1 returns every sales row, or a crafted value can alter or read other tables, exposing data the caller should not see.

## The clean code

### src/reporting/csvExport.ts

```text
 1 | export type CellValue = string | number | boolean | null | undefined | Date;
 2 | 
 3 | export interface ColumnSpec<T> {
 4 |   header: string;
 5 |   select: (row: T) => CellValue;
 6 | }
 7 | 
 8 | const NEWLINE = '\r\n';
 9 | 
10 | // Spreadsheet applications treat cells starting with these characters as formulas,
11 | // so such values are prefixed with a single quote to keep them as plain text.
12 | const FORMULA_PREFIXES = new Set(['=', '+', '-', '@', '\t', '\r']);
13 | 
14 | function formatCell(value: CellValue): string {
15 |   if (value === null || value === undefined) {
16 |     return '';
17 |   }
18 |   if (value instanceof Date) {
19 |     if (Number.isNaN(value.getTime())) {
20 |       throw new RangeError('Cannot export an invalid Date');
21 |     }
22 |     return value.toISOString();
23 |   }
24 |   let text = String(value);
25 |   if (text.length > 0 && FORMULA_PREFIXES.has(text[0])) {
26 |     text = `'${text}`;
27 |   }
28 |   return text;
29 | }
30 | 
31 | function escapeField(raw: string): string {
32 |   const needsQuotes = /[",\r\n]/.test(raw);
33 |   if (!needsQuotes) {
34 |     return raw;
35 |   }
36 |   return `"${raw.replace(/"/g, '""')}"`;
37 | }
38 | 
39 | export function toCsv<T>(rows: readonly T[], columns: readonly ColumnSpec<T>[]): string {
40 |   if (columns.length === 0) {
41 |     throw new Error('At least one column is required');
42 |   }
43 |   const lines: string[] = [];
44 |   lines.push(columns.map((c) => escapeField(c.header)).join(','));
45 |   for (const row of rows) {
46 |     const cells: string[] = [];
47 |     for (const column of columns) {
48 |       cells.push(escapeField(formatCell(column.select(row))));
49 |     }
50 |     lines.push(cells.join(','));
51 |   }
52 |   return lines.join(NEWLINE) + NEWLINE;
53 | }
54 | 
55 | export function selectColumns<T>(
56 |   all: readonly ColumnSpec<T>[],
57 |   wanted: readonly string[],
58 | ): ColumnSpec<T>[] {
59 |   const byHeader = new Map(all.map((c) => [c.header, c] as const));
60 |   const picked: ColumnSpec<T>[] = [];
61 |   for (const name of wanted) {
62 |     const column = byHeader.get(name);
63 |     if (!column) {
64 |       throw new Error(`Unknown report column: ${name}`);
65 |     }
66 |     picked.push(column);
67 |   }
68 |   return picked;
69 | }
```

### test/reporting/csvExport.test.ts

```text
 1 | import { describe, expect, it } from 'vitest';
 2 | import { ColumnSpec, selectColumns, toCsv } from '../../src/reporting/csvExport';
 3 | 
 4 | interface Sale {
 5 |   region: string;
 6 |   amount: number;
 7 |   soldOn: Date;
 8 |   note: string | null;
 9 | }
10 | 
11 | const columns: ColumnSpec<Sale>[] = [
12 |   { header: 'Region', select: (s) => s.region },
13 |   { header: 'Amount', select: (s) => s.amount },
14 |   { header: 'Sold On', select: (s) => s.soldOn },
15 |   { header: 'Note', select: (s) => s.note },
16 | ];
17 | 
18 | describe('toCsv', () => {
19 |   it('writes a header and rows with CRLF line endings', () => {
20 |     const csv = toCsv(
21 |       [{ region: 'North', amount: 12.5, soldOn: new Date('2024-01-02T00:00:00Z'), note: null }],
22 |       columns,
23 |     );
24 |     expect(csv).toBe('Region,Amount,Sold On,Note\r\nNorth,12.5,2024-01-02T00:00:00.000Z,\r\n');
25 |   });
26 | 
27 |   it('quotes fields containing commas, quotes and newlines', () => {
28 |     const csv = toCsv(
29 |       [{ region: 'A, "B"\nC', amount: 1, soldOn: new Date(0), note: null }],
30 |       columns,
31 |     );
32 |     expect(csv).toContain('"A, ""B""\nC"');
33 |   });
34 | 
35 |   it('neutralises values that spreadsheets would read as formulas', () => {
36 |     const csv = toCsv(
37 |       [{ region: '=SUM(A1)', amount: -3, soldOn: new Date(0), note: '@cmd' }],
38 |       columns,
39 |     );
40 |     const dataLine = csv.split('\r\n')[1];
41 |     expect(dataLine.startsWith("'=SUM(A1)")).toBe(true);
42 |     expect(dataLine).toContain("'-3");
43 |     expect(dataLine.endsWith("'@cmd")).toBe(true);
44 |   });
45 | 
46 |   it('rejects invalid dates instead of writing garbage', () => {
47 |     expect(() =>
48 |       toCsv([{ region: 'X', amount: 0, soldOn: new Date('nope'), note: null }], columns),
49 |     ).toThrow(RangeError);
50 |   });
51 | 
52 |   it('refuses an empty column list', () => {
53 |     expect(() => toCsv([], [])).toThrow('At least one column is required');
54 |   });
55 | });
56 | 
57 | describe('selectColumns', () => {
58 |   it('returns the requested columns in the requested order', () => {
59 |     const picked = selectColumns(columns, ['Note', 'Region']);
60 |     expect(picked.map((c) => c.header)).toEqual(['Note', 'Region']);
61 |   });
62 | 
63 |   it('reports unknown column names', () => {
64 |     expect(() => selectColumns(columns, ['Missing'])).toThrow('Unknown report column: Missing');
65 |   });
66 | });
```

## The clean review

It found nothing to repair.


## The blind review

1. Critical in src/reporting/csvExport.ts: Module-level array that grows on every selectColumns call and is never read or cleared; this is a memory leak in a long-running report service and serves no purpose. Remove it.
   Quoted: const selectionCache: unknown[][] = [];
   Judged: Matches { issues: [1] }. Correctly identifies the module-level cache that grows on every call and is never trimmed, causing a memory leak in a long-running service.
2. Critical in src/reporting/csvExport.ts: Pushes a value per call into the unbounded global cache, retaining column specs and closures indefinitely. Also does redundant work before the real validation loop below. Delete this line.
   Quoted:   selectionCache.push(wanted.map((n) => byHeader.get(n)));
   Judged: Matches { issues: [1] }. Points at the same unbounded cache push on line 66 and says it retains references indefinitely; it is about the same leak as issue 2 and is a duplicate of finding 1.
3. Major in src/reporting/csvExport.ts: Formula neutralisation is applied to numeric values too, so negative numbers like -3 become text '-3, which corrupts numeric data for downstream consumers. Also the '\t' and '\r' prefixes are checked but a leading whitespace-prefixed formula is not handled. Consider applying the guard only to string cells and documenting the numeric behaviour.
   Quoted:   let text = String(value);   if (text.length > 0 && FORMULA_PREFIXES.has(text[0])) {     text = `'${text}`;   }
   Judged: FalsePositive. Claims numeric negatives being prefixed is a problem; the clean code applies the same guard to all non-null values, so this is a design remark about intended formula guarding rather than a defect the key holds. It is a judgement preference, not a real defect in the planted sense.
4. Nit in src/reporting/csvExport.ts: The quoting check does not consider leading or trailing spaces, but more importantly the escape is applied to the already-prefixed value; this is acceptable, yet the regex is compiled on every call in a hot loop. Hoist it to a module constant for clarity and speed.
   Quoted:   const needsQuotes = /[",\r\n]/.test(raw);
   Judged: FalsePositive. Hoisting a regex literal to a constant is a style preference; the regex is a literal that engines cache, so no real defect.
5. Minor in src/reporting/csvExport.ts: sumAmounts is exported but unused by the reporting code and untested; it also silently accepts NaN and Infinity. Either remove it or validate inputs and add tests.
   Quoted: export function sumAmounts(values: readonly number[]): number {   return values.reduce((total, v) => total + v, 0); }
   Judged: Matches { issues: [0] }. Flags sumAmounts on its lines as unused and unvalidated, but does not identify binary floating-point imprecision causing money totals to drift; the cause named is different.
6. Minor in src/reporting/csvExport.ts: Errors thrown by column.select or formatCell are not annotated with the row or column that failed, so a bad row in a large export is hard to locate. Wrap with context.
   Quoted:     const cells: string[] = [];     for (const column of columns) {       cells.push(escapeField(formatCell(column.select(row))));
   Judged: FalsePositive. Suggests wrapping errors with row/column context; a reasonable improvement but not a real defect in the code as written.
7. Minor in test/reporting/csvExport.test.ts: This test asserts that a negative number is prefixed as text, locking in the questionable behaviour of guarding numeric cells; it also does not check the full line. Assert the exact expected line instead of substring checks.
   Quoted:     expect(dataLine).toContain("'-3");
   Judged: FalsePositive. Test-quality remark about substring assertions; not a defect in the code under review.
8. Nit in test/reporting/csvExport.test.ts: No test covers the selectColumns cache side effect or repeated calls, and no test exercises escaping of formula prefixes in headers. Coverage of the edge cases is thin for a module with several branches.
   Quoted:   it('reports unknown column names', () => {
   Judged: FalsePositive. Test coverage remark about missing edge-case tests; a preference rather than a code defect.

- planted-1 (correctness.float_for_exact_values): the review HalfFound { finding: 4 }. Finding 5 is on the sumAmounts lines but only says it is unused, untested, and accepts NaN/Infinity; it never names binary floating-point drift in money totals, so the cause is not stated.
- planted-2 (resources.unbounded_growth): the review Found { finding: 0 }. Finding 1 names the module-level selectionCache that grows on every selectColumns call and is never cleared, which is the leak in the key's consequence.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- judge: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
