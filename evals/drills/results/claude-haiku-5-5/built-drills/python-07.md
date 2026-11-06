# Faceted search filter engine for in-memory product catalog (python)

- Domain: search and filtering
- Built by: claude-haiku-5-5
- Runner: builtin-python
- Built in 113 seconds
- Patterns asked for: correctness.reference_equality, errors.retry_not_idempotent, resources.leaked_handle

## Counts

- Planted: 3
- Proven by a test: 1
- Proven by the judge: 1
- Dropped: 1
- Found in review: 1

## The code

### src/search.py

```text
 1 | from dataclasses import dataclass, field
 2 | from typing import Iterable
 3 | 
 4 | 
 5 | @dataclass(frozen=True)
 6 | class Product:
 7 |     sku: str
 8 |     name: str
 9 |     price_cents: int
10 |     tags: frozenset[str] = field(default_factory=frozenset)
11 | 
12 | 
13 | @dataclass(frozen=True)
14 | class Query:
15 |     text: str = ''
16 |     min_price_cents: int | None = None
17 |     max_price_cents: int | None = None
18 |     required_tags: frozenset[str] = frozenset()
19 |     limit: int = 20
20 |     offset: int = 0
21 | 
22 | 
23 | class QueryError(ValueError):
24 |     pass
25 | 
26 | 
27 | def validate(query: Query) -> None:
28 |     if query.limit < 1 or query.limit > 100:
29 |         raise QueryError('limit must be between 1 and 100')
30 |     if query.offset < 0:
31 |         raise QueryError('offset must not be negative')
32 |     if (
33 |         query.min_price_cents is not None
34 |         and query.max_price_cents is not None
35 |         and query.min_price_cents > query.max_price_cents
36 |     ):
37 |         raise QueryError('min price exceeds max price')
38 | 
39 | 
40 | def _matches(product: Product, query: Query, needle: str) -> bool:
41 |     if needle and needle not in product.name.casefold():
42 |         return False
43 |     if query.min_price_cents is not None and (product.price_cents < query.min_price_cents or product.price_cents is not query.min_price_cents):
44 |         return False
45 |     if query.max_price_cents is not None and product.price_cents > query.max_price_cents:
46 |         return False
47 |     return query.required_tags <= product.tags
48 | 
49 | 
50 | def search(catalog: Iterable[Product], query: Query) -> list[Product]:
51 |     validate(query)
52 |     needle = query.text.strip().casefold()
53 |     hits = [p for p in catalog if _matches(p, query, needle)]
54 |     # Sort on a stable key so that paging over repeated searches returns
55 |     # the same order every time, even when prices tie.
56 |     hits.sort(key=lambda p: (p.price_cents, p.sku))
57 |     return hits[query.offset : query.offset + query.limit]
58 | 
59 | 
60 | def facet_counts(catalog: Iterable[Product], query: Query) -> dict[str, int]:
61 |     validate(query)
62 |     needle = query.text.strip().casefold()
63 |     counts: dict[str, int] = {}
64 |     for product in catalog:
65 |         if not _matches(product, query, needle):
66 |             continue
67 |         for tag in product.tags:
68 |             counts[tag] = counts.get(tag, 0) + 1
69 |     return dict(sorted(counts.items(), key=lambda item: (-item[1], item[0])))
```

### tests/test_search.py

