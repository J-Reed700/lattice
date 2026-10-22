import { useState } from 'react';

import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ClaimActions, type ActionForm } from '@/features/learning/lessons/review/ClaimActions';
import { claimItem, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto } from '@/lib/bindings';

type Props = Partial<Parameters<typeof ClaimActions>[0]>;

function setup(item: ClaimReviewItemDto = unsupportedItem(1), initial: ActionForm | null = null, props: Props = {}) {
  const onAct = vi.fn(async () => true);
  const onFormChange = vi.fn();
  const onAcceptSource = vi.fn();
  const onAddSource = vi.fn();
  function Harness() {
    const [form, setForm] = useState<ActionForm | null>(initial);
    return <ClaimActions item={item} sharing={1} suggested="accept" busy={false} error={null} form={form}
      onFormChange={(next) => { onFormChange(next); setForm(next); }} onAct={onAct} onAcceptSource={onAcceptSource} onAddSource={onAddSource} {...props} />;
  }
  render(<Harness />);
  return { onAct, onFormChange, onAcceptSource, onAddSource };
}

const contradicted = () => claimItem({ ...unsupportedItem(1), verdictKind: { kind: 'contradicted' } });

describe('ClaimActions', () => {
  it('shows only the actions the claim offers', () => {
    setup(claimItem({ availableActions: ['remove'], support: null }));
    const bar = screen.getByRole('toolbar', { name: 'Claim actions' });
    expect(within(bar).getByRole('button', { name: 'Remove it' })).toBeInTheDocument();
    for (const name of ['Accept', 'Edit the sentence', 'Add a source']) expect(within(bar).queryByRole('button', { name })).not.toBeInTheDocument();
    expect(within(bar).queryByRole('button', { name: /Accept this source/ })).not.toBeInTheDocument();
  });

  it('accepts with the supporting source, and closes the form once it is saved', async () => {
    const { onAct, onFormChange } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(onAct).toHaveBeenCalledWith({ action: 'confirm', note: null, sourceVersionIds: ['v-field-guide'] }, 'confirm');
    expect(onFormChange).toHaveBeenCalledWith(null);
  });

  it('keeps the form open when the action fails', async () => {
    const { onFormChange } = setup(unsupportedItem(1), null, { onAct: vi.fn(async () => false) });
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(onFormChange).not.toHaveBeenCalled();
  });

  it('words the source button for one claim or for the sharing group', () => {
    setup();
    expect(screen.getByRole('button', { name: 'Accept this source for the course' })).toBeInTheDocument();
  });

  it('counts the claims that share the source', async () => {
    const { onAcceptSource } = setup(unsupportedItem(1), null, { sharing: 4 });
    await userEvent.click(screen.getByRole('button', { name: 'Accept this source for all 4' }));
    expect(onAcceptSource).toHaveBeenCalled();
  });

  it('asks for a note before accepting a claim a source contradicts', async () => {
    const { onAct } = setup(contradicted());
    await userEvent.click(screen.getByRole('button', { name: 'Accept with a note' }));
    expect(onAct).not.toHaveBeenCalled();
    const save = screen.getByRole('button', { name: 'Accept with this note' });
    expect(save).toBeDisabled();
    // The toolbar button with the form open and no note explains what is missing.
    await userEvent.click(screen.getByRole('button', { name: 'Accept with a note' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Write a note saying why the claim is right despite the source.');
    await userEvent.type(screen.getByLabelText('Why the claim is right (required)'), '  The newer survey disagrees.  ');
    await userEvent.click(save);
    expect(onAct).toHaveBeenCalledWith({ action: 'confirm', note: 'The newer survey disagrees.', sourceVersionIds: ['v-field-guide'] }, 'confirm');
  });

  it('cancels any open form', async () => {
    const { onFormChange } = setup(contradicted(), 'note');
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onFormChange).toHaveBeenCalledWith(null);
  });

  it('edits the sentence, refusing an empty or unchanged one', async () => {
    const item = unsupportedItem(1);
    const { onAct } = setup(item, 'edit');
    const box = screen.getByLabelText('The sentence');
    expect(box).toHaveValue(item.presentation.sentence);
    await userEvent.click(screen.getByRole('button', { name: 'Save the sentence' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Change the sentence first.');
    await userEvent.clear(box);
    await userEvent.click(screen.getByRole('button', { name: 'Save the sentence' }));
    expect(screen.getByRole('alert')).toHaveTextContent('To delete the sentence, choose Remove it.');
    expect(onAct).not.toHaveBeenCalled();
    await userEvent.type(box, 'Monarchs fly far.');
    await userEvent.click(screen.getByRole('button', { name: 'Save the sentence' }));
    expect(onAct).toHaveBeenCalledWith({ action: 'edit', before: item.presentation.sentence, after: 'Monarchs fly far.' }, 'edit');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('opens and closes the edit form from its button', async () => {
    const { onFormChange } = setup();
    const edit = screen.getByRole('button', { name: 'Edit the sentence' });
    await userEvent.click(edit);
    expect(onFormChange).toHaveBeenLastCalledWith('edit');
    expect(edit).toHaveAttribute('aria-expanded', 'true');
    await userEvent.click(edit);
    expect(onFormChange).toHaveBeenLastCalledWith(null);
  });

  it('removes the sentence in one click', async () => {
    const { onAct } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Remove it' }));
    expect(onAct).toHaveBeenCalledWith({ action: 'remove' }, 'remove');
  });

  it('offers another source, and demotes Remove, when the source the user added did not say it', () => {
    setup(unsupportedItem(1), null, { suggested: 'remove', closest: true });
    expect(screen.getByRole('button', { name: 'Add another source' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove it' })).toHaveClass('text-danger-fg');
  });

  it('opens the add-source sheet', async () => {
    const { onAddSource } = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Add a source' }));
    expect(onAddSource).toHaveBeenCalled();
  });

  it('searches with a hint, refusing an empty one, and also on Enter', async () => {
    const { onAct } = setup(unsupportedItem(1), 'hint');
    await userEvent.click(screen.getByRole('button', { name: 'Search' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Write what to search for.');
    await userEvent.type(screen.getByLabelText('What to search for'), ' monarch tagging study {Enter}');
    expect(onAct).toHaveBeenCalledWith({ action: 'research_with_hint', hint: 'monarch tagging study' }, 'research_with_hint');
  });

  it('opens the More menu with the actions the claim offers', async () => {
    const { onAct } = setup();
    const more = screen.getByRole('button', { name: /More/ });
    expect(more).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(more);
    const menu = screen.getByRole('menu', { name: 'More actions' });
    expect(within(menu).getByRole('menuitem', { name: 'History (0)' })).toBeInTheDocument();
    expect(within(menu).queryByRole('menuitem', { name: 'Withdraw acceptance' })).not.toBeInTheDocument();
    await userEvent.click(within(menu).getByRole('menuitem', { name: 'General knowledge' }));
    expect(onAct).toHaveBeenCalledWith({ action: 'general_knowledge' }, 'general_knowledge');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('closes the menu again from the More button', async () => {
    setup();
    const more = screen.getByRole('button', { name: /More/ });
    await userEvent.click(more);
    await userEvent.click(more);
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('opens the hint form from the menu', async () => {
    const { onFormChange } = setup();
    await userEvent.click(screen.getByRole('button', { name: /More/ }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Search with a hint' }));
    expect(onFormChange).toHaveBeenLastCalledWith('hint');
    expect(screen.getByLabelText('What to search for')).toBeInTheDocument();
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('withdraws an acceptance only from a confirmed claim', async () => {
    const confirmed = claimItem({ ...unsupportedItem(1), state: { state: 'confirmed', at: 3 }, availableActions: ['unconfirm', 'edit'] });
    const { onAct } = setup(confirmed);
    await userEvent.click(screen.getByRole('button', { name: /More/ }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Withdraw acceptance' }));
    expect(onAct).toHaveBeenCalledWith({ action: 'unconfirm' }, 'unconfirm');
    expect(screen.queryByRole('menuitem', { name: 'General knowledge' })).not.toBeInTheDocument();
  });

  it('shows the claim history with both checks side by side', async () => {
    const item = claimItem({
      ...unsupportedItem(1),
      localCheck: null,
      verifierCheck: { checker: { role: 'verifier', model: 'big' }, reason: 'The passage is about something else.', quotedSource: null, passages: [], at: 1, result: { verdict: 'failed', kind: { kind: 'unsupported' } } },
      history: [{ ordinal: 1, actor: 'user', at: 1_760_000_000_000, jobId: null, appVersion: 'test', checkerModel: null, event: { kind: 'user_action', action: { action: 'remove' }, note: null } }],
    });
    setup(item);
    await userEvent.click(screen.getByRole('button', { name: /More/ }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'History (1)' }));
    const checks = screen.getByRole('group', { name: 'First check and independent check' });
    expect(within(checks).getByText('First check')).toBeInTheDocument();
    expect(within(checks).getByText('No check recorded.')).toBeInTheDocument();
    expect(within(checks).getByText('Independent check')).toBeInTheDocument();
    expect(within(checks).getByText('The passage is about something else.')).toBeInTheDocument();
    expect(screen.getByRole('list', { name: 'Claim history' })).toHaveTextContent('Removed the sentence');
  });

  it('shows history without check columns when no independent check ran', async () => {
    setup(unsupportedItem(1), 'history');
    expect(screen.queryByRole('group', { name: 'First check and independent check' })).not.toBeInTheDocument();
    expect(screen.getByText('Nothing has been recorded for this claim yet.')).toBeInTheDocument();
  });

  it('shows Saving while busy and disables the actions', () => {
    setup(unsupportedItem(1), null, { busy: true });
    expect(screen.getByRole('status')).toHaveTextContent('Saving');
    expect(screen.getByRole('button', { name: 'Accept' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Remove it' })).toBeDisabled();
  });

  it('shows the error the action returned', () => {
    setup(unsupportedItem(1), null, { error: 'The review changed; reload it.' });
    expect(screen.getByRole('alert')).toHaveTextContent('The review changed; reload it.');
  });
});
