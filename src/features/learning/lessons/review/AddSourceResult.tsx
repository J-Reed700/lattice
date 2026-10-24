import { Button } from '@/components/ui/button';
import { failureLine } from '@/features/learning/lessons/review/claimReviewCopy';
import { useCaptureSourceSnapshot } from '@/features/learning/lessons/review/useLessonClaimReview';
import type { AttachNextStepDto, ClaimAttachReportDto } from '@/lib/bindings';

const NEXT_STEP: Record<AttachNextStepDto['step'], string> = {
  paste_text: 'Open it in your browser, then paste the relevant text',
  add_file: 'Save it as PDF and add the file',
  retry: 'Try again',
};

/**
 * What an attach found, or why it could not read the page and what to do
 * instead  failure states). A frozen copy that failed can be captured
 * again from here.
 */
export function AddSourceResult({ programId, report, line, host, onNextStep }: {
  programId: string;
  report: ClaimAttachReportDto;
  /** The summary line, when the source was added. */
  line: string | null;
  /** The address's host, for the failure words. */
  host: string | null;
  onNextStep: (step: AttachNextStepDto) => void;
}) {
  const capture = useCaptureSourceSnapshot(programId);
  const snapshot = report.snapshot;
  if (report.failure) {
    return (
      <div role="alert" className="space-y-2 rounded-lg bg-warning-muted p-3 text-sm leading-6 text-text-primary">
        <p>{failureLine(report.failure.kind, host, report.failure.message)}</p>
        {report.failure.nextSteps.length > 0 && <div className="flex flex-wrap gap-2">
          {report.failure.nextSteps.map((step) => (
            <Button key={step.step} type="button" variant="outline" onClick={() => onNextStep(step)}>{NEXT_STEP[step.step]}</Button>
          ))}
        </div>}
      </div>
    );
  }
  return (
    <div role="status" aria-label="What adding the source found" className="space-y-2 rounded-lg border border-border-default bg-surface-sunken/40 p-3 text-sm leading-6 text-text-secondary">
      {line && <p className="text-text-primary">{line}</p>}
      {snapshot.state === 'failed' && report.source && (
        <Button type="button" variant="outline" disabled={capture.isPending}
          onClick={() => capture.mutate({ sourceId: report.source!.sourceId, versionId: report.source!.sourceVersionId })}>
          {capture.isPending ? 'Saving a copy of the page…' : 'Save a copy of the page again'}
        </Button>
      )}
      {capture.data?.state === 'ready' && <p>A copy of the page is saved.</p>}
      {capture.data?.state === 'pending' && <p>A copy of the page is being saved; the reader shows the saved text until it is ready.</p>}
      {capture.data?.state === 'failed' && <p>No copy of the page was saved: {capture.data.reason}</p>}
      {capture.error && <p className="text-danger-fg">{capture.error.message}</p>}
    </div>
  );
}
