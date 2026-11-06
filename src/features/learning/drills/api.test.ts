import { beforeEach, describe, expect, it, vi } from 'vitest';

import { drillsApi } from './api';

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: ipc.invoke }));

const sent = () => ipc.invoke.mock.calls.map(([route, args]) => ({ route: route as string, args: args as Record<string, unknown> }));

beforeEach(() => {
  ipc.invoke.mockReset();
});

describe('drillsApi', () => {
  it('asks the learning plugin for the languages and returns them', async () => {
    ipc.invoke.mockResolvedValueOnce(['python', 'csharp']);
    await expect(drillsApi.codeReviewLanguages()).resolves.toEqual(['python', 'csharp']);
    expect(sent()).toEqual([{ route: 'plugin:learning|code_review_languages', args: {} }]);
  });

  it('starts a drill with the language, the mode, the size and the number of patterns', async () => {
    const started = { status: 'building', jobId: 'job-1' };
    ipc.invoke.mockResolvedValueOnce(started);
    await expect(drillsApi.startCodeReviewDrill('python', 'interview', 80, 4)).resolves.toEqual(started);
    expect(sent()).toEqual([
      { route: 'plugin:learning|start_code_review_drill', args: { language: 'python', mode: 'interview', lines: 80, patternsCount: 4 } },
    ]);
  });

  it('hands in the findings, the fix files (or none) and the time spent, by the names the command takes', async () => {
    ipc.invoke.mockResolvedValue({ status: 'submitted' });
    const findings = [{ span: null, text: 'No timeout.', severity: null }];
    await drillsApi.submitCodeReview('attempt-1', findings, null, 61_000);
    await drillsApi.submitCodeReview('attempt-1', findings, [{ path: 'a.py', text: 'a = 2\n' }], 0);
    expect(sent()).toEqual([
      { route: 'plugin:learning|submit_code_review', args: { attemptId: 'attempt-1', findings, fixFiles: null, timeSpentMs: 61_000 } },
      { route: 'plugin:learning|submit_code_review', args: { attemptId: 'attempt-1', findings, fixFiles: [{ path: 'a.py', text: 'a = 2\n' }], timeSpentMs: 0 } },
    ]);
  });

  it('reads and regrades an attempt by its id', async () => {
    ipc.invoke.mockResolvedValue({ status: 'graded' });
    await drillsApi.codeReviewResult('attempt-1');
    await drillsApi.regradeCodeReview('attempt-1');
    expect(sent()).toEqual([
      { route: 'plugin:learning|code_review_result', args: { attemptId: 'attempt-1' } },
      { route: 'plugin:learning|regrade_code_review', args: { attemptId: 'attempt-1' } },
    ]);
  });

  it('throws what the backend said when a command fails', async () => {
    ipc.invoke.mockRejectedValueOnce({ code: 'INVALID_INPUT', message: 'klingon is not a language drills are written in.' });
    await expect(drillsApi.startCodeReviewDrill('klingon', 'practice', 80, 4)).rejects.toThrow('klingon is not a language drills are written in.');
    ipc.invoke.mockRejectedValueOnce('This review is not there.');
    await expect(drillsApi.codeReviewResult('gone')).rejects.toThrow('This review is not there.');
  });
});
