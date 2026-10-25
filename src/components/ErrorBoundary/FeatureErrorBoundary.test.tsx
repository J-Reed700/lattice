import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';

import { FeatureErrorBoundary } from './FeatureErrorBoundary';

let broken = true;
function Feature() {
  if (broken) throw new Error('Load failed');
  return <p>Feature loaded</p>;
}

afterEach(() => vi.restoreAllMocks());

it('preserves the fallback DOM and focus across parent renders, then recovers', () => {
  broken = true;
  vi.spyOn(console, 'error').mockImplementation(() => {});
  const view = render(<FeatureErrorBoundary featureName="Search"><Feature /></FeatureErrorBoundary>);
  const retry = screen.getByRole('button', { name: 'Try again' });
  retry.focus();
  view.rerender(<FeatureErrorBoundary featureName="Library"><Feature /></FeatureErrorBoundary>);
  expect(screen.getByText("Couldn't load Library.")).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Try again' })).toBe(retry);
  expect(retry).toHaveFocus();
  broken = false;
  fireEvent.click(retry);
  expect(screen.getByText('Feature loaded')).toBeInTheDocument();
});
