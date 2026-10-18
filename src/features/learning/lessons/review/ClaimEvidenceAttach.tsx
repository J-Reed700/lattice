import { useId, useState } from 'react';

import { FileText, Globe2, LoaderCircle, Type } from 'lucide-react';

import { useCaptureSourceSnapshot } from '@/features/learning/lessons/review/useLessonClaimReview';
import { LearningDocumentPicker, type LearningDocumentSelection } from '@/features/learning/sources/LearningDocumentPicker';
import type { AttachNextStepDto, ClaimActionRequestDto, ClaimAttachReportDto } from '@/lib/bindings';

type Mode = 'link' | 'file' | 'paste';

const MODES: Array<{ id: Mode; label: string; icon: typeof Globe2 }> = [
  { id: 'link', label: 'Add a link', icon: Globe2 },
  { id: 'file', label: 'Add a file', icon: FileText },
  { id: 'paste', label: 'Paste text', icon: Type },
];

const PENDING: Record<Mode, string> = {
  link: 'Fetching the page, saving it to your references and checking this claim against it. This can take a minute.',
  file: 'Saving the document to your references and checking this claim against it. This can take a minute.',
  paste: 'Saving your text to your references and checking this claim against it.',
};

const MODE_OF: Partial<Record<ClaimActionRequestDto['action'], Mode>> = { attach_link: 'link', attach_document: 'file', attach_text: 'paste' };

const NEXT_STEP: Record<AttachNextStepDto['step'], string> = {
  paste_text: 'Open it in your browser, then paste the relevant text',
  add_file: 'Save it as PDF and add the file',
  retry: 'Try again',
};

/**
 * Link, file or pasted text for one claim (§5.4). The command saves the
 * source and checks the claim before it returns, with no progress events, so
 * the form shows its own pending state until then.
 */
