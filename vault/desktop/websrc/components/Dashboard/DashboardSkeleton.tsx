import { Skeleton } from '@/components/ui/skeleton';

export const DashboardSkeleton = () => (
    <div className="h-full overflow-auto bg-bg-primary">
      <div className="container mx-auto p-6 space-y-8">
        <div className="space-y-2">
          <Skeleton className="h-12 w-64" />
          <Skeleton className="h-6 w-48" />
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 auto-rows-fr">
          <Skeleton className="md:col-span-2 lg:row-span-2 h-64 min-h-[16rem]" />
          <Skeleton className="h-32 min-h-[8rem]" />
          <Skeleton className="h-32 min-h-[8rem]" />
          <Skeleton className="h-32 min-h-[8rem]" />
          <Skeleton className="h-32 min-h-[8rem]" />
        </div>
      </div>
    </div>
  );
