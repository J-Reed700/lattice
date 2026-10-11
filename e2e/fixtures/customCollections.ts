import { randomUUID } from 'node:crypto';
import type { Page } from '@playwright/test';
import type { CreateCustomCollectionRequest, CustomCollectionDto } from '../../src/lib/bindings';
import { serveCommands, type MockArgs } from './tauri';

/** Backend-owned fixture state survives renderer reloads, just like SQLite. */
export async function installCustomCollectionsFixture(page: Page) {
  const collections = new Map<string, CustomCollectionDto>();
  const update = (args: MockArgs, change: (collection: CustomCollectionDto) => void) => {
    const collection = collections.get(String(args.collectionId ?? ''));
    if (!collection) throw new Error(`Unknown collection: ${String(args.collectionId)}`);
    change(collection);
    collection.updatedAt = new Date().toISOString();
    return null;
  };
  const documentIds = (args: MockArgs) => (args.documentIds as string[] | undefined) ?? [];
  await serveCommands(page, () => ({
    list_custom_collections: () => [...collections.values()],
    create_custom_collection: (args) => {
      const request = args.request as CreateCustomCollectionRequest | undefined;
      if (!request) throw new Error('Missing collection request');
      const now = new Date().toISOString();
      const collection: CustomCollectionDto = { ...request, id: randomUUID(), createdAt: now, updatedAt: now };
      collections.set(collection.id, collection);
      return collection.id;
    },
    rename_custom_collection: (args) => update(args, (collection) => {
      const name = String(args.name ?? '');
      if (!name.trim()) throw new Error('Missing collection name');
      collection.name = name;
    }),
    add_documents_to_custom_collection: (args) => update(args, (collection) => {
      collection.documentIds = [...new Set([...collection.documentIds, ...documentIds(args)])];
    }),
    remove_documents_from_custom_collection: (args) => update(args, (collection) => {
      collection.documentIds = collection.documentIds.filter(id => !documentIds(args).includes(id));
    }),
    delete_custom_collection: (args) => update(args, (collection) => {
      collections.delete(collection.id);
    }),
  }));
}
