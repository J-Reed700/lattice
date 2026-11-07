import type { ReactNode } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ErrorBoundary } from './ErrorBoundary';
import { FeatureErrorBoundary } from './FeatureErrorBoundary';
import { RootErrorBoundary } from './RootErrorBoundary';
import { SectionErrorBoundary } from './SectionErrorBoundary';

/**
 * After six caught errors a boundary stops resetting (`reset` only warns). The
 * button the learner pressed to recover then does nothing at all. The fallback
 * must still offer a way out that works: Reload.
 */

function Broken(): never {
  throw new Error('The card failed to draw');
}

beforeEach(() => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

/** Presses the recovery button until the boundary refuses to reset any more. */
function exhaust(buttonName: string) {
  for (let attempt = 0; attempt < 6; attempt += 1) fireEvent.click(screen.getByRole('button', { name: buttonName }));
  expect(console.warn).toHaveBeenCalledWith('ErrorBoundary: Maximum error count reached. Not resetting.');
}

const fallbacks: { name: string; recovery: string; tree: ReactNode }[] = [
  { name: 'the isolated default fallback', recovery: 'Retry', tree: <ErrorBoundary isolate name="Card"><Broken /></ErrorBoundary> },
  { name: 'a section fallback', recovery: 'Try again', tree: <SectionErrorBoundary sectionName="Studio"><Broken /></SectionErrorBoundary> },
  { name: 'a feature fallback', recovery: 'Try again', tree: <FeatureErrorBoundary featureName="Search"><Broken /></FeatureErrorBoundary> },
];

describe('error boundary that has stopped resetting', () => {
  it.each(fallbacks)('offers Reload in $name', ({ recovery, tree }) => {
    render(tree);
    exhaust(recovery);
    expect(screen.getByRole('button', { name: 'Reload' })).toBeEnabled();
  });

  it('keeps Reload on the full-page fallback, which already offered it', () => {
    render(<RootErrorBoundary><Broken /></RootErrorBoundary>);
    exhaust('Try again');
    expect(screen.getByRole('button', { name: 'Reload' })).toBeEnabled();
  });
});
