import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { isConflict, newId } from '@/lib/api';
import { clearDraft, useDraft } from '@/shared/drafts/drafts';
import { toast } from '@/stores/toastStore';
import { getErrorMessage } from '@/types/api/result';

import { refreshAffected } from './affected';
import {
  applyContentEdit,
  contentEditKeys,
  openContentEdit,
  previewContentEdit,
  revertContentEdit,
} from './api';
import { ConflictView } from './ConflictView';
import {
  diffDocument,
  draftOf,
  readPointer,
  rebased,
  structuredCloneJson,
  writePointer,
  type DraftMember,
  type DraftState,
} from './diffDocument';
import { EditHistory } from './EditHistory';
import { closeEditor, openEditor, useEditorStore, type EditorRequest } from './editorStore';
import { FieldControl, hasInvalidNumberField } from './fields/FieldControl';
import { rangeOf } from './offsets';
import { selectorForLines } from './sourceAim';

import type {
  ContentEditOutcome,
  ContentEditPreview,
  ContentEditRequest,
  EditableContent,
  JsonValue,
  TextSelector,
} from './types';

const PREVIEW_DELAY_MS = 400;

type SessionState = {
  opened: EditableContent;
  draft: DraftState;
  editId: string | null;
  conflict: boolean;
};

type StoredDraftPayload = {
  revision: string;
  draft: DraftState;
  editId: string | null;
};

function parseStoredDraft(raw: string): StoredDraftPayload | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<StoredDraftPayload>;
    if (
      parsed
      && typeof parsed.revision === 'string'
      && parsed.draft
      && typeof parsed.draft === 'object'
    ) {
      return {
        revision: parsed.revision,
        draft: parsed.draft,
        editId: typeof parsed.editId === 'string' ? parsed.editId : null,
      };
    }
  } catch {
    return null;
  }
  return null;
}

function focusFirstAvailableControl(container: HTMLFormElement | null): void {
  const first = container?.querySelector<HTMLElement>(
    'input:not([disabled]), textarea:not([disabled]), select:not([disabled])',
  );
  if (!first) return;
  first.focus();
  if (
    first instanceof HTMLTextAreaElement
    || (first instanceof HTMLInputElement && ['text', 'search', 'url', 'tel', 'password'].includes(first.type))
  ) {
    first.setSelectionRange(0, 0);
  }
}

interface ContentEditorDialogProps {
  request: EditorRequest;
  onScheduleReturnFocus: (element: HTMLElement | null) => void;
}

