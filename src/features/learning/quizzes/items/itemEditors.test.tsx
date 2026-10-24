import { useState } from 'react';

import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { itemRegistry } from '@/features/learning/quizzes/items/itemRegistry';
import { draftOf, isAnswered, responseOf, type ItemDraft } from '@/features/learning/quizzes/items/itemResponse';
import { formItem } from '@/features/learning/quizzes/quizTestData';
import type { LearningAssessmentFormItemDto } from '@/lib/bindings';

vi.mock('@/components/TiptapEditor', () => ({ TiptapViewer: ({ content }: { content: string }) => <div>{content}</div> }));

/** Renders one format's editor with live draft state, the way the runner does. */
function Harness({ item, onDraft }: { item: LearningAssessmentFormItemDto; onDraft: (draft: ItemDraft) => void }) {
  const [draft, setDraft] = useState(() => draftOf(item));
  const { Editor } = itemRegistry[item.format];
  return <div>
    <p id="prompt">{item.prompt}</p>
    <Editor item={item} draft={draft} disabled={false} promptId="prompt" onChange={(patch) => setDraft((current) => { const next = { ...current, ...patch }; onDraft(next); return next; })} />
  </div>;
}

function mount(item: LearningAssessmentFormItemDto) {
  const onDraft = vi.fn();
  render(<Harness item={item} onDraft={onDraft} />);
  const latest = () => onDraft.mock.calls.at(-1)?.[0] as ItemDraft;
  return { user: userEvent.setup(), latest };
}

describe('quiz item editors, by keyboard alone', () => {
  it('single choice is a radiogroup: Tab in, arrows move the choice and wrap, Home and End jump', async () => {
    const { user, latest } = mount(formItem('single_choice', { options: ['Alpha', 'Beta', 'Gamma'] }));
    const group = screen.getByRole('radiogroup');
    await user.tab();
    expect(within(group).getByRole('radio', { name: /Alpha/ })).toHaveFocus();
    await user.keyboard(' ');
    expect(latest().selectedIndices).toEqual([0]);
    await user.keyboard('{ArrowDown}');
    expect(within(group).getByRole('radio', { name: /Beta/ })).toHaveFocus();
    expect(within(group).getByRole('radio', { name: /Beta/ })).toHaveAttribute('aria-checked', 'true');
    await user.keyboard('{ArrowUp}{ArrowUp}');
    expect(latest().selectedIndices).toEqual([2]);
    await user.keyboard('{Home}');
    expect(latest().selectedIndices).toEqual([0]);
    // Only the chosen option is in the tab order.
    expect(within(group).getAllByRole('radio').map((radio) => radio.tabIndex)).toEqual([0, -1, -1]);
  });

  it('true/false offers two radios and moves between them with arrows', async () => {
    const { user, latest } = mount(formItem('true_false', { options: ['True', 'False'] }));
    await user.tab();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('radio', { name: /False/ })).toHaveAttribute('aria-checked', 'true');
    expect(latest().selectedIndices).toEqual([1]);
  });

  it('multi-select is a checkbox group: Tab between options, Space ticks and unticks', async () => {
    const { user, latest } = mount(formItem('multi_select', { options: ['One', 'Two', 'Three', 'Four'] }));
    expect(screen.getByText('Choose all that apply.')).toBeVisible();
    await user.tab();
    await user.keyboard(' ');
    await user.tab();
    await user.tab();
    await user.keyboard('{Enter}');
    expect(latest().selectedIndices).toEqual([0, 2]);
    expect(screen.getByRole('checkbox', { name: /Three/ })).toHaveAttribute('aria-checked', 'true');
    await user.keyboard(' ');
    expect(latest().selectedIndices).toEqual([0]);
  });

  it('short answer is one labelled input', async () => {
    const { user, latest } = mount(formItem('short_answer'));
    await user.type(screen.getByLabelText('Your answer'), '3.2 mM');
    expect(latest().textResponse).toBe('3.2 mM');
  });

  it('cloze puts labelled inputs inline, one per blank', async () => {
    const item = formItem('cloze', { prompt: 'The {{1}} binds the {{2}}.', blankCount: 2 });
    const { user, latest } = mount(item);
    await user.tab();
    expect(screen.getByRole('textbox', { name: 'Blank 1 of 2' })).toHaveFocus();
    await user.keyboard('inhibitor');
    await user.tab();
    await user.keyboard('enzyme');
    expect(latest().blankResponses).toEqual(['inhibitor', 'enzyme']);
    expect(isAnswered(item, latest())).toBe(true);
  });

  it('matching uses one native select per entry, named by the entry', async () => {
    const item = formItem('matching', { leftItems: ['Competitive', 'Uncompetitive'], rightItems: ['Same Vmax', 'Lower Vmax and Km', 'No effect'] });
    const { user, latest } = mount(item);
    await user.selectOptions(screen.getByLabelText('Uncompetitive'), 'Lower Vmax and Km');
    expect(latest().matches).toEqual([null, 1]);
    expect(isAnswered(item, latest())).toBe(false);
    await user.selectOptions(screen.getByLabelText('Competitive'), 'Same Vmax');
    expect(isAnswered(item, latest())).toBe(true);
    expect(screen.getByText('One answer on the right matches nothing.')).toBeVisible();
  });

  it('ordering moves rows with buttons and with Alt+arrows, and announces each move', async () => {
    const item = formItem('ordering', { options: ['Bind', 'Convert', 'Release'] });
    const { user, latest } = mount(item);
    await user.click(screen.getByRole('button', { name: 'Move Bind down' }));
    expect(latest().orderedValues).toEqual(['Convert', 'Bind', 'Release']);
    expect(screen.getByRole('status')).toHaveTextContent('Bind moved to position 2 of 3.');
    screen.getByRole('listitem', { name: '1 of 3: Convert' }).focus();
    await user.keyboard('{Alt>}{ArrowDown}{/Alt}');
    expect(latest().orderedValues).toEqual(['Bind', 'Convert', 'Release']);
    expect(screen.getByRole('button', { name: 'Move Bind up' })).toBeDisabled();
  });

  it('ordering can keep the given order as the answer without moving anything', async () => {
    const item = formItem('ordering', { options: ['First', 'Second'] });
    const { user, latest } = mount(item);
    expect(isAnswered(item, draftOf(item))).toBe(false);
    await user.click(screen.getByRole('button', { name: 'Keep this order as my answer' }));
    expect(latest().orderedValues).toEqual(['First', 'Second']);
  });

  it('explanation shows what it is graded on and a word count with no minimum', async () => {
    const item = formItem('explanation', { points: 4, rubric: [{ id: 'a', title: 'Accuracy', description: '', maxPoints: 2 }, { id: 'e', title: 'Example', description: '', maxPoints: 2 }] });
    const { user } = mount(item);
    expect(screen.getByText(/accuracy, example · 4 points/)).toBeVisible();
    await user.type(screen.getByLabelText('Your explanation'), 'It binds the complex');
    expect(screen.getByText('4 words · no minimum')).toBeVisible();
  });
});

describe('saved responses', () => {
  it('fills only the fields a format uses', () => {
    const item = formItem('multi_select', { options: ['a', 'b', 'c'] });
    expect(responseOf(item, { ...draftOf(item), selectedIndices: [2, 0], textResponse: 'ignored' })).toMatchObject({ selectedIndices: [0, 2], text: null, blankResponses: [], matches: [], orderedValues: [] });
  });
});
