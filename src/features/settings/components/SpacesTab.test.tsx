import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { SpacesTab } from './SpacesTab';

const h = vi.hoisted(() => ({
  navigate: vi.fn(),
  setSelectedSpace: vi.fn(),
  editor: { isCreatingSpace: false, createSpace: vi.fn() },
  spaces: [] as Array<Record<string, unknown>>,
}));

vi.mock('react-router', () => ({ useNavigate: () => h.navigate }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (state: unknown) => unknown) => selector({ setSelectedSpace: h.setSelectedSpace }),
}));
vi.mock('@/features/spaces/hooks/useSpaceEditor', () => ({ useSpaceEditor: () => h.editor }));
vi.mock('@/features/spaces/components/SpacePickerPopover', () => ({ useOpenSpaces: () => h.spaces }));

describe('SpacesTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.editor = { isCreatingSpace: false, createSpace: vi.fn().mockResolvedValue(true) };
    h.spaces = [
      { id: 's1', name: 'Astronomy', icon: '🔭', description: 'Stars and planets' },
      { id: 's2', name: 'Pottery', icon: null, description: null },
    ];
  });

  it('creates a space and clears the name', async () => {
    const pending = deferred<boolean>();
    h.editor.createSpace.mockReturnValueOnce(pending.promise);
    render(<SpacesTab />);
    fireEvent.change(screen.getByLabelText('Space name'), { target: { value: 'Geology' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create space' }));
    expect(h.editor.createSpace).toHaveBeenCalledWith('Geology');
    await act(async () => { pending.resolve(true); await pending.promise; });
    expect(screen.getByLabelText('Space name')).toHaveValue('');
  });

  it('keeps the name when the space is refused', async () => {
    h.editor.createSpace.mockResolvedValueOnce(false);
    render(<SpacesTab />);
    fireEvent.change(screen.getByLabelText('Space name'), { target: { value: 'Astronomy' } });
    await act(async () => { fireEvent.submit(screen.getByLabelText('Space name').closest('form')!); });
    expect(screen.getByLabelText('Space name')).toHaveValue('Astronomy');
  });

  it('does not submit twice while creating', async () => {
    h.editor.isCreatingSpace = true;
    render(<SpacesTab />);
    expect(screen.getByRole('button', { name: 'Creating…' })).toBeDisabled();
    await act(async () => { fireEvent.submit(screen.getByLabelText('Space name').closest('form')!); });
    expect(h.editor.createSpace).not.toHaveBeenCalled();
  });

  it('lists open spaces and opens one in Chat', () => {
    render(<SpacesTab />);
    expect(screen.getByText('🔭 Astronomy')).toBeInTheDocument();
    expect(screen.getByText('Stars and planets')).toBeInTheDocument();
    expect(screen.getByText('Pottery')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Open Pottery in Chat' }));
    expect(h.setSelectedSpace).toHaveBeenCalledWith('s2');
    expect(h.navigate).toHaveBeenCalledWith('/chat');
  });

  it('hides the list with no spaces', () => {
    h.spaces = [];
    render(<SpacesTab />);
    expect(screen.queryByText('Your spaces')).not.toBeInTheDocument();
  });
});
