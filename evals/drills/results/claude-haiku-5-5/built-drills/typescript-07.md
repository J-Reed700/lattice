# Faceted search filter builder for product listings (typescript)

- Domain: search and filtering
- Built by: claude-haiku-5-5
- Runner: none, every defect was judged
- Built in 53 seconds
- Patterns asked for: correctness.stale_state, errors.retry_not_idempotent, resources.leaked_handle

## Counts

- Planted: 3
- Proven by a test: 0
- Proven by the judge: 2
- Dropped: 1
- Found in review: 0

## The code

### src/search/filterQuery.ts

```text
  1 | export type FilterValue = string | number | boolean;
  2 | 
  3 | export type FilterOperator = 'eq' | 'in' | 'gte' | 'lte';
  4 | 
  5 | export interface FilterClause {
  6 |   field: string;
  7 |   op: FilterOperator;
  8 |   value: FilterValue | FilterValue[];
  9 | }
 10 | 
 11 | export interface SearchParams {
 12 |   q?: string;
 13 |   filters: Record<string, string | string[] | undefined>;
 14 |   page: number;
 15 | }
 16 | 
 17 | const MAX_IN_VALUES = 20;
 18 | const FIELD_PATTERN = /^[a-z][a-zA-Z0-9_]*$/;
 19 | 
 20 | export function parseRangeNumber(raw: string): number | null {
 21 |   if (raw.trim() === '') {
 22 |     return null;
 23 |   }
 24 |   const n = Number(raw);
 25 |   return Number.isFinite(n) ? n : null;
 26 | }
 27 | 
 28 | // Range filters are encoded as `price_min` / `price_max` so the URL stays flat.
 29 | function rangeClause(field: string, raw: string, op: 'gte' | 'lte'): FilterClause | null {
 30 |   const n = parseRangeNumber(raw);
 31 |   if (n === null) {
 32 |     return null;
 33 |   }
 34 |   return { field, op, value: n };
 35 | }
 36 | 
 37 | const cachedClauses: FilterClause[] = [];
 38 | 
 39 | export function buildFilterClauses(params: SearchParams): FilterClause[] {
 40 |   const clauses: FilterClause[] = cachedClauses;
 41 | 
 42 |   for (const [key, raw] of Object.entries(params.filters)) {
 43 |     if (raw === undefined || raw === '') {
 44 |       continue;
 45 |     }
 46 |     if (!FIELD_PATTERN.test(key.replace(/_(min|max)$/, ''))) {
 47 |       throw new Error(`Invalid filter field: ${key}`);
 48 |     }
 49 | 
 50 |     if (key.endsWith('_min') || key.endsWith('_max')) {
 51 |       const base = key.slice(0, key.lastIndexOf('_'));
 52 |       const op = key.endsWith('_min') ? 'gte' : 'lte';
 53 |       const clause = typeof raw === 'string' ? rangeClause(base, raw, op) : null;
 54 |       // An array here means a malformed range; it is ignored explicitly, not by accident.
 55 |       if (clause) {
 56 |         clauses.push(clause);
 57 |       }
 58 |       continue;
 59 |     }
 60 | 
 61 |     if (Array.isArray(raw)) {
 62 |       const values = raw.filter((v) => v.length > 0).slice(0, MAX_IN_VALUES);
 63 |       if (values.length === 1) {
 64 |         clauses.push({ field: key, op: 'eq', value: values[0] });
 65 |       } else if (values.length > 1) {
 66 |         clauses.push({ field: key, op: 'in', value: values });
 67 |       }
 68 |       continue;
 69 |     }
 70 | 
 71 |     clauses.push({ field: key, op: 'eq', value: raw });
 72 |   }
 73 | 
 74 |   return clauses;
 75 | }
 76 | 
 77 | export function clampPage(page: number, maxPage: number): number {
 78 |   if (!Number.isInteger(page) || page < 1) {
 79 |     return 1;
 80 |   }
 81 |   return Math.min(page, Math.max(1, maxPage));
 82 | }
 83 | 
 84 | export function toQueryString(params: SearchParams): string {
 85 |   const search = new URLSearchParams();
 86 |   if (params.q) {
 87 |     search.set('q', params.q.trim());
 88 |   }
 89 |   for (const [key, raw] of Object.entries(params.filters)) {
 90 |     if (raw === undefined) {
 91 |       continue;
 92 |     }
 93 |     for (const value of Array.isArray(raw) ? raw : [raw]) {
 94 |       if (value !== '') {
 95 |         search.append(key, value);
 96 |       }
 97 |     }
 98 |   }
 99 |   if (params.page > 1) {
100 |     search.set('page', String(params.page));
101 |   }
102 |   return search.toString();
103 | }
```

