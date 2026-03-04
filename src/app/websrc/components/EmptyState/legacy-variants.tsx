import { EmptyState } from './EmptyState';

export function NoSearchResults({
  query,
  onClear,
}: {
  query?: string;
  onClear?: () => void;
}) {
  return (
    <EmptyState
      icon={
        <svg
          className="w-full h-full"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
          />
        </svg>
      }
      title="No results found"
      description={
        query
          ? `No documents match "${query}". Try different keywords or filters.`
          : 'Try adjusting your search query or filters.'
      }
      action={
        onClear
          ? {
              label: 'Clear filters',
              onClick: onClear,
            }
          : undefined
      }
    />
  );
}

export function NoFiles({ onAddFiles }: { onAddFiles?: () => void }) {
  return (
    <EmptyState
      icon={
        <svg
          className="w-full h-full"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
          />
        </svg>
      }
      title="No files yet"
      description="Add files to your vault to get started. You can import documents, create notes, or connect a folder."
      action={
        onAddFiles
          ? {
              label: 'Add files',
              onClick: onAddFiles,
            }
          : undefined
      }
    />
  );
}

export function NoTags() {
  return (
    <EmptyState
      icon={
        <svg
          className="w-full h-full"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M7 7h.01M7 3h5c.512 0 1.024.195 1.414.586l7 7a2 2 0 010 2.828l-7 7a2 2 0 01-2.828 0l-7-7A1.994 1.994 0 013 12V7a4 4 0 014-4z"
          />
        </svg>
      }
      title="No tags"
      description="Tags help organize and find related documents. Start by adding tags to your documents."
    />
  );
}

export function NoRecentDocuments() {
  return (
    <EmptyState
      icon={
        <svg
          className="w-full h-full"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
      }
      title="No recent documents"
      description="Documents you open will appear here for quick access."
    />
  );
}

export function FirstTimeDaily({ onCreate }: { onCreate?: () => void }) {
  return (
    <EmptyState
      icon={
        <svg
          className="w-full h-full"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
          />
        </svg>
      }
      title="Start your journal"
      description="Journals help you capture thoughts, tasks, and ideas in a structured notebook."
      action={
        onCreate
          ? {
              label: 'Create journal page',
              onClick: onCreate,
            }
          : undefined
      }
    />
  );
}

export function NoBacklinks() {
  return (
    <EmptyState
      icon={
        <svg
          className="w-full h-full"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1"
          />
        </svg>
      }
      title="No backlinks"
      description="Other documents that link to this one will appear here."
    />
  );
}
