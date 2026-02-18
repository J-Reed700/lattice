import { Search, FileText, Settings, Database } from 'lucide-react';

import { EmptyState, type EmptyStateProps } from './EmptyState';

export const SearchEmptyState = (props: Partial<EmptyStateProps>) => (
  <EmptyState
    icon={<Search size={48} />}
    title="No results found"
    description="Try adjusting your search query or filters to find what you're looking for."
    {...props}
  />
);

export const DocumentsEmptyState = (props: Partial<EmptyStateProps>) => (
  <EmptyState
    icon={<FileText size={48} />}
    title="No documents yet"
    description="Start by uploading your first document to build your knowledge base."
    {...props}
  />
);

export const SettingsEmptyState = (props: Partial<EmptyStateProps>) => (
  <EmptyState
    icon={<Settings size={48} />}
    title="Configure your settings"
    description="Customize your experience by adjusting the settings below."
    {...props}
  />
);

export const DataEmptyState = (props: Partial<EmptyStateProps>) => (
  <EmptyState
    icon={<Database size={48} />}
    title="No data available"
    description="There's no data to display at the moment. Check back later."
    {...props}
  />
);
