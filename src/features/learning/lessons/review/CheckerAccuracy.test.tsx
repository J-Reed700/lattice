import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { accuracyLine, CheckerAccuracy } from '@/features/learning/lessons/review/CheckerAccuracy';

const api = vi.hoisted(() => ({ accuracy: vi.fn(), exportExamples: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningCheckerAccuracy: api.accuracy, exportLearningCheckerExamples: api.exportExamples } }));

const row = { checkerModel: 'qwen3.5-9b', labelled: 412, falseFlags: 38, trueFlags: 375, falseFlagRate: 0.092, falsePasses: 1, verifierLabelled: 38, falsePassRate: 0.011, retrievalMisses: 14 };

describe('Checker accuracy', () => {
  beforeEach(() => vi.resetAllMocks());

  it('states the rates per checker model without a verdict', () => {
    expect(accuracyLine(row)).toBe('412 labelled claims · false flags 9.2% · false passes 1.1% (from 38 Verifier audits) · retrieval misses 14');
    expect(accuracyLine({ ...row, falseFlagRate: null, falsePassRate: null })).toBe('412 labelled claims · retrieval misses 14');
  });

  it('loads on open and exports the course’s examples, saying when the dialog was dismissed', async () => {
    api.accuracy.mockResolvedValue({ ok: true, data: [row] });
    api.exportExamples.mockResolvedValueOnce({ ok: true, data: { path: '/tmp/lesson-claims.jsonl', pairs: 12 } }).mockResolvedValueOnce({ ok: true, data: null });
    render(<QueryClientProvider client={new QueryClient()}><CheckerAccuracy programId="program-1" /></QueryClientProvider>);
    expect(api.accuracy).not.toHaveBeenCalled();
    await userEvent.click(screen.getByText('Checker accuracy on your lessons'));
    expect(await screen.findByText(/false flags 9.2%/)).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Export this course’s examples for evaluation' }));
    await waitFor(() => expect(api.exportExamples).toHaveBeenCalledWith({ programId: 'program-1' }));
    expect(await screen.findByText('Saved 12 claim pairs to /tmp/lesson-claims.jsonl.')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Export this course’s examples for evaluation' }));
    expect(await screen.findByText('Export cancelled.')).toBeVisible();
  });
});
