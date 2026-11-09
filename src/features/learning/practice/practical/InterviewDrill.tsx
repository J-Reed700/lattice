import { useEffect, useState } from "react";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Layers, LoaderCircle } from "lucide-react";

import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { useApi } from "@/contexts/ApiContext";
import { practiceHomeApi } from "@/features/learning/practice/home/practiceHomeApi";
import { FlashcardSession } from "@/features/learning/recall/flashcards/FlashcardSession";
import type { CreateInterviewStudyDeckRequestDto, LearningPracticalActivityDto, StudyDeckDto } from "@/lib/bindings";

/** An instant as a datetime-local value. */
function toLocalInput(ms: number) {
  return new Date(ms - new Date(ms).getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
}

/**
 * Turns an interview's questions and private answer guides into a recall
 * deck. The mock interview is the realistic rehearsal; the drill is where each
 * answer gets retrieved from memory again and again, spaced to fit the
 * interview date, with the guide shown only after each attempt.
 */
export function InterviewDrill({ programId, activity }: { programId: string; activity: LearningPracticalActivityDto }) {
  const api = useApi();
  const client = useQueryClient();
  const [deck, setDeck] = useState<StudyDeckDto | null>(null);
  const create = useMutation({
    mutationFn: async (request: CreateInterviewStudyDeckRequestDto) => {
      const response = await api.createInterviewStudyDeck(request);
      if (!response.ok) throw new Error(typeof response.details?.details === "string" ? response.details.details : response.error);
      return response.data;
    },
    onSuccess: (made) => {
      client.setQueryData(["study", "deck", made.id], made);
      void client.invalidateQueries({ queryKey: ["study"] });
      void client.invalidateQueries({ queryKey: ["learning-memory"] });
      setDeck(made);
    },
  });
  const profile = useQuery({ queryKey: ["learning-practice-profile", programId], queryFn: () => practiceHomeApi.getProfile(programId), retry: false });
  const [when, setWhen] = useState("");
  const profileDate = profile.data?.interviewAt ?? null;
  useEffect(() => {
    if (profileDate !== null) setWhen((current) => current || toLocalInput(profileDate));
  }, [profileDate]);
  const questions = activity.interviewPlan?.questions?.length ?? 0;
  const targetAt = when ? new Date(when).getTime() : NaN;
  const validTarget = Number.isFinite(targetAt) && targetAt > Date.now();
  if (!questions) return null;
  return (
    <section aria-labelledby={`interview-drill-${activity.id}`} className="rounded-2xl border border-border bg-surface p-4 sm:p-5">
      <div className="flex items-start gap-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent">
          <Layers size={18} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-xs font-semibold uppercase tracking-[.14em] text-accent">Spaced drill</div>
          <h3 id={`interview-drill-${activity.id}`} className="mt-1 font-serif text-xl text-text-primary">
            Drill these {questions} {questions === 1 ? "question" : "questions"} until they stick
          </h3>
          <p className="mt-1 text-xs leading-5 text-text-secondary">
            Each question becomes a card. You answer it aloud from memory, then tick off the points a strong answer covers. Misses come back in the same session, and reviews are spaced to land before your interview. Works best after one cold attempt at the mock interview above, and before a final one. Making it again opens the same deck, reviews kept, with the new date.
          </p>
        </div>
      </div>
      <form
        className="mt-4 flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          if (create.isPending) return;
          create.mutate({ programId, activityId: activity.id, targetAt: validTarget ? targetAt : null });
        }}
      >
        <label className="text-xs font-medium text-text-secondary">
          Interview date and time
          <input
            type="datetime-local"
            value={when}
            onChange={(event) => setWhen(event.target.value)}
            className="mt-1.5 block rounded-lg border border-border bg-background px-3 py-2 text-sm"
          />
        </label>
        <button
          type="submit"
          disabled={create.isPending}
          className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-45"
        >
          {create.isPending ? <LoaderCircle size={13} className="animate-spin" /> : <Layers size={13} />}
          {create.isPending ? "Making the drill…" : "Make drill deck"}
        </button>
        {!when && <span className="text-xs text-text-muted">Add your interview date to space the reviews before it.</span>}
        {when && !validTarget && <span className="text-xs text-text-muted">That time has passed; the deck will use its normal schedule.</span>}
      </form>
      {create.isError && (
        <p role="alert" className="mt-3 rounded-lg bg-danger-muted p-3 text-xs text-danger-fg">
          {create.error.message}
        </p>
      )}
      <Dialog open={deck !== null} onOpenChange={(open) => { if (!open) setDeck(null); }}>
        <DialogContent className="max-h-[92dvh] max-w-3xl overflow-y-auto rounded-2xl border border-border-default bg-surface p-5 sm:p-6">
          <DialogTitle className="pr-6 font-serif text-xl text-text-primary">{deck?.title}</DialogTitle>
          <DialogDescription className="sr-only">Review the cards made from this interview.</DialogDescription>
          {deck && <FlashcardSession cards={deck.cards} mode="flashcard" title={deck.title} targetAt={deck.targetAt} onClose={() => setDeck(null)} />}
        </DialogContent>
      </Dialog>
    </section>
  );
}
