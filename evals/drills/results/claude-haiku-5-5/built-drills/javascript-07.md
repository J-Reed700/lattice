# Faceted search filter engine for product catalogs (javascript)

- Domain: search and filtering
- Built by: claude-haiku-5-5
- Runner: builtin-javascript
- Built in 112 seconds
- Patterns asked for: correctness.stale_state, errors.retry_not_idempotent, resources.leaked_handle

## Counts

- Planted: 3
- Proven by a test: 2
- Proven by the judge: 0
- Dropped: 1
- Found in review: 2

## The code

### src/facets.mjs

```text
 1 | // Faceted filtering over an in-memory catalog.
 2 | // Facet values arrive as strings or arrays; comparisons are case-insensitive
 3 | // so that user-typed query strings match catalog data regardless of casing.
 4 | 
 5 | function normalize(value) {
 6 |   return String(value).trim().toLowerCase();
 7 | }
 8 | 
 9 | function matchesFacet(item, field, wanted) {
10 |   const actual = item[field];
11 |   if (actual === undefined || actual === null) {
12 |     return false;
13 |   }
14 |   const values = Array.isArray(actual) ? actual : [actual];
15 |   return values.some((v) => wanted.includes(normalize(v)));
16 | }
17 | 
18 | // Facets are ANDed across fields and ORed within a field, which matches
19 | // how a shopper narrows a list by ticking boxes in one group.
20 | let lastFacets;
21 | let lastActive = [];
22 | 
23 | export function applyFacets(items, facets) {
24 |   if (!Array.isArray(items)) {
25 |     throw new TypeError('items must be an array');
26 |   }
27 |   if (facets !== lastFacets) {
28 |     lastFacets = facets;
29 |     lastActive = Object.entries(facets ?? {})
30 |       .map(([field, values]) => {
31 |         const list = Array.isArray(values) ? values : [values];
32 |         return [field, list.filter((v) => v !== undefined && v !== null).map(normalize)];
33 |       })
34 |       .filter(([, list]) => list.length > 0);
35 |   }
36 |   const active = lastActive;
37 | 
38 |   return items.filter((item) =>
39 |     active.every(([field, wanted]) => matchesFacet(item, field, wanted)),
40 |   );
41 | }
42 | 
43 | export function countByFacet(items, field) {
44 |   const counts = new Map();
45 |   for (const item of items) {
46 |     const raw = item[field];
47 |     if (raw === undefined || raw === null) {
48 |       continue;
49 |     }
50 |     const values = Array.isArray(raw) ? raw : [raw];
51 |     for (const v of values) {
52 |       const key = normalize(v);
53 |       counts.set(key, (counts.get(key) ?? 0) + 1);
54 |     }
55 |   }
56 |   return Object.fromEntries([...counts.entries()].sort((a, b) => a[0].localeCompare(b[0])));
57 | }
```

### src/search.mjs

```text
 1 | import { applyFacets } from './facets.mjs';
 2 | 
 3 | // Ranks by how many query terms appear in the item's name, then applies facets.
 4 | // The fetch function is injected so the engine can run against any catalog store.
 5 | export async function searchCatalog({ query, facets, fetchPage, pageSize = 20 }) {
 6 |   if (typeof fetchPage !== 'function') {
 7 |     throw new TypeError('fetchPage must be a function');
 8 |   }
 9 |   if (!Number.isInteger(pageSize) || pageSize < 1) {
10 |     throw new RangeError('pageSize must be a positive integer');
11 |   }
12 |   const terms = String(query ?? '')
13 |     .toLowerCase()
14 |     .split(/\s+/)
15 |     .filter((t) => t.length > 0);
16 | 
17 |   const collected = [];
18 |   let cursor = null;
19 |   do {
20 |     let page;
21 |     for (let attempt = 0; ; attempt += 1) {
22 |       try {
23 |         page = await fetchPage({ cursor, limit: pageSize });
24 |         break;
25 |       } catch (err) {
26 |         if (attempt >= 2) throw err;
27 |       }
28 |     }
29 |     if (!page || !Array.isArray(page.items)) {
30 |       throw new Error('fetchPage returned a malformed page');
31 |     }
32 |     collected.push(...page.items);
33 |     cursor = page.nextCursor ?? null;
34 |   } while (cursor !== null);
35 | 
36 |   const matched = applyFacets(collected, facets);
37 |   const scored = matched
38 |     .map((item) => {
39 |       const name = String(item.name ?? '').toLowerCase();
40 |       const score = terms.reduce((n, t) => (name.includes(t) ? n + 1 : n), 0);
41 |       return { item, score };
42 |     })
43 |     .filter(({ score }) => terms.length === 0 || score > 0);
44 | 
45 |   scored.sort((a, b) => b.score - a.score || String(a.item.name).localeCompare(String(b.item.name)));
46 |   return scored.map(({ item }) => item);
47 | }
```

