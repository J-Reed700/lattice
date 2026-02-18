# Recall Desktop Examples & Demos

This directory contains example code, demos, and test utilities that are not part of the production application.

## Directory Structure

```
examples/
├── README.md                     # This file
├── components/                   # Component usage examples
│   ├── COMPONENTS_EXAMPLE.tsx   # UI component showcase
│   └── QueryRewriteExamples.tsx # Query rewrite panel examples
├── demos/                        # Feature demos
│   ├── ToastDemo.tsx            # Toast notification demo
│   └── SearchView.test-demo.tsx # Search view demo
├── error-testing/                # Error handling demos
│   ├── ErrorSimulator.tsx       # Error boundary testing utility
│   └── IndexingPanelWithErrors.example/ # Indexing error states
├── ProgressExample.tsx           # Progress indicator examples
├── ShortcutsExample.tsx          # Keyboard shortcuts examples
├── TaggingExample.tsx            # Tag management examples
├── TelemetryUsageExamples.tsx    # Telemetry integration examples
├── ToastIntegrationExamples.tsx  # Toast integration patterns
└── mention-examples.md           # Mention feature examples

## Purpose

These files serve several purposes:

1. **Documentation**: Show how to use various components and features
2. **Testing**: Provide utilities for testing error states and edge cases
3. **Development**: Quick demos for developing new features
4. **Reference**: Examples of best practices and patterns

## Usage

### Running Examples

Most example files are standalone React components. You can:

1. Import them into a development page
2. Use them as reference when building new features
3. Copy patterns into your production code

### Error Testing

The `error-testing/` directory contains utilities for testing error boundaries and error states:

```typescript
import { ErrorSimulator } from './examples/error-testing/ErrorSimulator';

// Use in development to test error handling
<ErrorSimulator />
```

### Component Examples

The `components/` directory shows how to use complex components:

- **COMPONENTS_EXAMPLE.tsx**: Comprehensive UI component showcase
- **QueryRewriteExamples.tsx**: 649 lines of query rewrite panel examples

## Guidelines

1. **Do not import these files into production code**
2. Keep examples up-to-date with API changes
3. Document non-obvious patterns
4. Use examples as integration test references

## Migration Notes

**2025-11-16**: Moved all example and demo files from `src/` to `examples/` to keep production source clean.

- Moved from `src/examples/` → `examples/`
- Moved from `src/components/*/example*.tsx` → `examples/components/`
- Moved from `src/components/*/Demo*.tsx` → `examples/demos/`
- Moved error testing utilities → `examples/error-testing/`

This cleanup ensures:
- Smaller production bundle size
- Clearer separation of concerns
- Easier navigation of production code
- Preserved examples for reference

## Related Documentation

- See `/vault/desktop/src/components/` for production component code
- See `/vault/desktop/e2e/` for end-to-end tests
- See `/vault/desktop/src/tests/` for unit and integration tests
