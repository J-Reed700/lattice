import { useId } from 'react';

import { Button } from '@/components/ui/button';
import { useLessonDraft } from '@/features/learning/lessons/active/useLessonDraft';

/**
 * Explaining the lesson from memory (learning science lessons §3.4): the
 * learner writes the ideas in their own words, as if teaching someone new,
 * before the key takeaways are shown. Retrieval, not rereading, is what
 * strengthens memory, and the gaps they find are the ones worth revisiting.
 */
export function LessonFromMemory({ lessonId, compared, onCompare }: {
  lessonId: string;
  /** Whether the takeaways are already shown. */
  compared: boolean;
  onCompare: () => void;
}) {
  const id = useId();
  const [explanation, setExplanation] = useLessonDraft(`${lessonId}.from-memory`);
  return (
    <div className="mt-5 rounded-lg border border-accent/30 bg-accent/5 p-4">
      <label htmlFor={id} className="block text-sm font-medium text-text-primary">Explain it from memory</label>
      <p className="mt-1 text-xs leading-5 text-text-secondary">Without scrolling back up. Then compare with the key takeaways: what you could not recall is what to revisit.</p>
      <textarea
        id={id}
        value={explanation}
        onChange={(event) => setExplanation(event.target.value)}
        rows={6}
        maxLength={8000}
        placeholder="The ideas, why they work, when they don’t apply, and how they connect…"
        className="mt-3 block w-full min-w-0 rounded-md border border-border bg-surface px-3 py-2 text-sm leading-6 text-text-primary"
      />
      {!compared && (
        <div className="mt-3 flex flex-wrap items-center gap-3">
          <Button variant="outline" type="button" disabled={!explanation.trim()} onClick={onCompare} className="studio-button studio-button-primary disabled:opacity-50">
            Compare with the key takeaways
          </Button>
          <button type="button" onClick={onCompare} className="text-xs text-text-secondary underline underline-offset-2 hover:text-text-primary">
            Show the takeaways now
          </button>
        </div>
      )}
    </div>
  );
}
