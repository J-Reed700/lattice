import { describe, expect, it, beforeEach } from 'vitest';

import { selectPaletteGroups, usePaletteCommandsStore } from '../paletteCommandsStore';

const noop = () => undefined;

describe('paletteCommandsStore', () => {
  beforeEach(() => {
    usePaletteCommandsStore.setState({ commands: new Map() });
  });

  it('registers commands and returns an unregister function', () => {
    const unregister = usePaletteCommandsStore.getState().register([
      { id: 'a.one', label: 'One', run: noop },
      { id: 'a.two', label: 'Two', group: 'Library', run: noop },
    ]);
    expect(usePaletteCommandsStore.getState().commands.size).toBe(2);
    unregister();
    expect(usePaletteCommandsStore.getState().commands.size).toBe(0);
  });

  it('replaces a command re-registered under the same id', () => {
    const { register } = usePaletteCommandsStore.getState();
    register([{ id: 'a.one', label: 'Old', run: noop }]);
    register([{ id: 'a.one', label: 'New', run: noop }]);
    const state = usePaletteCommandsStore.getState();
    expect(state.commands.size).toBe(1);
    expect(state.commands.get('a.one')?.label).toBe('New');
  });

  it('groups visible commands by heading and hides disabled ones', () => {
    usePaletteCommandsStore.getState().register([
      { id: 'a', label: 'A', run: noop },
      { id: 'b', label: 'B', group: 'Library', run: noop },
      { id: 'c', label: 'C', group: 'Library', enabled: false, run: noop },
    ]);
    const groups = selectPaletteGroups(usePaletteCommandsStore.getState());
    expect(groups.map((g) => g.heading)).toEqual(['Actions', 'Library']);
    expect(groups[1].commands.map((c) => c.id)).toEqual(['b']);
  });
});
