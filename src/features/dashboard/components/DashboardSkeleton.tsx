import { Skeleton } from '@/components/Skeleton';

const SectionSkeleton = ({ rows }: { rows: number }) => (
  <section className="mb-12">
    <Skeleton width="6rem" height="1.25rem" className="mb-3" />
    <div className="border-t border-border-subtle">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex items-start gap-3 border-b border-border-subtle px-2 py-3">
          <Skeleton width="1rem" height="1rem" className="mt-0.5" />
          <div className="flex-1 space-y-2">
            <Skeleton width="12rem" height="1rem" />
            <Skeleton width="16rem" height="0.75rem" />
          </div>
          <Skeleton width="3rem" height="0.75rem" />
        </div>
      ))}
    </div>
  </section>
);

export const DashboardSkeleton = () => (
  <main className="relative h-full overflow-y-auto bg-bg">
    <div className="mx-auto w-full max-w-[760px] px-6 pt-10 pb-16">
      <header className="mb-10">
        <Skeleton width="7rem" height="2rem" />
        <Skeleton width="11rem" height="1rem" className="mt-2" />
      </header>

      <section className="mb-12">
        <div className="grid grid-cols-2 gap-x-12 gap-y-8 sm:grid-cols-4">
          {Array.from({ length: 4 }, (_, i) => (
            <div key={i}>
              <Skeleton width="4rem" height="0.75rem" />
              <Skeleton width="5rem" height="1.5rem" className="mt-2" />
            </div>
          ))}
        </div>
        <Skeleton width="18rem" height="1rem" className="mt-6" />
      </section>

      <SectionSkeleton rows={3} />
      <SectionSkeleton rows={1} />
      <SectionSkeleton rows={3} />
      <SectionSkeleton rows={5} />
    </div>
  </main>
);
