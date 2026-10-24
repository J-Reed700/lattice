import { beforeEach, describe, expect, it, vi } from 'vitest';

import { COMMAND_PLUGINS } from '@/shared/ipc/routes.generated';

import { learningApi } from './client';

type Listener = ((message: unknown) => void) | undefined;

const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as { onmessage: Listener }[] }));

vi.mock('@tauri-apps/api/core', () => {
  class Channel {
    onmessage: Listener;
    constructor(onmessage?: Listener) {
      this.onmessage = onmessage;
      ipc.channels.push(this);
    }
  }
  return { invoke: ipc.invoke, Channel };
});

const sent = () => ipc.invoke.mock.calls.map(([route, args]) => ({ route: route as string, args: args as Record<string, unknown> }));
const lastArgs = () => sent().at(-1)!.args;

beforeEach(() => {
  ipc.invoke.mockReset().mockResolvedValue({ ok: 'payload' });
  ipc.channels.length = 0;
});

describe('learningApi', () => {
  it('sends every method to a registered command through the plugin that owns it', async () => {
    const methods = Object.entries(learningApi) as [string, (...args: unknown[]) => Promise<unknown>][];
    expect(methods.length).toBeGreaterThan(100);
    for (const [name, call] of methods) {
      ipc.invoke.mockClear();
      await call('arg-1', 'arg-2', 'arg-3', 'arg-4');
      expect(ipc.invoke, name).toHaveBeenCalledTimes(1);
      const [route] = ipc.invoke.mock.calls[0] as [string];
      const [, plugin, command] = /^plugin:([^|]+)\|(.+)$/.exec(route) ?? [];
      expect(COMMAND_PLUGINS[command as keyof typeof COMMAND_PLUGINS], `${name} → ${route}`).toBe(plugin);
    }
  });

  it('wraps a reply as ok data and a rejection as an error result', async () => {
    ipc.invoke.mockResolvedValueOnce([{ id: 'course-1' }]);
    await expect(learningApi.listLearningPrograms()).resolves.toEqual({ ok: true, data: [{ id: 'course-1' }] });

    ipc.invoke.mockRejectedValueOnce(new Error('course not found'));
    const failed = await learningApi.getLearningPlan('missing');
    expect(failed.ok).toBe(false);
    expect(failed.ok ? '' : failed.error).toContain('course not found');
    expect(sent().at(-1)).toEqual({ route: 'plugin:learning|get_learning_plan', args: { id: 'missing' } });
  });

  it('starts an outline without a request id when no progress options are given', async () => {
    const request = { title: 'Roman history' } as never;
    await learningApi.generateLearningProgram(request);
    expect(lastArgs()).toMatchObject({ request, requestId: null });
    expect(ipc.channels[0].onmessage).toBeUndefined();
  });

  it('streams outline progress to the caller and names the request so it can be cancelled', async () => {
    const onProgress = vi.fn();
    await learningApi.generateLearningProgram({ title: 'Cell biology' } as never, { requestId: 'outline-7', onProgress });
    expect(lastArgs()).toMatchObject({ requestId: 'outline-7', onProgress: ipc.channels[0] });
    ipc.channels[0].onmessage?.({ stage: 'drafting' });
    expect(onProgress).toHaveBeenCalledWith({ stage: 'drafting' });

    await learningApi.cancelLearningOutline('outline-7');
    expect(sent().at(-1)).toEqual({ route: 'plugin:learning|cancel_learning_outline', args: { requestId: 'outline-7' } });
  });

  it('streams repair progress through its own channel', async () => {
    const onProgress = vi.fn();
    await learningApi.repairLearningOutline({ programId: 'course-1' } as never, { requestId: 'repair-1', onProgress });
    expect(sent().at(-1)?.route).toBe('plugin:learning|repair_learning_outline');
    expect(lastArgs()).toMatchObject({ request: { programId: 'course-1' }, requestId: 'repair-1' });
    ipc.channels[0].onmessage?.({ stage: 'checking' });
    expect(onProgress).toHaveBeenCalledWith({ stage: 'checking' });
  });

  it.each([
    ['addLearningWebSource', 'add_learning_web_source'],
    ['addLearningDocumentSource', 'add_learning_document_source'],
    ['addLearningTextSource', 'add_learning_text_source'],
  ] as const)('%s does not check waiting claims unless asked', async (method, command) => {
    await learningApi[method]({ programId: 'course-1' } as never);
    expect(sent().at(-1)?.route).toBe(`plugin:learning|${command}`);
    expect(lastArgs()).toMatchObject({ request: { programId: 'course-1' }, checkWaiting: false });
    // The default reporter swallows progress rather than throwing.
    expect(() => ipc.channels[0].onmessage?.({ stage: 'saved' })).not.toThrow();
  });

  it('checks waiting claims against a new source and reports each stage', async () => {
    const onProgress = vi.fn();
    await learningApi.addLearningWebSource({ programId: 'course-1', url: 'https://example.org/bread' } as never, { checkWaiting: true, onProgress });
    expect(lastArgs()).toMatchObject({ checkWaiting: true, onProgress: ipc.channels[0] });
    ipc.channels[0].onmessage?.({ stage: 'checking', done: 1, total: 3 });
    expect(onProgress).toHaveBeenCalledWith({ stage: 'checking', done: 1, total: 3 });
  });

  it('reports claim-evidence progress through a channel', async () => {
    const onProgress = vi.fn();
    await learningApi.attachLearningClaimEvidence({ programId: 'course-1', lessonId: 'lesson-2' } as never, onProgress);
    expect(sent().at(-1)?.route).toBe('plugin:learning|attach_learning_claim_evidence');
    ipc.channels[0].onmessage?.({ stage: 'saving' });
    expect(onProgress).toHaveBeenCalledWith({ stage: 'saving' });
  });

  it('returns tangents directly and throws the backend message on failure', async () => {
    ipc.invoke.mockResolvedValueOnce({ id: 'tangent-1' });
    await expect(learningApi.createLearningTangent({ programId: 'course-1' } as never)).resolves.toEqual({ id: 'tangent-1' });
    expect(lastArgs()).toEqual({ request: { programId: 'course-1' } });

    ipc.invoke.mockRejectedValueOnce(new Error('tangent store unavailable'));
    await expect(learningApi.listLearningTangents('course-1')).rejects.toThrow('tangent store unavailable');
    expect(lastArgs()).toEqual({ request: { programId: 'course-1' } });
  });

  it('lists tangents across every course when no course is named', async () => {
    ipc.invoke.mockResolvedValueOnce([]);
    await expect(learningApi.listLearningTangents()).resolves.toEqual([]);
    expect(lastArgs()).toEqual({ request: { programId: null } });
  });
});