```text
 1 | from src.search import Product, Query, QueryError, search, facet_counts
 2 | 
 3 | CATALOG = [
 4 |     Product('A1', 'Oak Desk', 25000, frozenset({'furniture', 'wood'})),
 5 |     Product('B2', 'Pine Chair', 8000, frozenset({'furniture', 'wood'})),
 6 |     Product('C3', 'Steel Shelf', 12000, frozenset({'furniture', 'metal'})),
 7 |     Product('D4', 'Desk Lamp', 3000, frozenset({'lighting'})),
 8 |     Product('E5', 'Tie Rack', 3000, frozenset()),
 9 | ]
10 | 
11 | 
12 | def _load_catalog(path):
13 |     fh = open(path)
14 |     lines = fh.read().splitlines()
15 |     if not lines:
16 |         raise ValueError('empty catalog file')
17 |     fh.close()
18 |     return lines
19 | 
20 | 
21 | def test_text_match_is_case_insensitive():
22 |     hits = search(CATALOG, Query(text='  DESK '))
23 |     assert [p.sku for p in hits] == ['D4', 'A1']
24 | 
25 | 
26 | def test_price_range_and_tags():
27 |     q = Query(min_price_cents=5000, max_price_cents=20000, required_tags=frozenset({'furniture'}))
28 |     assert [p.sku for p in search(CATALOG, q)] == ['B2', 'C3']
29 | 
30 | 
31 | def test_ties_sorted_by_sku_and_paged():
32 |     q = Query(limit=2, offset=1)
33 |     assert [p.sku for p in search(CATALOG, q)] == ['E5', 'B2']
34 | 
35 | 
36 | def test_empty_query_returns_everything_up_to_limit():
37 |     assert len(search(CATALOG, Query())) == 5
38 |     assert search([], Query()) == []
39 | 
40 | 
41 | def test_invalid_queries_rejected():
42 |     for bad in (Query(limit=0), Query(limit=101), Query(offset=-1),
43 |                 Query(min_price_cents=10, max_price_cents=5)):
44 |         try:
45 |             search(CATALOG, bad)
46 |         except QueryError:
47 |             continue
48 |         raise AssertionError(f'expected QueryError for {bad}')
49 | 
50 | 
51 | def test_facet_counts_ordered_by_count_then_name():
52 |     counts = facet_counts(CATALOG, Query(text=''))
53 |     assert counts == {'furniture': 3, 'wood': 2, 'lighting': 1, 'metal': 1}
54 |     assert list(counts) == ['furniture', 'wood', 'lighting', 'metal']
55 | 
56 | 
57 | def test_facets_respect_filters():
58 |     counts = facet_counts(CATALOG, Query(max_price_cents=10000))
59 |     assert counts == {'lighting': 1, 'furniture': 1, 'wood': 1}
60 | 
61 | 
62 | if __name__ == '__main__':
63 |     for name, fn in list(globals().items()):
64 |         if name.startswith('test_') and callable(fn):
65 |             fn()
```

## The key

### 1. Identity compared where value was meant (correctness.reference_equality), Major, planted, src/search.py lines 43 to 43

- Consequence: A minimum price given as a computed int (for example parsed from a request) is compared by identity, so products whose price equals the minimum are wrongly dropped from results whenever the two ints are distinct objects, which is common outside small cached values.
- In an interview: The price check uses `is not` where a value comparison was meant, so equal integers that are separate objects are treated as different. Use only `<` for the range check, since identity has no meaning for prices, and reserve `is` for None checks.

Proven by a test that fails on the drill and passes on the clean code:

```text
from src.search import Product, Query, search
cat = [Product('P1', 'a', 1200), Product('P2', 'b', 900)]
assert [p.sku for p in search(cat, Query(min_price_cents=int('1200')))] == ['P1']
```

- The killing test on the clean code passed
- The killing test with the mutation failed with "Python exited with status 1."

The fix, in src/search.py:

```diff
-    if query.min_price_cents is not None and (product.price_cents < query.min_price_cents or product.price_cents is not query.min_price_cents):
+    if query.min_price_cents is not None and product.price_cents < query.min_price_cents:
```

### 2. Leaked handle or connection (resources.leaked_handle), Minor, planted, tests/test_search.py lines 12 to 21

- Consequence: When the fixture file is empty, the ValueError is raised before the handle is closed, so each failing load leaks a file descriptor and a long test run can exhaust the process's descriptors.
- In an interview: The file is closed only on the success path, so the early raise leaves the handle open. Use a with-statement or try/finally so the handle is released on every path.

Proven by the judge: The edit adds a helper _load_catalog in tests/test_search.py that opens a file, reads it, and raises ValueError on empty content before fh.close() is called. This is a real leaked-handle pattern on the error path: each failing call leaves the file descriptor open until garbage collection. However, the helper is never called anywhere in the test file, and the original code and tests contain no file handling at all, so the claimed consequence (repeated failing loads exhausting descriptors in a test run) cannot occur with the code as it exists. The edit does introduce the pattern textually, so the first question is yes in a literal sense, but the claimed consequence is not reachable; the pattern is present in newly added code regardless. The original lacks it entirely.


The fix, in tests/test_search.py:

```diff
-def _load_catalog(path):
-    fh = open(path)
-    lines = fh.read().splitlines()
-    if not lines:
-        raise ValueError('empty catalog file')
-    fh.close()
-    return lines
-
-
 def test_text_match_is_case_insensitive():
```

### 3. Identity compared where value was meant (correctness.reference_equality), Major, found in review, src/search.py lines 43 to 43

