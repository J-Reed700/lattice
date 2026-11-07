import type { TakenOutSentence } from '@/features/learning/lessons/review/claimReviewPresentation';

/**
 * What the checks took out of the lesson (design 3.12): each sentence once,
 * with every statement that stood on it and why it went. None of it is in the
 * lesson now; this is the record, and nothing here is waiting for the user.
 */
export function RemovedStatements({ sentences }: { sentences: TakenOutSentence[] }) {
  return (
    <section aria-label="Statements taken out of the lesson">
      <h3 className="font-serif text-xl text-text-primary">Taken out of the lesson</h3>
      <p className="mt-1 text-sm leading-6 text-text-secondary">
        The checks did not stand these statements up, so their sentences were taken out. Nothing below is in the lesson now.
      </p>
      <ul role="list" className="mt-4 space-y-3">
        {sentences.map((entry) => {
          const [only] = entry.statements;
          const alone = entry.statements.length === 1 && only?.statement === entry.sentence;
          return (
            <li key={entry.key} className="rounded-xl border border-border p-4">
              <p className="font-serif text-base leading-6 text-text-primary">{entry.sentence}</p>
              {alone ? <p className="mt-2 text-xs leading-5 text-text-secondary">{only.why}</p> : (
                <ul role="list" aria-label="Statements that stood on this sentence" className="mt-3 space-y-2">
                  {entry.statements.map((item) => (
                    <li key={item.claimId} className="text-sm leading-5 text-text-primary">
                      {item.statement}
                      <span className="block text-xs leading-5 text-text-secondary">{item.why}</span>
                    </li>
                  ))}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
