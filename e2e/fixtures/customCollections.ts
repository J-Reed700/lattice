import { randomUUID } from 'node:crypto';
import type { Page } from '@playwright/test';
import type { CreateCustomCollectionRequest, CustomCollectionDto } from '../../src/lib/bindings';

/** Backend-owned fixture state survives renderer reloads, just like SQLite. */
export async function installCustomCollectionsFixture(page: Page) {
  const collections = new Map<string, CustomCollectionDto>();
  await page.exposeFunction('__LATTICE_TEST_COLLECTIONS__', (command: string, args: {
    request?: CreateCustomCollectionRequest;
    collectionId?: string;
    documentIds?: string[];
    name?: string;
  } = {}) => {
    if (command === 'plugin:file|list_custom_collections') return [...collections.values()];
    if (command === 'plugin:file|create_custom_collection') {
      if (!args.request) throw new Error('Missing collection request');
      const now = new Date().toISOString();
      const collection = { ...args.request, id: randomUUID(), createdAt: now, updatedAt: now };
      collections.set(collection.id, collection);
      return collection.id;
    }
    const collection = collections.get(args.collectionId ?? '');
    if (!collection) throw new Error(`Unknown collection: ${args.collectionId}`);
    switch (command) {
      case 'plugin:file|rename_custom_collection':
        if (!args.name?.trim()) throw new Error('Missing collection name');
        collection.name = args.name;
        break;
      case 'plugin:file|add_documents_to_custom_collection':
        collection.documentIds = [...new Set([...collection.documentIds, ...(args.documentIds ?? [])])];
        break;
      case 'plugin:file|remove_documents_from_custom_collection':
        collection.documentIds = collection.documentIds.filter(id => !args.documentIds?.includes(id));
        break;
      case 'plugin:file|delete_custom_collection':
        collections.delete(collection.id);
        break;
      default:
        throw new Error(`Unsupported collection fixture command: ${command}`);
    }
    collection.updatedAt = new Date().toISOString();
    return null;
  });
}
