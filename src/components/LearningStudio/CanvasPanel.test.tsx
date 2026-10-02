import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { JsonValue, LearningCanvasDto, LearningCanvasWorkspaceDto } from '@/lib/bindings';
import { flushPendingSaves } from '@/lib/pendingSaves';

import CanvasPanel from './CanvasPanel';

import type { CanvasSurfaceHandle } from './CanvasSurface';


const api = vi.hoisted(() => ({
  getLearningCanvasWorkspace: vi.fn(), createLearningCanvas: vi.fn(), saveLearningCanvas: vi.fn(),
  createLearningCanvasSnapshot: vi.fn(), restoreLearningCanvasSnapshot: vi.fn(),
  exportScene: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: api }));
vi.mock('@excalidraw/excalidraw', () => ({ serializeAsJSON: (elements: unknown[], _state: unknown, _files: unknown, source: string) => JSON.stringify({ type: 'excalidraw', version: 2, source, elements, appState: {}, files: {} }) }));
vi.mock('./CanvasSurface', async () => {
  const React = await import('react');
  const MockCanvasSurface = React.forwardRef<CanvasSurfaceHandle, { sceneJson: string; onSceneChange: (scene: { sceneJson: string; outline: string[]; elementCount: number }) => void; onSceneError: (message: string) => void }>((props, ref) => {
    const { onSceneChange, sceneJson } = props;
    React.useImperativeHandle(ref, () => ({ exportScene: api.exportScene }));
    React.useEffect(() => {
      const elements = (JSON.parse(sceneJson) as { elements: unknown[] }).elements;
      onSceneChange({ sceneJson, outline: elements.length ? ['A drawn idea'] : [], elementCount: elements.length });
    }, [onSceneChange, sceneJson]);
    return <div data-testid="mock-canvas"><button type="button" onClick={() => props.onSceneChange({ sceneJson: JSON.stringify(scene([textElement])), outline: ['A drawn idea'], elementCount: 1 })}>Add text element</button><button type="button" onClick={() => props.onSceneError('Scene is too large')}>Report scene issue</button></div>;
  });
  MockCanvasSurface.displayName = 'MockCanvasSurface';
  return { default: MockCanvasSurface };
});

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const textElement = { id: 'text-1', type: 'text', isDeleted: false, text: 'A drawn idea' };
const emptyScene = { type: 'excalidraw', version: 2, source: 'test', elements: [], appState: {}, files: {} };
const scene = (elements: object[]) => ({ ...emptyScene, elements });

function canvas(id = 'canvas-1', overrides: Partial<LearningCanvasDto> = {}): LearningCanvasDto {
  return { id, programId: 'program-1', lessonId: 'lesson-1', title: 'First sketch', description: 'A written alternative', sceneJson: emptyScene, elementCount: 0, revision: 1, createdAt: 1_790_000_000_000, updatedAt: 1_790_000_000_000, snapshots: [], ...overrides };
}
function workspace(canvases: LearningCanvasDto[] = []): LearningCanvasWorkspaceDto { return { programId: 'program-1', canvases }; }
function mount(initial = workspace()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  api.getLearningCanvasWorkspace.mockResolvedValue(ok(initial));
  return { ...render(<QueryClientProvider client={client}><CanvasPanel programId="program-1" lessonId="lesson-1" lessonTitle="The central idea" theme="light" enabled /></QueryClientProvider>), client };
}

