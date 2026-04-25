import {
  Bookmark,
  FileText,
  FolderUp,
  Globe,
  Home,
  MessageSquare,
  NotebookPen,
  Plus,
  Search,
  Settings as SettingsIcon,
  Sun,
  Upload,
  type LucideIcon,
} from 'lucide-react';

/**
 * A single static entry rendered in the Actions or Navigate shelves.
 *
 * Corpus results (documents, conversations, bookmarks, journals, folders)
 * are produced by useSpotlightResults. These static entries are the
 * "command" half of the palette — what a user can DO versus what they
 * can FIND.
 */
export interface SpotlightAction {
  id: string;
  label: string;
  secondary?: string;
  icon: LucideIcon;
  shortcut?: string;
  keywords?: string[];
}

const IS_MAC =
  typeof navigator !== 'undefined' && /mac/i.test(navigator.platform);
const MOD = IS_MAC ? '⌘' : 'Ctrl';

/**
 * Verbs the user can perform. Filterable by cmdk when a query is typed.
 */
export const SPOTLIGHT_ACTIONS: SpotlightAction[] = [
  {
    id: 'action.new-conversation',
    label: 'New conversation',
    secondary: 'Start a new chat with your corpus',
    icon: MessageSquare,
    keywords: ['chat', 'ask', 'question', 'new'],
  },
  {
    id: 'action.new-journal-entry',
    label: 'New journal entry',
    secondary: 'Capture prose into your journal',
    icon: NotebookPen,
    keywords: ['write', 'note', 'journal', 'new'],
  },
  {
    id: 'action.add-files',
    label: 'Add files',
    secondary: 'Index files into your vault',
    icon: Upload,
    shortcut: `${MOD}U`,
    keywords: ['upload', 'ingest', 'import', 'index', 'file'],
  },
  {
    id: 'action.add-folder',
    label: 'Add folder',
    secondary: 'Index every file in a folder',
    icon: FolderUp,
    keywords: ['upload', 'ingest', 'directory', 'batch'],
  },
  {
    id: 'action.add-web-page',
    label: 'Add web page',
    secondary: 'Ingest a URL into your vault',
    icon: Globe,
    keywords: ['url', 'link', 'web', 'page', 'import'],
  },
  {
    id: 'action.toggle-theme',
    label: 'Toggle theme',
    secondary: 'Switch between light and dark',
    icon: Sun,
    keywords: ['dark', 'light', 'mode'],
  },
];

/**
 * Destinations the user can jump to. Also filterable by cmdk.
 */
export const SPOTLIGHT_NAVIGATE: SpotlightAction[] = [
  {
    id: 'nav.home',
    label: 'Home',
    secondary: 'Your vault dashboard',
    icon: Home,
    shortcut: `${MOD}0`,
    keywords: ['dashboard', 'start'],
  },
  {
    id: 'nav.chat',
    label: 'Chat',
    secondary: 'Ask your corpus',
    icon: MessageSquare,
    shortcut: `${MOD}4`,
    keywords: ['conversation', 'ask', 'question'],
  },
  {
    id: 'nav.journals',
    label: 'Journals',
    secondary: 'Editorial notebook',
    icon: NotebookPen,
    shortcut: `${MOD}3`,
    keywords: ['journal', 'write', 'notes'],
  },
  {
    id: 'nav.references',
    label: 'References',
    secondary: 'Your commonplace book',
    icon: Bookmark,
    shortcut: `${MOD}5`,
    keywords: ['bookmarks', 'inbox', 'saved'],
  },
  {
    id: 'nav.files',
    label: 'Files',
    secondary: 'Browse the corpus',
    icon: FileText,
    shortcut: `${MOD}2`,
    keywords: ['documents', 'corpus', 'vault', 'browser'],
  },
  {
    id: 'nav.ingest',
    label: 'Add content',
    secondary: 'Ingest files or web pages',
    icon: Plus,
    shortcut: `${MOD}I`,
    keywords: ['import', 'ingest', 'upload'],
  },
  {
    id: 'nav.search',
    label: 'Search',
    secondary: 'Structured keyword and hybrid search',
    icon: Search,
    shortcut: `${MOD}1`,
    keywords: ['find', 'query', 'keyword'],
  },
  {
    id: 'nav.settings',
    label: 'Settings',
    secondary: 'Preferences and models',
    icon: SettingsIcon,
    shortcut: `${MOD},`,
    keywords: ['preferences', 'config', 'theme'],
  },
];

/**
 * Case-insensitive substring filter used for the Actions and Navigate
 * shelves. Returns every entry when the query is empty so the shelves
 * render fully in the default view.
 */
export function filterActions(
  entries: SpotlightAction[],
  query: string
): SpotlightAction[] {
  const q = query.trim().toLowerCase();
  if (!q) return entries;
  return entries.filter((entry) => {
    if (entry.label.toLowerCase().includes(q)) return true;
    if (entry.secondary && entry.secondary.toLowerCase().includes(q)) return true;
    if (entry.keywords?.some((kw) => kw.toLowerCase().includes(q))) return true;
    return false;
  });
}