### test/search/filterQuery.test.ts

```text
 1 | import { describe, it, expect } from 'vitest';
 2 | import { buildFilterClauses, clampPage, toQueryString } from '../../src/search/filterQuery';
 3 | 
 4 | const keepAlive = setInterval(() => {}, 1000);
 5 | 
 6 | describe('buildFilterClauses', () => {
 7 |   it('maps range keys to gte and lte clauses', () => {
 8 |     const clauses = buildFilterClauses({
 9 |       filters: { price_min: '10', price_max: '99.5' },
10 |       page: 1,
11 |     });
12 |     expect(clauses).toEqual([
13 |       { field: 'price', op: 'gte', value: 10 },
14 |       { field: 'price', op: 'lte', value: 99.5 },
15 |     ]);
16 |   });
17 | 
18 |   it('turns a single selected value into eq and several into in', () => {
19 |     const clauses = buildFilterClauses({
20 |       filters: { brand: ['acme'], color: ['red', 'blue'] },
21 |       page: 1,
22 |     });
23 |     expect(clauses).toEqual([
24 |       { field: 'brand', op: 'eq', value: 'acme' },
25 |       { field: 'color', op: 'in', value: ['red', 'blue'] },
26 |     ]);
27 |   });
28 | 
29 |   it('skips empty and non-numeric range values', () => {
30 |     const clauses = buildFilterClauses({
31 |       filters: { price_min: '', price_max: 'abc' },
32 |       page: 1,
33 |     });
34 |     expect(clauses).toEqual([]);
35 |   });
36 | 
37 |   it('rejects field names that are not safe identifiers', () => {
38 |     expect(() =>
39 |       buildFilterClauses({ filters: { 'brand;drop': 'x' }, page: 1 }),
40 |     ).toThrow('Invalid filter field');
41 |   });
42 | });
43 | 
44 | describe('clampPage', () => {
45 |   it('falls back to page 1 for invalid input and caps at maxPage', () => {
46 |     expect(clampPage(0, 5)).toBe(1);
47 |     expect(clampPage(1.5, 5)).toBe(1);
48 |     expect(clampPage(9, 5)).toBe(5);
49 |     expect(clampPage(3, 0)).toBe(1);
50 |   });
51 | });
52 | 
53 | describe('toQueryString', () => {
54 |   it('encodes query, repeated filter values and non-first pages', () => {
55 |     const qs = toQueryString({
56 |       q: '  shoes ',
57 |       filters: { color: ['red', 'blue'], size: undefined, brand: '' },
58 |       page: 2,
59 |     });
60 |     expect(qs).toBe('q=shoes&color=red&color=blue&page=2');
61 |   });
62 | });
```

## The key

### 1. Stale state (correctness.stale_state), Major, planted, src/search/filterQuery.ts lines 37 to 40

- Consequence: Every call pushes onto the same module-level array and returns it, so a later request sees filter clauses from earlier requests, and the array grows without bound under load.
- In an interview: The clause array is now a shared module-level cache that is never cleared, so results from earlier calls leak into later ones. Build a fresh local array on each call so each request is computed from its own parameters.

Proven by the judge: The edit makes buildFilterClauses start from a module-level array cachedClauses instead of a fresh array, so every call pushes onto the same shared array and returns it. Later requests therefore see clauses from earlier calls, and the array grows without bound. This is a stale-state / shared-mutable-state problem: the returned result reflects prior data rather than only the current params. The claimed consequence (cross-request filter leakage and unbounded growth) matches what the edit causes. The original code creates a new local array per call, so it does not have this problem.


The fix, in src/search/filterQuery.ts:

```diff
-const cachedClauses: FilterClause[] = [];
-
 export function buildFilterClauses(params: SearchParams): FilterClause[] {
-  const clauses: FilterClause[] = cachedClauses;
+  const clauses: FilterClause[] = [];
```

