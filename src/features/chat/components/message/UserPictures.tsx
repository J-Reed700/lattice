import { useState } from 'react';

import { MediaLightbox } from '@/components/MediaLightbox';
import { useApi } from '@/contexts/ApiContext';
import { chatImageUrl } from '@/features/chat/images/chatImageUrl';
import { restagePicture } from '@/features/chat/images/stagedPictures';
import type { ChatImage } from '@/types/conversation';

interface UserPicturesProps {
  pictures: readonly ChatImage[];
  /** `false` when the model was told the pictures were there instead of shown them. */
  picturesSeen: boolean | null;
  /** The model that answered, when its turn record says. */
  answeredBy: string | null;
  /**
   * Where a copy can be staged for the next turn: set once the message is
   * saved, and only while the chat's model can see pictures.
   */
  restageIn: string | null;
}

const RESTAGE_CLASS =
  'rounded-md bg-surface-raised px-1.5 py-0.5 text-xxs text-[hsl(var(--text-secondary))] shadow-sheet transition-colors duration-fast hover:text-[hsl(var(--text-primary))] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring';

/**
 * The pictures a user message carried, from the same stored files and the
 * same enlarged view as an answer's. A later question about one is answered
 * from its caption unless it is shown again, so each offers that.
 */
export function UserPictures({ pictures, picturesSeen, answeredBy, restageIn }: UserPicturesProps) {
  const api = useApi();
  const [open, setOpen] = useState<ChatImage | null>(null);
  const [restaging, setRestaging] = useState<string | null>(null);
  if (pictures.length === 0) return null;

  const restage = async (picture: ChatImage) => {
    if (!restageIn || restaging) return;
    setRestaging(picture.id);
    try {
      await restagePicture(restageIn, picture.id, api);
    } finally {
      setRestaging(null);
    }
  };

  const nameOf = (picture: ChatImage) => picture.originalName ?? picture.caption;
  // Whether the model saw them is recorded per message, so one line covers
  // the row; a chip per picture would set each column's width by its text.
  const notShown = picturesSeen === false ? `Not shown to ${answeredBy ?? 'the model'}` : null;

  const restageButton = (picture: ChatImage, className: string) => (restageIn ? (
    <button
      type="button"
      onClick={() => void restage(picture)}
      disabled={restaging !== null}
      className={className}
    >
      {restaging === picture.id ? 'Adding…' : 'Show to the model again'}
    </button>
  ) : null);

  return (
    <div className="mt-1.5 ml-auto flex w-fit max-w-[85%] flex-col items-end gap-1">
      <div className="flex flex-wrap justify-end gap-2">
        {pictures.map((picture) => (
          <figure key={picture.id} className="group/picture relative m-0">
            <button
              type="button"
              onClick={() => setOpen(picture)}
              aria-label={`Enlarge picture: ${nameOf(picture)}`}
              className="block overflow-hidden rounded-xl bg-surface-sunken shadow-sheet focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            >
              <img
                src={chatImageUrl(picture)}
                alt={nameOf(picture)}
                width={picture.width}
                height={picture.height}
                loading="lazy"
                decoding="async"
                className="h-32 w-auto max-w-64 object-cover"
              />
            </button>
            {restageButton(
              picture,
              `${RESTAGE_CLASS} absolute bottom-1.5 right-1.5 opacity-0 group-hover/picture:opacity-100 focus-visible:opacity-100 disabled:opacity-60`,
            )}
          </figure>
        ))}
      </div>
      {notShown && (
        <p
          className="max-w-full truncate rounded-sm bg-warning-muted px-1.5 py-px text-xxs text-warning-fg"
          title="This model cannot see pictures; it was told they were attached."
        >
          {notShown}
        </p>
      )}
      {open && (
        <MediaLightbox
          open
          onOpenChange={(isOpen) => { if (!isOpen) setOpen(null); }}
          title={nameOf(open)}
          caption={`${open.width}×${open.height}${notShown ? ` · ${notShown}` : ''}`}
          actions={restageButton(open, 'media-lightbox-action')}
        >
          <img
            className="media-lightbox-image"
            src={chatImageUrl(open)}
            alt={nameOf(open)}
            width={open.width}
            height={open.height}
          />
        </MediaLightbox>
      )}
    </div>
  );
}
