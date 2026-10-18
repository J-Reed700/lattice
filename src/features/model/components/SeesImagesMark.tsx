import { Eye } from 'lucide-react';

/**
 * The catalog says this model can see pictures. Its projector is a separate
 * download, offered on the model once it is installed, so a memory or size
 * figure beside this mark is for the weights alone.
 */
export function SeesImagesMark() {
  return (
    <span
      className="inline-flex shrink-0 items-center gap-1 rounded-md bg-accent-muted px-1.5 py-0.5 text-[11px] font-medium text-accent"
      title="Can see the pictures you attach once its vision projector is downloaded (a separate download, not counted in the size shown)."
    >
      <Eye className="h-3 w-3" aria-hidden="true" />
      can see images
    </span>
  );
}
