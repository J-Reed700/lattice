# Test Fixtures

This directory contains test fixtures for integration tests.

## Structure

```
fixtures/
├── documents/          # Sample markdown documents
│   ├── test-note.md
│   ├── sample-with-links.md
│   └── frontmatter-example.md
└── expected/          # Expected test results
    ├── parsed-links.json
    └── extracted-titles.json
```

## Usage

Integration tests use these fixtures to verify:
- Wikilink parsing accuracy
- Title extraction from various formats
- Link resolution strategies
- Feature parity with Python implementation

## Adding Fixtures

When adding new test cases:
1. Add sample documents to `documents/`
2. Add expected results to `expected/`
3. Update integration tests to use the fixtures
4. Document the test case purpose
