import { renderHook } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { useRegisterPaletteCommands } from './useRegisterPaletteCommands';
import { selectPaletteGroups, usePaletteCommandsStore, type PaletteCommand } from '../stores/paletteCommandsStore';


const labels = () => selectPaletteGroups(usePaletteCommandsStore.getState()).flatMap((group) => group.commands.map((command) => command.label));

beforeEach(() => usePaletteCommandsStore.setState({ commands: new Map() }));

it('offers the commands while mounted, swaps them when they change and removes them on unmount', () => {
  const first: PaletteCommand[] = [{ id: 'recipes.scale', label: 'Scale recipe', run: vi.fn() }];
  const second: PaletteCommand[] = [{ id: 'recipes.convert', label: 'Convert units', run: vi.fn() }];
  const { rerender, unmount } = renderHook(({ commands }) => useRegisterPaletteCommands(commands), { initialProps: { commands: first } });
  expect(labels()).toEqual(['Scale recipe']);

  rerender({ commands: second });
  expect(labels()).toEqual(['Convert units']);

  unmount();
  expect(labels()).toEqual([]);
});
