import type { ReactNode } from 'react';

import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { deferred } from '@/tests/deferred';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { AppSettings } from '@/types/api/settings';

import { CloudMetaRow } from './CloudMetaRow';

const h = vi.hoisted(() => ({
  settings: undefined as AppSettings | undefined,
  updateSettings: vi.fn(),
  assignUtilityReference: vi.fn(),
  toast: { error: vi.fn() },
}));

vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: h.settings }),
  useUpdateSettingsMutation: () => ({ mutateAsync: h.updateSettings }),
}));
vi.mock('./ModelRolesContext', () => ({ useModelRoles: () => ({ assignUtilityReference: h.assignUtilityReference }) }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('../AITab/VisionDeclarationEditor', () => ({ ModelVisionDeclaration: () => <p>Vision declaration</p> }));
vi.mock('./LearningWindowNotice', () => ({ LearningWindowNotice: () => null }));
vi.mock('./ConnectionRolesDialog', () => ({
  ConnectionRolesDialog: ({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) => (
    <div role="dialog" aria-label={title}>{children}<button type="button" onClick={onClose}>Close roles</button></div>
  ),
}));

const modelRef = { kind: 'cloud', provider: 'anthropic', model: 'claude-opus-5' } as const;
const roleNames = ['Chat', 'Utility', 'Learning', 'Verifier'];
const renderRow = () => render(<TooltipProvider><CloudMetaRow modelRef={modelRef} roles={['Chat', 'Learning']} /></TooltipProvider>);
const openRoles = () => {
  fireEvent.click(screen.getByRole('button', { name: 'Configure model' }));
  return within(screen.getByRole('dialog', { name: 'Anthropic connection' }));
};

describe('CloudMetaRow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.settings = makeAppSettings();
    h.updateSettings.mockResolvedValue(undefined);
    h.assignUtilityReference.mockResolvedValue(undefined);
  });

  it('names the connection, its model and roles and offers all four connection roles', () => {
    renderRow();
    expect(screen.getByText('claude-opus-5')).toBeInTheDocument();
    expect(screen.getByText('Chat · Learning')).toBeInTheDocument();
    expect(screen.getByText('Vision declaration')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Configure model' })).toHaveAttribute('data-roles', 'chat utility learning verifier');
    const dialog = openRoles();
    for (const name of roleNames) expect(dialog.getByRole('button', { name })).toHaveAttribute('aria-pressed', 'false');
    expect(dialog.queryByRole('button', { name: 'Embedding' })).not.toBeInTheDocument();
    fireEvent.click(dialog.getByRole('button', { name: 'Close roles' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('sends the reader to the Chat settings to edit the connection', () => {
    const listener = vi.fn();
    window.addEventListener('settings:navigate-tab', listener);
    try {
      renderRow();
      fireEvent.click(screen.getByRole('button', { name: 'Edit connection' }));
      expect((listener.mock.calls[0][0] as CustomEvent).detail).toEqual({ tab: 'chat' });
    } finally {
      window.removeEventListener('settings:navigate-tab', listener);
    }
  });

  it('selects the exact cloud model for Chat even when another model from the same provider is selected', async () => {
    h.settings!.llm.provider = 'anthropic';
    h.settings!.llm.model = 'another-model';
    renderRow();
    const dialog = openRoles();
    expect(dialog.getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'false');
    await act(async () => { fireEvent.click(dialog.getByRole('button', { name: 'Chat' })); });
    expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { provider: 'anthropic', model: modelRef.model } });
  });

  it('shows all held roles and keeps the selected Chat role until another model is chosen', () => {
    Object.assign(h.settings!.llm, {
      provider: 'anthropic', model: modelRef.model,
      utilityModel: modelRef, learningModel: modelRef, verifierModel: modelRef,
    });
    renderRow();
    for (const name of roleNames) expect(screen.getByText(name)).toHaveAttribute('data-active-role', name.toLowerCase());
    const dialog = openRoles();
    for (const name of roleNames) expect(dialog.getByRole('button', { name })).toHaveAttribute('aria-pressed', 'true');
    expect(dialog.getByRole('button', { name: 'Chat' })).toBeDisabled();
  });

  it.each(['learningModel', 'verifierModel'] as const)('sets and clears %s independently of Chat', async (key) => {
    const name = key === 'learningModel' ? 'Learning' : 'Verifier';
    const { rerender } = renderRow();
    const dialog = openRoles();
    await act(async () => { fireEvent.click(dialog.getByRole('button', { name })); });
    expect(h.updateSettings).toHaveBeenLastCalledWith({ category: 'llm', updates: { [key]: modelRef } });
    h.settings!.llm[key] = modelRef;
    rerender(<TooltipProvider><CloudMetaRow modelRef={modelRef} roles={[name]} /></TooltipProvider>);
    expect(dialog.getByRole('button', { name })).toHaveAttribute('aria-pressed', 'true');
    await act(async () => { fireEvent.click(dialog.getByRole('button', { name })); });
    expect(h.updateSettings).toHaveBeenLastCalledWith({ category: 'llm', updates: { [key]: null } });
  });

  it.each([false, true])('uses the shared Utility assignment path (already assigned: %s)', async (active) => {
    h.settings!.llm.utilityModel = active ? modelRef : null;
    renderRow();
    const dialog = openRoles();
    await act(async () => { fireEvent.click(dialog.getByRole('button', { name: 'Utility' })); });
    expect(h.assignUtilityReference).toHaveBeenCalledWith(active ? null : modelRef);
    expect(h.updateSettings).not.toHaveBeenCalled();
  });

  it('prevents concurrent role changes while saving and enables them again afterwards', async () => {
    const pending = deferred<void>();
    h.updateSettings.mockReturnValueOnce(pending.promise);
    renderRow();
    const dialog = openRoles();
    fireEvent.click(dialog.getByRole('button', { name: 'Learning' }));
    for (const name of roleNames) expect(dialog.getByRole('button', { name })).toBeDisabled();
    fireEvent.click(dialog.getByRole('button', { name: 'Verifier' }));
    expect(h.updateSettings).toHaveBeenCalledTimes(1);
    await act(async () => { pending.resolve(); await pending.promise; });
    for (const name of roleNames) expect(dialog.getByRole('button', { name })).toBeEnabled();
  });

  it('reports a failed role change without showing it as assigned', async () => {
    h.updateSettings.mockRejectedValueOnce(new Error('Settings could not be saved'));
    renderRow();
    const dialog = openRoles();
    await act(async () => { fireEvent.click(dialog.getByRole('button', { name: 'Learning' })); });
    expect(h.toast.error).toHaveBeenCalledWith('Failed to set learning role', { message: 'Settings could not be saved' });
    expect(dialog.getByRole('button', { name: 'Learning' })).toHaveAttribute('aria-pressed', 'false');
    expect(dialog.getByRole('button', { name: 'Learning' })).toBeEnabled();
  });

  it('waits for settings before offering roles', () => {
    h.settings = undefined;
    renderRow();
    const dialog = openRoles();
    for (const name of roleNames) expect(dialog.getByRole('button', { name })).toBeDisabled();
  });
});
