import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { KnowledgePanel } from '../KnowledgePanel';

import type { KnowledgeItemDto } from '../../../lib/bindings';

const manageKnowledge = vi.hoisted(() => vi.fn());
vi.mock('../../../lib/api', () => ({ VaultAPI: { manageKnowledge } }));
const fact: KnowledgeItemDto = {
  id:'fact-1',conversationId:'one',conversationTitle:'Garden',label:'Budget is $120',kind:'user_fact',state:'active',scope:'space',
  learnedAt:'2026-09-23T12:00:00Z',validFrom:null,validUntil:null,verifiedAt:null,forgotten:false,supersededBy:null,
  evidence:[{messageId:'m1',sequence:1,role:'user',purpose:'assertion',startByte:0,endByte:14,text:'Budget is $120'}],availabilityReason:'Saved in this conversation',
};
function setup(items:KnowledgeItemDto[] = []) {
  manageKnowledge.mockResolvedValue({ok:true,data:{items,hasMore:false,lastAnswerMemoryIds:items.map(i=>i.id)}});
  render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false},mutations:{retry:false}}})}><KnowledgePanel conversationId="one" /></QueryClientProvider>);
  return userEvent.setup();
}
beforeEach(()=>vi.clearAllMocks());
describe('saved knowledge controls',()=>{
  it('saves the exact authored text with an explicit scope and kind',async()=>{
    const user=setup();
    await user.type(screen.getByLabelText('Memory text'),'Never publish before approval.');
    await user.selectOptions(screen.getByLabelText('Memory scope'),'personal');
    await user.selectOptions(screen.getByLabelText('Memory type'),'constraint');
    await user.click(screen.getByRole('button',{name:'Remember'}));
    await waitFor(()=>expect(manageKnowledge).toHaveBeenCalledWith(expect.objectContaining({action:'remember',conversationId:'one',text:'Never publish before approval.',scope:'personal',kind:'constraint'})));
  });
  it('shows evidence and actual latest-answer inclusion; corrections target the prior item',async()=>{
    const user=setup([fact]);
    expect(await screen.findByText('Included in the latest answer’s initial context')).toBeInTheDocument();
    await user.click(screen.getByRole('button',{name:'Correct'}));
    await user.clear(screen.getByLabelText('Memory text'));
    await user.type(screen.getByLabelText('Memory text'),'Budget is now $240.');
    await user.click(screen.getByRole('button',{name:'Save correction'}));
    await waitFor(()=>expect(manageKnowledge).toHaveBeenCalledWith(expect.objectContaining({action:'correct',itemId:'fact-1',text:'Budget is now $240.'})));
  });
  it('does not offer mutation controls for a foreign shared item',async()=>{
    setup([{...fact,conversationId:'other',availabilityReason:'Shared with this space'}]);
    await screen.findByText('Open its source conversation to change this item.');
    expect(screen.queryByRole('button',{name:'Forget saved item'})).not.toBeInTheDocument();
    expect(screen.queryByRole('button',{name:'Correct'})).not.toBeInTheDocument();
  });
  it('retains the correction draft when saving fails',async()=>{
    const user=setup([fact]);await screen.findByText('Budget is $120',{selector:'p'});
    await user.click(screen.getByRole('button',{name:'Correct'}));
    manageKnowledge.mockResolvedValueOnce({ok:false,error:'Memory changed; retry.'});
    await user.click(screen.getByRole('button',{name:'Save correction'}));
    expect(await screen.findByRole('alert')).toHaveTextContent('Memory changed; retry.');
    expect(screen.getByLabelText('Memory text')).toHaveValue('Budget is $120');
  });
});
