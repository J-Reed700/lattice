import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import type {
  FinderStatusDto,
  KeyServiceDto,
  KeyStatusDto,
  SourcesStatusDto,
} from '@/lib/bindings';
import { makeAppSettings } from '@/tests/fixtures/appSettings';

import { SourcesTab } from './SourcesTab';

const SECRET = 'BSA-test-key-0f3c9a7e';

const healthy = {
  state: 'healthy',
  until: null,
  lastError: null,
  calls: 0,
  answered: 0,
  empty: 0,
  failed: 0,
  skipped: 0,
  medianMs: null,
} as const;

function finder(overrides: Partial<FinderStatusDto> & Pick<FinderStatusDto, 'id' | 'name'>): FinderStatusDto {
  return {
    reach: 'openWeb',
    needs: { kind: 'nothing' },
    disclosure: `Your query is sent to ${overrides.name}, which may log it`,
    enabled: true,
    gate: null,
    health: { ...healthy },
    ...overrides,
  };
}

const KEY_NAMES: Record<KeyServiceDto, string> = {
  brave: 'Brave Search',
  tavily: 'Tavily',
  exa: 'Exa',
  github: 'GitHub',
  semanticScholar: 'Semantic Scholar',
  apiDataGov: 'api.data.gov',
  stackExchange: 'Stack Exchange',
};

function keys(stored: KeyServiceDto[] = []): KeyStatusDto[] {
  return (Object.keys(KEY_NAMES) as KeyServiceDto[]).map((service) => ({
    service,
    name: KEY_NAMES[service],
    keyPageUrl: `https://example.org/${service}`,
    stored: stored.includes(service),
  }));
}

function status(storedKeys: KeyServiceDto[] = []): SourcesStatusDto {
  const braveStored = storedKeys.includes('brave');
  return {
    finders: [
      finder({ id: 'duckduckgo', name: 'DuckDuckGo' }),
      finder({
        id: 'brave',
        name: 'Brave Search',
        needs: { kind: 'requiredKey', service: 'brave' },
        disclosure: 'Your query is sent to brave.com under your key',
        gate: braveStored ? null : 'noKey',
      }),
      finder({
        id: 'searxng',
        name: 'SearXNG',
        needs: { kind: 'instanceUrl' },
        disclosure: 'Your query is sent to the instance you set; its operator may log it',
        gate: 'noInstance',
      }),
      finder({
        id: 'github',
        name: 'GitHub',
        reach: 'codeHost',
        needs: { kind: 'optionalKey', service: 'github' },
      }),
      finder({ id: 'wayback_cdx', name: 'Internet Archive (CDX)', reach: 'archive' }),
    ],
    keys: keys(storedKeys),
  };
}

function renderSourcesTab() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<SourcesTab />, { wrapper });
}

describe('SourcesTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(VaultAPI.getSettings).mockResolvedValue({ ok: true, data: makeAppSettings() });
    vi.mocked(VaultAPI.updateSettings).mockResolvedValue({ ok: true, data: makeAppSettings() });
    vi.mocked(VaultAPI.getSourcesStatus).mockResolvedValue({ ok: true, data: status() });
  });

  it('lists finders by group with their privacy line and why they are off', async () => {
    renderSourcesTab();

    expect(await screen.findByRole('heading', { level: 1, name: 'Sources' })).toBeInTheDocument();
    expect(await screen.findByText('Your query is sent to brave.com under your key.')).toBeInTheDocument();
    expect(screen.getByTestId('status-brave')).toHaveTextContent('Needs a Brave Search key');
    expect(screen.getByTestId('status-searxng')).toHaveTextContent('Needs your SearXNG address');
    expect(screen.getByTestId('status-duckduckgo')).toHaveTextContent('Ready');
    expect(screen.getByRole('heading', { name: 'Code hosts' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Internet Archive' })).toBeInTheDocument();
    expect(screen.getByLabelText('Read pages from the Internet Archive')).toBeChecked();
  });

  it('saves a key and shows that one is saved, never the key', async () => {
    const user = userEvent.setup();
    vi.mocked(VaultAPI.saveSourceKey).mockResolvedValue({ ok: true, data: status(['brave']) });
    const { container } = renderSourcesTab();

    const field = await screen.findByLabelText('Brave Search key');
    await user.type(field, SECRET);
    const row = field.closest('div') as HTMLElement;
    await user.click(within(row).getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(VaultAPI.saveSourceKey).toHaveBeenCalledWith('brave', SECRET));
    expect(await within(row).findByText('Key saved')).toBeInTheDocument();
    expect(screen.getByLabelText('Brave Search key')).toHaveValue('');
    expect(container.innerHTML).not.toContain(SECRET);
    await waitFor(() => expect(screen.getByTestId('status-brave')).toHaveTextContent('Ready'));
  });

  it('removes a saved key', async () => {
    const user = userEvent.setup();
    vi.mocked(VaultAPI.getSourcesStatus).mockResolvedValue({ ok: true, data: status(['brave']) });
    vi.mocked(VaultAPI.removeSourceKey).mockResolvedValue({ ok: true, data: status() });
    renderSourcesTab();

    const row = (await screen.findByLabelText('Brave Search key')).closest('div') as HTMLElement;
    expect(within(row).getByText('Key saved')).toBeInTheDocument();
    await user.click(within(row).getByRole('button', { name: 'Remove' }));

    await waitFor(() => expect(VaultAPI.removeSourceKey).toHaveBeenCalledWith('brave'));
    await waitFor(() => expect(within(row).queryByText('Key saved')).not.toBeInTheDocument());
    expect(screen.getByTestId('status-brave')).toHaveTextContent('Needs a Brave Search key');
  });

  it('turns a finder off through the sourcing settings', async () => {
    const user = userEvent.setup();
    renderSourcesTab();

    await user.click(await screen.findByLabelText('Use DuckDuckGo'));

    await waitFor(() =>
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith({
        category: 'sourcing',
        updates: { finders: { duckduckgo: { enabled: false } } },
      })
    );
    await waitFor(() => expect(VaultAPI.getSourcesStatus).toHaveBeenCalledTimes(2));
  });

  it('checks the SearXNG address before saving it', async () => {
    const user = userEvent.setup();
    renderSourcesTab();

    const field = await screen.findByLabelText('Server address');
    await user.type(field, 'search.example.org');
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Enter an address that starts with http:// or https://'
    );
    const save = field.parentElement?.querySelector('button') as HTMLButtonElement;
    expect(save).toBeDisabled();

    await user.clear(field);
    await user.type(field, 'https://search.example.org');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await user.click(save);

    await waitFor(() =>
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith({
        category: 'sourcing',
        updates: { searxngUrl: 'https://search.example.org' },
      })
    );
  });
});
