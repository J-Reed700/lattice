import { useCallback, useEffect, useId, useRef } from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowDownToLine, ChevronDown, ChevronUp, List, PanelRightClose } from 'lucide-react';

import { useChatNavigationStore } from '@/features/chat/stores/chatNavigationStore';
import type { ConversationMessage, OptimisticMessage } from '@/types/conversation';

import '@/features/chat/components/conversation-navigator.css';

type NavigableMessage = ConversationMessage | OptimisticMessage;

interface ConversationNavigatorProps {
  messages: NavigableMessage[];
  activeIndex: number;
  getKey: (message: NavigableMessage) => string;
  onNavigate: (index: number) => void;
  onLatest: () => void;
}

function roleLabel(role: string): string {
  if (role === 'user') return 'You';
  if (role === 'assistant') return 'Assistant';
  return 'System';
}

function preview(message: NavigableMessage): string {
  // Bound the work even for a large answer streaming on every token.
  const text = message.content.slice(0, 500)
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/[#*`_>~]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  if (!text) return message.status === 'pending' || message.status === 'processing' ? 'Thinking…' : 'Empty message';
  return text.length > 96 ? `${text.slice(0, 96)}…` : text;
}

/** Small, fixed-height rows keep even a very long outline cheap to open. */
function Checkpoints({ messages, activeIndex, getKey, onNavigate, id }: Omit<ConversationNavigatorProps, 'onLatest'> & { id: string }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const getItemKey = useCallback((index: number) => getKey(messages[index]), [getKey, messages]);
  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => scrollRef.current,
    getItemKey,
    estimateSize: () => 60,
    overscan: 3,
  });

  useEffect(() => {
    virtualizer.scrollToIndex(activeIndex, { align: 'auto' });
  }, [activeIndex, virtualizer]);

  return (
    <div id={id} ref={scrollRef} className="conversation-checkpoints" aria-label="Message checkpoints">
      <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {virtualizer.getVirtualItems().map((row) => {
          const message = messages[row.index];
          const label = preview(message);
          const role = roleLabel(message.role);
          return (
            <button
              key={row.key}
              type="button"
              className="conversation-checkpoint"
              style={{ height: row.size, transform: `translateY(${row.start}px)` }}
              aria-label={`Message ${row.index + 1}, ${role}: ${label}`}
              aria-current={row.index === activeIndex ? 'location' : undefined}
              title={label}
              onClick={() => onNavigate(row.index)}
            >
              <span className="conversation-checkpoint-number">{row.index + 1}</span>
              <span className="min-w-0">
                <span className="block text-[10px] font-medium text-text-muted">{role}</span>
                <span className="line-clamp-2 text-[11px] leading-[15px]">{label}</span>
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function ConversationNavigator({ messages, activeIndex, getKey, onNavigate, onLatest }: ConversationNavigatorProps) {
  const expanded = useChatNavigationStore((state) => state.expanded);
  const setExpanded = useChatNavigationStore((state) => state.setExpanded);
  const toggleRef = useRef<HTMLButtonElement>(null);
  const checkpointsId = useId();
  if (messages.length < 2) return null;
  const current = Math.min(Math.max(activeIndex, 0), messages.length - 1);

  return (
    <nav
      className="conversation-navigator"
      data-expanded={expanded || undefined}
      aria-label="Conversation navigation"
      onKeyDown={(event) => {
        if (event.key === 'Escape' && expanded) {
          event.stopPropagation();
          setExpanded(false);
          toggleRef.current?.focus();
        }
      }}
    >
      <div className="conversation-navigator-heading">
        {expanded && <span className="pl-2 text-[11px] font-medium">Messages</span>}
        <button
          ref={toggleRef}
          type="button"
          className="conversation-navigator-control"
          aria-label={expanded ? 'Collapse conversation outline' : 'Expand conversation outline'}
          aria-expanded={expanded}
          aria-controls={expanded ? checkpointsId : undefined}
          title={expanded ? 'Collapse conversation outline' : 'Conversation outline'}
          onClick={() => setExpanded(!expanded)}
        >
          {expanded ? <PanelRightClose size={15} /> : <List size={16} />}
        </button>
      </div>
      <div className="conversation-navigator-position" title={`Message ${current + 1} of ${messages.length} · ${roleLabel(messages[current].role)}`}>
        {expanded ? `Message ${current + 1} of ${messages.length}` : <><span>{current + 1}</span><span className="text-text-muted">/{messages.length}</span></>}
      </div>
      {expanded && <Checkpoints id={checkpointsId} messages={messages} activeIndex={current} getKey={getKey} onNavigate={onNavigate} />}
      <div className="conversation-navigator-actions">
        <button type="button" className="conversation-navigator-control" aria-label="Previous message" title="Previous message" disabled={current === 0} onClick={() => onNavigate(current - 1)}>
          <ChevronUp size={15} />
        </button>
        <button type="button" className="conversation-navigator-control" aria-label="Next message" title="Next message" disabled={current === messages.length - 1} onClick={() => onNavigate(current + 1)}>
          <ChevronDown size={15} />
        </button>
        <button type="button" className="conversation-navigator-control" aria-label="Jump to latest message" title="Jump to latest message" onClick={onLatest}>
          <ArrowDownToLine size={14} />
          {expanded && <span className="ml-1 text-[11px]">Latest</span>}
        </button>
      </div>
    </nav>
  );
}
