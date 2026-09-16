import { Bookmark } from 'lucide-react';

import { SourceCitations } from '@/components/Chat/SourceCitations';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { ConversationSpaceDto } from '@/types/api/conversation';
import type { SourceWithMetadata } from '@/types/conversation';
import type { BookmarkPayload } from '@/utils/chatBookmarks';
import type { CapturedChatReference } from '@/utils/chatReferenceIndex';

import { ReferenceActionRail } from './ReferenceActionRail';
import { ReferenceAnnotationStrip } from './ReferenceAnnotationStrip';
import { ReferenceBody } from './ReferenceBody';
import { ReferenceHeader } from './ReferenceHeader';

import type { CaptureDestination } from './useReferenceInbox';

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
  if (!bookmark) {
    return (
      <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-[hsl(var(--bg))]">
        <div className="mx-auto flex w-full max-w-[clamp(680px,72vw,900px)] flex-1 flex-col items-center justify-center px-6 py-10 text-center">
          <Bookmark
            className="h-10 w-10 text-[hsl(var(--text-muted))]"
            strokeWidth={1.5}
          />
          <h1 className="mt-4 font-serif text-xl font-semibold text-[hsl(var(--text-primary))]">
            Your reference shelf.
          </h1>
          <p className="mt-3 max-w-[400px] text-sm text-[hsl(var(--text-tertiary))]">
            Messages you bookmark in Chat and passages you save while reading
            collect here.
          </p>
        </div>
      </main>
    );
  }

  const sources = payload?.sources ?? [];
  const captureLabel = resolveCaptureLabel(captureDestination);

  return (
    <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-[hsl(var(--bg))]">
      <div className="mx-auto flex w-full max-w-[clamp(680px,72vw,900px)] flex-1 flex-col px-6 pt-10 pb-8">
        <ReferenceHeader
          bookmark={bookmark}
          space={space}
          capture={capture}
          onOpenConversation={onOpenConversation}
          onOpenCapturedNote={onOpenCapturedNote}
        />
        <ReferenceBody
          bookmark={bookmark}
          payload={payload}
          resolutionFailed={resolutionFailed}
        />
        {sources.length > 0 && (
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
        <p className="mt-6 text-xs text-[hsl(var(--text-muted))]">
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
