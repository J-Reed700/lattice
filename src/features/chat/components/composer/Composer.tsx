import type { ReactNode, RefObject } from 'react';

import * as Popover from '@radix-ui/react-popover';
import { ArrowUp, ChevronDown, Library, Paperclip, Settings2, Square } from 'lucide-react';

import { ChatDropStaging } from '@/features/chat/components/ChatDropStaging';
import { ChatModelNotice, type ChatModelNoticeProps } from '@/features/chat/components/ChatModelNotice';
import { ComposerSuggest } from '@/features/chat/components/composer/ComposerSuggest';
import { FocusChips } from '@/features/chat/components/composer/FocusChips';
import { ModeChips } from '@/features/chat/components/composer/ModeChips';
import { ComposerControls } from '@/features/chat/components/ComposerControls';
import { ModelPickerPopover } from '@/features/chat/components/ModelPickerPopover';
import type { AttachmentImport } from '@/features/chat/hooks/useAttachmentImport';
import type { ComposerState } from '@/features/chat/hooks/useComposer';
import { SpacePickerPopover } from '@/features/spaces/components/SpacePickerPopover';

/** The textarea points at the suggestion list through this, for screen readers. */
const SUGGEST_LIST_ID = 'composer-suggest-list';

interface ComposerProps {
  composer: ComposerState;
  attachments: AttachmentImport;
  modelNotice: ChatModelNoticeProps;
  /** A line above the composer from the surface hosting this chat. */
  notice?: ReactNode;
  /** Chips at the top of the composer for what the host sends with the next turn. */
  chips?: ReactNode;
  /** The space this chat searches, shown when there is a choice to make. */
  space: { id: string | null; name: string | null; onChange: (_spaceId: string, _spaceName: string) => void } | null;
  model: {
    id: string | null;
    label: string | null;
    onSelect: (_modelId: string, _modelLabel: string) => void | Promise<void>;
    /** The palette opens the picker by clicking this. */
    triggerRef: RefObject<HTMLButtonElement | null>;
  };
  isSending: boolean;
  isCompacting: boolean;
  isChatUnavailable: boolean;
  onCancel: () => void;
}

