import type { ComponentProps } from 'react';

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ClaimAttachReportDto } from '@/lib/bindings';

import { AddSourceSheet } from './AddSourceSheet';

const mocks = vi.hoisted(() => ({
  add: { mutateAsync: vi.fn(), isPending: false, progress: null as unknown, cancel: vi.fn() },
  workspace: { data: undefined as unknown, isPending: false },
  version: { data: undefined as unknown },
  versionRequest: vi.fn(),
}));
vi.mock('./useAddSource', () => ({ useAddSource: () => mocks.add }));
vi.mock('@/features/learning/sources/useLearningSources', () => ({
  useLearningSourceWorkspace: () => mocks.workspace,
  useLearningSourceVersion: (request: unknown) => { mocks.versionRequest(request); return request ? mocks.version : { data: undefined }; },
}));
vi.mock('@/features/learning/sources/LearningDocumentPicker', () => ({
  LearningDocumentPicker: ({ onChange }: { onChange: (docs: { id: string; title: string; spaceName: string }[]) => void }) => (
    <button type="button" onClick={() => onChange([{ id: 'doc-7', title: 'Herbal.pdf', spaceName: 'Library' }])}>Pick Herbal.pdf</button>
  ),
}));
vi.mock('./AddSourceResult', () => ({
  AddSourceResult: ({ onNextStep }: { onNextStep: (step: unknown) => void }) => (
    <div role="region" aria-label="Attach result">
      <button type="button" onClick={() => onNextStep({ step: 'retry' })}>Retry</button>
      <button type="button" onClick={() => onNextStep({ step: 'paste_text', origin: 'https://example.org/herbs' })}>Paste instead</button>
      <button type="button" onClick={() => onNextStep({ step: 'add_file' })}>Add a file</button>
    </div>
  ),
}));

const report = (overrides: Partial<ClaimAttachReportDto> = {}) => ({ source: { sourceId: 's1' }, failure: null, outcomes: [], ...overrides } as unknown as ClaimAttachReportDto);

function renderSheet(props: Partial<ComponentProps<typeof AddSourceSheet>> = {}) {
  const handlers = { onClose: vi.fn(), onAdded: vi.fn() };
  render(<AddSourceSheet programId="course-1" lessonId="lesson-1" revision={4} target={{ level: 'claim', claimIds: ['c1'] }} initial={null} remaining={0} {...handlers} {...props} />);
  return handlers;
}

const mode = (name: string) => fireEvent.click(screen.getByRole('button', { name }));
const submit = () => fireEvent.click(screen.getByRole('button', { name: 'Add and check' }));

beforeEach(() => {
  mocks.add = { mutateAsync: vi.fn().mockResolvedValue(report()), isPending: false, progress: null, cancel: vi.fn() };
  mocks.workspace = { data: { sources: [] }, isPending: false };
  mocks.version = { data: undefined };
  mocks.versionRequest.mockReset();
});

