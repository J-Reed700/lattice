# Library (FileBrowser)

The corpus surface. This is where a user sees what their vault has become, so it
is styled as content, not as a file manager: one page header, one toolbar row, a
rail of the things the corpus is organised by, and the documents themselves as
hairline rows.

## Anatomy

```
FileBrowser              shell: PageHeader + toolbar + rail + main + dialogs
├── LibraryToolbar       search · source tabs · sort · group-by-date · rail toggle
├── LibraryRail          Folders / Collections / Themes / Saved searches / Sources
├── TypeFacets           the clickable type-count line under the scope heading
├── SelectionBar         shown only while ≥1 document is selected
├── CollectionDialogs    add selected files, choose collection members, rename
├── ListView             flat hairline rows (virtualized)
├── TreeView             the same rows, arranged by folder (virtualized)
├── GridView             hairline tiles (virtualized)
├── CorpusRow            the one row component: checkbox · icon · name · meta
├── FileIcon             the per-type document icon
├── ContextMenu          right-click actions + space membership
├── RenameDialog         rename a document
├── SavedSearchNameDialog rename a saved search
└── EmptyStates          "Nothing indexed yet." / "No files match."
```

`docMeta.ts` holds the shared document predicates and the meta-line formatter
(`9,800 words · 1h ago · PDF`). `hooks/useCorpusIdentity.ts` produces the corpus
readout — the type counts `TypeFacets` renders as one muted line under the scope
heading. `contentSearchCache.ts` bounds the file text held for content search.

## Organizing documents

- Select documents to **Add to collection**, choosing an existing collection or
  creating one with that selection. Documents can belong to multiple collections.
- Use **Add documents** inside a collection to search and select Library documents.
  New empty collections open this picker immediately.
- Collection rows show document counts and an actions menu for rename and delete.
- **Remove from collection** changes membership only. Deleting a collection also
  leaves its documents in the Library. Snapshots keep their captured membership.
- Each document has a visible actions menu in list, tree, and grid views.

## State

Documents come from `useLibraryDocumentsQuery()` (React Query over
`list_all_documents`), sorted and filtered by pure functions exported from
`stores/fileBrowserStore.ts`. Indexed folders come from `useIndexedFoldersQuery()`.
Collections, including snapshots, are stored in SQLite and read through
`useCustomCollectionsQuery()`; `useCustomCollectionActions()` holds the writes.

The Zustand store holds UI-only state: view mode, sort, search, source filter,
grouping, selection, context-menu position, and the current **scope**:

```ts
type LibraryScope =
  | { kind: 'all' }
  | { kind: 'folder'; path: string }
  | { kind: 'collection'; id: string }
  | { kind: 'theme'; id: string };   // one cluster from the latest themes run
```

Saved searches and client-side source connections are persisted to
`localStorage` by the store.

## Conventions

- Tokens only (`bg-surface`, `text-text-muted`, `border-border-subtle`, …).
- No badges, no pills, no cards, no sentence explaining a control.
- Row: `ROW_HEIGHT` (56px), hover `row-hover`, selected `bg-accent-muted`; no
  per-row border.
- The checkbox column is always reserved and only becomes visible on hover or
  when a selection is active.

## Tests

```bash
npx vitest run src/features/files
```
