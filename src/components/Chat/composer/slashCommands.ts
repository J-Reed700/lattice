import { BookOpen, CornerDownRight, Database, Globe, Scissors, Search, Sparkles, Telescope } from 'lucide-react';

import type { TurnMode } from '../../../types';
import type { LucideIcon } from 'lucide-react';

/**
 * The `/` commands the composer offers.
 *
 * Every one of them flips a switch the gear popover already flips; the menu is
 * a faster way to the same state, not a second one. `/compact` was here before
 * any of this — a bare regex on submit that nothing on screen mentioned — and
 * is folded in so it is discoverable and runs through the same path.
 */

export type SlashCommandId =
  | 'deep'
  | 'web'
  | 'wiki'
  | 'docs'
  | 'followup'
  | 'query'
  | 'auto'
  | 'compact';

/** What the composer's switches are set to right now, for the rows' state column. */
export interface ComposerModeState {
  turnMode: TurnMode;
  knowledgeBase: boolean;
  webSearch: boolean;
  wikipedia: boolean;
  deepResearch: boolean;
  /** There is nothing to fold up in a conversation with no older messages. */
  canCompact: boolean;
}

export interface SlashCommand {
  id: SlashCommandId;
  /** What is typed, without the slash. */
  token: string;
  /** Other spellings that should find this row. */
  aliases: string[];
  icon: LucideIcon;
  /** One line saying what accepting it does. */
  description: string;
  /** Where this switch stands now — the right-hand column of the row. */
  state: (_mode: ComposerModeState) => string;
  /** False hides the row: no command that would do nothing. */
  available: (_mode: ComposerModeState) => boolean;
}

const onOff = (enabled: boolean): string => (enabled ? 'On' : 'Off');
const always = () => true;

export const SLASH_COMMANDS: SlashCommand[] = [
  {
    id: 'deep',
    token: 'deep',
    aliases: ['deep research', 'research'],
    icon: Telescope,
    description: 'Multi-step research across sources. Slower.',
    state: (mode) => onOff(mode.deepResearch),
    available: always,
  },
  {
    id: 'docs',
    token: 'docs',
    aliases: ['documents', 'library', 'kb'],
    icon: Database,
    description: 'Search your documents for every answer.',
    state: (mode) => onOff(mode.knowledgeBase),
    available: always,
  },
  {
    id: 'web',
    token: 'web',
    aliases: ['search the web'],
    icon: Globe,
    description: 'Let the answer search the web.',
    state: (mode) => onOff(mode.webSearch),
    available: always,
  },
  {
    id: 'wiki',
    token: 'wiki',
    aliases: ['wikipedia'],
    icon: BookOpen,
    description: 'Search and summarize Wikipedia.',
    state: (mode) => onOff(mode.wikipedia),
    available: always,
  },
  {
    id: 'auto',
    token: 'auto',
    aliases: ['turn mode'],
    icon: Sparkles,
    description: 'Let the router decide: new topic or follow-up.',
    state: (mode) => (mode.turnMode === 'auto' ? 'Current' : ''),
    available: always,
  },
  {
    id: 'followup',
    token: 'followup',
    aliases: ['follow-up', 'follow up'],
    icon: CornerDownRight,
    description: 'Keep every turn on the current topic.',
    state: (mode) => (mode.turnMode === 'followup' ? 'Current' : ''),
    available: always,
  },
  {
    id: 'query',
    token: 'query',
    aliases: ['search first'],
    icon: Search,
    description: 'Always search sources before answering.',
    state: (mode) => (mode.turnMode === 'query' ? 'Current' : ''),
    available: always,
  },
  {
    id: 'compact',
    token: 'compact',
    aliases: ['summarize context'],
    icon: Scissors,
    description: 'Fold the older messages into a summary.',
    state: () => 'Runs now',
    available: (mode) => mode.canCompact,
  },
];

// The description is deliberately not searched: one letter of prose matches
// nearly every row, and a menu that never narrows is not a menu.
const haystack = (command: SlashCommand): string =>
  [command.token, ...command.aliases].join(' ').toLowerCase();

/**
 * The commands a typed query should offer, best first: what the token starts
 * with, then what the command is otherwise called.
 */
export function matchSlashCommands(query: string, mode: ComposerModeState): SlashCommand[] {
  const available = SLASH_COMMANDS.filter((command) => command.available(mode));
  const needle = query.trim().toLowerCase();
  if (!needle) return available;

  const prefix = available.filter((command) => command.token.startsWith(needle));
  const rest = available.filter(
    (command) => !command.token.startsWith(needle) && haystack(command).includes(needle)
  );
  return [...prefix, ...rest];
}

/**
 * A message that is nothing but a command runs the command instead of being
 * sent. This is where `/compact`'s old submit regex ended up, generalized: the
 * commands you can pick from the menu are the commands you can type and send.
 */
export function parseSlashSubmission(text: string): SlashCommandId | null {
  const token = text.trim().toLowerCase();
  if (!token.startsWith('/')) return null;
  const found = SLASH_COMMANDS.find((command) => `/${command.token}` === token);
  return found?.id ?? null;
}
