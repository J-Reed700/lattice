import { EmptyState } from '@/components/EmptyState';

export const DashboardError = ({ error }: { error: string }) => (
  <div className="flex h-full items-center justify-center overflow-auto bg-bg">
    <EmptyState
      title="Couldn't load Home."
      description={error}
      action={{ label: 'Try again', onClick: () => window.location.reload() }}
    />
  </div>
);