### tests/catalog.test.mjs

```text
 1 | import { applyFacets, countByFacet } from '../src/facets.mjs';
 2 | import { searchCatalog } from '../src/search.mjs';
 3 | 
 4 | function assertEqual(actual, expected, label) {
 5 |   const a = JSON.stringify(actual);
 6 |   const e = JSON.stringify(expected);
 7 |   if (a !== e) {
 8 |     throw new Error(`${label}: expected ${e}, got ${a}`);
 9 |   }
10 | }
11 | 
12 | const items = [
13 |   { name: 'Red Shirt', color: 'Red', tags: ['cotton', 'Summer'] },
14 |   { name: 'Blue Shirt', color: 'blue', tags: ['wool'] },
15 |   { name: 'Red Hat', color: 'RED', tags: ['summer'] },
16 |   { name: 'Green Sock', color: 'green' },
17 | ];
18 | 
19 | assertEqual(applyFacets(items, {}).length, 4, 'empty facets keep all');
20 | assertEqual(applyFacets(items, { color: ['red'] }).map((i) => i.name), ['Red Shirt', 'Red Hat'], 'color filter');
21 | assertEqual(applyFacets(items, { color: 'RED', tags: 'summer' }).map((i) => i.name), ['Red Shirt', 'Red Hat'], 'AND across fields');
22 | assertEqual(applyFacets(items, { color: ['blue', 'green'] }).length, 2, 'OR within field');
23 | assertEqual(applyFacets(items, { color: [] }).length, 4, 'empty value list ignored');
24 | 
25 | let threw = false;
26 | try {
27 |   applyFacets(null, {});
28 | } catch (e) {
29 |   threw = e instanceof TypeError;
30 | }
31 | if (!threw) throw new Error('non-array items should throw TypeError');
32 | 
33 | assertEqual(countByFacet(items, 'tags'), { cotton: 1, summer: 2, wool: 1 }, 'tag counts');
34 | assertEqual(countByFacet(items, 'color'), { blue: 1, green: 1, red: 2 }, 'color counts');
35 | 
36 | const pages = [
37 |   { items: [items[0], items[1]], nextCursor: 'p2' },
38 |   { items: [items[2], items[3]], nextCursor: null },
39 | ];
40 | let calls = 0;
41 | const fetchPage = async ({ cursor }) => {
42 |   calls += 1;
43 |   return cursor === null ? pages[0] : pages[1];
44 | };
45 | 
46 | const results = await searchCatalog({ query: 'red', facets: {}, fetchPage });
47 | assertEqual(results.map((i) => i.name), ['Red Hat', 'Red Shirt'], 'ranked by terms then name');
48 | assertEqual(calls, 2, 'followed cursor across pages');
49 | 
50 | const blank = await searchCatalog({ query: '  ', facets: {}, fetchPage });
51 | assertEqual(blank.length, 4, 'blank query returns all');
52 | 
53 | let badPage = false;
54 | try {
55 |   await searchCatalog({ query: 'x', fetchPage: async () => ({}) });
56 | } catch (e) {
57 |   badPage = true;
58 | }
59 | if (!badPage) throw new Error('malformed page should throw');
60 | 
61 | let badSize = false;
62 | try {
63 |   await searchCatalog({ fetchPage, pageSize: 0 });
64 | } catch (e) {
65 |   badSize = e instanceof RangeError;
66 | }
67 | if (!badSize) throw new Error('pageSize 0 should throw RangeError');
68 | 
69 | console.log('all checks passed');
```

## The key

### 1. Stale state (correctness.stale_state), Minor, planted, src/facets.mjs lines 20 to 36

