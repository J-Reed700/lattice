import { useState, type ReactNode } from 'react';
import { ArrowLeft } from 'lucide-react';

import { ConfirmDialog } from '@/components/ConfirmDialog/ConfirmDialog';
import { Button } from '@/components/ui/button';
import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';
import type { CodeReviewSeverity } from '@/lib/bindings';

import { DrillFailure } from './DrillFailure';
import { drillMeta, kindOf } from './drillMaterial';
import { describeSpan, lineCount, type DraftFinding } from './findings';
import { FindingsList } from './FindingsList';
import { InterviewClock } from './InterviewClock';
import { MaterialPane } from './MaterialPane';
import { ReviewComposer } from './ReviewComposer';

import type { CodeReviewDrill } from './useCodeReviewDrill';

interface Picked {
  path: string;
  range: ExplorerLineRange;
}

/**
 * The review: the drill's files on the left, to read and pick lines from, and on the right the comment being written,
 * the comments so far, the optional fix and the hand-in.
 */
export function DrillReview({ drill, back }: { drill: CodeReviewDrill; back?: ReactNode }) {
  const { session, review, failure, retry } = drill;
  const [activePath, setActivePath] = useState(session?.drill.files[0]?.path ?? '');
  const [picked, setPicked] = useState<Picked | null>(null);
  const [highlight, setHighlight] = useState<{ path: string; range: ExplorerLineRange; nonce: number } | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [writingFix, setWritingFix] = useState(false);
  const [confirmEmpty, setConfirmEmpty] = useState(false);

  if (!session) return null;
  const { files } = session.drill;
  const kind = kindOf(session.drill);
  const passage = kind === 'passage';
  const active = files.find((file) => file.path === activePath) ?? files[0];
  const editing = review.findings.find((finding) => finding.id === editingId) ?? null;
  const spanOfPicked = picked
    ? { path: picked.path, start: picked.range.startLine, end: picked.range.endLine }
    : null;

  const selectLines = (range: ExplorerLineRange | null) => {
    if (!range) {
      setPicked(null);
      return;
    }
    const last = lineCount(active.text);
    if (range.startLine > last) return;
    setPicked({ path: active.path, range: { startLine: range.startLine, endLine: Math.min(range.endLine, last) } });
  };

  const finish = () => {
    setEditingId(null);
    setPicked(null);
  };

  const save = (input: { text: string; severity: CodeReviewSeverity | null }) => {
    if (editing) review.update(editing.id, { span: spanOfPicked, ...input });
    else review.add({ span: spanOfPicked, ...input });
    finish();
  };

  const show = (finding: DraftFinding) => {
    const { span } = finding;
    if (!span) return;
    setWritingFix(false);
    setActivePath(span.path);
    setHighlight((previous) => ({
      path: span.path,
      range: { startLine: span.start, endLine: span.end },
      nonce: (previous?.nonce ?? 0) + 1,
    }));
  };

  const edit = (finding: DraftFinding) => {
    setEditingId(finding.id);
    const { span } = finding;
    setPicked(span ? { path: span.path, range: { startLine: span.start, endLine: span.end } } : null);
    if (span) {
      setWritingFix(false);
      setActivePath(span.path);
    }
  };

  const remove = (finding: DraftFinding) => {
    if (finding.id === editingId) finish();
    review.remove(finding.id);
  };

  const handIn = () => {
    if (review.findings.length === 0) setConfirmEmpty(true);
    else review.submit();
  };

  const changed = review.changedPaths.length;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-border-default px-4 py-3 sm:px-6">
        <Button
          type="button"
          variant="ghost"
          className="-ml-3 gap-2 text-text-secondary hover:text-text-primary"
          onClick={drill.toChoices}
          aria-label="Review drills"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          Back to drill setup
        </Button>
        <div className="min-w-0">
          <h2 className="truncate font-serif text-lg text-text-primary">{session.drill.title}</h2>
          <p className="text-xs text-text-muted">{drillMeta(session.drill, session.mode)}</p>
        </div>
        {session.limitMs !== null && (
          <div className="ml-auto">
            <InterviewClock startedAt={session.startedAt} limitMs={session.limitMs} />
          </div>
        )}
      </header>

      {/* P2-14: Task statement line above the panes */}
      <div className="border-b border-border-subtle bg-surface-sunken px-4 py-2 sm:px-6">
        <p className="text-xs font-medium text-text-secondary">
          {passage
            ? 'Find the issues in the explanation below'
            : 'Find the issues in the code below'}
        </p>
      </div>

      <div className="grid min-h-0 flex-1 gap-4 overflow-y-auto p-4 sm:p-6 lg:grid-cols-[minmax(0,1fr)_minmax(22rem,28rem)] lg:grid-rows-[minmax(0,1fr)] lg:overflow-hidden">
        <div className="h-[32rem] min-h-0 lg:h-full">
          <MaterialPane
            drill={session.drill}
            activePath={active.path}
            onActivePath={setActivePath}
            writingFix={writingFix}
            selection={picked?.path === active.path ? picked.range : null}
            onSelectLines={selectLines}
            highlight={highlight?.path === active.path ? highlight.range : null}
            highlightNonce={highlight?.path === active.path ? highlight.nonce : null}
            fixEdits={review.fixEdits}
            onFixEdit={review.editFix}
          />
        </div>

        <div className="flex min-h-0 flex-col gap-4 lg:overflow-y-auto lg:pr-1">
          <ReviewComposer
            key={editing?.id ?? 'new'}
            material={kind}
            selection={spanOfPicked ? describeSpan(spanOfPicked, kind) : null}
            onClearSelection={() => setPicked(null)}
            editing={editing}
            onSave={save}
            onCancel={finish}
          />

          <FindingsList
            findings={review.findings}
            material={kind}
            editingId={editingId}
            onShow={show}
            onEdit={edit}
            onRemove={remove}
            onMove={(finding, delta) => review.move(finding.id, delta)}
          />

          <section aria-labelledby="drill-fix-title" className="rounded-2xl border border-border-default bg-surface p-4">
            <h3 id="drill-fix-title" className="text-xs font-semibold text-text-primary">Your fix</h3>
            <p className="mt-1 text-xs leading-5 text-text-secondary">
              {passage
                ? 'Optional. Rewrite what you think is wrong in your own copy of the explanation, one sentence to a line.'
                : 'Optional. Edit your own copy of the files; only the files you change are handed in.'}
            </p>
            <div className="mt-3 flex flex-wrap items-center gap-2">
              <Button type="button" variant="outline" aria-expanded={writingFix} onClick={() => setWritingFix((open) => !open)}>
                {writingFix ? (passage ? 'Back to the explanation' : 'Back to the code') : 'Write a fix (optional)'}
              </Button>
              {changed > 0 && (
                <>
                  <span className="text-xs text-text-secondary">
                    {passage ? 'Explanation changed' : changed === 1 ? '1 file changed' : `${changed} files changed`}
                  </span>
                  <Button type="button" variant="ghost" size="sm" onClick={review.discardFix}>Discard my changes</Button>
                </>
              )}
            </div>
          </section>

          <div className="sticky bottom-0 mt-auto space-y-3 border-t border-border-default bg-bg pb-1 pt-3">
            {failure && <DrillFailure failure={failure} onRetry={retry} />}
            <Button type="button" size="lg" className="w-full" disabled={review.submitting} onClick={handIn}>
              {review.submitting ? 'Submitting…' : 'Submit review'}
            </Button>
          </div>
        </div>
      </div>

      <ConfirmDialog
        isOpen={confirmEmpty}
        title="Submit with no comments?"
        message="You have not left a comment. A review with none finds none of the issues."
        confirmLabel="Submit anyway"
        cancelLabel="Keep reviewing"
        onConfirm={() => review.submit()}
        onCancel={() => setConfirmEmpty(false)}
      />
    </div>
  );
}
