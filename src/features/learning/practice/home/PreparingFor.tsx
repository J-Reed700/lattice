import { type FormEvent, useState } from 'react';
import { Calendar, Code, Edit3, LoaderCircle } from 'lucide-react';

import {
  type LearningPracticeProfileDto,
  practiceHomeApi,
} from './practiceHomeApi';

export interface PreparingForProps {
  programId: string;
  courseGoal?: string;
  profile?: LearningPracticeProfileDto | null;
  onProfileChange?: (profile: LearningPracticeProfileDto) => void;
  className?: string;
}

export function PreparingFor({
  programId,
  courseGoal,
  profile,
  onProfileChange,
  className,
}: PreparingForProps): JSX.Element {
  const [isEditing, setIsEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [details, setDetails] = useState(profile?.details ?? '');
  const [language, setLanguage] = useState(profile?.language ?? '');
  const [interviewDate, setInterviewDate] = useState(() => {
    if (!profile?.interviewAt) return '';
    return new Date(profile.interviewAt).toISOString().slice(0, 10);
  });
  const [company, setCompany] = useState(profile?.company ?? '');
  const [role, setRole] = useState(profile?.role ?? '');

  const handleEditOpen = () => {
    setDetails(profile?.details ?? '');
    setLanguage(profile?.language ?? '');
    setInterviewDate(
      profile?.interviewAt
        ? new Date(profile.interviewAt).toISOString().slice(0, 10)
        : '',
    );
    setCompany(profile?.company ?? '');
    setRole(profile?.role ?? '');
    setError(null);
    setIsEditing(true);
  };

  const handleSave = async (e: FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      const interviewTimestamp = interviewDate
        ? new Date(interviewDate).getTime()
        : null;

      const saved = await practiceHomeApi.saveProfile({
        programId,
        details,
        language: language.trim() || null,
        interviewAt: interviewTimestamp,
        company: company.trim() || null,
        role: role.trim() || null,
        expectedRevision: profile?.revision,
      });

      onProfileChange?.(saved);
      setIsEditing(false);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : 'Failed to save practice profile',
      );
    } finally {
      setSaving(false);
    }
  };

  const formattedInterview = profile?.interviewAt
    ? `Interview ${new Intl.DateTimeFormat(undefined, {
        weekday: 'short',
        day: 'numeric',
        month: 'short',
        year: 'numeric',
      }).format(new Date(profile.interviewAt))}`
    : null;

  return (
    <section
      aria-labelledby="prep-heading"
      className={`rounded-xl border border-border-default bg-surface p-5 shadow-xs ${className ?? ''}`}
    >
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0 flex-1 space-y-3">
          <div
            id="prep-heading"
            className="text-xs font-semibold uppercase tracking-wider text-text-tertiary"
          >
            Preparing for
          </div>

          {!isEditing ? (
            <>
              {courseGoal && (
                <div className="font-serif text-lg font-medium text-text-primary">
                  {courseGoal}
                </div>
              )}
              {profile?.details && (
                <div className="text-sm leading-relaxed text-text-secondary">
                  {profile.details}
                </div>
              )}
              <div className="flex flex-wrap items-center gap-2">
                {profile?.language && (
                  <span className="inline-flex min-h-[28px] items-center gap-1.5 rounded-full bg-surface-raised px-2.5 text-xs font-medium text-text-secondary">
                    <Code className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{profile.language}</span>
                  </span>
                )}
                {formattedInterview && (
                  <span className="inline-flex min-h-[28px] items-center gap-1.5 rounded-full bg-accent-muted px-2.5 text-xs font-medium text-accent">
                    <Calendar className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{formattedInterview}</span>
                  </span>
                )}
                {profile?.role && (
                  <span className="inline-flex min-h-[28px] items-center rounded-full bg-surface-raised px-2.5 text-xs font-medium text-text-secondary">
                    {profile.role}
                  </span>
                )}
              </div>
              <p className="text-xs text-text-tertiary">
                Suggestions on this page follow this.
              </p>
            </>
          ) : (
            <form onSubmit={handleSave} className="space-y-4 pt-1">
              <div>
                <label
                  htmlFor="prep-details"
                  className="block text-xs font-semibold text-text-primary"
                >
                  Details
                </label>
                <textarea
                  id="prep-details"
                  aria-label="Details"
                  rows={3}
                  value={details}
                  onChange={(e) => setDetails(e.target.value)}
                  placeholder="What are you preparing for? (e.g. Easy problems in Python, live with a coach)"
                  className="mt-1.5 w-full rounded-lg border border-border-default bg-surface-sunken p-2.5 text-sm text-text-primary placeholder:text-text-muted focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                />
              </div>

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div>
                  <label
                    htmlFor="prep-language"
                    className="block text-xs font-semibold text-text-primary"
                  >
                    Language
                  </label>
                  <input
                    id="prep-language"
                    type="text"
                    value={language}
                    onChange={(e) => setLanguage(e.target.value)}
                    placeholder="e.g. Python, Rust"
                    className="mt-1 w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary placeholder:text-text-muted focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                  />
                </div>
                <div>
                  <label
                    htmlFor="prep-date"
                    className="block text-xs font-semibold text-text-primary"
                  >
                    Interview date
                  </label>
                  <input
                    id="prep-date"
                    type="date"
                    value={interviewDate}
                    onChange={(e) => setInterviewDate(e.target.value)}
                    className="mt-1 w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                  />
                </div>
              </div>

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div>
                  <label
                    htmlFor="prep-role"
                    className="block text-xs font-semibold text-text-primary"
                  >
                    Target role
                  </label>
                  <input
                    id="prep-role"
                    type="text"
                    value={role}
                    onChange={(e) => setRole(e.target.value)}
                    placeholder="e.g. Software Engineer"
                    className="mt-1 w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary placeholder:text-text-muted focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                  />
                </div>
                <div>
                  <label
                    htmlFor="prep-company"
                    className="block text-xs font-semibold text-text-primary"
                  >
                    Target organization
                  </label>
                  <input
                    id="prep-company"
                    type="text"
                    value={company}
                    onChange={(e) => setCompany(e.target.value)}
                    placeholder="Optional target organization"
                    className="mt-1 w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary placeholder:text-text-muted focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                  />
                </div>
              </div>

              {error && (
                <p role="alert" className="text-xs text-danger-fg">
                  {error}
                </p>
              )}

              <div className="flex items-center gap-2 pt-2">
                <button
                  type="submit"
                  disabled={saving}
                  className="inline-flex min-h-[44px] cursor-pointer items-center justify-center rounded-lg bg-accent px-5 text-sm font-semibold text-accent-fg transition-opacity hover:opacity-90 disabled:opacity-50"
                >
                  {saving && (
                    <LoaderCircle className="mr-2 h-4 w-4 animate-spin" />
                  )}
                  Save
                </button>
                <button
                  type="button"
                  disabled={saving}
                  onClick={() => setIsEditing(false)}
                  className="inline-flex min-h-[44px] cursor-pointer items-center justify-center rounded-lg border border-border-default bg-surface px-4 text-sm font-medium text-text-primary hover:bg-surface-raised disabled:opacity-50"
                >
                  Cancel
                </button>
              </div>
            </form>
          )}
        </div>

        {!isEditing && (
          <button
            type="button"
            onClick={handleEditOpen}
            className="inline-flex min-h-[44px] cursor-pointer items-center gap-1.5 rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary transition-colors hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Edit3 className="h-4 w-4" aria-hidden="true" />
            <span>Edit</span>
          </button>
        )}
      </div>
    </section>
  );
}
