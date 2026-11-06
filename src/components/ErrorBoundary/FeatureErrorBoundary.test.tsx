import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';

import {
  errorWithoutMessage,
  SANITIZED_MESSAGE,
  SENSITIVE_MESSAGE,
  SENSITIVE_PARTS,
} from '@/utils/__tests__/sensitiveMessage';

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

function Thrower({ error }: { error: Error }): never {
  throw error;
}

it('strips sensitive text from the message', () => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
  const { container } = render(
    <FeatureErrorBoundary featureName="Search"><Thrower error={new Error(SENSITIVE_MESSAGE)} /></FeatureErrorBoundary>
  );

  expect(screen.getByText(SANITIZED_MESSAGE)).toBeInTheDocument();
  for (const part of SENSITIVE_PARTS) {
    expect(container.textContent).not.toContain(part);
  }
});

it.each([errorWithoutMessage(), new Error('')])('prints no message line when there is none', (error) => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
  const { container } = render(
    <FeatureErrorBoundary featureName="Search"><Thrower error={error} /></FeatureErrorBoundary>
  );

  expect(screen.getByText("Couldn't load Search.")).toBeInTheDocument();
  expect(container.querySelectorAll('p')).toHaveLength(1);
});
