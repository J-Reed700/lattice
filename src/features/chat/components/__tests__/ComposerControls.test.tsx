import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { WIKI_TOOL_NAMES } from '@/features/chat/components/composer/toolNames';
import { ComposerControls } from '@/features/chat/components/ComposerControls';

const renderControls = (overrides: Partial<Parameters<typeof ComposerControls>[0]> = {}) => {
  const props = {
    turnMode: 'auto' as const,
    onTurnModeChange: vi.fn(),
    toolPreferences: { knowledgeBase: true, webSearch: false, deepResearchMode: false } as never,
    onToggleKnowledgeBase: vi.fn(),
    onToggleWebTools: vi.fn(),
    onToggleWikiTools: vi.fn(),
    onToggleDeepResearch: vi.fn(),
    customTools: [
      { name: 'tide_tables', description: 'Tide times', enabled: true },
      { name: 'star_atlas', description: '', enabled: true },
    ] as never,
    enabledToolSet: new Set(['tide_tables']),
    onToggleCustomTool: vi.fn(),
    ...overrides,
  };
  render(<ComposerControls {...props} />);
  return props;
};

describe('ComposerControls', () => {
  it('picks a turn mode', () => {
    const props = renderControls({ turnMode: 'query' });
    expect(screen.getByRole('tab', { name: 'Query' })).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(screen.getByRole('tab', { name: 'Follow-up' }));
    expect(props.onTurnModeChange).toHaveBeenCalledWith('followup');
  });

  it('shows each tool’s state and toggles it', () => {
    const props = renderControls();
    expect(screen.getByRole('checkbox', { name: /Your documents/ })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('checkbox', { name: /^Web/ })).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByRole('checkbox', { name: /Wikipedia/ })).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(screen.getByRole('checkbox', { name: /Your documents/ }));
    fireEvent.click(screen.getByRole('checkbox', { name: /^Web/ }));
    fireEvent.click(screen.getByRole('checkbox', { name: /Wikipedia/ }));
    fireEvent.click(screen.getByRole('checkbox', { name: /Deep research/ }));
    expect(props.onToggleKnowledgeBase).toHaveBeenCalled();
    expect(props.onToggleWebTools).toHaveBeenCalled();
    expect(props.onToggleWikiTools).toHaveBeenCalled();
    expect(props.onToggleDeepResearch).toHaveBeenCalled();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('lists custom tools, describing one without a description by its name', () => {
    const props = renderControls();
    const checks = screen.getAllByRole('checkbox');
    expect(checks).toHaveLength(6);
    expect(checks[4]).toHaveAttribute('aria-checked', 'true');
    expect(checks[4]).toHaveTextContent('Tide times');
    expect(checks[5]).toHaveTextContent('star_atlas');
    fireEvent.click(checks[5]);
    expect(props.onToggleCustomTool).toHaveBeenCalledWith('star_atlas');
  });

  it('marks Wikipedia on and warns that deep research is slow', () => {
    renderControls({ enabledToolSet: new Set([WIKI_TOOL_NAMES[0]]), toolPreferences: { deepResearchMode: true } as never, customTools: [] });
    expect(screen.getByRole('checkbox', { name: /Wikipedia/ })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('status')).toHaveTextContent('Deep research runs multiple rounds of search. Expect a longer wait.');
  });
});
