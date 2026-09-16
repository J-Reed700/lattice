import { Button } from '@/components/ui/button';
import { SectionHeading } from '@/components/ui/PageHeader';

import { useDismissStudyGeneration, useGenerateConversationStudyDeck, useGenerateStudyDeck, useStudyGenerations } from './useStudy';

export function StudyGenerations() {
  const generations = useStudyGenerations();
  const generate = useGenerateStudyDeck();
  const generateConversation = useGenerateConversationStudyDeck();
  const dismiss = useDismissStudyGeneration();
  if (!generations.length) return null;
  const busy = generations.some(item => item.status === 'pending');
  return <section className="mb-8" aria-label="Deck generation">
    <SectionHeading>Generation</SectionHeading>
    {generations.map(item => <div key={item.id} className="border-b border-border-subtle py-4" role={item.status === 'error' ? 'alert' : 'status'}>
      <p className="text-sm font-medium text-text-primary">{item.request.title}</p>
      <p className="mt-2 text-sm text-text-secondary">{item.status === 'pending' ? item.kind === 'conversation' ? 'Turning every verified claim into a cited flashcard… You can leave this screen.' : `Generating ${item.request.count} questions… You can leave this screen. Your deck will appear here when it is ready.` : `Generation failed: ${item.error}`}</p>
      {item.status === 'error' && <div className="mt-3 flex gap-2">
        <Button variant="secondary" size="sm" disabled={busy} onClick={() => { if (item.kind === 'conversation') generateConversation.mutate(item.request); else generate.mutate(item.request); dismiss(item.id); }}>Retry generation</Button>
        <Button variant="ghost" size="sm" onClick={() => dismiss(item.id)}>Dismiss</Button>
      </div>}
    </div>)}
  </section>;
}