/** The composer pinned to the bottom of the chat: staged files, notices and the page you write on. */
export function Composer({
  composer,
  attachments,
  modelNotice,
  notice,
  chips,
  space,
  model,
  isSending,
  isCompacting,
  isChatUnavailable,
  onCancel,
}: ComposerProps) {
  const {
    input,
    setInput,
    textareaRef,
    placeholder,
    focusLabel,
    suggest,
    focusDocuments,
    applyFocusDocuments,
  } = composer;

  return (
    <div className="relative bg-bg before:pointer-events-none before:absolute before:inset-x-0 before:-top-8 before:h-8 before:bg-linear-to-t before:from-[hsl(var(--bg))] before:to-transparent">
      <ChatDropStaging
        staged={attachments.staged}
        isImporting={attachments.isImporting}
        onRemove={attachments.remove}
        onClear={attachments.clear}
      />

      <ChatModelNotice {...modelNotice} />
      {notice}

      <form onSubmit={composer.handleSubmit} className="chat-column chat-beside-margin mx-auto w-full px-6 pb-5 pt-1">
        {/* One object: the page you write on, with its tools along the bottom edge. */}
        <div className="rounded-2xl bg-surface shadow-sheet transition-shadow duration-base focus-within:shadow-[var(--shadow-sheet),0_0_0_3px_hsl(var(--accent)/0.16)]">
          {chips}
          <FocusChips
            documents={focusDocuments}
            onRemove={(documentId) =>
              applyFocusDocuments(
                focusDocuments.filter((document) => document.documentId !== documentId)
              )
            }
            onClear={() => applyFocusDocuments([])}
          />

          <div className="composer-caret-field">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => {
                setInput(e.target.value);
                suggest.syncCaret(e.target);
              }}
              onKeyDown={composer.handleKeyDown}
              onKeyUp={(e) => suggest.syncCaret(e.currentTarget)}
              onClick={(e) => suggest.syncCaret(e.currentTarget)}
              onFocus={() => suggest.setFocused(true)}
              onBlur={() => suggest.setFocused(false)}
              placeholder={placeholder}
              rows={1}
              aria-label="Message composer"
              aria-autocomplete="list"
              aria-expanded={suggest.isOpen}
              aria-controls={suggest.isOpen ? SUGGEST_LIST_ID : undefined}
              aria-activedescendant={
                suggest.isOpen ? `${SUGGEST_LIST_ID}-${suggest.activeIndex}` : undefined
              }
              className="block w-full resize-none bg-transparent px-4 pb-1 pt-3.5 font-sans text-[15px] leading-[1.55] text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-hidden disabled:cursor-not-allowed disabled:opacity-50"
              style={{ minHeight: '44px', maxHeight: '240px' }}
            />

            {suggest.isOpen && suggest.point && (
              <ComposerSuggest
                items={suggest.items}
                activeIndex={suggest.activeIndex}
                heading={
                  suggest.trigger?.kind === 'mention'
                    ? space?.name
                      ? `Documents in ${space.name}`
                      : 'Documents this chat can read'
                    : 'Commands'
                }
                emptyLabel={
                  suggest.trigger?.kind === 'mention' ? 'Looking…' : 'No command matches.'
                }
                point={suggest.point}
                listId={SUGGEST_LIST_ID}
                onSelect={suggest.accept}
                onHover={suggest.setActiveIndex}
              />
            )}
          </div>

          {/* The tools along the bottom edge. The left group wraps when the
              modes fill it; the send button stays on the right either way. */}
          <div className="flex items-end gap-1 px-2.5 pb-2.5 pt-1">
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1">
              {/* Controls trigger (left) */}
            <Popover.Root open={composer.isControlsOpen} onOpenChange={composer.setIsControlsOpen}>
              <Popover.Trigger asChild>
                <button
                  type="button"
                  aria-label="Composer controls"
                  title="Turn mode and tools"
                  className={`pressable inline-flex h-7 w-7 items-center justify-center rounded-md transition-[background-color,color,scale] duration-fast ${
                    composer.isControlsOpen
                      ? 'bg-[hsl(var(--text-primary)/0.08)] text-[hsl(var(--text-primary))]'
                      : 'text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]'
                  }`}
                >
                  <Settings2 className="h-[15px] w-[15px]" strokeWidth={1.6} />
                </button>
              </Popover.Trigger>
              <Popover.Portal>
                <Popover.Content
                  side="top"
                  align="start"
                  sideOffset={8}
                  className="surface-pop z-50 w-[320px] max-h-[480px] overflow-y-auto rounded-xl bg-surface-overlay p-4 text-[hsl(var(--text-primary))] shadow-lg outline-hidden"
                >
                  <ComposerControls
                    turnMode={composer.turnMode}
                    onTurnModeChange={composer.handleTurnModeChange}
                    toolPreferences={composer.toolPreferences}
                    onToggleKnowledgeBase={composer.toggleKnowledgeBase}
                    onToggleWebTools={composer.toggleWebTools}
                    onToggleWikiTools={composer.toggleWikiTools}
                    onToggleDeepResearch={composer.toggleDeepResearch}
                    customTools={composer.customTools}
                    enabledToolSet={composer.enabledToolSet}
                    onToggleCustomTool={composer.toggleCustomTool}
                  />
                </Popover.Content>
              </Popover.Portal>
            </Popover.Root>

            {/* Dropping files on the thread works, but only once you know it
                does. The same flow the palette runs, in reach of the caret. */}
            <button
              type="button"
              onClick={() => void attachments.chooseFiles()}
              aria-label="Add files to this conversation"
              title="Add files to this conversation"
              className="pressable inline-flex h-7 w-7 items-center justify-center rounded-md text-[hsl(var(--text-tertiary))] transition-[background-color,color,scale] duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]"
            >
              <Paperclip className="h-[15px] w-[15px]" strokeWidth={1.6} />
            </button>

            {/* What a chat can search decides its answers more than the model
                does, so it sits beside the model, where the question is typed. */}
            {space && (
              <SpacePickerPopover
                activeSpaceId={space.id}
                heading="This chat searches"
                onSelect={space.onChange}
              >
                <button
                  type="button"
                  title="The documents this chat searches"
                  aria-label={`${focusLabel ?? `Searching ${space.name}`}. Change space`}
                  className="inline-flex h-7 min-w-0 items-center gap-1 rounded-md px-2 text-xs text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]"
                >
                  <Library className="h-3 w-3 shrink-0 opacity-70" strokeWidth={1.75} aria-hidden="true" />
                  <span className="truncate">{focusLabel ?? space.name}</span>
                  <ChevronDown className="h-3 w-3 shrink-0 opacity-70" strokeWidth={1.75} />
                </button>
              </SpacePickerPopover>
            )}

            {model.label && (
              <ModelPickerPopover
                activeModelId={model.id}
                onSelect={model.onSelect}
                align="start"
              >
                <button
                  ref={model.triggerRef}
                  type="button"
                  title="Active chat model"
                  className="inline-flex h-7 min-w-0 items-center gap-1 rounded-md px-2 text-xs text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]"
                >
                  <span className="truncate">{model.label}</span>
                  <ChevronDown className="h-3 w-3 shrink-0 opacity-70" strokeWidth={1.75} />
                </button>
              </ModelPickerPopover>
            )}

            <ModeChips
              turnMode={composer.turnMode}
              knowledgeBase={composer.composerMode.knowledgeBase}
              webSearch={composer.composerMode.webSearch}
              wikipedia={composer.composerMode.wikipedia}
              deepResearch={composer.composerMode.deepResearch}
              customTools={composer.enabledCustomToolNames}
              onRemove={composer.removeMode}
              onRemoveTool={composer.toggleCustomTool}
            />
            </div>

            <span className="hidden shrink-0 self-center pr-1 text-[11px] text-[hsl(var(--text-muted))] sm:block">
              {isSending
                ? 'Generating…'
                : isCompacting
                  ? 'Compacting context…'
                  : input.trim()
                    ? '↵ send · ⇧↵ new line'
                    : ''}
            </span>

            {/* Send / Stop */}
            {isSending ? (
              <button
                type="button"
                onClick={onCancel}
                aria-label="Stop generating response"
                title="Stop"
                className="pressable inline-flex h-8 w-8 items-center justify-center rounded-full bg-[hsl(var(--text-primary))] text-[hsl(var(--bg))] transition-[scale,opacity] duration-fast hover:opacity-90"
              >
                <Square className="h-3 w-3 fill-current" />
              </button>
            ) : (
              <button
                type="submit"
                disabled={!input.trim() || isChatUnavailable || isCompacting}
                aria-label="Send message"
                title={isCompacting ? 'Waiting for the context to finish compacting' : 'Send · Enter'}
                className="pressable inline-flex h-8 w-8 items-center justify-center rounded-full bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] shadow-action transition-[background-color,color,scale,box-shadow] duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:bg-[hsl(var(--text-primary)/0.08)] disabled:text-[hsl(var(--text-disabled))] disabled:shadow-none"
              >
                <ArrowUp className="h-4 w-4" strokeWidth={2.2} />
              </button>
            )}
          </div>
        </div>
      </form>
    </div>
  );
}
