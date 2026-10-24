import { useId, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ShieldCheck } from 'lucide-react';

import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { learningProgramKey } from '@/features/learning/api/learningQueryKeys';
import { useProgramVerifierAudit, useSetProgramVerifierAudit } from '@/features/learning/lessons/review/useVerifierAudit';
import type { LearningProgramDto } from '@/lib/bindings';

const PROVIDER_NAMES: Record<string, string> = { anthropic: 'Anthropic', openai: 'OpenAI', google: 'Google' };
export const providerName = (provider: string) => PROVIDER_NAMES[provider] ?? provider;

/**
 * The course's opt-in to a final grounded audit by the Verifier model
 * (§14.2). A cloud Verifier needs the consent dialog, naming the provider,
 * before the switch turns on; turning it off takes effect at once.
 */
export function VerifierAuditSwitch({ programId }: { programId: string }) {
  const client = useQueryClient();
  const audit = useProgramVerifierAudit(programId);
  const set = useSetProgramVerifierAudit(programId);
  const [consentOpen, setConsentOpen] = useState(false);
  const [webSearch, setWebSearch] = useState(false);
  const helpId = useId();
  const course = client.getQueryData<LearningProgramDto>(learningProgramKey(programId))?.summary.title ?? 'this course';
  const data = audit.data;

  if (audit.isPending) return <p role="status" className="text-xs text-text-muted">Loading the Verifier audit setting…</p>;
  if (audit.isError || !data) return <p className="text-xs text-text-muted">The Verifier audit setting is unavailable: {audit.error?.message}</p>;

  const provider = data.consentRequiredFor ? providerName(data.consentRequiredFor) : null;
  const toggle = () => {
    set.reset();
    if (data.on) { set.mutate({ on: false, webSearch: false, consentConfirmed: false }); return; }
    setWebSearch(data.webSearch);
    if (provider) { setConsentOpen(true); return; }
    set.mutate({ on: true, webSearch: data.webSearch, consentConfirmed: false });
  };
  const confirm = () => {
    set.mutate({ on: true, webSearch, consentConfirmed: true }, { onSuccess: () => setConsentOpen(false) });
  };

  return (
    <section aria-label="Verifier audit" className="rounded-2xl border border-border bg-surface p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="flex items-center gap-2 font-serif text-xl text-text-primary"><ShieldCheck size={18} className="text-accent" aria-hidden="true" />Audit lessons with your Verifier</h3>
          <p id={helpId} className="mt-2 max-w-2xl text-xs leading-5 text-text-secondary">
            {data.destination
              ? `Before a lesson becomes ready, its full text, its claims and the exact cited passages are sent to ${data.destination} for a second check. A disputed claim waits for your review; the audit never edits the lesson.`
              : 'No Verifier model is set. Choose one in Settings → Models under Verifier.'}
          </p>
        </div>
        <button type="button" role="switch" aria-checked={data.on} aria-describedby={helpId} disabled={set.isPending || (!data.on && !data.destination)} onClick={toggle}
          className={`min-h-9 shrink-0 rounded-full border px-4 text-xs font-semibold disabled:opacity-50 ${data.on ? 'border-accent bg-accent text-accent-fg' : 'border-border text-text-secondary'}`}>
          {data.on ? 'On' : 'Off'}
        </button>
      </div>
      {data.blocked && <p role="status" className="mt-3 rounded-lg border border-amber-600/30 p-3 text-xs leading-5 text-text-primary">{data.blocked}
        {provider && data.on && <button type="button" onClick={() => { setWebSearch(data.webSearch); setConsentOpen(true); }} className="ml-2 font-semibold underline">Confirm again</button>}
      </p>}
      {data.on && <p className="mt-3 text-xs text-text-muted">{data.webSearch ? 'The Verifier may search the web for this course. ' : ''}You can turn this off at any time; audits already running stop at the next batch.</p>}
      {data.usage.audits > 0 && <p className="mt-2 text-xs tabular-nums text-text-muted">This course: {data.usage.audits} {data.usage.audits === 1 ? 'audit' : 'audits'} · {data.usage.inputTokens.toLocaleString()} tokens sent · {data.usage.outputTokens.toLocaleString()} received</p>}
      {set.error && <p role="alert" className="mt-3 text-xs text-rose-700">{set.error.message}</p>}

      <Dialog open={consentOpen} onOpenChange={(open) => { if (!open && !set.isPending) setConsentOpen(false); }}>
        <DialogContent className="max-w-lg">
          <DialogTitle>Audit lessons with your Verifier model before publishing</DialogTitle>
          <DialogDescription asChild>
            <div className="space-y-3 text-sm leading-6 text-text-secondary">
              <p>Before a lesson of “{course}” becomes ready, its full lesson text (every section, question, answer and explanation), every factual claim extracted from it, and the exact cited passages from your references are sent to <strong className="text-text-primary">{data.destination ?? provider}</strong> for a second check. Nothing else from your library is sent. If the Verifier is allowed to search the web, the claim statements are also sent to your configured web search.</p>
              <p>Your provider’s usage terms and pricing apply; Lattice shows the tokens sent and received for each audit. You can turn this off at any time; audits already running stop at the next batch.</p>
            </div>
          </DialogDescription>
          <label className="mt-2 flex items-center gap-2 text-sm text-text-primary">
            <input type="checkbox" checked={webSearch} onChange={(event) => setWebSearch(event.target.checked)} />
            Allow the Verifier to use web search for this course
          </label>
          {set.error && <p role="alert" className="text-xs text-rose-700">{set.error.message}</p>}
          <div className="mt-4 flex flex-wrap justify-end gap-2">
            <button type="button" disabled={set.isPending} onClick={() => setConsentOpen(false)} className="min-h-9 rounded-full border border-border px-4 text-xs">Cancel</button>
            <button type="button" disabled={set.isPending} onClick={confirm} className="min-h-9 rounded-full bg-accent px-4 text-xs font-semibold text-accent-fg disabled:opacity-50">
              {set.isPending ? 'Turning on…' : `Turn on and send lesson text to ${provider}`}
            </button>
          </div>
        </DialogContent>
      </Dialog>
    </section>
  );
}