describe('AddSourceSheet', () => {
  it('asks for what each kind of source needs before sending', () => {
    renderSheet();
    submit();
    expect(screen.getByRole('alert')).toHaveTextContent('Enter the address of the page.');
    mode('Paste text instead');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    submit();
    expect(screen.getByRole('alert')).toHaveTextContent('Paste the text that states the claim.');
    mode('Choose a file…');
    submit();
    expect(screen.getByRole('alert')).toHaveTextContent('Choose a file.');
    mode('Pick a saved source…');
    submit();
    expect(screen.getByRole('alert')).toHaveTextContent('Choose one of your sources.');
    expect(mocks.add.mutateAsync).not.toHaveBeenCalled();
  });

  it('adds a link with Enter', async () => {
    const { onAdded } = renderSheet();
    const field = screen.getByLabelText('Page address');
    fireEvent.change(field, { target: { value: '  https://example.org/herbs  ' } });
    fireEvent.keyDown(field, { key: 'a' });
    expect(mocks.add.mutateAsync).not.toHaveBeenCalled();
    fireEvent.keyDown(field, { key: 'Enter' });
    await waitFor(() => expect(onAdded).toHaveBeenCalled());
    expect(mocks.add.mutateAsync).toHaveBeenCalledWith({ claimId: 'c1', expectedRevision: 4, evidence: { kind: 'attach_link', url: 'https://example.org/herbs' }, scope: { scope: 'claim' } });
  });

  it('adds pasted text, naming it by its origin or as pasted text', async () => {
    const { onAdded } = renderSheet();
    mode('Paste text instead');
    fireEvent.change(screen.getByLabelText('Text'), { target: { value: 'Sage calms a sore throat.' } });
    submit();
    await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(1));
    expect(mocks.add.mutateAsync).toHaveBeenLastCalledWith(expect.objectContaining({ evidence: { kind: 'attach_text', title: 'Pasted text', text: 'Sage calms a sore throat.' } }));
    fireEvent.change(screen.getByLabelText('Where it is from'), { target: { value: ' Culpeper’s Herbal ' } });
    submit();
    await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(2));
    expect(mocks.add.mutateAsync).toHaveBeenLastCalledWith(expect.objectContaining({ evidence: { kind: 'attach_text', title: 'Culpeper’s Herbal', text: 'Sage calms a sore throat.' } }));
  });

  it('adds a library file, and lets a dropped one be swapped', async () => {
    const { onAdded } = renderSheet({ initial: { kind: 'document', documentId: 'doc-1', name: 'Dropped.pdf' } as never });
    expect(screen.getByText('Dropped.pdf')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Choose another' }));
    fireEvent.click(screen.getByRole('button', { name: 'Pick Herbal.pdf' }));
    submit();
    await waitFor(() => expect(onAdded).toHaveBeenCalled());
    expect(mocks.add.mutateAsync).toHaveBeenCalledWith(expect.objectContaining({ evidence: { kind: 'attach_document', documentId: 'doc-7' } }));
  });

  it('adds a whole saved source as its passage', async () => {
    mocks.workspace = { data: { sources: [
      { id: 's1', activeVersion: { id: 'v1', title: 'Field guide to herbs' } },
      { id: 's2', activeVersion: null },
    ] }, isPending: false };
    mocks.version = { data: { fullText: 'Thyme é' } };
    const { onAdded } = renderSheet();
    mode('Pick a saved source…');
    expect(screen.getAllByRole('radio')).toHaveLength(1);
    fireEvent.click(screen.getByRole('radio', { name: 'Field guide to herbs' }));
    expect(mocks.versionRequest).toHaveBeenLastCalledWith({ programId: 'course-1', sourceId: 's1', versionId: 'v1' });
    submit();
    await waitFor(() => expect(onAdded).toHaveBeenCalled());
    expect(mocks.add.mutateAsync).toHaveBeenCalledWith(expect.objectContaining({ evidence: { kind: 'attach_saved_source', sourceVersionId: 'v1', startByte: 0, endByte: 8 } }));
  });

  it('says when the saved sources are loading or there are none', () => {
    mocks.workspace = { data: undefined, isPending: true };
    renderSheet();
    mode('Pick a saved source…');
    expect(screen.getByRole('status')).toHaveTextContent('Loading your sources…');
  });

  it('says the course has no saved sources yet', () => {
    renderSheet();
    mode('Pick a saved source…');
    expect(screen.getByText('The course has no saved sources yet.')).toBeInTheDocument();
  });

  it('can also check the other claims waiting on the subject', async () => {
    renderSheet({ target: { level: 'claim', claimIds: ['c1'], others: ['c2', 'c3'] } });
    const box = screen.getByRole('checkbox', { name: /Also check the 2 other claims that wait/ });
    expect(box).toBeChecked();
    fireEvent.change(screen.getByLabelText('Page address'), { target: { value: 'https://example.org/a' } });
    submit();
    await waitFor(() => expect(mocks.add.mutateAsync).toHaveBeenCalledWith(expect.objectContaining({ scope: { scope: 'claims', claimIds: ['c1', 'c2', 'c3'] } })));
    fireEvent.click(box);
    submit();
    await waitFor(() => expect(mocks.add.mutateAsync).toHaveBeenLastCalledWith(expect.objectContaining({ scope: { scope: 'claim' } })));
  });

  it('names a single other claim in the singular', () => {
    renderSheet({ target: { level: 'claim', claimIds: ['c1'], others: ['c2'] } });
    expect(screen.getByRole('checkbox', { name: /Also check the 1 other claim that waits/ })).toBeInTheDocument();
  });

  it('does nothing more when the run was cancelled, and reports a run that failed', async () => {
    mocks.add.mutateAsync.mockResolvedValueOnce(null).mockRejectedValueOnce(new Error('lesson changed')).mockRejectedValueOnce('odd');
    const { onAdded } = renderSheet();
    fireEvent.change(screen.getByLabelText('Page address'), { target: { value: 'https://example.org/a' } });
    submit();
    await waitFor(() => expect(mocks.add.mutateAsync).toHaveBeenCalledTimes(1));
    expect(onAdded).not.toHaveBeenCalled();
    submit();
    expect(await screen.findByRole('alert')).toHaveTextContent('lesson changed');
    submit();
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('The source could not be added.'));
  });

  it('offers the next steps after a failed attach, and retries the same source', async () => {
    mocks.add.mutateAsync.mockResolvedValue(report({ source: null, failure: { reason: 'blocked' } } as never));
    renderSheet();
    fireEvent.change(screen.getByLabelText('Page address'), { target: { value: 'https://example.org/a' } });
    submit();
    await screen.findByRole('region', { name: 'Attach result' });
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(mocks.add.mutateAsync).toHaveBeenCalledTimes(2));
    expect(mocks.add.mutateAsync.mock.calls[1][0].evidence).toEqual({ kind: 'attach_link', url: 'https://example.org/a' });

    await screen.findByRole('region', { name: 'Attach result' });
    fireEvent.click(screen.getByRole('button', { name: 'Paste instead' }));
    expect(screen.getByLabelText('Where it is from')).toHaveValue('https://example.org/herbs');

    mode('A link');
    submit();
    await screen.findByRole('region', { name: 'Attach result' });
    fireEvent.click(screen.getByRole('button', { name: 'Add a file' }));
    expect(screen.getByRole('button', { name: 'Pick Herbal.pdf' })).toBeInTheDocument();
  });

  it('cancels a run in flight when closed', () => {
    const { onClose } = renderSheet();
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(mocks.add.cancel).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(mocks.add.cancel).toHaveBeenCalledTimes(2);
  });

  it('shows checking progress while busy', () => {
    mocks.add = { ...mocks.add, isPending: true, progress: { stage: 'checking', done: 1, total: 4, statement: 'Sage calms' } };
    renderSheet();
    expect(screen.getByRole('progressbar', { name: 'Claims checked' })).toHaveAttribute('aria-valuenow', '1');
    expect(screen.getByRole('button', { name: 'Checking…' })).toBeDisabled();
  });

  it('replaces what it holds with a later drop', () => {
    const { rerender } = render(<AddSourceSheet programId="course-1" lessonId="lesson-1" revision={4} target={{ level: 'claim', claimIds: ['c1'] }} initial={null} remaining={0} onClose={vi.fn()} onAdded={vi.fn()} />);
    act(() => {
      rerender(<AddSourceSheet programId="course-1" lessonId="lesson-1" revision={4} target={{ level: 'claim', claimIds: ['c1'] }} initial={{ kind: 'text', text: 'Mint aids digestion.', origin: null } as never} remaining={0} onClose={vi.fn()} onAdded={vi.fn()} />);
    });
    expect(screen.getByLabelText('Text')).toHaveValue('Mint aids digestion.');
    expect(screen.getByLabelText('Where it is from')).toHaveValue('');
  });
});
