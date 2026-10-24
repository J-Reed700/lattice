import { useRef } from 'react';

import { createEvent, act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { sourceFromText, useReviewDrop, type DroppedSource } from '@/features/learning/lessons/review/useReviewDrop';

const mocks = vi.hoisted(() => ({
  start: vi.fn(), status: vi.fn(), poll: vi.fn(),
  fileDrop: { paths: null as null | ((paths: string[]) => void), dragging: false },
}));
vi.mock('@/lib/api', () => ({ default: { startBatchFileImport: mocks.start, getBatchJobStatus: mocks.status } }));
vi.mock('@/features/chat/model/attachmentImportPoll', () => ({ pollAttachmentImport: mocks.poll }));
vi.mock('@/features/chat/hooks/useChatFileDrop', () => ({
  useChatFileDrop: (_ref: unknown, onPaths: (paths: string[]) => void) => { mocks.fileDrop.paths = onPaths; return { isDragging: mocks.fileDrop.dragging }; },
}));

function Probe({ onSource }: { onSource: (source: DroppedSource) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const drop = useReviewDrop(ref, onSource);
  return (
    <div ref={ref} data-testid="root" {...drop.handlers}>
      <textarea aria-label="Note" />
      <span data-testid="dragging">{String(drop.dragging)}</span>
      <span data-testid="importing">{drop.importing ?? ''}</span>
      <span role="alert">{drop.error ?? ''}</span>
      <button type="button" onClick={drop.clearError}>Clear</button>
    </div>
  );
}

const data = (values: Record<string, string>, types: string[] = Object.keys(values)) => ({
  types, getData: (type: string) => values[type] ?? '',
});

beforeEach(() => {
  vi.clearAllMocks();
  mocks.fileDrop.dragging = false;
});

// jsdom has no DragEvent, so a drag-leave's relatedTarget has to be set by hand.
function leaveOnto(root: HTMLElement, target: Element | null) {
  const event = createEvent.dragLeave(root);
  Object.defineProperty(event, 'relatedTarget', { value: target });
  fireEvent(root, event);
}

describe('sourceFromText', () => {
  it('turns an address into a link, trimming it', () => {
    expect(sourceFromText('  https://example.org/a?b=1 ')).toEqual({ kind: 'link', url: 'https://example.org/a?b=1' });
  });

  it('turns text of more than one line into pasted text with where it came from', () => {
    expect(sourceFromText('Line one\nLine two', 'https://example.org')).toEqual({ kind: 'text', text: 'Line one\nLine two', origin: 'https://example.org' });
    expect(sourceFromText('Line one\nLine two')).toMatchObject({ origin: null });
  });

  it('ignores empty text, one line of words and addresses with spaces in them', () => {
    expect(sourceFromText('   ')).toBeNull();
    expect(sourceFromText('just a sentence')).toBeNull();
    expect(sourceFromText('https://example.org with words')).toBeNull();
  });
});

describe('useReviewDrop', () => {
  it('turns a pasted address into a link and swallows the paste', () => {
    const onSource = vi.fn();
    render(<Probe onSource={onSource} />);
    const notCancelled = fireEvent.paste(screen.getByTestId('root'), { clipboardData: data({ 'text/plain': 'https://example.org/page' }) });
    expect(onSource).toHaveBeenCalledWith({ kind: 'link', url: 'https://example.org/page' });
    expect(notCancelled).toBe(false);
  });

  it('takes the page a pasted selection came from out of the clipboard html', () => {
    const onSource = vi.fn();
    render(<Probe onSource={onSource} />);
    fireEvent.paste(screen.getByTestId('root'), { clipboardData: data({
      'text/plain': 'First line\nSecond line',
      'text/html': 'Version:0.9\nSourceURL:https://example.org/article\n<html></html>',
    }) });
    expect(onSource).toHaveBeenCalledWith({ kind: 'text', text: 'First line\nSecond line', origin: 'https://example.org/article' });
  });

  it('prefers the address in a uri list as the origin', () => {
    const onSource = vi.fn();
    render(<Probe onSource={onSource} />);
    fireEvent.paste(screen.getByTestId('root'), { clipboardData: data({ 'text/plain': 'a\nb', 'text/uri-list': '# comment\nhttps://example.org/from-list' }) });
    expect(onSource).toHaveBeenCalledWith({ kind: 'text', text: 'a\nb', origin: 'https://example.org/from-list' });
  });

  it('leaves a paste into a text field, and a paste that is not a source, alone', () => {
    const onSource = vi.fn();
    render(<Probe onSource={onSource} />);
    expect(fireEvent.paste(screen.getByLabelText('Note'), { clipboardData: data({ 'text/plain': 'https://example.org/page' }) })).toBe(true);
    expect(fireEvent.paste(screen.getByTestId('root'), { clipboardData: data({ 'text/plain': 'one line' }) })).toBe(true);
    expect(onSource).not.toHaveBeenCalled();
  });

  it('takes a dropped link and clears the drag highlight', () => {
    const onSource = vi.fn();
    render(<Probe onSource={onSource} />);
    const root = screen.getByTestId('root');
    fireEvent.dragOver(root, { dataTransfer: data({}, ['text/uri-list']) });
    expect(screen.getByTestId('dragging')).toHaveTextContent('true');
    fireEvent.drop(root, { dataTransfer: data({ 'text/uri-list': '# dragged\nhttps://example.org/dropped' }) });
    expect(onSource).toHaveBeenCalledWith({ kind: 'link', url: 'https://example.org/dropped' });
    expect(screen.getByTestId('dragging')).toHaveTextContent('false');
  });

  it('takes dropped text, falling back to its plain text', () => {
    const onSource = vi.fn();
    render(<Probe onSource={onSource} />);
    fireEvent.drop(screen.getByTestId('root'), { dataTransfer: data({ 'text/plain': 'Some\npassage' }) });
    expect(onSource).toHaveBeenCalledWith({ kind: 'text', text: 'Some\npassage', origin: null });
  });

  it('ignores a drop that carries no source', () => {
    const onSource = vi.fn();
    render(<Probe onSource={onSource} />);
    expect(fireEvent.drop(screen.getByTestId('root'), { dataTransfer: data({ 'text/plain': 'nothing here' }) })).toBe(true);
    expect(onSource).not.toHaveBeenCalled();
  });

  it('shows the drop target only for things it can take, and hides it when the drag leaves', () => {
    render(<Probe onSource={vi.fn()} />);
    const root = screen.getByTestId('root');
    fireEvent.dragOver(root, { dataTransfer: data({}, ['application/x-other']) });
    expect(screen.getByTestId('dragging')).toHaveTextContent('false');
    fireEvent.dragOver(root, { dataTransfer: data({}, ['Files']) });
    expect(screen.getByTestId('dragging')).toHaveTextContent('true');
    // Moving onto a child is not leaving.
    leaveOnto(root, screen.getByLabelText('Note'));
    expect(screen.getByTestId('dragging')).toHaveTextContent('true');
    leaveOnto(root, null);
    expect(screen.getByTestId('dragging')).toHaveTextContent('false');
  });

  it('also reports a drag the webview’s own file drop saw', () => {
    mocks.fileDrop.dragging = true;
    render(<Probe onSource={vi.fn()} />);
    expect(screen.getByTestId('dragging')).toHaveTextContent('true');
  });

  it('imports a dropped file into the library and hands over its document', async () => {
    mocks.start.mockResolvedValue({ ok: true, data: 'job-1' });
    mocks.poll.mockResolvedValue({ kind: 'finished', added: 1, failed: 0, documentIds: ['doc-7'] });
    const onSource = vi.fn();
    render(<Probe onSource={onSource} />);
    act(() => mocks.fileDrop.paths?.(['C:\\notes\\monarchs.pdf', 'other.pdf']));
    await waitFor(() => expect(onSource).toHaveBeenCalledWith({ kind: 'document', documentId: 'doc-7', name: 'monarchs.pdf' }));
    expect(mocks.start).toHaveBeenCalledWith(['C:\\notes\\monarchs.pdf']);
    expect(screen.getByTestId('importing')).toHaveTextContent('');
  });

  it('shows the file being imported while it runs', async () => {
    mocks.start.mockResolvedValue({ ok: true, data: 'job-1' });
    let finish: (value: unknown) => void = () => undefined;
    mocks.poll.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    render(<Probe onSource={vi.fn()} />);
    act(() => mocks.fileDrop.paths?.(['/tmp/monarchs.pdf']));
    await waitFor(() => expect(screen.getByTestId('importing')).toHaveTextContent('monarchs.pdf'));
    await act(async () => finish({ kind: 'finished', added: 1, failed: 0, documentIds: ['d'] }));
    await waitFor(() => expect(screen.getByTestId('importing')).toHaveTextContent(''));
  });

  it('ignores an empty file drop', () => {
    render(<Probe onSource={vi.fn()} />);
    act(() => mocks.fileDrop.paths?.([]));
    expect(mocks.start).not.toHaveBeenCalled();
  });

  it.each([
    ['the import does not start', () => { mocks.start.mockResolvedValue({ ok: false, error: 'Disk is full.' }); }, 'Disk is full.'],
    ['the job cannot be reached', () => { mocks.start.mockResolvedValue({ ok: true, data: 'j' }); mocks.poll.mockResolvedValue({ kind: 'unreachable', error: 'Lost the import job.' }); }, 'Lost the import job.'],
    ['nothing was imported', () => { mocks.start.mockResolvedValue({ ok: true, data: 'j' }); mocks.poll.mockResolvedValue({ kind: 'finished', added: 0, failed: 1, documentIds: [] }); }, 'monarchs.pdf could not be imported.'],
  ])('shows why a file could not be added when %s, and clears it on request', async (_name, arrange, message) => {
    arrange();
    const onSource = vi.fn();
    render(<Probe onSource={onSource} />);
    act(() => mocks.fileDrop.paths?.(['/tmp/monarchs.pdf']));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(message));
    expect(onSource).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    expect(screen.getByRole('alert')).toBeEmptyDOMElement();
  });

  it('uses a generic message when the failure carries none', async () => {
    mocks.start.mockRejectedValue('boom');
    render(<Probe onSource={vi.fn()} />);
    act(() => mocks.fileDrop.paths?.(['/tmp/a.pdf']));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('The file could not be imported.'));
  });
});
