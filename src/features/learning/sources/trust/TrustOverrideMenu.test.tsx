import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TrustOverrideMenu } from '@/features/learning/sources/trust/TrustOverrideMenu';
import { entryOf, trustOf } from '@/features/learning/sources/trust/trustTestData';
import type { LearningSourceTrustEntryDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ set: vi.fn(), clear: vi.fn(), get: vi.fn() }));
vi.mock('@/lib/api', () => ({
  default: { setSourceTrustOverride: api.set, clearSourceTrustOverride: api.clear, getLearningSourceTrust: api.get },
}));

const changed = (claims: number) => ({
  ok: true,
  data: {
    change: { scope: { scope: 'site', registrableDomain: 'example-dev.net' }, sourceVersionIds: ['version-1'] },
    impact: { versions: 1, claims, lessons: 1 },
    redecided: { verified: claims, reopened: 0, stillFailing: 0, notLevelled: 0, lessons: claims ? 1 : 0 },
  },
});

function show(entry: LearningSourceTrustEntryDto = entryOf(trustOf('established', { subject: 'OpenAI API' })), subjectOptions: string[] = []) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <TrustOverrideMenu entry={entry} programId="program-1" sourceTitle="A blog post" subjectOptions={subjectOptions} />
    </QueryClientProvider>,
  );
}

describe('Trust override menu', () => {
  beforeEach(() => vi.resetAllMocks());

  it('is reached from the keyboard and says what it will re-decide before applying', async () => {
    show();
    const toggle = screen.getByRole('button', { name: /Change trust/ });
    toggle.focus();
    await userEvent.keyboard('{Enter}');
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('status')).toHaveTextContent('3 checked claims in 2 lessons rest on this source; they will be re-decided.');
    expect(screen.getByRole('button', { name: 'Apply and re-decide 3 claims' })).toBeVisible();
    await userEvent.keyboard('{Escape}');
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(toggle).toHaveFocus();
  });

  it('sets a source override at any level, primary included', async () => {
    api.set.mockResolvedValue(changed(3));
    show();
    await userEvent.click(screen.getByRole('button', { name: /Change trust/ }));
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Level' }), 'primary');
    await userEvent.click(screen.getByRole('button', { name: 'Apply and re-decide 3 claims' }));
    await waitFor(() => expect(api.set).toHaveBeenCalledWith({ scope: 'source', programId: 'program-1', sourceId: 'source-1', level: 'primary', note: null }));
    expect(await screen.findByText(/^Saved\. 3 claims/)).toBeVisible();
  });

  it('gives a site override a standing only, never a level', async () => {
    api.set.mockResolvedValue(changed(5));
    show();
    await userEvent.click(screen.getByRole('button', { name: /Change trust/ }));
    await userEvent.click(screen.getByRole('radio', { name: /example-dev\.net, everywhere/ }));
    expect(screen.queryByRole('combobox', { name: 'Level' })).toBeNull();
    const standing = screen.getByRole('combobox', { name: 'Standing' });
    expect([...(standing as HTMLSelectElement).options].map((option) => option.value)).toEqual(['institutional', 'identified', 'community', 'distrusted']);
    expect(screen.getByText(/A site is never primary by itself/)).toBeVisible();
    await userEvent.selectOptions(standing, 'institutional');
    await userEvent.click(screen.getByRole('button', { name: 'Apply and re-decide 5 claims' }));
    await waitFor(() => expect(api.set).toHaveBeenCalledWith({ scope: 'site', registrableDomain: 'example-dev.net', standing: 'institutional', note: null }));
  });

  it('trusts a site as primary for each chosen subject, one override per class', async () => {
    api.set.mockResolvedValue(changed(5));
    show(entryOf(trustOf('established')), ['OpenAI API', 'OpenAI platform', 'LLM APIs']);
    await userEvent.click(screen.getByRole('button', { name: /Change trust/ }));
    await userEvent.click(screen.getByRole('radio', { name: /example-dev\.net, for a subject/ }));
    await userEvent.click(screen.getByRole('button', { name: /^Apply/ }));
    expect(screen.getByRole('alert')).toHaveTextContent('Choose at least one subject this site is trusted for.');
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Level' }), 'primary');
    await userEvent.click(screen.getByRole('checkbox', { name: 'OpenAI API' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Another subject' }), 'Structured outputs');
    expect(screen.getByText('Trust example-dev.net as primary for: OpenAI API, Structured outputs')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: /^Apply/ }));
    await waitFor(() => expect(api.set).toHaveBeenCalledTimes(2));
    expect(api.set.mock.calls.map(([request]) => request.classLabel)).toEqual(['OpenAI API', 'Structured outputs']);
    expect(api.set.mock.calls.every(([request]) => request.scope === 'site_subject' && request.level === 'primary')).toBe(true);
  });

  it('resets an override in force', async () => {
    api.clear.mockResolvedValue(changed(0));
    show(entryOf(trustOf('primary'), {
      overrides: [{ scope: 'source', programId: 'program-1', sourceId: 'source-1', level: 'primary', note: null, setAt: 1 }],
    }));
    await userEvent.click(screen.getByRole('button', { name: /Change trust/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Reset: This source set to primary for this course' }));
    await waitFor(() => expect(api.clear).toHaveBeenCalledWith({ scope: 'source', programId: 'program-1', sourceId: 'source-1' }));
  });
});