- Consequence: In _matches, the clause 'product.price_cents is not query.min_price_cents' is true for any equal-valued int that is a separate object. CPython caches only small ints (-5 to 256), so for larger prices a product priced exactly at min_price_cents is wrongly excluded whenever the minimum is above 256, and searches silently miss boundary matches. The 'or' clause is also redundant because the '<' check already handles real price comparisons.
- In an interview: Python's 'is' tests object identity, and equal integers above 256 are often distinct objects, so the min-price check wrongly rejects products priced exactly at the minimum. The fix is to drop the identity clause and keep only the value comparison, which is the intended boundary logic.

Proven by the judge: Says no test covers the min-price boundary, which is a test gap rather than a naming of the identity bug's cause; it does not name the identity comparison as the problem, so it only points at the issue's consequence area without naming the cause.

The fix, in src/search.py:

```diff
- and (product.price_cents < query.min_price_cents or product.price_cents is not query.min_price_cents):
+ and product.price_cents < query.min_price_cents:
```

## Dropped

- errors.retry_not_idempotent: The edit wraps the read-only list comprehension in a retry loop catching OSError. The search operation has no side effects on the product catalog; it only filters and returns a list. The claimed consequence requires the catalog iteration to cause side effects that are repeated on retry, but the pattern as defined concerns non-idempotent operations such as payments or inserts. The search itself is idempotent and the retry only re-iterates a possibly partially consumed iterable, which may be a problem for one-shot iterators but is not a repeated side-effecting operation by the code itself. The claimed consequence (catalog source sees repeated operations due to side effects on iteration) is speculative and not the defined pattern of retrying a side-effecting write without an idempotency key. The original code has no retry at all, so it lacks this issue.
  Consequence claimed: If the catalog is backed by a remote source with side effects on iteration, a timeout after partial consumption causes a retry that re-runs the work, so the catalog source sees repeated operations or duplicated results.

## The clean code

### src/search.py

```text
 1 | from dataclasses import dataclass, field
 2 | from typing import Iterable
 3 | 
 4 | 
 5 | @dataclass(frozen=True)
 6 | class Product:
 7 |     sku: str
 8 |     name: str
 9 |     price_cents: int
10 |     tags: frozenset[str] = field(default_factory=frozenset)
11 | 
12 | 
13 | @dataclass(frozen=True)
14 | class Query:
15 |     text: str = ''
16 |     min_price_cents: int | None = None
17 |     max_price_cents: int | None = None
18 |     required_tags: frozenset[str] = frozenset()
19 |     limit: int = 20
20 |     offset: int = 0
21 | 
22 | 
23 | class QueryError(ValueError):
24 |     pass
25 | 
26 | 
27 | def validate(query: Query) -> None:
28 |     if query.limit < 1 or query.limit > 100:
29 |         raise QueryError('limit must be between 1 and 100')
30 |     if query.offset < 0:
31 |         raise QueryError('offset must not be negative')
32 |     if (
33 |         query.min_price_cents is not None
34 |         and query.max_price_cents is not None
35 |         and query.min_price_cents > query.max_price_cents
36 |     ):
37 |         raise QueryError('min price exceeds max price')
38 | 
39 | 
40 | def _matches(product: Product, query: Query, needle: str) -> bool:
41 |     if needle and needle not in product.name.casefold():
42 |         return False
43 |     if query.min_price_cents is not None and product.price_cents < query.min_price_cents:
44 |         return False
45 |     if query.max_price_cents is not None and product.price_cents > query.max_price_cents:
46 |         return False
47 |     return query.required_tags <= product.tags
48 | 
49 | 
50 | def search(catalog: Iterable[Product], query: Query) -> list[Product]:
51 |     validate(query)
52 |     needle = query.text.strip().casefold()
53 |     hits = [p for p in catalog if _matches(p, query, needle)]
54 |     # Sort on a stable key so that paging over repeated searches returns
55 |     # the same order every time, even when prices tie.
56 |     hits.sort(key=lambda p: (p.price_cents, p.sku))
57 |     return hits[query.offset : query.offset + query.limit]
58 | 
59 | 
60 | def facet_counts(catalog: Iterable[Product], query: Query) -> dict[str, int]:
61 |     validate(query)
62 |     needle = query.text.strip().casefold()
63 |     counts: dict[str, int] = {}
64 |     for product in catalog:
65 |         if not _matches(product, query, needle):
66 |             continue
67 |         for tag in product.tags:
68 |             counts[tag] = counts.get(tag, 0) + 1
69 |     return dict(sorted(counts.items(), key=lambda item: (-item[1], item[0])))
```