- Consequence: A caller that reuses one facets object and edits it between searches gets results filtered by the old selections, so shoppers see the wrong items until the object is replaced.
- In an interview: The function caches the parsed facets by object identity, so mutating the same object in place is never noticed. Parse the facets on every call, or cache on a value key rather than the reference, so edits are reflected.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { applyFacets } from './src/facets.mjs';
const items = [{ name: 'a', color: 'red' }, { name: 'b', color: 'blue' }];
const f = { color: ['red'] };
applyFacets(items, f);
f.color = ['blue'];
const second = applyFacets(items, f).map((i) => i.name);
if (JSON.stringify(second) !== JSON.stringify(['b'])) throw new Error('stale facets: ' + JSON.stringify(second));
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:7:105)"

The fix, in src/facets.mjs:

```diff
-let lastFacets;
-let lastActive = [];
-
 export function applyFacets(items, facets) {
   if (!Array.isArray(items)) {
     throw new TypeError('items must be an array');
   }
-  if (facets !== lastFacets) {
-    lastFacets = facets;
-    lastActive = Object.entries(facets ?? {})
-      .map(([field, values]) => {
-        const list = Array.isArray(values) ? values : [values];
-        return [field, list.filter((v) => v !== undefined && v !== null).map(normalize)];
-      })
-      .filter(([, list]) => list.length > 0);
-  }
-  const active = lastActive;
+  const active = Object.entries(facets ?? {})
+    .map(([field, values]) => {
+      const list = Array.isArray(values) ? values : [values];
+      return [field, list.filter((v) => v !== undefined && v !== null).map(normalize)];
+    })
+    .filter(([, list]) => list.length > 0);
```

### 2. Retry of a non-idempotent operation (errors.retry_not_idempotent), Major, planted, src/search.mjs lines 20 to 28

- Consequence: When a store advances a server-side cursor and the response is lost to a timeout, the retry resumes after the lost page, so catalog items silently disappear from search results.
- In an interview: The page fetch is retried on any error without checking whether the server already consumed the cursor. Retry only when the store guarantees idempotency, for example by passing the cursor as a key, or surface the error instead.

Proven by a test that fails on the drill and passes on the clean code:

```text
import { searchCatalog } from './src/search.mjs';
const pages = [{ items: [{ name: 'a' }], nextCursor: null }, { items: [{ name: 'b' }], nextCursor: null }];
let pointer = 0;
const fetchPage = async () => {
  const i = pointer;
  pointer += 1;
  if (i === 0) throw new Error('timeout after commit');
  return pages[i];
};
let result = null;
try {
  result = await searchCatalog({ query: '', facets: {}, fetchPage });
} catch (e) {
  result = null;
}
if (result !== null && result.length !== 2) throw new Error('partial results returned silently: ' + result.length);
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "    at <anonymous> (checks.mjs:16:101)"

The fix, in src/search.mjs:

```diff
-    let page;
-    for (let attempt = 0; ; attempt += 1) {
-      try {
-        page = await fetchPage({ cursor, limit: pageSize });
-        break;
-      } catch (err) {
-        if (attempt >= 2) throw err;
-      }
-    }
+    const page = await fetchPage({ cursor, limit: pageSize });
```

### 3. Unbounded growth (resources.unbounded_growth), Major, found in review, src/search.mjs lines 17 to 19

- Consequence: If a store returns a cursor that repeats, the do-while loop never ends. Each pass pushes another page into collected, so memory grows until the process is killed or crashes. The loop should refuse a cursor it has already seen.
- In an interview: The loop keeps appending pages to an array with no bound, so a buggy store that repeats its cursor makes memory grow forever. Remembering each cursor in a Set and throwing on a repeat stops the loop as soon as the store starts cycling.

Proven by the judge: Flags full in-memory buffering, spread argument limits, and missing cycle guard. These are real risks but not the planted retry issue.

The fix, in src/search.mjs:

```diff
   const collected = [];
