import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SourceCleanup } from '@/features/learning/sources/SourceCleanup';
import type { LearningRemovableSourceDto } from '@/lib/bindings';

const mocks = vi.hoisted(() => ({ remove: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { deleteLearningSource: mocks.remove } }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

const login: LearningRemovableSourceDto = { sourceId: 'source-login', revision: 3, title: 'Sign in', url: 'https://access.example.com/login', reason: 'login page, 65 words' };
const home: LearningRemovableSourceDto = { sourceId: 'source-home', revision: 1, title: 'Welcome', url: 'https://example.org/', reason: 'home page, 126 words' };
const stale: LearningRemovableSourceDto = { sourceId: 'source-stale', revision: 7, title: 'Parsing guide', url: 'https://docs.example.org/parsing', reason: null };

function show(notSources: LearningRemovableSourceDto[], unusedResearch: LearningRemovableSourceDto[]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<SourceCleanup programId="program-1" notSources={notSources} unusedResearch={unusedResearch} />, { wrapper });
}

describe('the library lists what the user may remove', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.remove.mockResolvedValue(ok({ programId: 'program-1', sources: [], notSources: [], unusedResearch: [] }));
  });

  it('shows nothing when no page was refused and no research source is unused', () => {
    const { container } = show([], []);
    expect(container).toBeEmptyDOMElement();
  });

  it('lists the pages the gate refused with the reason for each, apart from the research nothing cites', () => {
    show([login, home], [stale]);

    const refused = screen.getByRole('region', { name: 'Pages that are not sources' });
    expect(within(refused).getByText('Sign in')).toBeVisible();
    expect(within(refused).getByText('login page, 65 words')).toBeVisible();
    expect(within(refused).getByText('home page, 126 words')).toBeVisible();
    expect(within(refused).getByRole('button', { name: 'Remove 2 pages' })).toBeVisible();

    const research = screen.getByRole('region', { name: 'Added by research for a statement that no longer needs it' });
    expect(within(research).getByText('Parsing guide')).toBeVisible();
    expect(within(research).getByText('https://docs.example.org/parsing')).toBeVisible();
    expect(within(research).getByRole('button', { name: 'Remove 1 source' })).toBeVisible();
  });

  it('removes nothing until the user confirms, then every listed source with its revision and a reason of its own', async () => {
    const user = userEvent.setup();
    show([login, home], []);

    await user.click(screen.getByRole('button', { name: 'Remove 2 pages' }));
    expect(mocks.remove).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Keep them' }));
    expect(mocks.remove).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Remove 2 pages' }));
    await user.click(screen.getByRole('button', { name: 'Confirm removal' }));
    await waitFor(() => expect(mocks.remove).toHaveBeenCalledTimes(2));
    const [first, second] = mocks.remove.mock.calls.map(([request]) => request);
    expect(first).toMatchObject({ programId: 'program-1', sourceId: 'source-login', expectedRevision: 3, reason: 'Not a source: login page, 65 words' });
    expect(second).toMatchObject({ programId: 'program-1', sourceId: 'source-home', expectedRevision: 1, reason: 'Not a source: home page, 126 words' });
    expect(first.operationId).toMatch(UUID);
    expect(second.operationId).toMatch(UUID);
    expect(first.operationId).not.toBe(second.operationId);
  });

  it('records why research sources were removed', async () => {
    const user = userEvent.setup();
    show([], [stale]);

    await user.click(screen.getByRole('button', { name: 'Remove 1 source' }));
    await user.click(screen.getByRole('button', { name: 'Confirm removal' }));

    await waitFor(() => expect(mocks.remove).toHaveBeenCalledTimes(1));
    expect(mocks.remove.mock.calls[0][0]).toMatchObject({ sourceId: 'source-stale', expectedRevision: 7, reason: 'Added by research for a statement that no longer needs it' });
  });

  it('stops at the first source it cannot remove and says how many went', async () => {
    const user = userEvent.setup();
    mocks.remove.mockResolvedValueOnce(ok({ programId: 'program-1', sources: [], notSources: [], unusedResearch: [] }));
    mocks.remove.mockResolvedValueOnce(fail('Source changed; reload and retry.'));
    show([login, home], []);

    await user.click(screen.getByRole('button', { name: 'Remove 2 pages' }));
    await user.click(screen.getByRole('button', { name: 'Confirm removal' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('1 of 2 removed. Source changed; reload and retry.');
    expect(mocks.remove).toHaveBeenCalledTimes(2);
  });
});
