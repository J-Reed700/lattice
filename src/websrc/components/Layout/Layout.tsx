import { type ReactNode } from 'react';

import { AnimatePresence } from 'framer-motion';
import {
  Bookmark,
  FolderOpen,
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

const ICON_CLASS = 'h-[18px] w-[18px]';

const PRIMARY_NAV: NavItem[] = [
  { view: 'home', label: 'Home', shortcut: '⌘0', icon: <Home className={ICON_CLASS} strokeWidth={1.75} /> },
  { view: 'search', label: 'Search', shortcut: '⌘1', icon: <Search className={ICON_CLASS} strokeWidth={1.75} /> },
  { view: 'files', label: 'Library', shortcut: '⌘2', icon: <FolderOpen className={ICON_CLASS} strokeWidth={1.75} /> },
  { view: 'journals', label: 'Journal', shortcut: '⌘3', icon: <NotebookPen className={ICON_CLASS} strokeWidth={1.75} /> },
  { view: 'chat', label: 'Chat', shortcut: '⌘4', icon: <MessageCircle className={ICON_CLASS} strokeWidth={1.75} /> },
  { view: 'references', label: 'References', shortcut: '⌘5', icon: <Bookmark className={ICON_CLASS} strokeWidth={1.75} /> },
  { view: 'study', label: 'Study', shortcut: '⌘6', icon: <GraduationCap className={ICON_CLASS} strokeWidth={1.75} /> },
];

const IMPORT_NAV: NavItem = {
  view: 'ingest',
  label: 'Import',
  shortcut: '⌘I',
  icon: <Plus className={ICON_CLASS} strokeWidth={1.75} />,
};

const SETTINGS_NAV: NavItem = {
  view: 'settings',
  label: 'Settings',
  shortcut: '⌘,',
  icon: <Settings className={ICON_CLASS} strokeWidth={1.75} />,
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
  return (
    <Tooltip delayDuration={400}>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={onClick}
          aria-label={item.label}
          aria-current={isActive ? 'page' : undefined}
          className={cn(
            'group relative flex h-10 w-full items-center justify-center gap-3 rounded-lg px-3 transition-colors duration-fast xl:justify-start',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-surface',
            isActive
              ? 'bg-accent-muted font-medium text-accent'
              : 'text-text-muted hover:bg-surface-raised hover:text-text-primary',
          )}
        >
          {isActive ? (
            <span
              aria-hidden="true"
              className="absolute left-0 top-1/2 h-4 w-[2px] -translate-y-1/2 rounded-r-full bg-accent"
            />
          ) : null}
          {item.icon}
          <span className="hidden text-[13px] xl:block">{item.label}</span>
          <kbd aria-hidden="true" className="ml-auto hidden font-sans text-[10px] text-text-tertiary opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100 xl:block">{item.shortcut}</kbd>
        </button>
      </TooltipTrigger>
      <TooltipContent side="right" sideOffset={10}>
        <span className="flex items-center gap-2">
          <span>{item.label}</span>
          <kbd className="font-mono text-xxs text-text-muted">{item.shortcut}</kbd>
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
    <div className="flex h-screen bg-bg">
      <nav
        aria-label="Main navigation"
        className="lattice-navigation flex w-[68px] shrink-0 flex-col items-center border-r border-border-subtle bg-bg px-2 pb-4 pt-6 xl:w-[196px] xl:items-stretch xl:px-3"
      >
        <div className="mb-8 flex h-8 items-center justify-center gap-2.5 text-text-primary xl:justify-start xl:px-3" aria-label="Lattice">
          <Layers3 className="h-6 w-6 text-accent" strokeWidth={1.5} />
          <span className="hidden font-serif text-[22px] tracking-tight xl:block">Lattice</span>
        </div>
        <div className="mb-3 hidden px-3 text-[10px] font-medium uppercase tracking-[0.16em] text-text-tertiary xl:block">Workspace</div>
        <div className="flex w-full flex-col gap-1">
          {PRIMARY_NAV.map((item) => (
            <NavButton key={item.view} item={item} isActive={activeView === item.view} onClick={() => go(item.view)} />
          ))}
        </div>

        <div className="my-4 h-px w-full bg-border-subtle" aria-hidden="true" />

        <NavButton item={IMPORT_NAV} isActive={activeView === IMPORT_NAV.view} onClick={() => go(IMPORT_NAV.view)} />

        <div className="flex-1" />

        <div className="flex w-full flex-col items-center gap-1">
          <IndexingStatusRail />
          <HeaderDownloadsIndicator />
          <NavButton item={SETTINGS_NAV} isActive={activeView === SETTINGS_NAV.view} onClick={() => go(SETTINGS_NAV.view)} />
        </div>
      </nav>

      <div className="flex min-w-0 flex-1 flex-col overflow-hidden bg-surface">
        <AnimatePresence mode="wait">
          <Outlet key={location.pathname} />
        </AnimatePresence>
      </div>

      {/* Downloads UI. The single IPC listener lives in App.tsx; these only read the store. */}
      <DrawerTrigger />
      <DownloadsDrawer />
    </div>
  );
}
