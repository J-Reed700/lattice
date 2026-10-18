import { useEffect, useRef, useState, type ReactNode } from 'react';

import { MessageSquarePlus, MoreHorizontal, Pencil, Trash2 } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { useTangentActions } from '@/features/chat/hooks/useTangentActions';
import { useConversationUiStore } from '@/features/chat/stores/conversationUiStore';
import type { ConversationTangentDto } from '@/lib/bindings';
import { formatRelativeTime } from '@/utils/dateUtils';

import type { TangentNode } from './tangentGroups';

/** Past this depth a nested tangent stops indenting, so the sidebar keeps its width. */
const MAX_INDENT_DEPTH = 3;

export function TangentRow({ node, depth = 0, actions, onOpen }: {
  node: TangentNode;
  depth?: number;
  actions: ReturnType<typeof useTangentActions>;
  onOpen: (tangentId: string) => void;
}) {
  const { tangent, children } = node;
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const openRef = useRef<HTMLButtonElement>(null);
  const settled = useRef(false);
  const refocus = useRef(false);
  const sending = useConversationUiStore(state => state.inFlightGenerations.has(tangent.conversationId));
  const busy = sending || actions.mutating;

  useEffect(() => {
    if (renaming) { settled.current = false; inputRef.current?.select(); }
    else if (refocus.current) { refocus.current = false; openRef.current?.focus(); }
  }, [renaming]);

  const endRename = (title: string | null, returnFocus: boolean) => {
    if (settled.current) return;
    settled.current = true;
    refocus.current = returnFocus;
    setRenaming(false);
    if (title !== null) void actions.rename(tangent, title);
  };
  const menuItem = (label: string, icon: ReactNode, action: () => void, disabled = false) => (
    <button type="button" role="menuitem" className="tangent-menu-item" disabled={disabled}
      onClick={() => { setMenuOpen(false); action(); }}>{icon}{label}</button>
  );

  return <li>
    <div className="relative rounded-lg border border-border-subtle hover:bg-surface">
      {renaming ? <div className="p-2">
        <input ref={inputRef} defaultValue={tangent.title} aria-label="Tangent title"
          className="w-full rounded border border-border-subtle bg-surface px-2 py-1 text-sm outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          onBlur={event => endRename(event.currentTarget.value, false)}
          onKeyDown={event => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing) { event.preventDefault(); endRename(event.currentTarget.value, true); }
            // Handled here, so the sidebar's Escape doesn't close it too.
            if (event.key === 'Escape') { event.preventDefault(); endRename(null, true); }
          }} />
      </div> : <button ref={openRef} type="button" onClick={() => onOpen(tangent.conversationId)}
        className="block w-full rounded-lg py-2.5 pl-3 pr-10 text-left focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring">
        <span className="block truncate text-sm font-medium">{tangent.title}</span>
        <span className="mt-1 line-clamp-2 text-xs leading-relaxed text-text-muted">{tangent.selectedText}</span>
        <time dateTime={tangent.updatedAt} className="mt-1.5 block text-[11px] text-text-muted">{formatRelativeTime(tangent.updatedAt)}</time>
      </button>}
      {!renaming && <Popover open={menuOpen} onOpenChange={setMenuOpen}>
        <PopoverTrigger asChild>
          <IconButton label={`Actions for ${tangent.title}`} aria-haspopup="menu" className="absolute right-1.5 top-1.5"><MoreHorizontal /></IconButton>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-52 p-1">
          <div role="menu" aria-label="Tangent actions" className="flex flex-col">
            {menuItem('Rename', <Pencil className="h-3.5 w-3.5" aria-hidden="true" />, () => setRenaming(true), actions.mutating)}
            {menuItem('Make conversation', <MessageSquarePlus className="h-3.5 w-3.5" aria-hidden="true" />, () => void actions.finish(tangent, 'promote'), busy)}
            {menuItem('Delete', <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />, () => void actions.finish(tangent, 'delete'), busy)}
          </div>
        </PopoverContent>
      </Popover>}
    </div>
    {children.length > 0 && <ul aria-label={`Tangents opened in ${tangent.title}`}
      className={`mt-2 space-y-2 ${depth < MAX_INDENT_DEPTH ? 'ml-3 border-l border-border-subtle pl-2' : ''}`}>
      {children.map(child => <TangentRow key={child.tangent.conversationId} node={child} depth={depth + 1} actions={actions} onOpen={onOpen} />)}
    </ul>}
  </li>;
}

/** A row for the flat "Recent activity" order, where nothing nests. */
export const flatNode = (tangent: ConversationTangentDto): TangentNode => ({ tangent, children: [] });
