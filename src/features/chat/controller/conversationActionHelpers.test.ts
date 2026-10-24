import { describe, expect, it } from 'vitest';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';

import { forgetRequestedConversation, restoreMissingItem, setUiError } from './conversationActionHelpers';

const items = (...ids: string[]) => ids.map((id) => ({ id }));
const ids = (list: Array<{ id: string }> | undefined) => list?.map((item) => item.id);

describe('conversationActionHelpers', () => {
  describe('restoreMissingItem', () => {
    const previous = items('a', 'b', 'c', 'd');

    it('puts the item back before its first surviving successor', () => {
      expect(ids(restoreMissingItem(items('a', 'd'), previous, 'b'))).toEqual(['a', 'b', 'd']);
    });

    it('falls back to after its last surviving predecessor', () => {
      expect(ids(restoreMissingItem(items('a'), previous, 'c'))).toEqual(['a', 'c']);
    });

    it('falls back to its old position when nothing around it survived', () => {
      expect(ids(restoreMissingItem(items('x'), previous, 'b'))).toEqual(['x', 'b']);
      expect(ids(restoreMissingItem(undefined, previous, 'a'))).toEqual(['a']);
    });

    it('changes nothing when the item is still there or was never there', () => {
      const current = items('a', 'b');
      expect(restoreMissingItem(current, previous, 'b')).toBe(current);
      expect(restoreMissingItem(current, previous, 'z')).toBe(current);
    });
  });

  it('shows an error from anything thrown', () => {
    setUiError(new Error('db locked'));
    expect(conversationUiStore.getState().error).toBe('db locked');
    setUiError('plain text');
    expect(conversationUiStore.getState().error).toBe('plain text');
  });

  it('forgets everything held for a removed conversation', () => {
    conversationUiStore.setState({
      requestedLinkedConversationIds: new Set(['c1', 'c2']),
      requestedWebSourceConversationIds: new Set(['c1']),
      optimisticMessages: new Map([['t1', { conversationId: 'c1' }], ['t2', { conversationId: 'c2' }]]),
      inFlightGenerations: new Map([['c1', 'r1']]),
      liveRetrieval: new Map([['c1', {}]]),
      liveSteps: new Map([['c1', []]]),
      liveModel: new Map([['c1', {}]]),
    } as never);
    forgetRequestedConversation('c1');
    const state = conversationUiStore.getState();
    expect([...state.requestedLinkedConversationIds]).toEqual(['c2']);
    expect(state.requestedWebSourceConversationIds.size).toBe(0);
    expect([...state.optimisticMessages.keys()]).toEqual(['t2']);
    expect(state.inFlightGenerations.size + state.liveRetrieval.size + state.liveSteps.size + state.liveModel.size).toBe(0);
  });
});
