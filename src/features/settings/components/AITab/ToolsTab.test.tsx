import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { ToolsTab } from './ToolsTab';

const h = vi.hoisted(() => ({
  llm: { llmSettings: null as null | Record<string, unknown>, isLoading: false, saveLlmUpdates: vi.fn() },
  probe: vi.fn(),
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock('./useLlmSettings', () => ({ useLlmSettings: () => h.llm }));
vi.mock('@/features/settings/hooks/connectionProbes', () => ({ probeCustomTool: h.probe }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@tauri-apps/plugin-fs', () => ({ readTextFile: vi.fn(), writeTextFile: vi.fn() }));

const tidesTool = {
  enabled: true,
  name: 'tide_tables',
  description: 'Look up tide times',
  endpoint: 'https://tides.example.org/search',
  queryParam: 'q',
  maxResultsParam: 'limit',
  defaultMaxResults: 5,
};

const withTools = (customTools: unknown[] = [tidesTool]) => {
  h.llm.llmSettings = { customTools };
};

describe('ToolsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.llm.isLoading = false;
    h.llm.saveLlmUpdates.mockResolvedValue(true);
    withTools();
  });

  it('waits for settings', () => {
    h.llm.llmSettings = null;
    render(<ToolsTab />);
    expect(screen.getByText('Loading…')).toBeInTheDocument();
    expect(screen.queryByText('Built in')).not.toBeInTheDocument();
  });

  it('lists the built-in tools as always on', () => {
    render(<ToolsTab />);
    for (const name of ['web_search', 'fetch_url_content', 'wiki_search', 'wiki_summary']) {
      expect(screen.getByText(name)).toBeInTheDocument();
    }
    expect(screen.getAllByText('Always on')).toHaveLength(4);
  });

  it('shows no custom tools as an empty state', () => {
    withTools([]);
    h.llm.llmSettings = { customTools: undefined };
    render(<ToolsTab />);
    expect(screen.getByText('No custom tools yet.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Export' })).toBeDisabled();
    expect(screen.queryByLabelText('Test query')).not.toBeInTheDocument();
  });

  describe('presets', () => {
    it('describes the chosen preset', () => {
      render(<ToolsTab />);
      expect(screen.getByText('https://your-searxng-domain.example/search?format=json')).toBeInTheDocument();
      expect(screen.getByText('none')).toBeInTheDocument();
      fireEvent.change(screen.getByLabelText('Preset'), { target: { value: 'openalex_search' } });
      expect(screen.getByText('https://api.openalex.org/works')).toBeInTheDocument();
      expect(screen.getByText('per-page')).toBeInTheDocument();
      expect(screen.getByRole('link', { name: 'Open docs' })).toHaveAttribute('href', 'https://docs.openalex.org/');
    });

    it('adds a preset as a tool once', () => {
      render(<ToolsTab />);
      fireEvent.change(screen.getByLabelText('Preset'), { target: { value: 'openalex_search' } });
      fireEvent.click(screen.getByRole('button', { name: 'Add as tool' }));
      expect(screen.getByText('openalex_search')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Add as tool' }));
      expect(h.toast.info).toHaveBeenCalledWith('"openalex_search" is already in your tools');
      expect(screen.getAllByText('openalex_search')).toHaveLength(1);
    });

    it('ignores a preset id it does not know', () => {
      render(<ToolsTab />);
      const select = screen.getByLabelText('Preset');
      // Force a value outside the options, as a stale saved choice would.
      fireEvent.change(select, { target: { value: 'gone' } });
      fireEvent.click(screen.getByRole('button', { name: 'Add as tool' }));
      expect(screen.getAllByRole('button', { name: /^Remove / })).toHaveLength(1);
    });
  });

  describe('editing', () => {
    it('adds a blank tool and edits every field', () => {
      withTools([]);
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Add tool' }));
      expect(screen.getByText('Untitled tool')).toBeInTheDocument();
      fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'bird_calls' } });
      fireEvent.change(screen.getByLabelText('Query param'), { target: { value: 'species' } });
      fireEvent.change(screen.getByLabelText('Description'), { target: { value: 'Find bird calls' } });
      fireEvent.change(screen.getByLabelText('Endpoint'), { target: { value: 'https://birds.example.org/api' } });
      fireEvent.change(screen.getByLabelText('Max results param'), { target: { value: 'n' } });
      fireEvent.change(screen.getByLabelText('Default max results'), { target: { value: '12' } });
      fireEvent.change(screen.getByLabelText('Default max results'), { target: { value: '' } });
      expect(screen.getByText('bird_calls')).toBeInTheDocument();
      expect(screen.getByLabelText('Endpoint')).toHaveValue('https://birds.example.org/api');
      expect(screen.getByRole('button', { name: 'Save tools' })).toBeEnabled();
    });

    it('switches a tool off and removes it', () => {
      render(<ToolsTab />);
      const toggle = screen.getByRole('switch', { name: 'Enable tide_tables' });
      expect(toggle).toBeChecked();
      fireEvent.click(toggle);
      expect(screen.getByRole('switch', { name: 'Enable tide_tables' })).not.toBeChecked();
      fireEvent.click(screen.getByRole('button', { name: 'Remove tide_tables' }));
      expect(screen.getByText('No custom tools yet.')).toBeInTheDocument();
    });

    it('labels controls generically for an unnamed tool', () => {
      withTools([{ ...tidesTool, name: '' }]);
      render(<ToolsTab />);
      expect(screen.getByRole('switch', { name: 'Enable this tool' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Remove this tool' })).toBeInTheDocument();
    });

    it('discards unsaved edits', () => {
      render(<ToolsTab />);
      expect(screen.getByRole('button', { name: 'Discard changes' })).toBeDisabled();
      fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'tides_v2' } });
      fireEvent.click(screen.getByRole('button', { name: 'Discard changes' }));
      expect(screen.getByLabelText('Name')).toHaveValue('tide_tables');
      expect(screen.getByRole('button', { name: 'Save tools' })).toBeDisabled();
    });
  });

  describe('saving', () => {
    it('saves normalized tools and reports it', async () => {
      const pending = deferred<boolean>();
      h.llm.saveLlmUpdates.mockReturnValueOnce(pending.promise);
      render(<ToolsTab />);
      fireEvent.change(screen.getByLabelText('Name'), { target: { value: '  tides_v2  ' } });
      fireEvent.change(screen.getByLabelText('Default max results'), { target: { value: '500' } });
      fireEvent.click(screen.getByRole('button', { name: 'Save tools' }));
      expect(screen.getByRole('button', { name: 'Saving…' })).toBeDisabled();
      expect(h.llm.saveLlmUpdates).toHaveBeenCalledWith({
        customTools: [{ ...tidesTool, name: 'tides_v2', defaultMaxResults: 100 }],
      });
      await act(async () => { pending.resolve(true); await pending.promise; });
      expect(h.toast.success).toHaveBeenCalledWith('Tools saved');
      expect(screen.getByLabelText('Name')).toHaveValue('tides_v2');
    });

    it('keeps the draft when the save is refused', async () => {
      h.llm.saveLlmUpdates.mockResolvedValueOnce(false);
      render(<ToolsTab />);
      fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'x ' } });
      fireEvent.click(screen.getByRole('button', { name: 'Save tools' }));
      await waitFor(() => expect(screen.getByRole('button', { name: 'Save tools' })).toBeEnabled());
      expect(h.toast.success).not.toHaveBeenCalled();
      expect(screen.getByLabelText('Name')).toHaveValue('x ');
    });
  });

  describe('testing a tool', () => {
    it('probes the endpoint with the test query and reports the response', async () => {
      const pending = deferred<unknown>();
      h.probe.mockReturnValueOnce(pending.promise);
      render(<ToolsTab />);
      fireEvent.change(screen.getByLabelText('Test query'), { target: { value: '  spring tides  ' } });
      fireEvent.click(screen.getByRole('button', { name: 'Test' }));
      expect(screen.getByRole('button', { name: 'Testing…' })).toBeDisabled();
      expect(h.probe).toHaveBeenCalledWith({
        endpoint: 'https://tides.example.org/search', queryParam: 'q', maxResultsParam: 'limit',
        defaultMaxResults: 5, query: 'spring tides', maxResults: 5,
      });
      await act(async () => { pending.resolve({ ok: true, data: { status: 200, finalUrl: 'https://tides.example.org/search?q=spring' } }); await pending.promise; });
      expect(h.toast.success).toHaveBeenCalledWith('Tool responded (HTTP 200)', { message: 'https://tides.example.org/search?q=spring' });
      expect(screen.getByRole('button', { name: 'Test' })).toBeEnabled();
    });

    it('falls back to a default query and reports a failed probe', async () => {
      h.probe.mockResolvedValueOnce({ ok: false, error: 'HTTP 404' });
      render(<ToolsTab />);
      fireEvent.change(screen.getByLabelText('Test query'), { target: { value: '   ' } });
      fireEvent.click(screen.getByRole('button', { name: 'Test' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("The tool didn't respond", { message: 'HTTP 404' }));
      expect(h.probe).toHaveBeenCalledWith(expect.objectContaining({ query: 'test' }));
    });

    it('needs an endpoint and a query param first', () => {
      withTools([{ ...tidesTool, endpoint: ' ' }, { ...tidesTool, name: 'second', queryParam: '' }]);
      render(<ToolsTab />);
      const tests = screen.getAllByRole('button', { name: 'Test' });
      fireEvent.click(tests[0]);
      expect(h.toast.error).toHaveBeenCalledWith('Add an endpoint first');
      fireEvent.click(tests[1]);
      expect(h.toast.error).toHaveBeenCalledWith('Add a query param first');
      expect(h.probe).not.toHaveBeenCalled();
    });
  });

  describe('import', () => {
    it('imports an array, reading snake_case fields and defaults', async () => {
      vi.mocked(open).mockResolvedValueOnce('/tools.json');
      vi.mocked(readTextFile).mockResolvedValueOnce(JSON.stringify([
        { name: 'star_atlas', endpoint: 'https://stars.example.org', query_param: 'term', max_results_param: 'rows', default_max_results: 7, enabled: false },
        { name: 'plain', endpoint: 'https://plain.example.org' },
        null,
        'not a tool',
      ]));
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Import' }));
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Imported 2 tool(s)'));
      expect(open).toHaveBeenCalledWith({ multiple: false, filters: [{ name: 'JSON', extensions: ['json'] }] });
      const names = screen.getAllByLabelText('Name').map((input) => (input as HTMLInputElement).value);
      expect(names).toEqual(['star_atlas', 'plain']);
      expect(screen.getAllByLabelText('Query param').map((input) => (input as HTMLInputElement).value)).toEqual(['term', 'q']);
      expect(screen.getAllByLabelText('Max results param').map((input) => (input as HTMLInputElement).value)).toEqual(['rows', '']);
      expect(screen.getAllByLabelText('Default max results').map((input) => (input as HTMLInputElement).value)).toEqual(['7', '5']);
      expect(screen.getByRole('switch', { name: 'Enable star_atlas' })).not.toBeChecked();
    });

    it('imports an export file with camelCase fields', async () => {
      vi.mocked(open).mockResolvedValueOnce('/tools.json');
      vi.mocked(readTextFile).mockResolvedValueOnce(JSON.stringify({
        version: 1,
        customTools: [{ name: 'herbarium', description: 'Plants', endpoint: 'https://h.example.org', queryParam: 'name', maxResultsParam: 'max', defaultMaxResults: 3, enabled: true }],
      }));
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Import' }));
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Imported 1 tool(s)'));
      expect(screen.getByLabelText('Query param')).toHaveValue('name');
      expect(screen.getByLabelText('Description')).toHaveValue('Plants');
    });

    it('does nothing when the picker is cancelled', async () => {
      vi.mocked(open).mockResolvedValueOnce(null);
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Import' }));
      await waitFor(() => expect(screen.getByRole('button', { name: 'Import' })).toBeEnabled());
      expect(readTextFile).not.toHaveBeenCalled();
    });

    it('rejects a file of the wrong shape', async () => {
      vi.mocked(open).mockResolvedValueOnce('/tools.json');
      vi.mocked(readTextFile).mockResolvedValueOnce('{"tools": []}');
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Import' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't read that file", { message: 'Expected an array or { customTools: [...] }' }));
    });

    it('rejects a file with no usable tools', async () => {
      vi.mocked(open).mockResolvedValueOnce('/tools.json');
      vi.mocked(readTextFile).mockResolvedValueOnce('[1, 2]');
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Import' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith('No valid tools in that file'));
    });

    it('reports unreadable JSON', async () => {
      vi.mocked(open).mockResolvedValueOnce('/tools.json');
      vi.mocked(readTextFile).mockResolvedValueOnce('{oops');
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Import' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't import tools", expect.objectContaining({ message: expect.stringContaining('SyntaxError') })));
    });

    it('shows progress while importing', async () => {
      const picked = deferred<string | null>();
      vi.mocked(open).mockReturnValueOnce(picked.promise as never);
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Import' }));
      expect(screen.getByRole('button', { name: 'Importing…' })).toBeDisabled();
      await act(async () => { picked.resolve(null); await picked.promise; });
      expect(screen.getByRole('button', { name: 'Import' })).toBeEnabled();
    });
  });

  describe('export', () => {
    it('writes the tools to the chosen file', async () => {
      vi.mocked(save).mockResolvedValueOnce('/backup/tools.json');
      vi.mocked(writeTextFile).mockResolvedValueOnce(undefined);
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Export' }));
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Tools exported'));
      expect(save).toHaveBeenCalledWith({ defaultPath: 'lattice-custom-tools.json', filters: [{ name: 'JSON', extensions: ['json'] }] });
      expect(writeTextFile).toHaveBeenCalledWith('/backup/tools.json', JSON.stringify({ version: 1, customTools: [tidesTool] }, null, 2));
    });

    it('does nothing when the save dialog is cancelled', async () => {
      vi.mocked(save).mockResolvedValueOnce(null);
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Export' }));
      await waitFor(() => expect(screen.getByRole('button', { name: 'Export' })).toBeEnabled());
      expect(writeTextFile).not.toHaveBeenCalled();
    });

    it('reports a failed write', async () => {
      vi.mocked(save).mockResolvedValueOnce('/readonly/tools.json');
      vi.mocked(writeTextFile).mockRejectedValueOnce('permission denied');
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Export' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't export tools", { message: 'permission denied' }));
    });

    it('shows progress while exporting', async () => {
      const picked = deferred<string | null>();
      vi.mocked(save).mockReturnValueOnce(picked.promise as never);
      render(<ToolsTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Export' }));
      expect(screen.getByRole('button', { name: 'Exporting…' })).toBeDisabled();
      await act(async () => { picked.resolve(null); await picked.promise; });
    });
  });
});
