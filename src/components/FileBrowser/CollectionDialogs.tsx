import { useMemo, useState } from 'react';

import { FolderPlus, Search } from 'lucide-react';

import { metaLine } from './docMeta';
import { FileIcon } from './FileIcon';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';
import { toast } from '../../stores/toastStore';
import { type CustomCollection, type DocumentMetadata } from '../../types/fileBrowser';
import { Button } from '../ui/button';
import {
  Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
} from '../ui/dialog';
import { Input } from '../ui/input';

/** Mounted for a captured selection, so navigating the library cannot change the target files. */
export function AddToCollectionDialog({ documentIds, onClose }: {
  documentIds: string[];
  onClose: () => void;
}) {
  const collections = useFileBrowserStore(state => state.customCollections);
  const addDocuments = useFileBrowserStore(state => state.addDocumentsToCustomCollection);
  const createCollection = useFileBrowserStore(state => state.createCustomCollection);
  const [query, setQuery] = useState('');
  const [name, setName] = useState('');
  const [error, setError] = useState('');
  const manualCollections = collections.filter(collection => collection.kind === 'manual');
  const matches = manualCollections
    .filter(collection => collection.name.toLowerCase().includes(query.trim().toLowerCase()))
    .sort((a, b) => a.name.localeCompare(b.name));

  const add = (collectionId: string, collectionName: string) => {
    addDocuments(collectionId, documentIds);
    toast.success(`Added to ${collectionName}`);
    onClose();
  };

  const create = () => {
    const id = createCollection(name);
    if (!id) {
      setError('Enter a unique collection name.');
      return;
    }
    add(id, name.trim());
  };

  return (
    <Dialog open onOpenChange={open => { if (!open) onClose(); }}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add to collection</DialogTitle>
          <DialogDescription>
            {documentIds.length} {documentIds.length === 1 ? 'document selected' : 'documents selected'}.
            {' '}Documents can belong to more than one collection.
          </DialogDescription>
        </DialogHeader>
        {manualCollections.length > 0 ? (
          <>
            <Input aria-label="Find a collection" placeholder="Find a collection" value={query} onChange={event => setQuery(event.target.value)} />
            <div className="max-h-60 overflow-y-auto" aria-label="Collections">
              {matches.map(collection => {
                const included = documentIds.filter(id => collection.documentIds.includes(id)).length;
                const allIncluded = included === documentIds.length;
                return (
                  <button
                    key={collection.id}
                    type="button"
                    disabled={allIncluded}
                    onClick={() => add(collection.id, collection.name)}
                    className="flex w-full items-center gap-3 rounded-sm px-3 py-3 text-left text-sm text-text-primary hover:bg-surface disabled:opacity-50"
                  >
                    <FolderPlus className="h-4 w-4 shrink-0 text-text-muted" />
                    <span className="min-w-0 flex-1 truncate">{collection.name}</span>
                    {included > 0 ? (
                      <span className="shrink-0 text-xs text-text-muted">{allIncluded ? 'Already added' : `${included} already added`}</span>
                    ) : null}
                  </button>
                );
              })}
              {matches.length === 0 ? <p className="p-3 text-sm text-text-muted">No collections match.</p> : null}
            </div>
          </>
        ) : <p className="text-sm text-text-secondary">Create your first collection for these documents.</p>}
        <form onSubmit={event => { event.preventDefault(); create(); }} className="space-y-3 border-t border-border-subtle pt-4">
          <label htmlFor="new-collection-name" className="text-sm text-text-secondary">New collection</label>
          <Input id="new-collection-name" placeholder="Collection name" maxLength={120} value={name} onChange={event => { setName(event.target.value); setError(''); }} />
          {error ? <p role="alert" className="text-sm text-danger">{error}</p> : null}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={!name.trim()}>Create and add</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function CollectionDocumentsDialog({ collection, documents, onClose }: {
  collection: CustomCollection;
  documents: DocumentMetadata[];
  onClose: () => void;
}) {
  const addDocuments = useFileBrowserStore(state => state.addDocumentsToCustomCollection);
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const available = useMemo(() => {
    const existing = new Set(collection.documentIds);
    return documents.filter(doc => !existing.has(doc.id));
  }, [collection.documentIds, documents]);
  const matches = available.filter(doc => doc.fileName.toLowerCase().includes(query.trim().toLowerCase()));
  const allSelected = matches.length > 0 && matches.every(doc => selected.has(doc.id));

  const submit = () => {
    const ids = available.filter(doc => selected.has(doc.id)).map(doc => doc.id);
    if (ids.length === 0) return;
    addDocuments(collection.id, ids);
    toast.success(`Added ${ids.length} ${ids.length === 1 ? 'document' : 'documents'} to ${collection.name}`);
    onClose();
  };

  return (
    <Dialog open onOpenChange={open => { if (!open) onClose(); }}>
      <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Add documents to {collection.name}</DialogTitle>
          <DialogDescription>Choose documents from your Library.</DialogDescription>
        </DialogHeader>
        <Input aria-label="Find documents" leftIcon={<Search className="h-4 w-4" />} placeholder="Find documents" value={query} onChange={event => setQuery(event.target.value)} />
        <div className="flex items-center justify-between text-xs text-text-muted">
          <span>{selected.size} selected · {matches.length} available</span>
          <button type="button" disabled={matches.length === 0} className="text-text-secondary hover:text-text-primary disabled:opacity-50" onClick={() => {
            setSelected(previous => {
              const next = new Set(previous);
              matches.forEach(doc => { if (allSelected) next.delete(doc.id); else next.add(doc.id); });
              return next;
            });
          }}>{allSelected ? 'Deselect shown' : 'Select shown'}</button>
        </div>
        <div className="min-h-0 overflow-y-auto border-y border-border-subtle">
          {matches.map(doc => (
            <label key={doc.id} className="flex cursor-pointer items-center gap-3 border-b border-border-subtle px-2 py-3 last:border-0 hover:bg-surface">
              <input type="checkbox" className="h-4 w-4 shrink-0 accent-[hsl(var(--accent))]" checked={selected.has(doc.id)} onChange={() => setSelected(previous => {
                const next = new Set(previous);
                if (next.has(doc.id)) next.delete(doc.id); else next.add(doc.id);
                return next;
              })} aria-label={`Select ${doc.fileName}`} />
              <FileIcon file={doc} size={16} className="shrink-0" />
              <span className="min-w-0">
                <span className="block truncate text-sm text-text-primary">{doc.fileName}</span>
                <span className="block truncate text-xs text-text-muted">{metaLine(doc)}</span>
              </span>
            </label>
          ))}
          {matches.length === 0 ? (
            <p className="px-3 py-8 text-center text-sm text-text-muted">
              {documents.length === 0 ? 'Your Library is empty. Import documents to get started.' : available.length === 0 ? 'All Library documents are already in this collection.' : 'No documents match your search.'}
            </p>
          ) : null}
        </div>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
          <Button type="button" disabled={selected.size === 0} onClick={submit}>Add {selected.size || ''} {selected.size === 1 ? 'document' : 'documents'}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function RenameCollectionDialog({ collection, onClose }: { collection: CustomCollection; onClose: () => void }) {
  const update = useFileBrowserStore(state => state.updateCustomCollection);
  const [name, setName] = useState(collection.name);
  const [error, setError] = useState('');
  return (
    <Dialog open onOpenChange={open => { if (!open) onClose(); }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Rename collection</DialogTitle>
          <DialogDescription>Choose a name for {collection.name}.</DialogDescription>
        </DialogHeader>
        <form className="space-y-4" onSubmit={event => {
          event.preventDefault();
          if (!update(collection.id, { name })) { setError('Enter a unique collection name.'); return; }
          onClose();
        }}>
          <Input aria-label="Collection name" value={name} maxLength={120} onChange={event => { setName(event.target.value); setError(''); }} />
          {error ? <p role="alert" className="text-sm text-danger">{error}</p> : null}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={!name.trim()}>Rename</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
