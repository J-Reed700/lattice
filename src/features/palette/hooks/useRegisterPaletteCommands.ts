import { useEffect } from 'react';

import { usePaletteCommandsStore, type PaletteCommand } from '../stores/paletteCommandsStore';

/**
 * Register palette verbs for as long as the calling surface is mounted.
 *
 * Pass a memoised array; the commands are re-registered whenever the array
 * identity changes, so derive it with `useMemo` over the state the verbs
 * depend on (selection, active conversation, …).
 *
 * ```tsx
 * const commands = useMemo<PaletteCommand[]>(() => [
 *   { id: 'chat.regenerate', label: 'Regenerate answer', icon: RefreshCw, enabled: canRegenerate, run: regenerate },
 * ], [canRegenerate, regenerate]);
 * useRegisterPaletteCommands(commands);
 * ```
 */
export function useRegisterPaletteCommands(commands: PaletteCommand[]): void {
  const register = usePaletteCommandsStore((state) => state.register);
  useEffect(() => register(commands), [commands, register]);
}
