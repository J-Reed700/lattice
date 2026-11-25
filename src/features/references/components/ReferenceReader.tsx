import { Bookmark } from 'lucide-react';

import { CitationVisibilityToggle } from '@/features/reading/components/CitationVisibilityToggle';
import { SourceCitations } from '@/features/reading/components/SourceCitations';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { ReferenceActionRail } from '@/features/references/components/ReferenceActionRail';
import { ReferenceAnnotationStrip } from '@/features/references/components/ReferenceAnnotationStrip';
import { ReferenceBody } from '@/features/references/components/ReferenceBody';
import { ReferenceHeader } from '@/features/references/components/ReferenceHeader';
import type { CaptureDestination } from '@/features/references/hooks/useReferenceInbox';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { ConversationSpaceDto } from '@/types/api/conversation';
import type { SourceWithMetadata } from '@/types/conversation';
import type { BookmarkPayload } from '@/utils/chatBookmarks';
import type { CapturedChatReference } from '@/utils/chatReferenceIndex';


interface ReferenceReaderProps {
  bookmark: ConversationMessageBookmarkDto | null;
  space: ConversationSpaceDto | null;
  capture: CapturedChatReference | null;
  payload: BookmarkPayload | null;
  resolutionFailed: boolean;
  captureDestination: CaptureDestination | null;
  onSaveAnnotations: (next: {
    title: string | null;
    note: string | null;
  }) => Promise<boolean>;
  onOpenInChat: () => void;
  onOpenConversation: () => void;
  onOpenCapturedNote: () => void;
  onCapture: () => Promise<void>;
  onCopy: () => Promise<boolean>;
  onDelete: () => void;
  onViewSource: (source: SourceWithMetadata) => void;
}

/**
 * Main reader pane: centered reading column containing header, body,
 * citations, annotation strip, and action rail. Spec §5.
 */
export function ReferenceReader({
  bookmark,
  space,
  capture,
  payload,
  resolutionFailed,
  captureDestination,
  onSaveAnnotations,
  onOpenInChat,
  onOpenConversation,
  onOpenCapturedNote,
  onCapture,
  onCopy,
  onDelete,
  onViewSource,
}: ReferenceReaderProps) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  if (!bookmark) {
    return (
      <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-bg">
        <div className="mx-auto flex w-full max-w-[clamp(680px,72vw,860px)] flex-1 flex-col items-center justify-center px-6 py-10">
          <div className="w-full max-w-[440px] rounded-xl border border-border-subtle bg-surface p-8 text-center shadow-sm">
            <span className="mx-auto flex h-11 w-11 items-center justify-center rounded-xl bg-accent-muted text-accent ring-1 ring-inset ring-accent/12">
              <Bookmark className="h-5 w-5" strokeWidth={1.65} />
            </span>
            <h1 className="mt-4 font-sans text-lg font-semibold tracking-[-0.02em] text-text-primary">
              Your reference shelf.
            </h1>
            <p className="mt-2 text-sm leading-relaxed text-text-secondary">
              Messages you bookmark in Chat and passages you save while reading
              collect here.
            </p>
          </div>
        </div>
      </main>
    );
  }

  const sources = payload?.sources ?? [];
  const captureLabel = resolveCaptureLabel(captureDestination);

  return (
    <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-bg">
      <div className="mx-auto flex w-full max-w-[clamp(680px,72vw,860px)] flex-1 flex-col px-6 py-8">
        <div className="rounded-xl border border-border-subtle bg-surface p-6 shadow-sm sm:p-8">
          <ReferenceHeader
            bookmark={bookmark}
            space={space}
            capture={capture}
            onOpenConversation={onOpenConversation}
            onOpenCapturedNote={onOpenCapturedNote}
          />
          <div className="mb-4 flex justify-end">
            <CitationVisibilityToggle />
          </div>
          <ReferenceBody
            bookmark={bookmark}
            payload={payload}
            resolutionFailed={resolutionFailed}
            onViewSource={onViewSource}
          />
          {showCitations && sources.length > 0 && (
            <div className="mt-6">
              <SourceCitations sources={sources} onViewSource={onViewSource} />
            </div>
          )}
          <ReferenceAnnotationStrip
            id={bookmark.id}
            title={bookmark.title}
            note={bookmark.note}
            onSave={onSaveAnnotations}
          />
          <p className="mt-6 text-xs text-text-muted">
            Lattice uses your saved references in future answers.
          </p>
          <ReferenceActionRail
            isCaptured={Boolean(capture)}
            captureDestinationLabel={captureLabel}
            onOpenInChat={onOpenInChat}
            onCopy={onCopy}
            onCapture={onCapture}
            onOpenCapturedNote={onOpenCapturedNote}
            onDelete={onDelete}
          />
        </div>
      </div>
    </main>
  );
}

function resolveCaptureLabel(destination: CaptureDestination | null): string {
  if (!destination) return 'Capture reference';
  if (destination.type === 'journal' && destination.space) {
    return `Capture to journal "${destination.space.name}"`;
  }
  return "Capture to today's Research Inbox";
}