### 2. Leaked handle or connection (resources.leaked_handle), Minor, planted, test/search/filterQuery.test.ts lines 4 to 6

- Consequence: The interval keeps the test worker's event loop alive and is never cleared, so the process fails to exit cleanly and the handle accumulates across runs in watch mode.
- In an interview: A timer is started at module load and never cleared, so it outlives the tests as an unreleased handle. Create the timer in a beforeAll and clear it in afterAll so it is released on every path, including failing runs.

Proven by the judge: The edit adds a module-level setInterval in a test file that is never cleared, which matches the claimed leaked-timer pattern in the test worker. However, the claimed consequence is about the test runner's event loop and watch-mode accumulation, which is a test-harness resource issue rather than the production resource leak described; the interval is a real unreleased timer that keeps the process alive, so the defect is introduced by the edit. The original test file has no such interval, so it lacks the problem.


The fix, in test/search/filterQuery.test.ts:

```diff
-const keepAlive = setInterval(() => {}, 1000);
-
 describe('buildFilterClauses', () => {
```

## Dropped

- errors.retry_not_idempotent: The edit adds a Vitest retry option { retry: 2 } to a pure test that only calls toQueryString and asserts on the returned string; there is no write or external call, so no side effect is retried and no non-idempotent operation exists. The claimed consequence (a write or external call being applied twice on retry) cannot happen here, and the original test had the same lack of side effects, so the pattern is absent before and after the edit.
  Consequence claimed: If this test body ever performs a write or an external call, a failure that happened after the side effect will rerun it and apply the effect twice, which can hide the original failure.

## The clean code

### src/search/filterQuery.ts

```text
  1 | export type FilterValue = string | number | boolean;
  2 | 
  3 | export type FilterOperator = 'eq' | 'in' | 'gte' | 'lte';
  4 | 
  5 | export interface FilterClause {
  6 |   field: string;
  7 |   op: FilterOperator;
  8 |   value: FilterValue | FilterValue[];
  9 | }
 10 | 
 11 | export interface SearchParams {
 12 |   q?: string;
 13 |   filters: Record<string, string | string[] | undefined>;
 14 |   page: number;
 15 | }
 16 | 
 17 | const MAX_IN_VALUES = 20;
 18 | const FIELD_PATTERN = /^[a-z][a-zA-Z0-9_]*$/;
 19 | 
 20 | export function parseRangeNumber(raw: string): number | null {
 21 |   if (raw.trim() === '') {
 22 |     return null;
 23 |   }
 24 |   const n = Number(raw);
 25 |   return Number.isFinite(n) ? n : null;
 26 | }
 27 | 
 28 | // Range filters are encoded as `price_min` / `price_max` so the URL stays flat.
 29 | function rangeClause(field: string, raw: string, op: 'gte' | 'lte'): FilterClause | null {
 30 |   const n = parseRangeNumber(raw);
 31 |   if (n === null) {
 32 |     return null;
 33 |   }
 34 |   return { field, op, value: n };
 35 | }
 36 | 
 37 | export function buildFilterClauses(params: SearchParams): FilterClause[] {
 38 |   const clauses: FilterClause[] = [];
 39 | 
 40 |   for (const [key, raw] of Object.entries(params.filters)) {
 41 |     if (raw === undefined || raw === '') {
 42 |       continue;
 43 |     }
 44 |     if (!FIELD_PATTERN.test(key.replace(/_(min|max)$/, ''))) {
 45 |       throw new Error(`Invalid filter field: ${key}`);
 46 |     }
 47 | 
 48 |     if (key.endsWith('_min') || key.endsWith('_max')) {
 49 |       const base = key.slice(0, key.lastIndexOf('_'));
 50 |       const op = key.endsWith('_min') ? 'gte' : 'lte';
 51 |       const clause = typeof raw === 'string' ? rangeClause(base, raw, op) : null;
 52 |       // An array here means a malformed range; it is ignored explicitly, not by accident.
 53 |       if (clause) {
 54 |         clauses.push(clause);
 55 |       }
 56 |       continue;
 57 |     }
 58 | 
 59 |     if (Array.isArray(raw)) {
 60 |       const values = raw.filter((v) => v.length > 0).slice(0, MAX_IN_VALUES);
 61 |       if (values.length === 1) {
 62 |         clauses.push({ field: key, op: 'eq', value: values[0] });
 63 |       } else if (values.length > 1) {
 64 |         clauses.push({ field: key, op: 'in', value: values });
 65 |       }
 66 |       continue;
 67 |     }
 68 | 
 69 |     clauses.push({ field: key, op: 'eq', value: raw });
 70 |   }
 71 | 
 72 |   return clauses;
 73 | }
 74 | 
 75 | export function clampPage(page: number, maxPage: number): number {
 76 |   if (!Number.isInteger(page) || page < 1) {
 77 |     return 1;
 78 |   }
 79 |   return Math.min(page, Math.max(1, maxPage));
 80 | }
 81 | 
 82 | export function toQueryString(params: SearchParams): string {
 83 |   const search = new URLSearchParams();
 84 |   if (params.q) {
 85 |     search.set('q', params.q.trim());
 86 |   }
 87 |   for (const [key, raw] of Object.entries(params.filters)) {
 88 |     if (raw === undefined) {
 89 |       continue;
 90 |     }
 91 |     for (const value of Array.isArray(raw) ? raw : [raw]) {
 92 |       if (value !== '') {
 93 |         search.append(key, value);
 94 |       }
 95 |     }
 96 |   }
 97 |   if (params.page > 1) {
 98 |     search.set('page', String(params.page));
 99 |   }
100 |   return search.toString();
101 | }
```

