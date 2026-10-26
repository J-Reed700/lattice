# React architecture

`src/AppProviders.tsx` is the composition root. It provides the API client,
React Query cache, and conversation controller. Keep API and query client
instances stable for the lifetime of a mounted app. An embedded app or test
with a different API should also receive its own query client, so cached data
cannot cross service boundaries.

Feature hooks read the client with `useApi()`. Query functions, mutations,
event callbacks, and refreshes use that same client. Non-React helpers take
an explicit final `api` parameter; their desktop default preserves standalone
callers. Pass the contextual client when a hook or component calls them.
Query option factories follow this rule too, so prefetches and invalidations
use the same backend as the visible query. The API context also defaults to
the desktop adapter for standalone consumers.

```tsx
export function useExampleQuery(id: string) {
  const api = useApi();
  return useQuery({
    queryKey: ['example', id],
    queryFn: () => loadExample(id, api),
  });
}
```

Keep backend data in React Query and transient UI state in local state or
Zustand. Views compose feature hooks and shared UI primitives. Extract a
component when it has a clear responsibility or a real second consumer;
keep ordinary display data and event handlers as props. Context supplies
dependencies and state shared across a subtree. It should not replace every
local prop.

Define component types at module scope. For dynamic error fallbacks, use
`ErrorBoundary`'s `renderFallback` prop and return a stable component type
with data passed as props. Creating a component function inside a render can
remount the fallback and discard focus or local state.

Effects own their subscriptions and timers, including cleanup when native
listener registration resolves after unmount. Async effects must ignore
results from disposed requests. State updater functions stay pure; show
toasts and perform other side effects outside them. Return the existing state
when reconciliation finds no changes, so repeating an effect cannot create a
render loop. Memoize context objects
and callbacks when their identity affects subscribers or effect lifetimes.
Avoid adding memoization where identity has no consumer.

For isolated tests, use `ApiProvider` with `createTestApi` from
`src/tests/apiClient.ts` and a fresh `QueryClientProvider`. Prefer replacing
dependencies through the provider over mocking a whole hook. Test observable
behavior such as refetches after writes, competing async results, timer
cleanup, and focus preservation. The lint configuration enforces Hooks
rules, exhaustive dependencies, stable context values, module-scope
components, and contextual API access.
