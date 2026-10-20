/**
 * ConnectionRolesDialog
 *
 * The roles menu a connection row (llama.cpp, Ollama, a cloud model) opens
 * from its Configure button: what the connection serves, then the role
 * toggles it can take. A downloaded model opens LocalModelConfigModal
 * instead, which also holds its load settings.
 */

import type { ReactNode } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { ChevronLeft } from 'lucide-react';

interface ConnectionRolesDialogProps {
  title: string;
  /** Label over the served model, e.g. "Connected model". */
  servedLabel: string;
  served: string;
  /** A second line under the served model, such as the server URL. */
  detail?: string;
  /** The line under the toggles. */
  hint: string;
  onClose: () => void;
  /** The role toggles. */
  children: ReactNode;
}

export function ConnectionRolesDialog({
  title,
  servedLabel,
  served,
  detail,
  hint,
  onClose,
  children,
}: ConnectionRolesDialogProps) {
  return (
    <Dialog.Root open onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/60 backdrop-blur-[2px]" />
        <Dialog.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-1/2 z-50 w-full max-w-[520px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border-subtle bg-surface-raised shadow-2xl outline-none"
        >
          <div className="relative flex items-center justify-center border-b border-border-subtle px-5 py-3.5">
            <button
              type="button"
              onClick={onClose}
              aria-label="Close"
              className="absolute left-4 flex h-7 w-7 items-center justify-center rounded-md text-text-secondary transition-colors hover:bg-surface hover:text-text-primary"
            >
              <ChevronLeft className="h-4 w-4" />
            </button>
            <Dialog.Title className="truncate text-sm font-semibold text-text-primary">{title}</Dialog.Title>
          </div>

          <div className="space-y-5 px-6 py-5">
            <div>
              <div className="mb-1.5 text-xs font-medium text-text-secondary">{servedLabel}</div>
              <div className="flex flex-col gap-0.5 rounded-lg border border-border-subtle bg-surface px-3.5 py-2.5">
                <span className="truncate text-sm font-medium text-text-primary">{served}</span>
                {detail ? <span className="truncate font-mono text-xs text-text-muted">{detail}</span> : null}
              </div>
            </div>

            <div>
              <div className="mb-1.5 text-xs font-medium text-text-secondary">Roles</div>
              <div className="flex flex-wrap items-center gap-1.5">{children}</div>
              <p className="mt-1.5 text-[11px] text-text-muted">{hint}</p>
            </div>
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-border-subtle px-6 py-4">
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-white shadow-xs transition-opacity hover:opacity-90"
            >
              Done
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
