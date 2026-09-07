import { type ReactNode } from 'react';

import { AnimatePresence } from 'framer-motion';
import {
  Bookmark,
  FolderOpen,
  Home,
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
 * Layout — the persistent left rail plus the routed content area.
 *
 * The rail lists the app's surfaces in the same order as their ⌘-number
 * shortcuts so the two never disagree. Desktop only: the Tauri window has an
 * 800px minimum width, so there is no mobile breakpoint to serve.
 */

type View = 'home' | 'search' | 'files' | 'journals' | 'chat' | 'references' | 'ingest' | 'settings';

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
            'relative flex h-9 w-9 items-center justify-center rounded-md transition-colors duration-fast',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-surface',
            isActive
              ? 'bg-accent-muted text-accent'
              : 'text-text-muted hover:bg-surface-raised hover:text-text-primary',
          )}
        >
          {isActive ? (
            <span
              aria-hidden="true"
              className="absolute -left-[14px] top-1/2 h-4 w-[2px] -translate-y-1/2 rounded-r-full bg-accent"
            />
          ) : null}
          {item.icon}
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
        className="flex w-[60px] shrink-0 flex-col items-center border-r border-border-subtle bg-surface pb-3 pt-4"
      >
        <div className="flex flex-col items-center gap-1">
          {PRIMARY_NAV.map((item) => (
            <NavButton key={item.view} item={item} isActive={activeView === item.view} onClick={() => go(item.view)} />
          ))}
        </div>

        <div className="my-3 h-px w-6 bg-border-subtle" aria-hidden="true" />

        <NavButton item={IMPORT_NAV} isActive={activeView === IMPORT_NAV.view} onClick={() => go(IMPORT_NAV.view)} />

        <div className="flex-1" />

        <div className="flex flex-col items-center gap-1">
          <IndexingStatusRail />
          <HeaderDownloadsIndicator />
          <NavButton item={SETTINGS_NAV} isActive={activeView === SETTINGS_NAV.view} onClick={() => go(SETTINGS_NAV.view)} />
        </div>
      </nav>

      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
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
