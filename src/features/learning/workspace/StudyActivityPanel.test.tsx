import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { StudyActivityPanel } from '@/features/learning/workspace/StudyActivityPanel';
import { queryClient } from '@/lib/queryClient';
import { beginStudyActivity, STUDY_ACTIVITY_KEY, type StudyActivity } from '@/lib/studyActivity';


beforeEach(() => { vi.useFakeTimers(); queryClient.clear(); });
afterEach(() => { cleanup(); queryClient.clear(); vi.useRealTimers(); });
const advance = async (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

it('shows honest elapsed progress, task details, and survives collapse and navigation', async () => {
  const open = vi.fn();
  const view = render(<StudyActivityPanel onOpen={open} />);
  const task = beginStudyActivity('learning', 'generate_learning_practical_activity', { request: { programId: 'course' } })!;
  await advance(800);
  await advance(1000);
  expect(screen.getByRole('region', { name: 'Study activity' })).toBeVisible();
  await advance(1000);
  expect(screen.getByText(/Elapsed 0:0[1-3]/)).toBeVisible();
  fireEvent.click(screen.getByText('Task details'));
  expect(screen.getByText(/not live completion indicators/)).toBeVisible();
  expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: /Collapse activity/ }));
  await advance(60_000);
  expect(screen.getByText(/^1:0[0-3]$/)).toBeVisible();
  expect(screen.getByText('Building your practical activity')).not.toBeVisible();
  view.unmount();
  render(<StudyActivityPanel onOpen={open} />);
  expect(screen.getByText(/Elapsed 1:0[0-3]/)).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Open task' }));
  expect(open).toHaveBeenCalledWith(expect.objectContaining({ programId: 'course', tab: 'practical' }));
  act(() => task.finish());
  await advance(10);
  fireEvent.click(screen.getByRole('button', { name: /Expand activity/ }));
  expect(screen.getByText('Request completed')).toBeVisible();
  expect(screen.getByRole('button', { name: 'View result' })).toBeVisible();
});

it('keeps failures available without cancelling another request or silently retrying', async () => {
  render(<StudyActivityPanel onOpen={vi.fn()} />);
  const source = beginStudyActivity('learning', 'refresh_learning_source', { request: { programId: 'course' } })!;
  const model = beginStudyActivity('learning', 'start_learning_diagnostic', { request: { programId: 'course' } })!;
  await advance(1000);
  act(() => source.finish('The reference website did not respond.'));
  await advance(10);
  const card = screen.getByRole('article', { name: 'Checking your reference for updates' });
  expect(within(card).getByRole('alert')).toHaveTextContent('The reference website did not respond.');
  expect(within(card).getByText('The reference website did not respond.')).toBeVisible();
  expect(within(card).getByRole('button', { name: 'Return to task to retry' })).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Clear finished activity' }));
  await advance(10);
  expect(screen.queryByText('Checking your reference for updates')).not.toBeInTheDocument();
  expect(screen.getByText('Preparing your starting-point check')).toBeVisible();
  expect(queryClient.getQueryData<StudyActivity[]>(STUDY_ACTIVITY_KEY)).toMatchObject([{ status: 'pending' }]);
  act(() => model.finish());
  await advance(10);
});
