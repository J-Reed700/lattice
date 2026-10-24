import type { ReactNode } from 'react';

import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { deferred } from '@/tests/deferred';

import { CloudMetaRow } from './CloudMetaRow';

const h = vi.hoisted(() => ({
  settings: undefined as unknown,
  updateSettings: vi.fn(),
  toggleProps: null as null | Record<string, any>,
}));

vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: h.settings }),
  useUpdateSettingsMutation: () => ({ mutateAsync: h.updateSettings }),
}));
vi.mock('../AITab/VisionDeclarationEditor', () => ({ ModelVisionDeclaration: () => <p>Vision declaration</p> }));
vi.mock('./ConnectionRolesDialog', () => ({
  ConnectionRolesDialog: ({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) => (
    <div role="dialog" aria-label={title}>{children}<button type="button" onClick={onClose}>Close roles</button></div>
  ),
}));
vi.mock('./RoleButton', () => ({
  ActiveRoleBadge: ({ label }: { label: string }) => <span>{label} badge</span>,
  RoleToggle: (props: Record<string, any>) => {
    h.toggleProps = props;
    return <button type="button" disabled={props.disabled} onClick={props.onToggle}>{props.active ? 'Stop verifying' : 'Verify with this'}</button>;
  },
}));

const modelRef = { kind: 'cloud', provider: 'anthropic', model: 'claude-opus-5' } as const;

const renderRow = () => render(<TooltipProvider><CloudMetaRow modelRef={modelRef as never} roles={['Chat', 'Learning']} /></TooltipProvider>);

describe('CloudMetaRow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.settings = { llm: { verifierModel: null } };
    h.updateSettings.mockResolvedValue(undefined);
  });

  it('names the connection, its model and roles', () => {
    renderRow();
    expect(screen.getByText('claude-opus-5')).toBeInTheDocument();
    expect(screen.getByText('Chat · Learning')).toBeInTheDocument();
    expect(screen.getByText('Vision declaration')).toBeInTheDocument();
    expect(screen.queryByText('Verifier badge')).not.toBeInTheDocument();
  });

  it('sends the reader to the Chat settings to edit the connection', () => {
    const listener = vi.fn();
    window.addEventListener('settings:navigate-tab', listener);
    renderRow();
    fireEvent.click(screen.getByRole('button', { name: 'Edit connection' }));
    expect((listener.mock.calls[0][0] as CustomEvent).detail).toEqual({ tab: 'chat' });
    window.removeEventListener('settings:navigate-tab', listener);
  });

  it('makes the model the Verifier from its roles dialog', async () => {
    const pending = deferred<void>();
    h.updateSettings.mockReturnValueOnce(pending.promise);
    renderRow();
    fireEvent.click(screen.getByRole('button', { name: 'Configure model' }));
    expect(screen.getByRole('dialog', { name: /connection$/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Verify with this' }));
    expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { verifierModel: modelRef } });
    expect(h.toggleProps?.pending).toBe(true);
    // A second press while saving does nothing.
    fireEvent.click(screen.getByRole('button', { name: 'Verify with this' }));
    expect(h.updateSettings).toHaveBeenCalledTimes(1);
    await act(async () => { pending.resolve(); await pending.promise; });
    expect(h.toggleProps?.pending).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Close roles' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('clears the Verifier when it is already this model', async () => {
    h.settings = { llm: { verifierModel: modelRef } };
    renderRow();
    expect(screen.getByText('Verifier badge')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Configure model' }));
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Stop verifying' })); });
    expect(h.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { verifierModel: null } });
  });

  it('waits for settings before offering the role', () => {
    h.settings = undefined;
    renderRow();
    fireEvent.click(screen.getByRole('button', { name: 'Configure model' }));
    expect(screen.getByRole('button', { name: 'Verify with this' })).toBeDisabled();
  });
});
