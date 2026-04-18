import { FileText } from 'lucide-react';

import { QuickActions } from './QuickActions';

interface DashboardEmptyProps {
  onNavigate?: (view: 'search' | 'files' | 'settings') => void;
}

export function DashboardEmpty({ onNavigate }: DashboardEmptyProps) {
  return (
    <div className="text-center py-16 px-4">
      <div className="max-w-md mx-auto">
        <div className="w-16 h-16 bg-[hsl(var(--accent))] bg-opacity-10 rounded-full flex items-center justify-center mx-auto mb-4">
          <FileText className="w-8 h-8 text-[hsl(var(--accent))]" />
        </div>
        <h2 className="text-2xl font-semibold text-[hsl(var(--text-primary))] mb-2">
          Welcome to Vault
        </h2>
        <p className="text-[hsl(var(--text-secondary))] mb-6">
          Get started by indexing your first folder. Vault will analyze your documents
          and make them searchable.
        </p>
        <QuickActions onNavigate={onNavigate} variant="large" />
      </div>
    </div>
  );
}