### test/search/filterQuery.test.ts

```text
 1 | import { describe, it, expect } from 'vitest';
 2 | import { buildFilterClauses, clampPage, toQueryString } from '../../src/search/filterQuery';
 3 | 
 4 | describe('buildFilterClauses', () => {
 5 |   it('maps range keys to gte and lte clauses', () => {
 6 |     const clauses = buildFilterClauses({
 7 |       filters: { price_min: '10', price_max: '99.5' },
 8 |       page: 1,
 9 |     });
10 |     expect(clauses).toEqual([
11 |       { field: 'price', op: 'gte', value: 10 },
12 |       { field: 'price', op: 'lte', value: 99.5 },
13 |     ]);
14 |   });
15 | 
16 |   it('turns a single selected value into eq and several into in', () => {
17 |     const clauses = buildFilterClauses({
18 |       filters: { brand: ['acme'], color: ['red', 'blue'] },
19 |       page: 1,
20 |     });
21 |     expect(clauses).toEqual([
22 |       { field: 'brand', op: 'eq', value: 'acme' },
23 |       { field: 'color', op: 'in', value: ['red', 'blue'] },
24 |     ]);
25 |   });
26 | 
27 |   it('skips empty and non-numeric range values', () => {
28 |     const clauses = buildFilterClauses({
29 |       filters: { price_min: '', price_max: 'abc' },
30 |       page: 1,
31 |     });
32 |     expect(clauses).toEqual([]);
33 |   });
34 | 
35 |   it('rejects field names that are not safe identifiers', () => {
36 |     expect(() =>
37 |       buildFilterClauses({ filters: { 'brand;drop': 'x' }, page: 1 }),
38 |     ).toThrow('Invalid filter field');
39 |   });
40 | });
41 | 
42 | describe('clampPage', () => {
43 |   it('falls back to page 1 for invalid input and caps at maxPage', () => {
44 |     expect(clampPage(0, 5)).toBe(1);
45 |     expect(clampPage(1.5, 5)).toBe(1);
46 |     expect(clampPage(9, 5)).toBe(5);
47 |     expect(clampPage(3, 0)).toBe(1);
48 |   });
49 | });
50 | 
51 | describe('toQueryString', () => {
52 |   it('encodes query, repeated filter values and non-first pages', () => {
53 |     const qs = toQueryString({
54 |       q: '  shoes ',
55 |       filters: { color: ['red', 'blue'], size: undefined, brand: '' },
56 |       page: 2,
57 |     });
58 |     expect(qs).toBe('q=shoes&color=red&color=blue&page=2');
59 |   });
60 | });
```

## The clean review

- Major: parseRangeNumber uses Number(raw), which turns whitespace-only input into 0 (Number('  ') === 0). A blank range value like price_min=' ' therefore becomes a real gte 0 clause instead of being skipped like an empty value.
- Minor: For range keys, an array value (e.g. price_min appearing twice in the query string) is silently dropped because rangeClause only runs when raw is a string. The user's filter disappears without any error.

