import { AlertCircle } from 'lucide-react';

import { EmptyState } from '@/components/EmptyState';

export const DashboardError = ({ error }: { error: string }) => (
    <div className="h-full overflow-auto bg-[hsl(var(--bg))] flex items-center justify-center">
      <EmptyState
        icon={<AlertCircle size={48} />}
        title="Failed to load dashboard"
        description={error}
        action={{
          label: 'Retry',
          onClick: () => window.location.reload(),
        }}
      />
    </div>
  );
