import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SpacePickerPopover } from '../SpacePickerPopover';

const storeState = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));

vi.mock('../../../stores/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) => selector(storeState.current),
}));

const space = (id: string, name: string, extra: Record<string, unknown> = {}) => ({
  id,
  name,
  description: null,
  icon: null,
  accentColor: null,
  isArchived: false,
  ...extra,
});

function renderPicker(onSelect = vi.fn(), activeSpaceId: string | null = 'space_movies') {
  render(
    <SpacePickerPopover activeSpaceId={activeSpaceId} heading="This chat searches" onSelect={onSelect}>
      <button type="button">Movies</button>
    </SpacePickerPopover>
  );
  return onSelect;
}

describe('SpacePickerPopover', () => {
  beforeEach(() => {
    storeState.current = {
      spaces: [
        space('space_general', 'General'),
        space('space_movies', 'Movies'),
        space('space_old', 'Old project', { isArchived: true }),
      ],
    };
  });

  it('says what is being chosen and what each space can reach', async () => {
    renderPicker();
    await userEvent.click(screen.getByRole('button', { name: 'Movies' }));

    expect(screen.getByText('This chat searches')).toBeInTheDocument();
    // General is a space with its own contents, not "everything".
    expect(screen.getByText('Filed in General, or not filed anywhere')).toBeInTheDocument();
    expect(screen.getByText('Only documents filed in Movies')).toBeInTheDocument();
  });

  it('does not offer an archived space', async () => {
    renderPicker();
    await userEvent.click(screen.getByRole('button', { name: 'Movies' }));

    expect(screen.queryByText('Old project')).not.toBeInTheDocument();
  });

  it('reports the chosen space by id and name', async () => {
    const onSelect = renderPicker();
    await userEvent.click(screen.getByRole('button', { name: 'Movies' }));
    await userEvent.click(screen.getByText('General'));

    expect(onSelect).toHaveBeenCalledWith('space_general', 'General');
  });

  it('does nothing when the current space is picked again', async () => {
    const onSelect = renderPicker();
    await userEvent.click(screen.getByRole('button', { name: 'Movies' }));
    await userEvent.click(screen.getByText('Only documents filed in Movies'));

    expect(onSelect).not.toHaveBeenCalled();
  });
});
