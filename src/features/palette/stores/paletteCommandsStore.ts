import { create } from 'zustand';

import type { LucideIcon } from 'lucide-react';

/** UI command registered for execution in the command palette. */
export interface PaletteCommand {
  id: string;
  label: string;
  group?: string;
  icon?: LucideIcon;
  shortcut?: string;
  description?: string;
  keywords?: string[];
  enabled?: boolean;
  run: () => void | Promise<void>;
}

interface PaletteCommandsState {
  commands: Map<string, PaletteCommand>;
  register: (commands: PaletteCommand[]) => () => void;
  unregister: (ids: string[]) => void;
}

export const usePaletteCommandsStore = create<PaletteCommandsState>((set, get) => ({
  commands: new Map(),
  register: (commands) => {
    set((state) => {
      const next = new Map(state.commands);
      for (const command of commands) next.set(command.id, command);
      return { commands: next };
    });
    const ids = commands.map((command) => command.id);
    return () => get().unregister(ids);
  },
  unregister: (ids) => {
    set((state) => {
      if (ids.length === 0) return state;
      const next = new Map(state.commands);
      for (const id of ids) next.delete(id);
      return { commands: next };
    });
  },
}));

export const DEFAULT_PALETTE_GROUP = 'Actions';

/** Visible commands grouped by heading, in registration order. */
export function selectPaletteGroups(state: Pick<PaletteCommandsState, 'commands'>): Array<{ heading: string; commands: PaletteCommand[] }> {
  const groups = new Map<string, PaletteCommand[]>();
  for (const command of state.commands.values()) {
    if (command.enabled === false) continue;
    const heading = command.group ?? DEFAULT_PALETTE_GROUP;
    const list = groups.get(heading) ?? [];
    list.push(command);
    groups.set(heading, list);
  }
  return [...groups.entries()].map(([heading, commands]) => ({ heading, commands }));
}