function ContentEditorDialog({ request, onScheduleReturnFocus }: ContentEditorDialogProps) {
  const queryClient = useQueryClient();
  const { content } = request.address;
  const draftKey = `edit:${content.kind}:${content.key}`;
  const [storedDraftRaw, setStoredDraftRaw] = useDraft(draftKey);
  const [session, setSession] = useState<SessionState | null>(null);
  const [preview, setPreview] = useState<ContentEditPreview | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [lastFailedRequest, setLastFailedRequest] = useState<ContentEditRequest | null>(null);

  const formRef = useRef<HTMLFormElement>(null);
  const controlRefs = useRef<Map<string, HTMLElement>>(new Map());
  const aimAppliedRef = useRef(false);
  const appliedFragmentRef = useRef<TextSelector | null>(null);
  const initialPreviewSentRef = useRef(false);
  const lastOpsSigRef = useRef<string>('[]');
  const previewSeqRef = useRef(0);

  const registerControl = useCallback((path: string, element: HTMLElement | null) => {
    if (element) controlRefs.current.set(path, element);
    else controlRefs.current.delete(path);
  }, []);

  const openQuery = useQuery({
    queryKey: contentEditKeys.open(content),
    queryFn: () => openContentEdit(content),
    staleTime: 0,
    gcTime: 0,
  });

  useEffect(() => {
    if (session !== null || !openQuery.data) return;
    const opened = openQuery.data;
    const initialValues = request.options.initialValues;

    if (initialValues && Object.keys(initialValues).length > 0) {
      const baseDoc = structuredCloneJson(opened.document);
      const docObj = baseDoc !== null && typeof baseDoc === 'object' && !Array.isArray(baseDoc)
        ? (baseDoc as Record<string, JsonValue>)
        : {};
      for (const [path, value] of Object.entries(initialValues)) {
        writePointer(docObj, path, value);
      }
      const nextDraft = draftOf(opened.form, docObj);
      const ops = diffDocument(opened.form, opened.document, nextDraft);
      const nextEditId = ops.length > 0 ? newId() : null;
      if (nextEditId) {
        setStoredDraftRaw(JSON.stringify({ revision: opened.revision, draft: nextDraft, editId: nextEditId }));
      }
      setSession({ opened, draft: nextDraft, editId: nextEditId, conflict: false });
      return;
    }

    const stored = parseStoredDraft(storedDraftRaw);
    if (stored) {
      if (stored.revision === opened.revision) {
        setSession({ opened, draft: stored.draft, editId: stored.editId, conflict: false });
      } else {
        setSession({ opened, draft: stored.draft, editId: stored.editId, conflict: true });
      }
      return;
    }

    if (storedDraftRaw) clearDraft(draftKey);
    setSession({
      opened,
      draft: draftOf(opened.form, opened.document),
      editId: null,
      conflict: false,
    });
  }, [draftKey, openQuery.data, request.options.initialValues, session, setStoredDraftRaw, storedDraftRaw]);

  useLayoutEffect(() => {
    if (!session || session.conflict || aimAppliedRef.current) return;
    aimAppliedRef.current = true;

    const targetPath = request.address.path;
    const textField = targetPath
      ? session.opened.form.find((field) => field.type === 'text' && field.path === targetPath)
      : undefined;
    const openedValue = targetPath ? readPointer(session.opened.document, targetPath) : undefined;

    let selector: TextSelector | null = null;
    if (request.options.aim && textField && typeof openedValue === 'string') {
      selector = selectorForLines(openedValue, request.options.aim);
    } else if (request.address.fragment) {
      selector = request.address.fragment;
    }

    let applied: TextSelector | null = null;
    const targetControl = targetPath ? controlRefs.current.get(targetPath) : undefined;
    if (
      selector
      && textField
      && typeof openedValue === 'string'
      && !openedValue.includes('\r')
      && (targetControl instanceof HTMLTextAreaElement || targetControl instanceof HTMLInputElement)
    ) {
      const range = rangeOf(openedValue, selector);
      if (range) {
        targetControl.focus();
        targetControl.setSelectionRange(range[0], range[1]);
        targetControl.scrollIntoView?.({ block: 'center' });
        applied = selector;
      }
    }

    appliedFragmentRef.current = applied;
    if (!applied) {
      focusFirstAvailableControl(formRef.current);
    }
  }, [request.address.fragment, request.address.path, request.options.aim, session]);

  const ops = useMemo(
    () => (session ? diffDocument(session.opened.form, session.opened.document, session.draft) : []),
    [session],
  );
  const opsSignature = useMemo(() => JSON.stringify(ops), [ops]);

  const buildEditRequest = useCallback(
    (opened: EditableContent, nextOps: typeof ops, editId: string): ContentEditRequest => ({
      editId,
      content,
      expectedRevision: opened.revision,
      ops: nextOps,
      source: request.options.editSource ?? { type: 'learner' },
      target: request.address.path
        ? { content, path: request.address.path, fragment: appliedFragmentRef.current }
        : null,
    }),
    [content, request.address.path, request.options.editSource],
  );

  const runPreview = useCallback(
    async (opened: EditableContent, nextOps: typeof ops, editId: string) => {
      const seq = previewSeqRef.current + 1;
      previewSeqRef.current = seq;
      try {
        const result = await previewContentEdit(buildEditRequest(opened, nextOps, editId));
        if (seq !== previewSeqRef.current) return;
        if (!result.current) {
          const latest = await openContentEdit(content);
          if (seq !== previewSeqRef.current) return;
          setSession((prev) => (prev ? { ...prev, opened: latest, conflict: true } : prev));
          return;
        }
        setPreview(result);
      } catch {
        // Preview failures should not block editing or overwrite active form feedback.
      }
    },
    [buildEditRequest, content],
  );

  useEffect(() => {
    if (!session || session.conflict) return;
    if (!initialPreviewSentRef.current) {
      initialPreviewSentRef.current = true;
      lastOpsSigRef.current = '[]';
      void runPreview(session.opened, [], session.editId ?? newId());
      if (opsSignature === '[]') return;
    }

    if (opsSignature === lastOpsSigRef.current) return;
    const timer = setTimeout(() => {
      lastOpsSigRef.current = opsSignature;
      void runPreview(session.opened, ops, session.editId ?? newId());
    }, PREVIEW_DELAY_MS);
    return () => clearTimeout(timer);
  }, [ops, opsSignature, runPreview, session]);

  const invalidateEdits = useCallback(
    async (outcome: ContentEditOutcome) => {
      await refreshAffected(outcome.affected, outcome.scope, queryClient);
      void queryClient.invalidateQueries({ queryKey: contentEditKeys.open(outcome.content) });
      void queryClient.invalidateQueries({ queryKey: contentEditKeys.history(outcome.content) });
      void queryClient.invalidateQueries({ queryKey: ['content-edits'] });
    },
    [queryClient],
  );

  const handleUndoOutcome = useCallback(
    async (outcome: ContentEditOutcome) => {
      try {
        const reverted = await revertContentEdit({
          revertId: newId(),
          editId: outcome.editId,
          expectedRevision: outcome.revision,
          andLater: false,
        });
        await invalidateEdits(reverted);
        toast.success('Undone.');
      } catch (error) {
        toast.error(getErrorMessage(error));
      }
    },
    [invalidateEdits],
  );

  const finishSave = useCallback(
    async (outcome: ContentEditOutcome) => {
      clearDraft(draftKey);
      onScheduleReturnFocus(request.options.returnFocusTo ?? null);
      closeEditor();
      await invalidateEdits(outcome);
      toast.success('Saved.', {
        action: {
          label: 'Undo',
          onClick: () => {
            void handleUndoOutcome(outcome);
          },
        },
      });
    },
    [draftKey, handleUndoOutcome, invalidateEdits, onScheduleReturnFocus, request.options.returnFocusTo],
  );

  const sendApplyRequest = async (editRequest: ContentEditRequest) => {
    setSaving(true);
    setSaveError(null);
    setLastFailedRequest(null);
    try {
      const outcome = await applyContentEdit(editRequest);
      await finishSave(outcome);
    } catch (error) {
      if (isConflict(error)) {
        try {
          const latest = await openContentEdit(content);
          setSession((prev) => (prev ? { ...prev, opened: latest, conflict: true } : prev));
        } catch (refreshError) {
          setSaveError(getErrorMessage(refreshError));
        }
      } else {
        setLastFailedRequest(editRequest);
        setSaveError(getErrorMessage(error));
      }
    } finally {
      setSaving(false);
    }
  };

  const updateDraft = (updater: (draft: DraftState) => DraftState) => {
    setSaveError(null);
    setLastFailedRequest(null);
    setSession((prev) => {
      if (!prev) return prev;
      const nextDraft = updater(prev.draft);
      const nextOps = diffDocument(prev.opened.form, prev.opened.document, nextDraft);
      const nextEditId = nextOps.length > 0 ? (prev.editId ?? newId()) : prev.editId;
      setStoredDraftRaw(
        JSON.stringify({ revision: prev.opened.revision, draft: nextDraft, editId: nextEditId }),
      );
      return { ...prev, draft: nextDraft, editId: nextEditId };
    });
  };

  const handleCancel = () => {
    if (!session || ops.length === 0) {
      clearDraft(draftKey);
    } else {
      setStoredDraftRaw(
        JSON.stringify({ revision: session.opened.revision, draft: session.draft, editId: session.editId }),
      );
    }
    onScheduleReturnFocus(request.options.returnFocusTo ?? null);
    closeEditor();
  };

  const handleDiscard = () => {
    clearDraft(draftKey);
    onScheduleReturnFocus(request.options.returnFocusTo ?? null);
    closeEditor();
  };

  const handleSave = () => {
    if (!session || ops.length === 0) return;
    const activeEditId = session.editId ?? newId();
    if (!session.editId) {
      setSession((prev) => (prev ? { ...prev, editId: activeEditId } : prev));
    }
    const editRequest = buildEditRequest(session.opened, ops, activeEditId);
    void sendApplyRequest(editRequest);
  };

  const handleSaveOver = () => {
    if (!session) return;
    const nextDraft = rebased(session.opened.form, session.opened.document, session.draft);
    const nextOps = diffDocument(session.opened.form, session.opened.document, nextDraft);
    void sendApplyRequest(buildEditRequest(session.opened, nextOps, newId()));
  };

  const dialogTitle = session?.opened.label ?? openQuery.data?.label ?? 'Edit';
  const isNotNow = session?.opened.editable.state === 'not_now';
  const hasInvalidNumber = session ? hasInvalidNumberField(session.opened.form, session.draft) : false;
  const globalProblems = (preview?.problems ?? []).filter((problem) => problem.path === null);
  const consequences = preview?.consequences ?? [];
  const saveDisabled =
    ops.length === 0
    || (preview?.problems.length ?? 0) > 0
    || hasInvalidNumber
    || saving;

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) handleCancel();
      }}
    >
      <DialogContent
        className="max-h-[85vh] max-w-3xl overflow-y-auto"
        onOpenAutoFocus={(event) => {
          if (session && !session.conflict) event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{dialogTitle}</DialogTitle>
        </DialogHeader>

        {openQuery.isLoading && (
          <p className="text-ui text-[hsl(var(--text-secondary))]">Opening…</p>
        )}

        {openQuery.isError && !session && (
          <div className="space-y-4">
            <p role="alert" className="text-ui text-[hsl(var(--danger-fg))]">
              {getErrorMessage(openQuery.error)}
            </p>
            <div className="flex justify-end">
              <Button type="button" variant="outline" onClick={handleCancel}>
                Close
              </Button>
            </div>
          </div>
        )}

        {session && (
          <Tabs defaultValue="edit">
            <TabsList>
              <TabsTrigger value="edit">Edit</TabsTrigger>
              <TabsTrigger value="history">History</TabsTrigger>
            </TabsList>

            <TabsContent value="edit" className="mt-4">
              {session.conflict ? (
                <ConflictView
                  current={session.opened}
                  draft={session.draft}
                  saving={saving}
                  error={saveError}
                  onSaveOver={handleSaveOver}
                  onDiscard={() => {
                    clearDraft(draftKey);
                    setSaveError(null);
                    setLastFailedRequest(null);
                    setSession({
                      opened: session.opened,
                      draft: draftOf(session.opened.form, session.opened.document),
                      editId: null,
                      conflict: false,
                    });
                  }}
                />
              ) : (
                <form
                  ref={formRef}
                  className="space-y-4"
                  onSubmit={(event) => {
                    event.preventDefault();
                    if (!isNotNow && !saveDisabled) handleSave();
                  }}
                >
                  {session.opened.notes.length > 0 && (
                    <ul className="space-y-1 rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-sunken))] p-3 text-xs text-[hsl(var(--text-secondary))]">
                      {session.opened.notes.map((note, index) => (
                        <li key={`${index}:${note}`}>{note}</li>
                      ))}
                    </ul>
                  )}

                  {session.opened.form.map((field) => (
                    <FieldControl
                      key={field.path}
                      field={field}
                      fullPath={field.path}
                      value={session.draft.values[field.path] ?? null}
                      draftMembers={field.type === 'list' ? (session.draft.lists[field.path] ?? []) : undefined}
                      onChangeValue={(nextValue) => {
                        updateDraft((draft) => ({
                          ...draft,
                          values: { ...draft.values, [field.path]: nextValue },
                        }));
                      }}
                      onChangeList={(nextMembers: DraftMember[]) => {
                        updateDraft((draft) => ({
                          ...draft,
                          lists: { ...draft.lists, [field.path]: nextMembers },
                        }));
                      }}
                      readOnly={Boolean(isNotNow)}
                      editedPaths={session.opened.editedPaths}
                      problems={preview?.problems ?? []}
                      notices={preview?.notices ?? []}
                      registerControl={registerControl}
                    />
                  ))}

                  {globalProblems.length > 0 && (
                    <div role="alert" className="space-y-1 text-xs text-[hsl(var(--danger-fg))]">
                      {globalProblems.map((problem, index) => (
                        <p key={`${index}:${problem.message}`}>{problem.message}</p>
                      ))}
                    </div>
                  )}

                  {consequences.length > 0 && (
                    <div className="space-y-2 rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-sunken))] p-3 text-xs">
                      {consequences.map((consequence) => (
                        <div key={consequence.code} className="space-y-1">
                          <p className="text-[hsl(var(--text-secondary))]">{consequence.message}</p>
                          {consequence.related.length > 0 && (
                            <div className="flex flex-wrap gap-2">
                              {consequence.related.map((related) => (
                                <Button
                                  key={`${related.content.kind}:${related.content.key}`}
                                  type="button"
                                  variant="outline"
                                  size="sm"
                                  onClick={() => openEditor({ content: related.content, path: null, fragment: null })}
                                >
                                  {related.label}
                                </Button>
                              ))}
                            </div>
                          )}
                        </div>
                      ))}
                    </div>
                  )}

                  {saveError && (
                    <div role="alert" className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-[hsl(var(--border-default))] bg-[hsl(var(--surface-sunken))] p-3 text-xs text-[hsl(var(--danger-fg))]">
                      <span>{saveError}</span>
                      {lastFailedRequest && (
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          disabled={saving}
                          onClick={() => {
                            void sendApplyRequest(lastFailedRequest);
                          }}
                        >
                          Try again
                        </Button>
                      )}
                    </div>
                  )}

                  <div className="flex flex-wrap items-center justify-end gap-2">
                    <Button type="button" variant="ghost" onClick={handleDiscard}>
                      Discard changes
                    </Button>
                    <Button type="button" variant="outline" onClick={handleCancel}>
                      Cancel
                    </Button>
                    {session.opened.editable.state === 'not_now' ? (
                      <span className="text-ui text-[hsl(var(--text-secondary))]">
                        {session.opened.editable.reason}
                      </span>
                    ) : (
                      <Button type="submit" disabled={saveDisabled}>
                        Save
                      </Button>
                    )}
                  </div>
                </form>
              )}
            </TabsContent>

            <TabsContent value="history" className="mt-4">
              <EditHistory content={content} form={session.opened.form} />
            </TabsContent>
          </Tabs>
        )}
      </DialogContent>
    </Dialog>
  );
}

export function ContentEditor(): ReactNode {
  const current = useEditorStore((state) => state.current);
  const pendingFocusRef = useRef<HTMLElement | null>(null);
  const lastRequestRef = useRef<EditorRequest | null>(null);
  const seqRef = useRef(0);

  if (current !== lastRequestRef.current) {
    lastRequestRef.current = current;
    if (current !== null) seqRef.current += 1;
  }

  useLayoutEffect(() => {
    if (current !== null) return;
    const target = pendingFocusRef.current;
    pendingFocusRef.current = null;
    if (target?.isConnected) target.focus();
  }, [current]);

  if (!current) return null;
  return (
    <ContentEditorDialog
      key={seqRef.current}
      request={current}
      onScheduleReturnFocus={(element) => {
        pendingFocusRef.current = element;
      }}
    />
  );
}
