import type { ComponentProps } from 'react';

import { confirm } from '@tauri-apps/plugin-dialog';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import { JournalPickerMenu } from '@/features/journal/components/JournalPickerMenu';
import type { ConversationJournalDto } from '@/types/api/conversation';

vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: vi.fn() }));

const garden = { id: 'garden', name: 'Garden log', icon: '🌱', accentColor: '#3a7' } as ConversationJournalDto;
const history = { id: 'history', name: 'History reading', icon: '' } as ConversationJournalDto;

function renderMenu(overrides: Partial<ComponentProps<typeof JournalPickerMenu>> = {}) {
  const props: ComponentProps<typeof JournalPickerMenu> = {
    journals: [garden, history], currentJournal: garden, onSwitch: vi.fn(), onCreate: vi.fn(),
    onRenameCurrent: vi.fn(), onDeleteCurrent: vi.fn(), meta: '3 pages', ...overrides,
  };
  render(<TooltipProvider><JournalPickerMenu {...props} /></TooltipProvider>);
  return props;
}

beforeEach(() => {
  vi.mocked(confirm).mockReset();
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.hasPointerCapture = () => false;
  Element.prototype.releasePointerCapture = vi.fn();
});

describe('JournalPickerMenu', () => {
  it('shows the current journal with its meta line', () => {
    renderMenu();
    const trigger = screen.getByRole('combobox', { name: 'Switch journal' });
    expect(trigger).toHaveTextContent('Garden log');
    expect(trigger).toHaveTextContent('3 pages');
    expect(trigger).toHaveTextContent('🌱');
  });

  it('switches journal, or creates one from the last option', async () => {
    const user = userEvent.setup();
    const props = renderMenu();
    await user.click(screen.getByRole('combobox', { name: 'Switch journal' }));
    await user.click(await screen.findByRole('option', { name: /History reading/ }));
    expect(props.onSwitch).toHaveBeenCalledWith('history');

    await user.click(screen.getByRole('combobox', { name: 'Switch journal' }));
    await user.click(await screen.findByRole('option', { name: /New journal/ }));
    expect(props.onCreate).toHaveBeenCalledTimes(1);
    expect(props.onSwitch).toHaveBeenCalledTimes(1);
  });

  it('disables the picker and actions when there is no journal at all', () => {
    renderMenu({ journals: [], currentJournal: null });
    expect(screen.getByRole('combobox', { name: 'Switch journal' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Journal actions' })).toBeDisabled();
  });

  it('renames the current journal on Save, skipping an unchanged or blank name', async () => {
    const user = userEvent.setup();
    const props = renderMenu();
    await user.click(screen.getByRole('button', { name: 'Journal actions' }));
    await user.click(screen.getByRole('button', { name: 'Rename journal' }));
    const input = screen.getByDisplayValue('Garden log');

    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(props.onRenameCurrent).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Rename journal' }));
    await user.clear(screen.getByDisplayValue('Garden log'));
    await user.keyboard('   {Enter}');
    expect(props.onRenameCurrent).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Rename journal' }));
    await user.clear(screen.getByDisplayValue('Garden log'));
    await user.type(screen.getByDisplayValue(''), ' Allotment diary ');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(props.onRenameCurrent).toHaveBeenCalledWith('Allotment diary');
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Rename journal' })).not.toBeInTheDocument());
    expect(input).not.toBeInTheDocument();
  });

  it('renames with Enter and abandons with Escape or Cancel', async () => {
    const user = userEvent.setup();
    const props = renderMenu();
    await user.click(screen.getByRole('button', { name: 'Journal actions' }));
    await user.click(screen.getByRole('button', { name: 'Rename journal' }));
    // Escape closes the menu; reopening starts from the action list, not the half-typed name.
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByDisplayValue('Garden log')).not.toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Journal actions' }));

    await user.click(screen.getByRole('button', { name: 'Rename journal' }));
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await user.click(screen.getByRole('button', { name: 'Rename journal' }));
    await user.type(screen.getByDisplayValue('Garden log'), ' 2{Enter}');
    expect(props.onRenameCurrent).toHaveBeenCalledWith('Garden log 2');
  });

  it('creates a new journal from the actions menu', async () => {
    const user = userEvent.setup();
    const props = renderMenu();
    await user.click(screen.getByRole('button', { name: 'Journal actions' }));
    await user.click(screen.getByRole('button', { name: /New journal/ }));
    expect(props.onCreate).toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole('button', { name: /Delete journal/ })).not.toBeInTheDocument());
  });

  it('deletes only after the warning is confirmed', async () => {
    const user = userEvent.setup();
    const props = renderMenu();
    vi.mocked(confirm).mockResolvedValueOnce(false);
    await user.click(screen.getByRole('button', { name: 'Journal actions' }));
    await user.click(screen.getByRole('button', { name: /Delete journal/ }));
    expect(confirm).toHaveBeenCalledWith(
      'This permanently deletes the journal and cannot be undone.',
      { title: 'Delete "Garden log"?', kind: 'warning' },
    );
    expect(props.onDeleteCurrent).not.toHaveBeenCalled();

    vi.mocked(confirm).mockResolvedValueOnce(true);
    await user.click(screen.getByRole('button', { name: /Delete journal/ }));
    await waitFor(() => expect(props.onDeleteCurrent).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole('button', { name: /Delete journal/ })).not.toBeInTheDocument());
  });
});