### tests/test_search.py

```text
 1 | from src.search import Product, Query, QueryError, search, facet_counts
 2 | 
 3 | CATALOG = [
 4 |     Product('A1', 'Oak Desk', 25000, frozenset({'furniture', 'wood'})),
 5 |     Product('B2', 'Pine Chair', 8000, frozenset({'furniture', 'wood'})),
 6 |     Product('C3', 'Steel Shelf', 12000, frozenset({'furniture', 'metal'})),
 7 |     Product('D4', 'Desk Lamp', 3000, frozenset({'lighting'})),
 8 |     Product('E5', 'Tie Rack', 3000, frozenset()),
 9 | ]
10 | 
11 | 
12 | def test_text_match_is_case_insensitive():
13 |     hits = search(CATALOG, Query(text='  DESK '))
14 |     assert [p.sku for p in hits] == ['D4', 'A1']
15 | 
16 | 
17 | def test_price_range_and_tags():
18 |     q = Query(min_price_cents=5000, max_price_cents=20000, required_tags=frozenset({'furniture'}))
19 |     assert [p.sku for p in search(CATALOG, q)] == ['B2', 'C3']
20 | 
21 | 
22 | def test_ties_sorted_by_sku_and_paged():
23 |     q = Query(limit=2, offset=1)
24 |     assert [p.sku for p in search(CATALOG, q)] == ['E5', 'B2']
25 | 
26 | 
27 | def test_empty_query_returns_everything_up_to_limit():
28 |     assert len(search(CATALOG, Query())) == 5
29 |     assert search([], Query()) == []
30 | 
31 | 
32 | def test_invalid_queries_rejected():
33 |     for bad in (Query(limit=0), Query(limit=101), Query(offset=-1),
34 |                 Query(min_price_cents=10, max_price_cents=5)):
35 |         try:
36 |             search(CATALOG, bad)
37 |         except QueryError:
38 |             continue
39 |         raise AssertionError(f'expected QueryError for {bad}')
40 | 
41 | 
42 | def test_facet_counts_ordered_by_count_then_name():
43 |     counts = facet_counts(CATALOG, Query(text=''))
44 |     assert counts == {'furniture': 3, 'wood': 2, 'lighting': 1, 'metal': 1}
45 |     assert list(counts) == ['furniture', 'wood', 'lighting', 'metal']
46 | 
47 | 
48 | def test_facets_respect_filters():
49 |     counts = facet_counts(CATALOG, Query(max_price_cents=10000))
50 |     assert counts == {'lighting': 1, 'furniture': 1, 'wood': 1}
51 | 
52 | 
53 | if __name__ == '__main__':
54 |     for name, fn in list(globals().items()):
55 |         if name.startswith('test_') and callable(fn):
56 |             fn()
```

### The hidden suite

```text
from src.search import Product, Query, QueryError, search, facet_counts

CAT = [
    Product('X1', 'Blue Mug', 1200, frozenset({'kitchen'})),
    Product('X2', 'Red Mug', 1200, frozenset({'kitchen', 'red'})),
    Product('X3', 'Chair', 9000, frozenset({'furniture'})),
    Product('X4', 'Mugshot Frame', 500, frozenset()),
]

assert [p.sku for p in search(CAT, Query(text='MUG'))] == ['X4', 'X1', 'X2']
assert [p.sku for p in search(CAT, Query(text='  '))] == ['X4', 'X1', 'X2', 'X3']
assert [p.sku for p in search(CAT, Query(required_tags=frozenset({'kitchen', 'red'})))] == ['X2']
assert [p.sku for p in search(CAT, Query(min_price_cents=1200, max_price_cents=1200))] == ['X1', 'X2']
assert [p.sku for p in search(CAT, Query(limit=1, offset=3))] == ['X3']
assert search(CAT, Query(offset=10)) == []
assert search([], Query(text='x')) == []

for bad in (Query(limit=0), Query(limit=101), Query(offset=-1), Query(min_price_cents=5, max_price_cents=4)):
    try:
        search(CAT, bad)
    except QueryError:
        pass
    else:
        raise AssertionError('expected QueryError for %r' % (bad,))

assert list(facet_counts(CAT, Query(text='mug'))) == ['kitchen', 'red']
assert facet_counts(CAT, Query(text='mug')) == {'kitchen': 2, 'red': 1}
assert facet_counts(CAT, Query()) == {'kitchen': 2, 'furniture': 1, 'red': 1}
assert facet_counts(CAT, Query(max_price_cents=1000)) == {}

print('all checks passed')
```

