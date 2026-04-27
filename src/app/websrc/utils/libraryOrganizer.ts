/**
 * Library Organizer Utilities
 *
 * Organizes documents by metadata (category, type, date) rather than filesystem paths.
 * Implements custom library view following "Personal Knowledge Library" philosophy.
 */

import type { DocumentMetadata } from '../types';

// Re-export for convenience
export type { DocumentMetadata };

export interface LibraryNode {
  id: string;
  name: string;
  type: 'category' | 'document';
  icon?: string;
  children?: LibraryNode[];
  documentId?: string;
  metadata?: DocumentMetadata;
}

/**
 * Build library tree organized by category and file type
 *
 * Categories:
 * - Documents (doc, docx, txt, md, rtf)
 * - PDFs (pdf)
 * - Web Pages (web_archive category)
 * - Images (jpg, jpeg, png, gif, svg, webp)
 * - Code (js, ts, py, rs, go, java, cpp, c, html, css, json, xml)
 * - Spreadsheets (xlsx, xls, csv)
 * - Presentations (pptx, ppt)
 * - Archives (zip, rar, tar, gz, 7z)
 * - Other (everything else)
 */
export function buildLibraryTree(documents: DocumentMetadata[]): LibraryNode[] {
  const tree: LibraryNode[] = [];

  // Define category groups with their file types
  const categoryGroups: Record<string, { icon: string; extensions: string[] | 'web_archive' }> = {
    Documents: {
      icon: '📄',
      extensions: ['doc', 'docx', 'txt', 'md', 'markdown', 'rtf', 'odt']
    },
    PDFs: {
      icon: '📕',
      extensions: ['pdf']
    },
    'Web Pages': {
      icon: '🌐',
      extensions: 'web_archive' // Special: use category field
    },
    Images: {
      icon: '🖼️',
      extensions: ['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'ico']
    },
    Code: {
      icon: '💻',
      extensions: ['js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', 'cpp', 'c', 'h', 'hpp', 'html', 'css', 'scss', 'json', 'xml', 'yaml', 'yml']
    },
    Spreadsheets: {
      icon: '📊',
      extensions: ['xlsx', 'xls', 'csv', 'ods']
    },
    Presentations: {
      icon: '📽️',
      extensions: ['pptx', 'ppt', 'odp', 'key']
    },
    Archives: {
      icon: '🗜️',
      extensions: ['zip', 'rar', 'tar', 'gz', '7z', 'bz2', 'xz']
    },
  };

  // Group documents by category
  const categorizedDocs: Record<string, DocumentMetadata[]> = {
    Documents: [],
    PDFs: [],
    'Web Pages': [],
    Images: [],
    Code: [],
    Spreadsheets: [],
    Presentations: [],
    Archives: [],
    Other: [],
  };

  documents.forEach(doc => {
    const fileType = doc.fileType.toLowerCase();
    let categorized = false;

    // Check each category
    for (const [categoryName, categoryDef] of Object.entries(categoryGroups)) {
      if (categoryDef.extensions === 'web_archive') {
        // Special case: web archive uses category field
        if (doc.category === 'web_archive') {
          categorizedDocs[categoryName].push(doc);
          categorized = true;
          break;
        }
      } else if (Array.isArray(categoryDef.extensions)) {
        // Standard case: match by file extension
        if (categoryDef.extensions.includes(fileType)) {
          categorizedDocs[categoryName].push(doc);
          categorized = true;
          break;
        }
      }
    }

    // If not categorized, add to "Other"
    if (!categorized) {
      categorizedDocs['Other'].push(doc);
    }
  });

  // Build tree structure
  for (const [categoryName, docs] of Object.entries(categorizedDocs)) {
    if (docs.length === 0) continue;

    const categoryDef = categoryGroups[categoryName];
    const icon = categoryDef?.icon || '📁';

    tree.push({
      id: `category-${categoryName.toLowerCase().replace(/\s+/g, '-')}`,
      name: `${icon} ${categoryName}`,
      type: 'category',
      icon,
      children: docs.map(doc => ({
        id: doc.id,
        name: doc.fileName,
        type: 'document' as const,
        documentId: doc.id,
        metadata: doc,
      })),
    });
  }

  return tree;
}

/**
 * Get category statistics for UI display
 */
export function getCategoryStats(documents: DocumentMetadata[]): Record<string, number> {
  const tree = buildLibraryTree(documents);
  const stats: Record<string, number> = {};

  tree.forEach(node => {
    if (node.type === 'category' && node.children) {
      const categoryName = node.name.replace(/^[^\s]+\s+/, ''); // Remove icon
      stats[categoryName] = node.children.length;
    }
  });

  return stats;
}

/**
 * Filter library tree by search query
 */
export function filterLibraryTree(
  tree: LibraryNode[],
  searchQuery: string
): LibraryNode[] {
  if (!searchQuery.trim()) return tree;

  const query = searchQuery.toLowerCase();

  return tree
    .map(node => {
      if (node.type === 'category' && node.children) {
        const filteredChildren = node.children.filter(child =>
          child.name.toLowerCase().includes(query) ||
          child.metadata?.fileName.toLowerCase().includes(query)
        );

        if (filteredChildren.length === 0) return null;

        return {
          ...node,
          children: filteredChildren,
        };
      }
      return node;
    })
    .filter((node): node is LibraryNode => node !== null);
}

/**
 * Get all document IDs from library tree
 */
export function getAllDocumentIds(tree: LibraryNode[]): string[] {
  const ids: string[] = [];

  tree.forEach(node => {
    if (node.type === 'document' && node.documentId) {
      ids.push(node.documentId);
    } else if (node.type === 'category' && node.children) {
      ids.push(...getAllDocumentIds(node.children));
    }
  });

  return ids;
}

/**
 * Find document node by ID in library tree
 */
export function findDocumentById(
  tree: LibraryNode[],
  documentId: string
): LibraryNode | null {
  for (const node of tree) {
    if (node.type === 'document' && node.documentId === documentId) {
      return node;
    }
    if (node.type === 'category' && node.children) {
      const found = findDocumentById(node.children, documentId);
      if (found) return found;
    }
  }
  return null;
}

/**
 * Get documents in range for multi-selection
 */
export function getDocumentsInRange(
  tree: LibraryNode[],
  startId: string,
  endId: string
): string[] {
  const allDocs = getAllDocumentIds(tree);
  const startIndex = allDocs.indexOf(startId);
  const endIndex = allDocs.indexOf(endId);

  if (startIndex === -1 || endIndex === -1) {
    return [];
  }

  const [min, max] = [Math.min(startIndex, endIndex), Math.max(startIndex, endIndex)];
  return allDocs.slice(min, max + 1);
}