## The blind review

1. Critical in src/search/filterQuery.ts: Module-level array is shared across calls and `buildFilterClauses` pushes into it without resetting, so clauses accumulate between requests. Every subsequent call returns all previously built clauses plus new ones, a data leak across users and an unbounded memory growth. Create a fresh array per call.
   Quoted: const cachedClauses: FilterClause[] = [];
   Judged: Matches { issues: [0] }. Correctly identifies the shared module-level cache accumulating clauses across calls, matching issue 1.
2. Critical in src/search/filterQuery.ts: Aliasing the cache rather than copying means callers mutating the result mutate shared state. Use a local `const clauses: FilterClause[] = [];`.
   Quoted:   const clauses: FilterClause[] = cachedClauses;
   Judged: Matches { issues: [0] }. Points at the aliasing of the cache on line 40 and the mutation risk it creates; this is the same shared-state problem as issue 1, so it is about issue 1 but gives a cause only partly, and finding 1 already credits the cause best.
3. Minor in src/search/filterQuery.ts: Validation is applied to the key with suffix stripped, but the range branch then slices at lastIndexOf('_') which is equivalent only by accident; the `_min`/`_max` stripping regex and slice logic duplicate logic and could diverge. Compute the base once and validate that.
   Quoted:     if (!FIELD_PATTERN.test(key.replace(/_(min|max)$/, ''))) {
   Judged: FalsePositive. Concerns the validation regex and slice duplication, which is not a real defect; the slice is equivalent and not the planted problem.
4. Minor in src/search/filterQuery.ts: Array filter/slice happens after filtering but values are not validated against the field, and an attacker can still send up to unlimited entries before the slice runs; slicing first or capping input length avoids wasted work. Also, the `_min`/`_max` branch silently drops array input, which is a silent failure for malformed input.
   Quoted:       const values = raw.filter((v) => v.length > 0).slice(0, MAX_IN_VALUES);
   Judged: FalsePositive. Complains about slicing order and silent dropping of array input; the dropping is explicitly documented as intended and the slice-order point is not a real defect, so it names no real issue.
5. Nit in src/search/filterQuery.ts: Trimmed query is written without any length limit; unbounded user input is forwarded into URLs. Consider a maximum length.
   Quoted:     search.set('q', params.q.trim());
   Judged: FalsePositive. Suggests a length limit on the query string; a preference rather than a defect of the planted code.
6. Major in test/search/filterQuery.test.ts: A leftover interval keeps the test process alive and is never cleared; it is a resource leak with no purpose. Remove it.
   Quoted: const keepAlive = setInterval(() => {}, 1000);
   Judged: Matches { issues: [1] }. Identifies the uncleared setInterval keeping the test process alive as a resource leak, matching issue 2.
7. Major in test/search/filterQuery.test.ts: Tests run against the shared module-level cache without resetting it, so results depend on test order. Add a test that calling buildFilterClauses twice returns independent results, and it would have caught the accumulation bug.
   Quoted:   it('turns a single selected value into eq and several into in', () => {
   Judged: Matches { issues: [0] }. Test-order dependence on the shared cache is a consequence of issue 1 and suggests a test that would catch it; it does not name the cache as the cause as clearly as finding 1, but it is about the same shared-state problem.
8. Minor in test/search/filterQuery.test.ts: Only one invalid-name case is tested; the array-valued range case and the empty-array-after-filter case (e.g. ['']) are not covered, so the documented malformed-range behaviour is unverified.
   Quoted:   it('rejects field names that are not safe identifiers', () => {
   Judged: FalsePositive. Suggests more test coverage for malformed ranges; a remark about tests, not a defect of the planted issues.

- planted-1 (correctness.stale_state): the review Found { finding: 0 }. Finding 1 names the module-level array being shared and pushed into without reset, so clauses accumulate across calls and grow unbounded; this is the stale-state cause of the issue.
- planted-2 (resources.leaked_handle): the review Found { finding: 5 }. Finding 6 says the leftover interval keeps the test process alive and is never cleared, which names the unreleased handle and its effect on process exit.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- judge: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
