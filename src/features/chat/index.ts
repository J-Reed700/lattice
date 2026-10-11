// Main chat components
export { ChatView } from '@/features/chat/components/ChatView';
export { ChatPanel } from '@/features/chat/components/ChatPanel';
export { ConversationSidebar } from '@/features/chat/components/ConversationSidebar';
export { ConversationSpotlight } from '@/features/chat/components/ConversationSpotlight';
export { ConversationLinkedDocumentsPanel } from '@/features/chat/components/ConversationLinkedDocumentsPanel';
export { ConversationMemoryPanel } from '@/features/chat/components/ConversationMemoryPanel';
export { Message } from '@/features/chat/components/Message';

// Utility components
export { CitationFootnote } from '@/features/chat/components/CitationFootnote';
export { ChatDropStaging } from '@/features/chat/components/ChatDropStaging';
export { ChatModelNotice } from '@/features/chat/components/ChatModelNotice';
export { ChatStarters } from '@/features/chat/components/ChatStarters';
export { MessageEditor } from '@/features/chat/components/MessageEditor';
export { ModelPickerPopover } from '@/features/chat/components/ModelPickerPopover';
export { useChatFileDrop } from '@/features/chat/hooks/useChatFileDrop';
export type { StagedFile } from '@/features/chat/hooks/useChatFileDrop';
export {
  isReferencedSource,
  isVaultNoteSource,
  provenanceLabel,
  sourceProvenance,
} from '@/features/chat/model/sourceProvenance';
export type { SourceProvenance } from '@/features/chat/model/sourceProvenance';
