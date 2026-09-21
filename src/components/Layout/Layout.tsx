import { type ReactNode } from 'react';

import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import {
  Bookmark,
  Command,
  LibraryBig,
  GraduationCap,
  Home,
  Layers3,
  MessageCircle,
  NotebookPen,
  Plus,
  Search,
  Settings,
} from 'lucide-react';
import { Outlet, useLocation, useNavigate } from 'react-router';

import { OPEN_PALETTE_EVENT } from '@/hooks/useCommandPalette';
import { cn } from '@/lib/utils';

import { DownloadsDrawer } from '../Downloads/DownloadsDrawer';
import { DrawerTrigger } from '../Downloads/DrawerTrigger';
import { HeaderDownloadsIndicator } from '../Downloads/HeaderDownloadsIndicator';
import { IndexingStatusRail } from '../IndexingStatus/IndexingStatusRail';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/tooltip';

/**
 * Layout — labeled navigation on wide windows, compact rail on smaller ones.
 *
 * The rail lists the app's surfaces in the same order as their ⌘-number
 * shortcuts so the two never disagree. Desktop only: the Tauri window has an
 * 800px minimum width, so there is no mobile breakpoint to serve.
 */

type View = 'home' | 'search' | 'files' | 'journals' | 'chat' | 'references' | 'study' | 'ingest' | 'settings';

interface NavItem {
  view: View;
  label: string;
  shortcut: string;
  icon: ReactNode;
}

const ICON_CLASS = 'h-4 w-4 shrink-0';
const STROKE = 1.6;

const PRIMARY_NAV: NavItem[] = [
  { view: 'home', label: 'Home', shortcut: '⌘0', icon: <Home className={ICON_CLASS} strokeWidth={STROKE} /> },
  { view: 'search', label: 'Search', shortcut: '⌘1', icon: <Search className={ICON_CLASS} strokeWidth={STROKE} /> },
  { view: 'files', label: 'Library', shortcut: '⌘2', icon: <LibraryBig className={ICON_CLASS} strokeWidth={STROKE} /> },
  { view: 'journals', label: 'Journal', shortcut: '⌘3', icon: <NotebookPen className={ICON_CLASS} strokeWidth={STROKE} /> },
  { view: 'chat', label: 'Chat', shortcut: '⌘4', icon: <MessageCircle className={ICON_CLASS} strokeWidth={STROKE} /> },
  { view: 'references', label: 'References', shortcut: '⌘5', icon: <Bookmark className={ICON_CLASS} strokeWidth={STROKE} /> },
  { view: 'study', label: 'Study', shortcut: '⌘6', icon: <GraduationCap className={ICON_CLASS} strokeWidth={STROKE} /> },
];

const IMPORT_NAV: NavItem = {
  view: 'ingest',
  label: 'Import',
  shortcut: '⌘I',
  icon: <Plus className={ICON_CLASS} strokeWidth={STROKE} />,
};

const SETTINGS_NAV: NavItem = {
  view: 'settings',
  label: 'Settings',
  shortcut: '⌘,',
  icon: <Settings className={ICON_CLASS} strokeWidth={STROKE} />,
};

function resolveActiveView(pathname: string): View {
  const first = pathname.split('/').filter(Boolean)[0] ?? 'home';
  if (first === 'daily') return 'journals';
  return (first as View) || 'home';
}

interface NavButtonProps {
  item: NavItem;
  isActive: boolean;
  onClick: () => void;
}

