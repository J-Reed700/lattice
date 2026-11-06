import { ArrowLeft } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { PageHeader } from '@/components/ui/PageHeader';
import { cn } from '@/lib/utils';

import { DrillBuilding } from './DrillBuilding';
import { DrillChoose } from './DrillChoose';
import { DrillFailure } from './DrillFailure';
import { DrillGrading } from './DrillGrading';
import { languageLabel } from './drillOptions';
import { DrillReview } from './DrillReview';
import { DrillReveal } from './reveal/DrillReveal';
import { useCodeReviewDrill } from './useCodeReviewDrill';

/**
 * A code review drill, from the choice of one to the reveal of what was planted in it. It opens at
 * `/studio?drills=open`, and the attempt it is on is `&attempt=<id>`, so an attempt that was left is found again.
 */
export function CodeReviewDrillScreen({ onBack }: { onBack: () => void }) {
  const drill = useCodeReviewDrill();
  const { stage, failure, result, session } = drill;
  const back = (
    <Button type="button" variant="ghost" className="-ml-3 gap-2" onClick={onBack}>
      <ArrowLeft className="h-4 w-4" aria-hidden="true" />
      Studio
    </Button>
  );

  if ((stage === 'reviewing' || stage === 'submitting') && session) {
    return (
      <main className="h-full min-h-0 bg-background">
        <DrillReview drill={drill} back={back} />
      </main>
    );
  }

  const meta = session ? `${languageLabel(session.drill.language)} · ${session.mode === 'interview' ? 'Interview' : 'Practice'}` : undefined;
  let content;
  if (stage === 'choose') {
    content = <DrillChoose options={drill.options} onChange={drill.setOptions} onStart={() => drill.start()} />;
  } else if (stage === 'starting') {
    content = (
      <>
        <PageHeader title="Code review drills" />
        {failure
          ? (
            <DrillFailure failure={failure} onRetry={drill.retry}>
              <Button type="button" variant="ghost" onClick={drill.restart}>Back to the choices</Button>
            </DrillFailure>
          )
          : <DrillBuilding building={drill.building} onStop={drill.stopWaiting} />}
      </>
    );
  } else if (stage === 'loading' || !result) {
    content = failure
      ? (
        <DrillFailure failure={failure} onRetry={drill.retry}>
          <Button type="button" variant="ghost" onClick={drill.restart}>Start a new drill</Button>
        </DrillFailure>
      )
      : <p role="status" className="text-sm text-text-secondary">Opening your review…</p>;
  } else if (stage === 'grading') {
    content = (
      <>
        <PageHeader title={result.drill.title} meta={meta} />
        {failure && <div className="mb-4"><DrillFailure failure={failure} onRetry={drill.retry} /></div>}
        <DrillGrading result={result} message={drill.gradingMessage} regrading={drill.regrading} onRegrade={drill.regrade} />
      </>
    );
  } else {
    content = (
      <>
        {failure && <div className="mb-4"><DrillFailure failure={failure} onRetry={drill.retry} /></div>}
        <DrillReveal result={result} onNextDrill={drill.nextDrill} onRegrade={drill.regrade} regrading={drill.regrading} />
      </>
    );
  }

  return (
    <main className="h-full overflow-y-auto bg-background">
      <div className={cn('mx-auto w-full px-6 pb-16 pt-10', stage === 'revealed' ? 'max-w-6xl' : 'max-w-[800px]')}>
        <div className="mb-5">{back}</div>
        {content}
      </div>
    </main>
  );
}