+  const seen = new Set();
   let cursor = null;
   do {
+    if (cursor !== null) {
+      if (seen.has(cursor)) throw new Error('fetchPage returned a repeated cursor');
+      seen.add(cursor);
+    }
```

### 4. Untested branch (testing.untested_branch), Minor, found in review, tests/catalog.test.mjs lines 57 to 57

- Consequence: The malformed-page check passes on any exception, including a TypeError from a bug in searchCatalog, so the test can go green for the wrong reason. The retry branch in searchCatalog (a failed fetch that succeeds on retry, and the give-up after three attempts) is never exercised, so a regression there would reach production unnoticed.
- In an interview: The catch-all accepts any exception as the expected failure, so the assertion should check the error type and message that searchCatalog actually throws. The retry logic also needs its own tests, one where fetchPage fails and then succeeds and one where it fails three times and the error propagates.

Proven by the judge: Points out the test's catch-all and missing retry-path tests. Real test weakness, but not a problem of the planted code issues.

The fix, in tests/catalog.test.mjs:

```diff
-  badPage = true;
+  badPage = e instanceof Error && /malformed/.test(e.message);
```

## Dropped

- resources.leaked_handle: The edit adds a plain in-memory object with a close() method and calls reader.close() after the assertion. The 'reader' is not a real file, socket, or connection; it is just a JS object with a boolean flag, so no handle is acquired. The claimed consequence (handles leaking on assertion failure) cannot happen: nothing is opened, and if the assertion throws, the object is simply garbage collected. The original code had no leaked handle either, so the edit does not introduce the pattern.
  Consequence claimed: If the assertion fails, the reader is never closed, so repeated failing runs in one process keep handles open and eventually exhaust them.

## The clean code

### src/facets.mjs

```text
 1 | // Faceted filtering over an in-memory catalog.
 2 | // Facet values arrive as strings or arrays; comparisons are case-insensitive
 3 | // so that user-typed query strings match catalog data regardless of casing.
 4 | 
 5 | function normalize(value) {
 6 |   return String(value).trim().toLowerCase();
 7 | }
 8 | 
 9 | function matchesFacet(item, field, wanted) {
10 |   const actual = item[field];
11 |   if (actual === undefined || actual === null) {
12 |     return false;
13 |   }
14 |   const values = Array.isArray(actual) ? actual : [actual];
15 |   return values.some((v) => wanted.includes(normalize(v)));
16 | }
17 | 
18 | // Facets are ANDed across fields and ORed within a field, which matches
19 | // how a shopper narrows a list by ticking boxes in one group.
20 | export function applyFacets(items, facets) {
21 |   if (!Array.isArray(items)) {
22 |     throw new TypeError('items must be an array');
23 |   }
24 |   const active = Object.entries(facets ?? {})
25 |     .map(([field, values]) => {
26 |       const list = Array.isArray(values) ? values : [values];
27 |       return [field, list.filter((v) => v !== undefined && v !== null).map(normalize)];
28 |     })
29 |     .filter(([, list]) => list.length > 0);
30 | 
31 |   return items.filter((item) =>
32 |     active.every(([field, wanted]) => matchesFacet(item, field, wanted)),
33 |   );
34 | }
35 | 
36 | export function countByFacet(items, field) {
37 |   const counts = new Map();
38 |   for (const item of items) {
39 |     const raw = item[field];
40 |     if (raw === undefined || raw === null) {
41 |       continue;
42 |     }
43 |     const values = Array.isArray(raw) ? raw : [raw];
44 |     for (const v of values) {
45 |       const key = normalize(v);
46 |       counts.set(key, (counts.get(key) ?? 0) + 1);
47 |     }
48 |   }
49 |   return Object.fromEntries([...counts.entries()].sort((a, b) => a[0].localeCompare(b[0])));
50 | }
```

### src/search.mjs

```text
 1 | import { applyFacets } from './facets.mjs';
 2 | 
 3 | // Ranks by how many query terms appear in the item's name, then applies facets.
 4 | // The fetch function is injected so the engine can run against any catalog store.
 5 | export async function searchCatalog({ query, facets, fetchPage, pageSize = 20 }) {
 6 |   if (typeof fetchPage !== 'function') {
 7 |     throw new TypeError('fetchPage must be a function');
 8 |   }
 9 |   if (!Number.isInteger(pageSize) || pageSize < 1) {
10 |     throw new RangeError('pageSize must be a positive integer');
11 |   }
12 |   const terms = String(query ?? '')
13 |     .toLowerCase()
14 |     .split(/\s+/)
15 |     .filter((t) => t.length > 0);
16 | 
17 |   const collected = [];
18 |   let cursor = null;
19 |   do {
20 |     const page = await fetchPage({ cursor, limit: pageSize });
21 |     if (!page || !Array.isArray(page.items)) {
22 |       throw new Error('fetchPage returned a malformed page');
23 |     }
24 |     collected.push(...page.items);
25 |     cursor = page.nextCursor ?? null;
26 |   } while (cursor !== null);
27 | 
28 |   const matched = applyFacets(collected, facets);
29 |   const scored = matched
30 |     .map((item) => {
31 |       const name = String(item.name ?? '').toLowerCase();
32 |       const score = terms.reduce((n, t) => (name.includes(t) ? n + 1 : n), 0);
33 |       return { item, score };
34 |     })
35 |     .filter(({ score }) => terms.length === 0 || score > 0);
36 | 
37 |   scored.sort((a, b) => b.score - a.score || String(a.item.name).localeCompare(String(b.item.name)));
38 |   return scored.map(({ item }) => item);
39 | }
```

### tests/catalog.test.mjs

```text
 1 | import { applyFacets, countByFacet } from '../src/facets.mjs';
 2 | import { searchCatalog } from '../src/search.mjs';
 3 | 
 4 | function assertEqual(actual, expected, label) {
 5 |   const a = JSON.stringify(actual);
 6 |   const e = JSON.stringify(expected);
 7 |   if (a !== e) {
 8 |     throw new Error(`${label}: expected ${e}, got ${a}`);
 9 |   }
10 | }
11 | 
12 | const items = [
13 |   { name: 'Red Shirt', color: 'Red', tags: ['cotton', 'Summer'] },
14 |   { name: 'Blue Shirt', color: 'blue', tags: ['wool'] },
15 |   { name: 'Red Hat', color: 'RED', tags: ['summer'] },
16 |   { name: 'Green Sock', color: 'green' },
17 | ];
18 | 
19 | assertEqual(applyFacets(items, {}).length, 4, 'empty facets keep all');
20 | assertEqual(applyFacets(items, { color: ['red'] }).map((i) => i.name), ['Red Shirt', 'Red Hat'], 'color filter');
21 | assertEqual(applyFacets(items, { color: 'RED', tags: 'summer' }).map((i) => i.name), ['Red Shirt', 'Red Hat'], 'AND across fields');
22 | assertEqual(applyFacets(items, { color: ['blue', 'green'] }).length, 2, 'OR within field');
23 | assertEqual(applyFacets(items, { color: [] }).length, 4, 'empty value list ignored');
24 | 
25 | let threw = false;
26 | try {
27 |   applyFacets(null, {});
28 | } catch (e) {
29 |   threw = e instanceof TypeError;
30 | }
31 | if (!threw) throw new Error('non-array items should throw TypeError');
32 | 
33 | assertEqual(countByFacet(items, 'tags'), { cotton: 1, summer: 2, wool: 1 }, 'tag counts');
34 | assertEqual(countByFacet(items, 'color'), { blue: 1, green: 1, red: 2 }, 'color counts');
35 | 
36 | const pages = [
37 |   { items: [items[0], items[1]], nextCursor: 'p2' },
38 |   { items: [items[2], items[3]], nextCursor: null },
39 | ];
40 | let calls = 0;
41 | const fetchPage = async ({ cursor }) => {
42 |   calls += 1;
43 |   return cursor === null ? pages[0] : pages[1];
44 | };
45 | 
46 | const results = await searchCatalog({ query: 'red', facets: {}, fetchPage });
47 | assertEqual(results.map((i) => i.name), ['Red Hat', 'Red Shirt'], 'ranked by terms then name');
48 | assertEqual(calls, 2, 'followed cursor across pages');
49 | 
50 | const blank = await searchCatalog({ query: '  ', facets: {}, fetchPage });
51 | assertEqual(blank.length, 4, 'blank query returns all');
52 | 
53 | let badPage = false;
54 | try {
55 |   await searchCatalog({ query: 'x', fetchPage: async () => ({}) });
56 | } catch (e) {
57 |   badPage = true;
58 | }
59 | if (!badPage) throw new Error('malformed page should throw');
60 | 
61 | let badSize = false;
62 | try {
63 |   await searchCatalog({ fetchPage, pageSize: 0 });
64 | } catch (e) {
65 |   badSize = e instanceof RangeError;
66 | }
67 | if (!badSize) throw new Error('pageSize 0 should throw RangeError');
68 | 
69 | console.log('all checks passed');
```

### The hidden suite

```text
import { applyFacets, countByFacet } from './src/facets.mjs';
import { searchCatalog } from './src/search.mjs';

function eq(actual, expected, label) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${label}: expected ${e}, got ${a}`);
}

const items = [
  { name: 'Red Shirt', color: 'Red', tags: ['cotton', 'Summer'] },
  { name: 'Blue Shirt', color: 'blue', tags: ['wool'] },
  { name: 'Red Hat', color: 'RED', tags: ['summer'] },
  { name: 'Green Sock', color: 'green' },
];

eq(applyFacets(items, {}).length, 4, 'empty facets');
eq(applyFacets(items, { color: ['red'] }).map((i) => i.name), ['Red Shirt', 'Red Hat'], 'color');
eq(applyFacets(items, { color: 'RED', tags: 'summer' }).map((i) => i.name), ['Red Shirt', 'Red Hat'], 'AND');
eq(applyFacets(items, { color: ['blue', 'green'] }).length, 2, 'OR');
eq(applyFacets(items, { color: [] }).length, 4, 'empty list ignored');

let threw = false;
try { applyFacets(null, {}); } catch (e) { threw = e instanceof TypeError; }
if (!threw) throw new Error('non-array should throw');

eq(countByFacet(items, 'tags'), { cotton: 1, summer: 2, wool: 1 }, 'tag counts');
eq(countByFacet(items, 'color'), { blue: 1, green: 1, red: 2 }, 'color counts');

const pages = [
  { items: [items[0], items[1]], nextCursor: 'p2' },
  { items: [items[2], items[3]], nextCursor: null },
];
let calls = 0;
const fetchPage = async ({ cursor }) => {
  calls += 1;
  return cursor === null ? pages[0] : pages[1];
};

const results = await searchCatalog({ query: 'red', facets: {}, fetchPage });
eq(results.map((i) => i.name), ['Red Hat', 'Red Shirt'], 'ranked by terms then name');
eq(calls, 2, 'cursor followed');

const blank = await searchCatalog({ query: '  ', facets: {}, fetchPage });
eq(blank.length, 4, 'blank query returns all');

let bad = false;
try { await searchCatalog({ query: 'x', fetchPage: async () => ({}) }); } catch (e) { bad = true; }
if (!bad) throw new Error('malformed page should throw');

let badSize = false;
try { await searchCatalog({ fetchPage, pageSize: 0 }); } catch (e) { badSize = e instanceof RangeError; }
if (!badSize) throw new Error('pageSize 0 should throw');

console.log('suite passed');
```

## The clean review

- Minor: A null or undefined facet value is normalized to the string "null"/"undefined" and then matched literally, so a filter such as {color: null} wrongly matches items whose color is the text "null". Null values should be dropped like empty lists.

## The blind review

1. Major in src/facets.mjs: Module-level memoization keyed on object identity is shared mutable state. It is also unsafe if a caller mutates the same facets object between calls (identity unchanged, so the stale cached filter is reused and results are wrong). Compute the normalized filter on each call or drop the cache; it is cheap relative to filtering.
   Quoted: let lastFacets; let lastActive = [];
   Judged: Matches { issues: [0] }. Correctly identifies that the identity-keyed module cache reuses a stale normalized filter when the same facets object is mutated, giving wrong results.
2. Minor in src/facets.mjs: normalize(v) calls String(value) and trims; for objects or symbols String() may throw or produce '[object Object]', and nulls inside arrays are coerced to 'null' and can match a wanted value 'null'. Nested or null array elements should be filtered explicitly rather than silently matched.
   Quoted: return values.some((v) => wanted.includes(normalize(v)));
   Judged: FalsePositive. Concerns String() coercion of objects and nulls inside arrays. Not the stale-cache problem and not a real defect of the key's pattern; a minor robustness remark, not a planted issue.
3. Minor in src/facets.mjs: Non-array facet values are wrapped, but the filter only drops undefined/null after wrapping, so a facet value of null becomes an empty list and is ignored silently, while a value of 0 or false is normalized to '0'/'false' strings, which may not match boolean catalog data. Behavior for non-string facet types is undocumented and inconsistent.
   Quoted: const list = Array.isArray(values) ? values : [values];
   Judged: FalsePositive. Discusses handling of non-string facet types and null values; it does not name the stale cache cause. It points at the cached-lines area only by wrapping logic, so it is not about issue 2.
4. Nit in src/facets.mjs: Keys are lowercased and sorted with localeCompare, which is locale-dependent; output order can differ across environments. Use a fixed comparator for deterministic results.
   Quoted: return Object.fromEntries([...counts.entries()].sort((a, b) => a[0].localeCompare(b[0])));
   Judged: FalsePositive. Locale-dependent sorting of count keys is a valid preference but not a real defect of the planted issues; it names no real problem tied to the key.
5. Major in src/search.mjs: Retries are immediate with no backoff or jitter, and retry any error including non-transient ones (e.g. auth or validation), amplifying load on a failing store. Add a delay and only retry transient errors.
   Quoted:     for (let attempt = 0; ; attempt += 1) {       try {         page = await fetchPage({ cursor, limit: pageSize });         break;       } catch (err) {         if (attempt >= 2) throw err;       }     }
   Judged: Matches { issues: [1] }. Points at the retry loop and flags retrying without backoff and retrying non-transient errors. It does not identify that retrying a non-idempotent cursor fetch can silently drop items, so it only points at issue 1 without naming its cause, making it half_found.
6. Major in src/search.mjs: The whole catalog is buffered in memory before filtering, and spreading large arrays into push can exceed argument limits. Also there is no guard against a cursor that repeats forever, so a buggy store causes an infinite loop. Add a cycle check or max-page bound and consider streaming/filtering per page.
   Quoted:     collected.push(...page.items);     cursor = page.nextCursor ?? null;   } while (cursor !== null);
   Judged: ValidExtra. Flags full in-memory buffering, spread argument limits, and missing cycle guard. These are real risks but not the planted retry issue.
