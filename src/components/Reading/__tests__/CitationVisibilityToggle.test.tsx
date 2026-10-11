import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, expect, it } from 'vitest';

import { CitationVisibilityToggle } from '@/components/Reading/CitationVisibilityToggle';
import { CITATION_DISPLAY_STORAGE_KEY, useCitationDisplayStore } from '@/stores/citationDisplayStore';

beforeEach(() => {
  localStorage.clear();
  useCitationDisplayStore.setState({ visible: true });
});

it('keeps reading controls on separate surfaces synchronized and persists the choice', async () => {
  render(<><CitationVisibilityToggle /><CitationVisibilityToggle compact /></>);
  await userEvent.click(screen.getAllByRole('button', { name: 'Hide citations' })[0]!);
  expect(screen.getAllByRole('button', { name: 'Show citations' })).toHaveLength(2);
  expect(localStorage.getItem(CITATION_DISPLAY_STORAGE_KEY)).toBe('0');
  await userEvent.click(screen.getAllByRole('button', { name: 'Show citations' })[1]!);
  expect(screen.getAllByRole('button', { name: 'Hide citations' })).toHaveLength(2);
  expect(localStorage.getItem(CITATION_DISPLAY_STORAGE_KEY)).toBe('1');
});