export function ClaimEvidenceAttach({ programId, onSubmit, onCancel }: {
  programId: string;
  onSubmit: (action: ClaimActionRequestDto) => Promise<ClaimAttachReportDto | null>;
  onCancel: () => void;
}) {
  const [mode, setMode] = useState<Mode>('link');
  const [url, setUrl] = useState('');
  const [documents, setDocuments] = useState<LearningDocumentSelection[]>([]);
  const [title, setTitle] = useState('');
  const [text, setText] = useState('');
  const [formError, setFormError] = useState('');
  const [pending, setPending] = useState<Mode | null>(null);
  const [report, setReport] = useState<ClaimAttachReportDto | null>(null);
  const [lastAction, setLastAction] = useState<ClaimActionRequestDto | null>(null);
  const capture = useCaptureSourceSnapshot(programId);
  const id = useId();

  const actionFor = (current: Mode): ClaimActionRequestDto | string => {
    if (current === 'link') return url.trim() ? { action: 'attach_link', url: url.trim() } : 'Enter the address of the page.';
    if (current === 'file') return documents[0] ? { action: 'attach_document', documentId: documents[0].id } : 'Choose a document.';
    return text.trim() ? { action: 'attach_text', title: title.trim() || 'Pasted text', text } : 'Paste the text that supports or contradicts the claim.';
  };

  const run = async (action: ClaimActionRequestDto, current: Mode) => {
    setFormError('');
    setReport(null);
    setPending(current);
    setLastAction(action);
    try {
      setReport(await onSubmit(action));
    } catch (error) {
      setFormError(error instanceof Error ? error.message : 'The evidence could not be added.');
    } finally {
      setPending(null);
    }
  };

  const submit = () => {
    const action = actionFor(mode);
    if (typeof action === 'string') { setFormError(action); return; }
    void run(action, mode);
  };

  const nextStep = (step: AttachNextStepDto) => {
    if (step.step === 'retry') { if (lastAction) void run(lastAction, MODE_OF[lastAction.action] ?? mode); return; }
    setReport(null);
    if (step.step === 'paste_text') {
      setMode('paste');
      if (step.origin && !title) setTitle(`Text from ${step.origin}`);
    } else setMode('file');
  };

  const busy = pending !== null;
  const snapshot = report?.snapshot;
  return (
    <div className="mt-3 rounded-xl border border-border bg-background p-4" aria-busy={busy}>
      <div className="flex flex-wrap gap-2" role="group" aria-label="Kind of evidence">
        {MODES.map(({ id: kind, label, icon: Icon }) => (
          <button key={kind} type="button" disabled={busy} aria-pressed={mode === kind} onClick={() => { setMode(kind); setFormError(''); }} className={`inline-flex min-h-9 items-center gap-1.5 rounded-full border px-3 text-xs font-medium disabled:opacity-50 ${mode === kind ? 'border-accent/50 bg-accent/10 text-accent' : 'border-border text-text-secondary'}`}>
            <Icon size={13} aria-hidden="true" />{label}
          </button>
        ))}
      </div>
      {mode === 'link' && (
        <div className="mt-3">
          <label htmlFor={`${id}-url`} className="text-xs font-medium text-text-secondary">Page address</label>
          <input id={`${id}-url`} type="url" inputMode="url" value={url} disabled={busy} onChange={(event) => setUrl(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') submit(); }} placeholder="https://" className="mt-1 w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
        </div>
      )}
      {mode === 'file' && (
        <div className="mt-3 space-y-2">
          <p className="text-xs leading-5 text-text-secondary">Choose an indexed document from a Space. To use a new file, import it into a Space first; it then appears here.</p>
          <LearningDocumentPicker selected={documents} onChange={setDocuments} limit={1} />
        </div>
      )}
      {mode === 'paste' && (
        <div className="mt-3 space-y-2">
          <label htmlFor={`${id}-title`} className="block text-xs font-medium text-text-secondary">Where it is from</label>
          <input id={`${id}-title`} value={title} disabled={busy} onChange={(event) => setTitle(event.target.value)} placeholder="Title or address of the original" className="w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
          <label htmlFor={`${id}-text`} className="block text-xs font-medium text-text-secondary">Text</label>
          <textarea id={`${id}-text`} value={text} disabled={busy} onChange={(event) => setText(event.target.value)} rows={5} className="w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
          <p className="text-xs leading-5 text-text-muted">Pasted text has no public provenance; it is checked like any other source.</p>
        </div>
      )}
      <div className="mt-3 flex flex-wrap gap-2">
        <button type="button" disabled={busy} onClick={submit} className="inline-flex min-h-9 items-center gap-2 rounded-full bg-accent px-4 text-xs font-semibold text-accent-fg disabled:opacity-50">
          {busy && <LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />}
          {busy ? 'Checking…' : 'Add and check'}
        </button>
        <button type="button" disabled={busy} onClick={onCancel} className="min-h-9 rounded-full border border-border px-4 text-xs text-text-secondary disabled:opacity-50">Close</button>
      </div>
      {pending && <p role="status" className="mt-3 text-xs leading-5 text-text-secondary">{PENDING[pending]}</p>}
      {formError && <p role="alert" className="mt-3 text-xs leading-5 text-rose-700">{formError}</p>}
      {report && (
        <div role="status" aria-label="What adding the evidence found" className={`mt-3 space-y-1.5 rounded-lg p-3 text-xs leading-5 ${report.failure ? 'bg-amber-500/10 text-text-primary' : 'bg-surface text-text-secondary'}`}>
          {report.failure && <p className="font-semibold text-text-primary">Not added</p>}
          {report.lines.map((line, index) => <p key={index}>{line}</p>)}
          {report.failure && report.failure.nextSteps.length > 0 && (
            <div className="flex flex-wrap gap-2 pt-1">
              {report.failure.nextSteps.map((step) => (
                <button key={step.step} type="button" onClick={() => nextStep(step)} className="min-h-9 rounded-full border border-border bg-background px-3 text-xs font-medium text-text-primary">{NEXT_STEP[step.step]}</button>
              ))}
            </div>
          )}
          {snapshot?.state === 'failed' && report.source && (
            <button type="button" disabled={capture.isPending} onClick={() => capture.mutate({ versionId: report.source!.sourceVersionId })} className="min-h-9 rounded-full border border-border bg-background px-3 text-xs font-medium disabled:opacity-50">
              {capture.isPending ? 'Capturing…' : 'Capture again'}
            </button>
          )}
          {capture.data?.state === 'ready' && <p>Frozen copy captured.</p>}
          {capture.data?.state === 'pending' && <p>Frozen copy requested; the reader shows the saved text until it is ready.</p>}
          {capture.data?.state === 'failed' && <p>Frozen copy not captured: {capture.data.reason}</p>}
          {capture.error && <p className="text-rose-700">{capture.error.message}</p>}
        </div>
      )}
    </div>
  );
}