## The clean review

It found nothing to repair.


## The blind review

1. Critical in src/search.py: Uses identity (`is not`) to compare integers. This is a bug: ints outside the small-int cache are not guaranteed to be the same object, so products with a price equal to min_price_cents can be wrongly excluded. Use `<` (already covered by the preceding clause) or `!=`/`<` consistently, and drop the identity check.
   Quoted: product.price_cents is not query.min_price_cents
   Judged: Matches { issues: [0] }. Correctly identifies the identity comparison on line 43 as a bug causing equal-priced products to be excluded.
2. Major in tests/test_search.py: No test covers the min-price boundary or prices equal to min_price_cents with large values. That is why the identity-comparison bug in _matches goes unnoticed. Add a boundary test with an exact-equal price above 256 (e.g. 12345) for both min and max bounds.
   Quoted: def test_price_range_and_tags():
   Judged: ValidExtra. Says no test covers the min-price boundary, which is a test gap rather than a naming of the identity bug's cause; it does not name the identity comparison as the problem, so it only points at the issue's consequence area without naming the cause.
3. Major in tests/test_search.py: The file handle is opened without a context manager and is leaked if read() or splitlines() raises. Also, the ValueError for an empty file is raised before close(), so the handle is never closed on that path. Use `with open(path) as fh:`.
   Quoted: fh = open(path)
   Judged: Matches { issues: [1] }. Names the leaked file handle on the empty-file ValueError path, matching issue 2.
4. Minor in tests/test_search.py: _load_catalog is dead code: it is never called by any test and returns raw lines instead of Product objects, so it proves nothing. Remove it or test it properly.
   Quoted: def _load_catalog(path):
   Judged: FalsePositive. Says _load_catalog is dead code and returns raw lines; this is not about the leaked handle cause, though it sits on the same lines. It is a remark about unused code, not a real defect cause for issue 2.
5. Minor in tests/test_search.py: The invalid-query test does not check the error message and iterates through the cases in a try/continue loop, so a wrong exception type path is only partially checked. Use pytest.raises with match=, or separate test functions, so each invalid case is reported individually.
   Quoted:         raise AssertionError(f'expected QueryError for {bad}')
   Judged: FalsePositive. Preference about pytest.raises and error messages in the test loop; not a real defect matching any key issue.
6. Minor in src/search.py: The whole matching result set is sorted on every call just to return one page. For large catalogs this is O(n log n) work per page. Consider heapq.nsmallest over the key with offset+limit, or accept the cost explicitly and document it.
   Quoted: hits.sort(key=lambda p: (p.price_cents, p.sku))
   Judged: FalsePositive. Performance remark about sorting the whole result set per page; a design preference rather than a real defect in this code, and it is not one of the key issues.
7. Nit in src/search.py: This line is long and mixes two checks, one of them buggy. Split it into a simple `if product.price_cents < query.min_price_cents: return False` check, which is clearer and correct.
   Quoted:     if query.min_price_cents is not None and (product.price_cents < query.min_price_cents or product.price_cents is not query.min_price_cents):
   Judged: Matches { issues: [0] }. Suggests splitting the long line and notes the buggy mixed check; it identifies line 43 as mixing checks with a bug, which points at issue 1 but gives the cause as mere length and mixing rather than the identity comparison itself. It is a style remark on the same problem.

- planted-1 (correctness.reference_equality): the review Found { finding: 0 }. Finding 1 names the identity check `is not` on ints as the bug, explaining equal prices can be wrongly excluded because ints are not guaranteed to be the same object, which is the cause of the planted issue.
- planted-2 (resources.leaked_handle): the review Found { finding: 2 }. Finding 3 says the handle is leaked when read() or splitlines() raises and that the ValueError for an empty file is raised before close(), so the handle is never closed on that path. This names the leaked handle on the error path, which is the cause of issue 2.

## Models

Every call ran on: claude-haiku-5-5

- clean code: claude-haiku-5-5
- clean code (asked again): claude-haiku-5-5
- clean review: claude-haiku-5-5
- plant: claude-haiku-5-5
- judge: claude-haiku-5-5
- judge: claude-haiku-5-5
- blind review: claude-haiku-5-5
- key entry: claude-haiku-5-5
