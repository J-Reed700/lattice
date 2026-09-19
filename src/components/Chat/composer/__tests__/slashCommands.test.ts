import { describe, expect, it } from 'vitest';

import { matchSlashCommands, parseSlashSubmission, SLASH_COMMANDS } from '../slashCommands';

import type { ComposerModeState } from '../slashCommands';

const mode = (overrides: Partial<ComposerModeState> = {}): ComposerModeState => ({
  turnMode: 'auto',
  knowledgeBase: false,
  webSearch: false,
  wikipedia: false,
  deepResearch: false,
  canCompact: true,
  ...overrides,
});

const tokensOf = (commands: { token: string }[]) => commands.map((command) => command.token);

describe('the slash commands', () => {
  it('offers every command to an empty query', () => {
    expect(tokensOf(matchSlashCommands('', mode()))).toEqual(tokensOf(SLASH_COMMANDS));
  });

  it('puts what the token starts with above what it merely mentions', () => {
    // "followup" also has a w in it; it goes below the two names that open with one.
    expect(tokensOf(matchSlashCommands('w', mode()))).toEqual(['web', 'wiki', 'followup']);
  });

  it('finds a command by what it does, not only by its name', () => {
    expect(tokensOf(matchSlashCommands('research', mode()))).toContain('deep');
  });

  it('hides compacting when there is nothing to fold up', () => {
    expect(tokensOf(matchSlashCommands('', mode({ canCompact: false })))).not.toContain('compact');
  });

  it('says on each row where that switch stands now', () => {
    const [deep] = matchSlashCommands('deep', mode({ deepResearch: true }));
    expect(deep.state(mode({ deepResearch: true }))).toBe('On');
    expect(deep.state(mode())).toBe('Off');

    const [followup] = matchSlashCommands('followup', mode({ turnMode: 'followup' }));
    expect(followup.state(mode({ turnMode: 'followup' }))).toBe('Current');
    expect(followup.state(mode())).toBe('');
  });
});

describe('a message that is only a command', () => {
  // /compact used to be a bare regex on submit that nothing on screen
  // mentioned. Typing it still works, and now so does every other command.
  it('still runs /compact when it is typed and sent', () => {
    expect(parseSlashSubmission('/compact')).toBe('compact');
    expect(parseSlashSubmission('  /COMPACT  ')).toBe('compact');
  });

  it('runs the other commands the same way', () => {
    expect(parseSlashSubmission('/deep')).toBe('deep');
    expect(parseSlashSubmission('/query')).toBe('query');
  });

  it('is a question, not a command, as soon as it says anything more', () => {
    expect(parseSlashSubmission('/compact the notes')).toBeNull();
    expect(parseSlashSubmission('/unknown')).toBeNull();
    expect(parseSlashSubmission('what does /deep do?')).toBeNull();
  });
});
