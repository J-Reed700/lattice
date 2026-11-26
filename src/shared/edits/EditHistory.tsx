import { useState } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import { Button } from '@/components/ui/button';
import { newId } from '@/lib/api';
import { toast } from '@/stores/toastStore';
import { getErrorMessage } from '@/types/api/result';

import { refreshAffected } from './affected';
import { contentEditKeys, getContentEditHistory, revertContentEdit } from './api';
import { fieldLabelForPath, formatFieldValue } from './formatValue';
import { originLabel } from './origins';

import type { ContentEditRecord, ContentRef, FieldSpec } from './types';

export interface EditHistoryProps {
  content: ContentRef;
  form?: FieldSpec[];
}

function revertButtonLabel(row: ContentEditRecord): string {
  if (!row.standing) return 'Redo';
  if (row.later === 0) return 'Undo';
  return `Undo this and ${row.later} later ${row.later === 1 ? 'change' : 'changes'}`;
}

export function EditHistory({ content, form }: EditHistoryProps) {
  const queryClient = useQueryClient();
  const [pendingId, setPendingId] = useState<string | null>(null);

  const historyQuery = useQuery({
    queryKey: contentEditKeys.history(content),
    queryFn: () => getContentEditHistory(content),
  });

  if (historyQuery.isLoading) {
    return <p className="text-ui text-[hsl(var(--text-secondary))]">Loading history…</p>;
  }

  if (historyQuery.isError) {
    return (
      <p role="alert" className="text-ui text-[hsl(var(--danger-fg))]">
        {getErrorMessage(historyQuery.error)}
      </p>
    );
  }

  const history = historyQuery.data;
  if (!history || history.edits.length === 0) {
    return <p className="text-ui text-[hsl(var(--text-secondary))]">No edits yet.</p>;
  }

  const rows = [...history.edits].sort((a, b) => b.seq - a.seq || b.createdAt - a.createdAt);

  const handleRevert = async (row: ContentEditRecord) => {
    const isRedo = !row.standing;
    setPendingId(row.id);
    try {
      const outcome = await revertContentEdit({
        revertId: newId(),
        editId: row.id,
        expectedRevision: history.revision,
        andLater: row.standing && row.later > 0,
      });
      await refreshAffected(outcome.affected, outcome.scope, queryClient);
      void queryClient.invalidateQueries({ queryKey: contentEditKeys.open(content) });
      void queryClient.invalidateQueries({ queryKey: contentEditKeys.history(content) });
      void queryClient.invalidateQueries({ queryKey: ['content-edits'] });
      toast.success(isRedo ? 'Redone.' : 'Undone.');
    } catch (error) {
      toast.error(getErrorMessage(error));
    } finally {
      setPendingId(null);
    }
  };

  return (
    <ul className="space-y-3">
      {rows.map((row) => (
        <li
          key={row.id}
          className="space-y-2 rounded-md border border-[hsl(var(--border-subtle))] p-3 text-ui"
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <div className="font-medium text-[hsl(var(--text-primary))]">{originLabel(row.origin)}</div>
              <div className="text-xs text-[hsl(var(--text-tertiary))]">
                {new Date(row.createdAt).toLocaleString()}
              </div>
            </div>
            {row.revertible ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={pendingId !== null}
                onClick={() => {
                  void handleRevert(row);
                }}
              >
                {revertButtonLabel(row)}
              </Button>
            ) : null}
          </div>
          {row.changes.length > 0 && (
            <dl className="space-y-1 text-xs">
              {row.changes.map((change, index) => (
                <div key={`${change.path}:${index}`} className="flex flex-wrap gap-2">
                  <dt className="font-medium text-[hsl(var(--text-secondary))]">
                    {fieldLabelForPath(form, change.path)}:
                  </dt>
                  <dd className="text-[hsl(var(--text-primary))]">
                    <span>{formatFieldValue(change.before, form, change.path)}</span>
                    {' → '}
                    <span>{formatFieldValue(change.after, form, change.path)}</span>
                  </dd>
                </div>
              ))}
            </dl>
          )}
        </li>
      ))}
    </ul>
  );
}