7. Nit in src/search.mjs: Scoring recomputes name lowercasing per item and uses substring matching, so 'red' matches 'reddish' and terms like 'a' match many items. Acceptable as a design choice but worth documenting; lowercasing is done once per item which is fine.
   Quoted: const score = terms.reduce((n, t) => (name.includes(t) ? n + 1 : n), 0);
   Judged: FalsePositive. Substring matching and design choice remark; a preference rather than a defect.
8. Nit in src/search.mjs: Sort uses locale-dependent comparison and String(a.item.name) repeatedly inside the comparator, which is wasteful for large catalogs; precompute the name key once per item.
   Quoted: scored.sort((a, b) => b.score - a.score || String(a.item.name).localeCompare(String(b.item.name)));
   Judged: FalsePositive. Comparator performance and locale remark; a style preference, not a real defect.
9. Major in tests/catalog.test.mjs: The test sets badPage true on any exception, not specifically the expected Error, so an unrelated failure would pass. Also the retry path (fetchPage rejecting then succeeding, and the three-attempt limit) is never tested.
   Quoted:   await searchCatalog({ query: 'x', fetchPage: async () => ({}) }); } catch (e) {   badPage = true; }
   Judged: ValidExtra. Points out the test's catch-all and missing retry-path tests. Real test weakness, but not a problem of the planted code issues.
10. Minor in tests/catalog.test.mjs: The facet cache is never exercised for the same-object-mutated case, and no test covers repeated calls with different facet objects, so the memoization bug above goes undetected.
   Quoted: assertEqual(applyFacets(items, { color: 'RED', tags: 'summer' }).map((i) => i.name), ['Red Shirt', 'Red Hat'], 'AND across fields');
   Judged: Matches { issues: [0] }. Notes the missing test for the same-object-mutated facet cache case. It points at the stale-cache problem as untested, so it is about issue 2 by pointing at the exposing code, though it gives no cause beyond test coverage.

- planted-1 (correctness.stale_state): the review Found { finding: 0 }. Finding 1 names the cause: the cache is keyed on object identity, so if the caller mutates the same facets object between calls the stale cached filter is reused and results are wrong. This is the stale-state problem of the key.
- planted-2 (errors.retry_not_idempotent): the review HalfFound { finding: 4 }. Finding 5 is about the retry loop at lines 21-27 but complains about immediate retries, no backoff, and retrying non-transient errors. It never says that retrying a cursor-advancing fetch can skip a page after a lost response, so the cause is not named.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
- key entry: claude-haiku-5-5
