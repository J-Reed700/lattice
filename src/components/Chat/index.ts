// Main chat components
export { ChatView } from './ChatView';
export { ChatPanel } from './ChatPanel';
export { ConversationSidebar } from './ConversationSidebar';
export { ConversationSpotlight } from './ConversationSpotlight';
export { ConversationLinkedDocumentsPanel } from './ConversationLinkedDocumentsPanel';
export { Message } from './Message';
// Backwards-compat alias — downstream code may still import MessageBubble.
export { Message as MessageBubble } from './Message';

// Utility components
export { CitationFootnote } from './CitationFootnote';
export { CitationRail } from './CitationRail';
export { ChatDropStaging } from './ChatDropStaging';
export { ChatModelNotice } from './ChatModelNotice';
export { ChatStarters } from './ChatStarters';
export { FilePreviewModal } from './FilePreviewModal';
export { MessageEditor } from './MessageEditor';
export { ModelPickerPopover } from './ModelPickerPopover';
export { useChatFileDrop } from './useChatFileDrop';
export type { StagedFile } from './useChatFileDrop';
export {
  isReferencedSource,
  isVaultNoteSource,
  provenanceLabel,
  sourceProvenance,
} from './sourceProvenance';
export type { SourceProvenance } from './sourceProvenance';

// Viewers
export { ImageViewer } from './viewers/ImageViewer';
export { MarkdownViewer } from './viewers/MarkdownViewer';
export { PDFViewer } from './viewers/PDFViewer';
export { TextViewer } from './viewers/TextViewer';
