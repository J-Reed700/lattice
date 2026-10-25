import '@/tests/interactionDom';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CloudConnection } from './CloudConnection';

const api = vi.hoisted(() => ({ listCloudModels: vi.fn(), setCloudApiKey: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const models = [{ id: 'claude-new', name: 'Claude New' }, { id: 'claude-small', name: 'Claude Small' }];
const draw = (model = '') => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const save = vi.fn().mockResolvedValue(true);
  render(<QueryClientProvider client={client}><CloudConnection provider="anthropic" model={model} onSaveModel={save} /></QueryClientProvider>);
  return save;
};

beforeEach(() => {
  api.listCloudModels.mockReset().mockResolvedValue(ok({ hasApiKey: true, models }));
  api.setCloudApiKey.mockReset().mockResolvedValue(ok(undefined));
});

describe('CloudConnection', () => {
  it('loads, searches and saves a returned model without a hardcoded default', async () => {
    const save = draw();
    expect(screen.getByText('Loading available models…')).toBeInTheDocument();
    await screen.findByText('2 available Anthropic models.');
    expect(save).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('combobox', { name: 'Model' }));
    await userEvent.type(screen.getByRole('combobox', { name: 'Search model' }), 'small');
    await userEvent.click(screen.getByRole('option', { name: /Claude Small/ }));
    expect(save).toHaveBeenCalledWith('claude-small');
  });

  it('loads the model list after saving a key', async () => {
    api.listCloudModels.mockResolvedValueOnce(ok({ hasApiKey: false, models: [] }));
    draw();
    await screen.findByText('Save an API key to load available Anthropic models.');
    expect(screen.getByRole('combobox', { name: 'Model' })).toBeDisabled();
    await userEvent.type(screen.getByLabelText('API key'), 'secret');
    await userEvent.click(screen.getByRole('button', { name: 'Save key' }));
    await screen.findByText('2 available Anthropic models.');
    expect(api.setCloudApiKey).toHaveBeenCalledWith('anthropic', 'secret');
    expect(screen.getByLabelText('API key')).toHaveValue('');
  });

  it('reports a rejected key and retries without replacing the saved model', async () => {
    api.listCloudModels.mockResolvedValueOnce({ ok: false, error: 'Anthropic API key was rejected' });
    const save = draw('claude-previous');
    expect(await screen.findByRole('alert')).toHaveTextContent('Anthropic API key was rejected');
    await userEvent.click(screen.getByRole('button', { name: 'Refresh models' }));
    await screen.findByText('2 available Anthropic models.');
    expect(screen.getByRole('combobox', { name: 'Model' })).toHaveTextContent('claude-previous');
    expect(screen.getByText(/saved model was not returned/)).toBeInTheDocument();
    expect(save).not.toHaveBeenCalled();
  });

  it('allows a manual ID if model listing permissions are restricted', async () => {
    const save = draw();
    await screen.findByText('2 available Anthropic models.');
    fireEvent.click(screen.getByRole('button', { name: 'Enter a model ID manually' }));
    fireEvent.change(screen.getByLabelText('Model'), { target: { value: ' custom-model ' } });
    fireEvent.blur(screen.getByLabelText('Model'));
    await waitFor(() => expect(save).toHaveBeenCalledWith('custom-model'));
  });
});