describe('Learning Studio Canvas workspace', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.exportScene.mockResolvedValue(new Blob(['drawing']));
    api.createLearningCanvas.mockImplementation(async (request: { canvasId: string; title: string; description: string; sceneJson: JsonValue }) => ok(workspace([canvas(request.canvasId, { title: request.title, description: request.description, sceneJson: request.sceneJson })])));
    api.saveLearningCanvas.mockImplementation(async (request: { canvasId: string; expectedRevision: number; title: string; description: string; sceneJson: JsonValue }) => ok(workspace([canvas(request.canvasId, { revision: request.expectedRevision + 1, title: request.title, description: request.description, sceneJson: request.sceneJson, updatedAt: Date.now() })])));
    api.createLearningCanvasSnapshot.mockImplementation(async (request: { canvasId: string; snapshotId: string; name: string }) => ok(workspace([canvas(request.canvasId, { snapshots: [{ id: request.snapshotId, canvasId: request.canvasId, name: request.name, title: 'First sketch', description: '', sceneJson: emptyScene, elementCount: 0, canvasRevision: 1, createdAt: Date.now() }] })])));
    api.restoreLearningCanvasSnapshot.mockImplementation(async (request: { canvasId: string; snapshotId: string }) => ok(workspace([canvas(request.canvasId, { title: 'Restored sketch', revision: 2 })])));
  });

  it('creates a canvas with an accessible written alternative and opens its drawing surface', async () => {
    const user = userEvent.setup();
    mount(workspace());
    await user.click(await screen.findByRole('button', { name: 'Create your first canvas' }));
    await user.type(screen.getByLabelText('Canvas title'), 'A map of the argument');
    await user.type(screen.getByLabelText('Describe your drawing or its meaning'), 'Claims connect to evidence.');
    await user.click(screen.getByRole('button', { name: /^Create$/ }));
    expect(await screen.findByTestId('mock-canvas')).toBeVisible();
    expect(api.createLearningCanvas).toHaveBeenCalledWith(expect.objectContaining({ title: 'A map of the argument', description: 'Claims connect to evidence.', lessonId: 'lesson-1' }));
    expect(screen.getByLabelText('Describe your drawing or its meaning')).toHaveValue('Claims connect to evidence.');
  });

  it('autosaves edits and exposes the visible text outline', async () => {
    const user = userEvent.setup();
    mount(workspace([canvas()]));
    await screen.findByTestId('mock-canvas');
    await user.click(screen.getByRole('button', { name: 'Add text element' }));
    await waitFor(() => expect(api.saveLearningCanvas).toHaveBeenCalledTimes(1), { timeout: 2500 });
    expect(api.saveLearningCanvas).toHaveBeenCalledWith(expect.objectContaining({ canvasId: 'canvas-1', expectedRevision: 1, sceneJson: scene([textElement]) }));
    expect(screen.getByText('A drawn idea')).toBeVisible();
    expect(screen.getByText('Saved')).toBeVisible();
  });

  it('flushes the current canvas before creating and switching to another one', async () => {
    const user = userEvent.setup();
    mount(workspace([canvas()]));
    await screen.findByTestId('mock-canvas');
    fireEvent.change(screen.getByLabelText('Canvas title'), { target: { value: 'First canvas renamed' } });
    await user.click(screen.getByRole('button', { name: 'Create canvas' }));
    const titleInputs = screen.getAllByLabelText('Canvas title');
    await user.type(titleInputs[0], 'New concept map');
    await user.click(screen.getByRole('button', { name: /^Create$/ }));
    await waitFor(() => expect(api.createLearningCanvas).toHaveBeenCalledTimes(1));
    expect(api.saveLearningCanvas).toHaveBeenCalledTimes(1);
    expect(api.saveLearningCanvas.mock.invocationCallOrder[0]).toBeLessThan(api.createLearningCanvas.mock.invocationCallOrder[0]);
    expect(screen.getByLabelText('Canvas title')).toHaveValue('New concept map');
  });

  it('retries a failed save with the same operation and retains the latest content', async () => {
    const user = userEvent.setup();
    api.saveLearningCanvas.mockResolvedValueOnce(fail('Disk is temporarily unavailable'));
    mount(workspace([canvas()]));
    await screen.findByTestId('mock-canvas');
    await user.click(screen.getByRole('button', { name: 'Add text element' }));
    await screen.findByRole('alert');
    const failedRequest = api.saveLearningCanvas.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(api.saveLearningCanvas).toHaveBeenCalledTimes(2));
    expect(api.saveLearningCanvas.mock.calls[1][0]).toEqual(failedRequest);
    expect(api.saveLearningCanvas.mock.calls[1][0].sceneJson).toEqual(scene([textElement]));
  });

  it('does not repeat a successful save when the title has trailing whitespace', async () => {
    mount(workspace([canvas()]));
    await screen.findByTestId('mock-canvas');
    fireEvent.change(screen.getByLabelText('Canvas title'), { target: { value: 'First sketch renamed ' } });
    let result = false;
    await act(async () => { result = await flushPendingSaves(); });
    expect(result).toBe(true);
    expect(api.saveLearningCanvas).toHaveBeenCalledTimes(1);
    expect(api.saveLearningCanvas.mock.calls[0][0].title).toBe('First sketch renamed');
  });

  it('treats an advanced response revision as a conflict and pauses writes until reload', async () => {
    api.saveLearningCanvas.mockResolvedValueOnce(ok(workspace([canvas('canvas-1', { revision: 5, title: 'Server version' })])));
    mount(workspace([canvas()]));
    await screen.findByTestId('mock-canvas');
    fireEvent.change(screen.getByLabelText('Canvas title'), { target: { value: 'My local title' } });
    let result = true;
    await act(async () => { result = await flushPendingSaves(); });
    expect(result).toBe(false);
    expect(screen.getByRole('button', { name: 'Reload latest' })).toBeVisible();
    fireEvent.change(screen.getByLabelText('Describe your drawing or its meaning'), { target: { value: 'New local description' } });
    await act(async () => { result = await flushPendingSaves(); });
    expect(result).toBe(false);
    expect(api.saveLearningCanvas).toHaveBeenCalledTimes(1);
    api.getLearningCanvasWorkspace.mockResolvedValueOnce(ok(workspace([canvas('canvas-1', { revision: 5, title: 'Server version' })])));
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Reload latest' })); });
    await waitFor(() => expect(screen.getByLabelText('Canvas title')).toHaveValue('Server version'));
  });

  it('serializes and drains edits made during a save using the returned revision', async () => {
    let finishFirst!: (result: ReturnType<typeof ok<LearningCanvasWorkspaceDto>>) => void;
    api.saveLearningCanvas.mockImplementationOnce(() => new Promise((resolve) => { finishFirst = resolve; }));
    mount(workspace([canvas()]));
    await screen.findByTestId('mock-canvas');
    fireEvent.change(screen.getByLabelText('Canvas title'), { target: { value: 'Updated title' } });
    const flushing = flushPendingSaves();
    await waitFor(() => expect(api.saveLearningCanvas).toHaveBeenCalledTimes(1));
    fireEvent.change(screen.getByLabelText('Describe your drawing or its meaning'), { target: { value: 'New description during write' } });
    finishFirst(ok(workspace([canvas('canvas-1', { revision: 2, title: 'Updated title' })])));
    expect(await flushing).toBe(true);
    await waitFor(() => expect(api.saveLearningCanvas).toHaveBeenCalledTimes(2));
    expect(api.saveLearningCanvas.mock.calls[1][0].expectedRevision).toBe(2);
    expect(api.saveLearningCanvas.mock.calls[1][0].description).toBe('New description during write');
    expect(api.saveLearningCanvas.mock.calls[1][0].operationId).not.toBe(api.saveLearningCanvas.mock.calls[0][0].operationId);
  });

  it('does not let an in-flight save flush navigation after a newer scene error', async () => {
    let finishSave!: (result: ReturnType<typeof ok<LearningCanvasWorkspaceDto>>) => void;
    api.saveLearningCanvas.mockImplementationOnce(() => new Promise((resolve) => { finishSave = resolve; }));
    mount(workspace([canvas()]));
    await screen.findByTestId('mock-canvas');
    fireEvent.change(screen.getByLabelText('Canvas title'), { target: { value: 'New title' } });
    const flushing = flushPendingSaves();
    await waitFor(() => expect(api.saveLearningCanvas).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole('button', { name: 'Report scene issue' }));
    finishSave(ok(workspace([canvas('canvas-1', { revision: 2, title: 'New title' })])));
    expect(await flushing).toBe(false);
    expect(screen.getByRole('alert')).toHaveTextContent('Scene is too large');
  });

  it('flushes before switching canvases and keeps the source editor selected on save failure', async () => {
    const user = userEvent.setup();
    api.saveLearningCanvas.mockResolvedValueOnce(fail('revision conflict'));
    mount(workspace([canvas(), canvas('canvas-2', { title: 'Second sketch' })]));
    await screen.findByTestId('mock-canvas');
    fireEvent.change(screen.getByLabelText('Canvas title'), { target: { value: 'Unsaved title' } });
    await user.click(screen.getByRole('button', { name: 'Canvas: Second sketch' }));
    expect(await screen.findByRole('button', { name: 'Reload latest' })).toBeVisible();
    expect(screen.getByLabelText('Canvas title')).toHaveValue('Unsaved title');
    expect(screen.getByRole('button', { name: 'Canvas: First sketch' })).toHaveAttribute('aria-pressed', 'true');
    await user.click(screen.getByRole('button', { name: 'Reload latest' }));
  });

  it('creates checkpoints and warns before restoring an immutable version', async () => {
    const user = userEvent.setup();
    const existing = canvas('canvas-1', { snapshots: [{ id: 'snapshot-1', canvasId: 'canvas-1', name: 'Before changes', title: 'First sketch', description: '', sceneJson: emptyScene, elementCount: 0, canvasRevision: 1, createdAt: 1_790_000_000_000 }] });
    mount(workspace([existing]));
    await screen.findByTestId('mock-canvas');
    await user.type(screen.getByLabelText('Checkpoint name'), 'Working version');
    await user.click(screen.getByRole('button', { name: 'Create checkpoint' }));
    await waitFor(() => expect(api.createLearningCanvasSnapshot).toHaveBeenCalledWith(expect.objectContaining({ name: 'Working version', expectedRevision: 1 })));
    await user.click(screen.getByRole('button', { name: 'Restore checkpoint Working version' }));
    expect(screen.getByRole('alertdialog')).toHaveTextContent('A checkpoint of the current version will be created first');
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Restore checkpoint Working version' }));
    await user.click(screen.getByRole('button', { name: /^Restore checkpoint$/ }));
    await waitFor(() => expect(api.restoreLearningCanvasSnapshot).toHaveBeenCalledWith(expect.objectContaining({ snapshotId: api.createLearningCanvasSnapshot.mock.calls[0][0].snapshotId, preRestoreSnapshotId: expect.any(String) })));
  });

  it('offers exports only for nonempty scenes and reports export errors accessibly', async () => {
    const user = userEvent.setup();
    mount(workspace([canvas()]));
    await screen.findByTestId('mock-canvas');
    expect(screen.getByRole('button', { name: '.excalidraw' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Add text element' }));
    await waitFor(() => expect(screen.getByRole('button', { name: '.excalidraw' })).toBeEnabled());
    api.exportScene.mockRejectedValueOnce(new Error('Export is unavailable'));
    await user.click(screen.getByRole('button', { name: '.excalidraw' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Export is unavailable');
  });

  it('exposes a keyboard-operable outline disclosure', async () => {
    const user = userEvent.setup();
    mount(workspace([canvas()]));
    await screen.findByTestId('mock-canvas');
    const disclosure = screen.getByRole('button', { name: 'Canvas element outline' });
    expect(disclosure).toHaveAttribute('aria-expanded', 'true');
    await user.click(disclosure);
    expect(disclosure).toHaveAttribute('aria-expanded', 'false');
  });
});
