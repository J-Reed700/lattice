import { Skeleton } from '@/components/ui/skeleton';

export const DashboardSkeleton = () => (
  <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-[hsl(var(--bg))]">
    <div className="mx-auto w-full max-w-[760px] px-6 pt-10 pb-16">
      <header className="mb-12">
        <Skeleton className="h-8 w-40" />
        <Skeleton className="mt-2 h-4 w-48" />
      </header>

      <section className="mb-12">
        <div className="grid grid-cols-2 gap-x-12 gap-y-8 sm:grid-cols-4">
          {Array.from({ length: 4 }, (_, i) => (
            <div key={i}>
              <Skeleton className="h-3 w-16" />
              <Skeleton className="mt-2 h-6 w-20" />
            </div>
          ))}
        </div>
      </section>

      <section className="mb-12">
        <Skeleton className="mb-3 h-5 w-28" />
        <div className="border-t border-[hsl(var(--border-subtle))]">
          {Array.from({ length: 3 }, (_, i) => (
            <div
              key={i}
              className="flex items-center gap-4 border-b border-[hsl(var(--border-subtle))] px-2 py-4"
            >
              <Skeleton className="h-[18px] w-[18px] rounded" />
              <div className="flex-1 space-y-2">
                <Skeleton className="h-4 w-32" />
                <Skeleton className="h-3 w-40" />
              </div>
            </div>
          ))}
        </div>
      </section>

      <section>
        <Skeleton className="mb-3 h-5 w-32" />
        <div className="border-t border-[hsl(var(--border-subtle))]">
          {Array.from({ length: 5 }, (_, i) => (
            <div
              key={i}
              className="flex items-center gap-3 border-b border-[hsl(var(--border-subtle))] px-2 py-3"
            >
              <Skeleton className="h-4 w-4 rounded" />
              <div className="flex-1 space-y-2">
                <Skeleton className="h-4 w-48" />
                <Skeleton className="h-3 w-24" />
              </div>
            </div>
          ))}
        </div>
      </section>
    </div>
  </main>
);
