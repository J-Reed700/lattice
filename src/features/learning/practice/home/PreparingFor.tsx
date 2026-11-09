import { useId, useState } from 'react';

import { CalendarClock, Pencil, TriangleAlert } from 'lucide-react';

import { StatusLine, type StatusState } from '@/features/learning/practice/shared/StatusLine';
import { isConflict } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';

import { practiceHomeApi } from './practiceHomeApi';

export function toLocalInput(timestamp: number | null | undefined): string {
  if (!timestamp) return '';
  const date = new Date(timestamp);
  return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
}

function fromLocalInput(value: string): number | null {
  if (!value) return null;
  const time = new Date(value).getTime();
  return Number.isFinite(time) ? time : null;
}

const blankToNull = (value: string) => value.trim() || null;

interface Form {
  details: string;
  language: string;
  interviewAt: string;
  company: string;
  role: string;
}

function formOf(profile: Wire.LearningPracticeProfileDto | null | undefined): Form {
  return {
    details: profile?.details ?? '',
    language: profile?.language ?? '',
    interviewAt: toLocalInput(profile?.interviewAt),
    company: profile?.company ?? '',
    role: profile?.role ?? '',
  };
}

export interface PreparingForProps {
  programId: string;
  goal: string;
  profile: Wire.LearningPracticeProfileDto | null | undefined;
  loading?: boolean;
  loadError?: string | null;
  onRetryLoad?: () => void;
  onSaved: (profile: Wire.LearningPracticeProfileDto) => void;
  /** The profile as it is saved now, for when an edit finds it changed elsewhere. */
  onReload: () => Promise<Wire.LearningPracticeProfileDto | null | undefined>;
}

/** What the learner is preparing for: the course goal, plus the details, date, language, company and role they add. */
export function PreparingFor({ programId, goal, profile, loading, loadError, onRetryLoad, onSaved, onReload }: PreparingForProps) {
  const uid = useId();
  const [editing, setEditing] = useState(false);
  const [form, setForm] = useState<Form>(() => formOf(profile));
  const [saving, setSaving] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [status, setStatus] = useState<StatusState>({ kind: 'idle' });

  const set = (field: keyof Form) => (value: string) => setForm((current) => ({ ...current, [field]: value }));

  const open = () => {
    setForm(formOf(profile));
    setConflict(false);
    setStatus({ kind: 'idle' });
    setEditing(true);
  };

  const save = async () => {
    if (saving) return;
    setSaving(true);
    setConflict(false);
    setStatus({ kind: 'busy', label: 'Saving…' });
    try {
      const saved = await practiceHomeApi.saveProfile({
        programId,
        details: form.details.trim(),
        language: blankToNull(form.language),
        interviewAt: fromLocalInput(form.interviewAt),
        company: blankToNull(form.company),
        role: blankToNull(form.role),
        expectedRevision: profile?.revision ?? 0,
      });
      onSaved(saved);
      setStatus({ kind: 'idle' });
      setEditing(false);
    } catch (error) {
      if (isConflict(error)) {
        setConflict(true);
        setStatus({ kind: 'idle' });
      } else {
        setStatus({ kind: 'error', message: getErrorMessage(error), retry: () => void save() });
      }
    } finally {
      setSaving(false);
    }
  };

  const loadLatest = async () => {
    try {
      const latest = await onReload();
      setForm(formOf(latest));
      setConflict(false);
    } catch (error) {
      setStatus({ kind: 'error', message: getErrorMessage(error), retry: () => void loadLatest() });
    }
  };

  const details = profile?.details.trim();
  const interviewAt = profile?.interviewAt ?? null;
  const field = 'mt-1 block w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring';

  return (
    <section aria-labelledby={`${uid}-heading`} className="rounded-xl border border-border-default bg-surface p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <h2 id={`${uid}-heading`} className="text-xs font-semibold uppercase tracking-wider text-text-muted">
            Preparing for
          </h2>
          <p className="mt-1 text-base font-medium text-text-primary">{goal}</p>
          {details && <p className="mt-1 whitespace-pre-line text-sm text-text-secondary">{details}</p>}
          {interviewAt !== null && (
            <p className="mt-2 inline-flex items-center gap-1.5 text-sm text-text-secondary">
              <CalendarClock className="h-4 w-4" aria-hidden="true" />
              <span>Interview on {new Date(interviewAt).toLocaleDateString(undefined, { dateStyle: 'long' })}</span>
            </p>
          )}
          {loading && <StatusLine state={{ kind: 'busy', label: 'Loading what you added…' }} />}
          {loadError && <StatusLine state={{ kind: 'error', message: loadError, retry: onRetryLoad }} />}
        </div>
        {!editing && (
          <button
            type="button"
            onClick={open}
            disabled={loading || Boolean(loadError)}
            className="inline-flex min-h-11 items-center gap-1.5 rounded-lg border border-border-default px-3 text-sm font-medium text-text-primary hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
          >
            <Pencil className="h-4 w-4" aria-hidden="true" />
            Edit
          </button>
        )}
      </div>

      {editing && (
        <form
          className="mt-4 space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <label className="block text-sm font-medium text-text-secondary">
            What you are preparing for
            <textarea
              value={form.details}
              onChange={(event) => set('details')(event.target.value)}
              rows={3}
              placeholder="For example: easy problems, solved while someone watches."
              className={field}
            />
          </label>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="block text-sm font-medium text-text-secondary">
              Language
              <input type="text" value={form.language} onChange={(event) => set('language')(event.target.value)} className={field} />
            </label>
            <label className="block text-sm font-medium text-text-secondary">
              Interview date
              <input type="datetime-local" value={form.interviewAt} onChange={(event) => set('interviewAt')(event.target.value)} className={field} />
            </label>
            <label className="block text-sm font-medium text-text-secondary">
              Company
              <input type="text" value={form.company} onChange={(event) => set('company')(event.target.value)} className={field} />
            </label>
            <label className="block text-sm font-medium text-text-secondary">
              Role
              <input type="text" value={form.role} onChange={(event) => set('role')(event.target.value)} className={field} />
            </label>
          </div>
          {conflict && (
            <div role="alert" className="flex flex-wrap items-center gap-3 rounded-lg bg-warning-muted px-3 py-2 text-sm text-warning-fg">
              <TriangleAlert className="h-4 w-4 shrink-0" aria-hidden="true" />
              <span>This changed somewhere else since you opened it. Load the latest to see it.</span>
              <button type="button" onClick={() => void loadLatest()} className="min-h-11 font-semibold underline">
                Load the latest
              </button>
            </div>
          )}
          <StatusLine state={status} />
          <div className="flex flex-wrap gap-2">
            <button
              type="submit"
              disabled={saving || conflict}
              className="inline-flex min-h-11 items-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
            >
              Save
            </button>
            <button
              type="button"
              onClick={() => setEditing(false)}
              disabled={saving}
              className="inline-flex min-h-11 items-center rounded-lg border border-border-default px-4 text-sm font-medium text-text-primary hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            >
              Cancel
            </button>
          </div>
          {conflict && <p className="text-sm text-text-secondary">Save is off until you load the latest, so nothing newer is overwritten.</p>}
        </form>
      )}
    </section>
  );
}
