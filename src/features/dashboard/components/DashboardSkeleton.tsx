import { Skeleton } from '@/components/Skeleton';

const ListCardSkeleton = ({ rows }: { rows: number }) => (
  <section>
    <Skeleton width="7rem" height="1rem" className="mb-2.5" />
    <div className="rounded-xl border border-border-subtle bg-surface p-2 shadow-sm">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex items-start gap-3 px-2 py-2.5">
          <Skeleton width="1.75rem" height="1.75rem" className="rounded-lg" />
          <div className="flex-1 space-y-1.5">
            <Skeleton width="11rem" height="0.9rem" />
            <Skeleton width="15rem" height="0.7rem" />
          </div>
          <Skeleton width="2.5rem" height="0.7rem" />
        </div>
      ))}
    </div>
  </section>
);

export const DashboardSkeleton = () => (
  <main className="relative h-full overflow-y-auto bg-bg">
    <div className="mx-auto w-full max-w-[1080px] px-8 pb-16 pt-9 lg:px-12">
      <header className="mb-6">
        <Skeleton width="9rem" height="0.8rem" />
        <Skeleton width="13rem" height="2rem" className="mt-2" />
      </header>

      <Skeleton width="100%" height="2.75rem" className="mb-3.5 rounded-xl" />

      <div className="mb-8 grid grid-cols-2 gap-2.5 sm:grid-cols-3 lg:grid-cols-5">
        {Array.from({ length: 5 }, (_, i) => (
          <Skeleton key={i} width="100%" height="3.25rem" className="rounded-xl" />
        ))}
      </div>

      <div className="grid gap-x-8 gap-y-8 lg:grid-cols-[minmax(0,1.42fr)_minmax(0,1fr)]">
        <div className="space-y-7">
          <section>
            <Skeleton width="8.5rem" height="1rem" className="mb-2.5" />
            <div className="rounded-xl border border-border-subtle bg-surface p-5 shadow-sm">
              <div className="flex items-start justify-between gap-4">
                <div className="flex-1">
                  <Skeleton width="8rem" height="1.15rem" className="rounded-md" />
                  <Skeleton width="16rem" height="1.35rem" className="mt-3" />
                  <Skeleton width="20rem" height="0.8rem" className="mt-2" />
                </div>
                <Skeleton width="2.875rem" height="2.875rem" className="rounded-full" />
              </div>
              <Skeleton width="100%" height="1px" className="mt-4" />
              <div className="mt-3.5 flex items-center justify-between">
                <Skeleton width="6rem" height="0.8rem" />
                <Skeleton width="7.5rem" height="2rem" className="rounded-lg" />
              </div>
            </div>
          </section>
          <ListCardSkeleton rows={1} />
          <ListCardSkeleton rows={3} />
        </div>

        <div className="space-y-7">
          <ListCardSkeleton rows={3} />
          <section>
            <Skeleton width="5rem" height="1rem" className="mb-2.5" />
            <div className="rounded-xl border border-border-subtle bg-surface p-5 shadow-sm">
              <div className="grid grid-cols-2 gap-x-4 gap-y-4">
                {Array.from({ length: 4 }, (_, i) => (
                  <div key={i}>
                    <Skeleton width="4.5rem" height="1.4rem" />
                    <Skeleton width="5rem" height="0.75rem" className="mt-2" />
                  </div>
                ))}
              </div>
              <Skeleton width="100%" height="0.375rem" className="mt-5 rounded-full" />
            </div>
          </section>
          <ListCardSkeleton rows={4} />
        </div>
      </div>
    </div>
  </main>
);
