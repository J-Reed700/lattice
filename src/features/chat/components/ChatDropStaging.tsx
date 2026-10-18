import { X } from 'lucide-react';
import { Link } from 'react-router';

import { IconButton } from '@/components/ui';
import type { StagedFile } from '@/features/chat/hooks/useChatFileDrop';
import { chatImageUrl } from '@/features/chat/images/chatImageUrl';
import type { ChatImage } from '@/types/conversation';

/**
 * The staged-files row above the composer.
 *
 * Staged files join the conversation when the message is sent — there is no
 * separate "add" step to forget. This row says what is queued and offers the
 * ways out: remove one file, or cancel them all. Pictures show as thumbnails
 * of the stored file, and say so when the model answering cannot see them.
 */
export interface ChatDropStagingProps {
  staged: StagedFile[];
  isImporting: boolean;
  onRemove: (_path: string) => void;
  onClear: () => void;
  pictures?: readonly ChatImage[];
  isAttachingPictures?: boolean;
  onRemovePicture?: (_id: string) => void;
  /** The model answering this chat, when it is not declared to see pictures. */
  blindModelName?: string | null;
}

const plural = (count: number, noun: string) => `${count} ${noun}${count !== 1 ? 's' : ''}`;

export function ChatDropStaging({
  staged,
  isImporting,
  onRemove,
  onClear,
  pictures = [],
  isAttachingPictures = false,
  onRemovePicture,
  blindModelName = null,
}: ChatDropStagingProps) {
  if (staged.length === 0 && pictures.length === 0 && !isAttachingPictures) return null;

  const preview = staged.slice(0, 3);
  const queued = [
    pictures.length > 0 && plural(pictures.length, 'picture'),
    staged.length > 0 && plural(staged.length, 'file'),
  ].filter(Boolean).join(' and ');

  return (
    <div className="border-t border-subtle px-6 py-2 text-xs">
      {pictures.length > 0 || isAttachingPictures ? (
        <ul className="mb-2 flex flex-wrap gap-2" aria-label="Pictures for your next message">
          {pictures.map((picture) => {
            const name = picture.originalName ?? picture.caption;
            return (
              <li key={picture.id} className="relative">
                <img
                  src={chatImageUrl(picture)}
                  alt={name}
                  title={name}
                  width={picture.width}
                  height={picture.height}
                  className="h-14 w-14 rounded-md bg-surface-sunken object-cover shadow-sheet"
                />
                {onRemovePicture && (
                  <IconButton
                    label={`Remove ${name}`}
                    className="absolute -right-1.5 -top-1.5 h-5 w-5 rounded-full bg-surface-raised shadow-sheet [&>svg]:h-3 [&>svg]:w-3"
                    onClick={() => onRemovePicture(picture.id)}
                  >
                    <X />
                  </IconButton>
                )}
              </li>
            );
          })}
          {isAttachingPictures && (
            <li className="h-14 w-14 animate-pulse rounded-md bg-surface-sunken" aria-label="Attaching a picture" />
          )}
        </ul>
      ) : null}
      {pictures.length > 0 && blindModelName ? (
        <p role="note" className="mb-2 flex flex-wrap items-center gap-x-2 rounded-md bg-warning-muted px-2.5 py-1.5 text-warning-fg">
          <span>{blindModelName} can&apos;t see pictures. It will be told they&apos;re attached, but won&apos;t see them.</span>
          <Link to="/settings?tab=downloaded-models" className="underline underline-offset-2">
            Vision settings
          </Link>
        </p>
      ) : null}
      {queued ? (
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2 text-[hsl(var(--text-secondary))]">
            <span className="shrink-0">
              {isImporting ? `Adding ${queued}…` : `${queued} — added when you send`}
            </span>
            <span className="min-w-0 truncate text-[hsl(var(--text-muted))]">
              {preview.map((file, index) => (
                <span key={file.path}>
                  {index > 0 && ' · '}
                  {file.name}
                  {!isImporting && (
                    <IconButton
                      label={`Remove ${file.name}`}
                      className="ml-1 h-4 w-4 align-middle [&>svg]:h-3 [&>svg]:w-3"
                      onClick={() => onRemove(file.path)}
                    >
                      <X />
                    </IconButton>
                  )}
                </span>
              ))}
            </span>
          </div>
          <div className="flex shrink-0 items-center gap-3">
            <button
              type="button"
              onClick={onClear}
              disabled={isImporting}
              className="text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))] disabled:cursor-not-allowed disabled:opacity-50"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
