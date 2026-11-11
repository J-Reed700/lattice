import type * as Wire from '@/lib/bindings';

/** "1:30" for ninety seconds. */
export function clockText(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

export const SETTING_NAMES: Record<Wire.Setting, string> = {
  coached: 'Coached',
  mock: 'Mock',
  watched: 'Watched',
};

export const HELP_NAMES: Record<Wire.Setting, string> = {
  coached: 'Full help',
  mock: 'Hints only',
  watched: 'Hints only',
};
