// Main chat components
export { ChatView } from '@/features/chat/components/ChatView';
export { ChatPanel } from '@/features/chat/components/ChatPanel';
export { ConversationSidebar } from '@/features/chat/components/ConversationSidebar';
export { ConversationSpotlight } from '@/features/chat/components/ConversationSpotlight';
export { ConversationLinkedDocumentsPanel } from '@/features/chat/components/ConversationLinkedDocumentsPanel';
export { ConversationMemoryPanel } from '@/features/chat/components/ConversationMemoryPanel';
export { Message } from '@/features/chat/components/Message';
// Backwards-compat alias — downstream code may still import MessageBubble.
export { Message as MessageBubble } from '@/features/chat/components/Message';

// Utility components
export { CitationFootnote } from '@/features/chat/components/CitationFootnote';
export { CitationRail } from '@/features/chat/components/CitationRail';
export { ChatDropStaging } from '@/features/chat/components/ChatDropStaging';
export { ChatModelNotice } from '@/features/chat/components/ChatModelNotice';
export { ChatStarters } from '@/features/chat/components/ChatStarters';
export { FilePreviewModal } from '@/features/chat/components/FilePreviewModal';
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

// Viewers
export { ImageViewer } from '@/features/chat/components/viewers/ImageViewer';
export { MarkdownViewer } from '@/features/chat/components/viewers/MarkdownViewer';
export { PDFViewer } from '@/features/chat/components/viewers/PDFViewer';
export { TextViewer } from '@/features/chat/components/viewers/TextViewer';
