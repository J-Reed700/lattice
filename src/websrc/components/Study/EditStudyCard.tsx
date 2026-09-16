import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { settingsFieldClass } from '@/components/ui/SettingsSection';
import type { StudyCardDto } from '@/lib/bindings';

import { useUpdateStudyCard } from './useStudy';

export function EditStudyCard({ card, onClose }: { card: StudyCardDto; onClose: () => void }) {
  const [question, setQuestion] = useState(card.question);
  const [answer, setAnswer] = useState(card.answer);
  const [explanation, setExplanation] = useState(card.explanation);
  const update = useUpdateStudyCard();
  return <Dialog open onOpenChange={open => { if (!open && !update.isPending) onClose(); }}>
    <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-xl">
      <DialogTitle>Edit card</DialogTitle>
      <DialogDescription>The original source excerpt stays attached.</DialogDescription>
      <form className="space-y-4" onSubmit={event => {
        event.preventDefault();
        if (!update.isPending) update.mutate({ cardId: card.id, question, answer, explanation }, { onSuccess: onClose });
      }}>
        <div><label htmlFor="edit-study-question" className="mb-2 block text-sm">Question</label><textarea id="edit-study-question" className={`${settingsFieldClass} min-h-24 w-full`} required maxLength={2000} value={question} onChange={e => setQuestion(e.target.value)} /></div>
        <div><label htmlFor="edit-study-answer" className="mb-2 block text-sm">Correct answer</label><textarea id="edit-study-answer" className={`${settingsFieldClass} min-h-20 w-full`} required maxLength={1000} value={answer} onChange={e => setAnswer(e.target.value)} /></div>
        <div><label htmlFor="edit-study-explanation" className="mb-2 block text-sm">Explanation</label><textarea id="edit-study-explanation" className={`${settingsFieldClass} min-h-32 w-full`} required maxLength={3000} value={explanation} onChange={e => setExplanation(e.target.value)} /></div>
        {update.isError && <p role="alert" className="text-sm text-danger-fg">{update.error.message}</p>}
        <div className="flex justify-end gap-2"><Button type="button" variant="ghost" disabled={update.isPending} onClick={onClose}>Cancel</Button><Button type="submit" disabled={update.isPending || !question.trim() || !answer.trim() || !explanation.trim()}>{update.isPending ? 'Saving…' : 'Save card'}</Button></div>
      </form>
    </DialogContent>
  </Dialog>;
}