function NavButton({ item, isActive, onClick }: NavButtonProps) {
  const reduceMotion = useReducedMotion();
  return (
    <Tooltip delayDuration={500}>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={onClick}
          aria-label={item.label}
          aria-current={isActive ? 'page' : undefined}
          className={cn(
            'group relative flex h-8 w-full items-center justify-center gap-2.5 rounded-md px-2.5 transition-colors duration-fast xl:justify-start',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
            isActive ? 'text-text-primary' : 'text-text-tertiary hover:text-text-primary',
          )}
        >
          {/* One pill, shared by every item: it travels to the route you pick. */}
          {isActive ? (
            <motion.span
              layoutId="rail-active"
              aria-hidden="true"
              transition={reduceMotion ? { duration: 0 } : { type: 'spring', visualDuration: 0.22, bounce: 0.12 }}
              className="absolute inset-0 rounded-md bg-[hsl(var(--text-primary)/0.08)] shadow-[inset_0_0_0_1px_hsl(var(--text-primary)/0.04)]"
            />
          ) : (
            <span aria-hidden="true" className="row-hover absolute inset-0 rounded-md" />
          )}
          <span className={cn('relative flex', isActive && 'text-accent')}>{item.icon}</span>
          <span className={cn('relative hidden text-ui xl:block', isActive && 'font-medium')}>{item.label}</span>
          <kbd aria-hidden="true" className="relative ml-auto hidden font-sans text-[11px] tabular-nums text-text-muted opacity-0 transition-opacity duration-fast group-hover:opacity-100 group-focus-visible:opacity-100 xl:block">{item.shortcut}</kbd>
        </button>
      </TooltipTrigger>
      <TooltipContent side="right" sideOffset={10} className="xl:hidden">
        <span className="flex items-center gap-2">
          <span>{item.label}</span>
          <kbd className="kbd">{item.shortcut}</kbd>
        </span>
      </TooltipContent>
    </Tooltip>
  );
}

export function Layout() {
  const navigate = useNavigate();
  const location = useLocation();
  const activeView = resolveActiveView(location.pathname);

  const go = (view: View) => navigate(`/${view}`);

  return (
    <div className="flex h-screen bg-chrome">
      {/* The rail is window chrome: dim, quiet, and draggable where it is empty. */}
      <aside
        data-tauri-drag-region
        className="flex w-[60px] shrink-0 flex-col px-2 pb-2.5 pt-[calc(var(--titlebar-inset)+12px)] xl:w-[212px] xl:px-2.5"
      >
        <div data-tauri-drag-region className="mb-3 flex h-8 items-center justify-center gap-2 xl:justify-start xl:px-2.5" aria-label="Lattice">
          <Layers3 className="pointer-events-none h-[18px] w-[18px] text-accent" strokeWidth={1.6} />
          <span className="pointer-events-none hidden font-serif text-[17px] font-medium tracking-[-0.02em] text-text-primary xl:block">Lattice</span>
        </div>

        <button
          type="button"
          onClick={() => window.dispatchEvent(new Event(OPEN_PALETTE_EVENT))}
          aria-label="Search and commands"
          className="pressable mb-3 flex h-8 w-full items-center justify-center gap-2 rounded-md bg-[hsl(var(--text-primary)/0.055)] px-2.5 text-text-tertiary transition-[background-color,color,scale] duration-fast hover:bg-[hsl(var(--text-primary)/0.09)] hover:text-text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring xl:justify-start"
        >
          <Command className="h-3.5 w-3.5 shrink-0 xl:hidden" strokeWidth={STROKE} />
          <Search className="hidden h-3.5 w-3.5 shrink-0 xl:block" strokeWidth={STROKE} />
          <span className="hidden text-ui xl:block">Find anything</span>
          <kbd className="kbd ml-auto hidden xl:inline-flex">⌘K</kbd>
        </button>

        <nav aria-label="Main navigation" className="flex min-h-0 flex-1 flex-col">
          <div className="flex w-full flex-col gap-px">
            {PRIMARY_NAV.map((item) => (
              <NavButton key={item.view} item={item} isActive={activeView === item.view} onClick={() => go(item.view)} />
            ))}
          </div>

          <div className="mx-2.5 my-2.5 h-px bg-border-subtle" aria-hidden="true" />

          <NavButton item={IMPORT_NAV} isActive={activeView === IMPORT_NAV.view} onClick={() => go(IMPORT_NAV.view)} />

          <div data-tauri-drag-region className="min-h-4 flex-1" />

          <div className="flex w-full flex-col items-center gap-px">
            <IndexingStatusRail />
            <HeaderDownloadsIndicator />
            <NavButton item={SETTINGS_NAV} isActive={activeView === SETTINGS_NAV.view} onClick={() => go(SETTINGS_NAV.view)} />
          </div>
        </nav>
      </aside>

      {/* The page: one lit sheet set into the chrome. */}
      <div className="relative my-2 mr-2 flex min-w-0 flex-1 flex-col overflow-hidden rounded-xl bg-bg shadow-sheet">
        <AnimatePresence mode="wait" initial={false}>
          <Outlet key={location.pathname} />
        </AnimatePresence>
      </div>

      {/* Downloads UI. The single IPC listener lives in App.tsx; these only read the store. */}
      <DrawerTrigger />
      <DownloadsDrawer />
    </div>
  );
}
