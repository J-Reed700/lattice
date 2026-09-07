import { Skeleton } from '@/components/ui/skeleton';

const SectionSkeleton = ({ rows }: { rows: number }) => (
  <section className="mb-12">
    <Skeleton className="mb-3 h-5 w-24" />
    <div className="border-t border-border-subtle">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex items-start gap-3 border-b border-border-subtle px-2 py-3">
          <Skeleton className="mt-0.5 h-4 w-4 rounded" />
          <div className="flex-1 space-y-2">
            <Skeleton className="h-4 w-48" />
            <Skeleton className="h-3 w-64" />
          </div>
          <Skeleton className="h-3 w-12" />
        </div>
      ))}
    </div>
  </section>
);

export const DashboardSkeleton = () => (
  <main className="relative h-full overflow-y-auto bg-bg">
    <div className="mx-auto w-full max-w-[760px] px-6 pt-10 pb-16">
      <header className="mb-10">
        <Skeleton className="h-8 w-28" />
        <Skeleton className="mt-2 h-4 w-44" />
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
        <Skeleton className="mt-6 h-4 w-72" />
      </section>

      <SectionSkeleton rows={3} />
      <SectionSkeleton rows={1} />
      <SectionSkeleton rows={3} />
      <SectionSkeleton rows={5} />
    </div>
  </main>
);
