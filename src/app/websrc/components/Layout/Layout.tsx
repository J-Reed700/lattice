import { useState, type ReactNode } from 'react'

import { AnimatePresence } from 'framer-motion'
import { Home, Search, FolderOpen, PlusCircle, MessageCircle, NotebookPen, Settings, Menu, X, Bookmark } from 'lucide-react'
import { Outlet, useNavigate, useLocation } from 'react-router-dom'

import { DownloadsDrawer } from '../Downloads/DownloadsDrawer'
import { DrawerTrigger } from '../Downloads/DrawerTrigger'
import { HeaderDownloadsIndicator } from '../Downloads/HeaderDownloadsIndicator'

interface NavButtonProps {
  view: 'home' | 'search' | 'files' | 'ingest' | 'chat' | 'settings' | 'journals' | 'references'
  activeView: string
  onClick: () => void
  icon: ReactNode
  label: string
  shortcut: string
  isMobile?: boolean
}

function NavButton({ view, activeView, onClick, icon, label, shortcut, isMobile = false }: NavButtonProps) {
  const isActive = activeView === view

  return (
    <div className="relative group">
      <button
        onClick={onClick}
        className={`relative min-w-[44px] min-h-[44px] p-3 rounded-md transition-colors duration-fast flex items-center justify-center ${
          isActive
            ? 'bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))]'
            : 'text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--surface-raised))]'
        } ${isMobile ? 'w-full justify-start gap-3' : ''}`}
        title={`${label} (${shortcut})`}
        aria-label={label}
        aria-current={isActive ? 'page' : undefined}
      >
        {isActive && !isMobile && (
          <span className="absolute left-0 top-1/2 -translate-y-1/2 w-[2px] h-5 bg-[hsl(var(--accent))] rounded-r-full" />
        )}
        {icon}
        {isMobile && <span className="font-medium">{label}</span>}
      </button>

      {!isMobile && (
        <div className="hidden md:block absolute left-full ml-4 px-2.5 py-1.5 bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))] text-xs rounded-md shadow-md opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-opacity duration-fast whitespace-nowrap z-50 pointer-events-none border border-[hsl(var(--border-subtle))]">
          <div className="font-medium">{label}</div>
          <div className="text-[10px] text-[hsl(var(--text-muted))] mt-0.5">{shortcut}</div>
        </div>
      )}
    </div>
  )
}

export function Layout() {
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)
  const navigate = useNavigate()
  const location = useLocation()

  const pathView = location.pathname.slice(1) || 'home'
  const activeView = pathView === 'daily' ? 'journals' : pathView

  const handleNavClick = (view: 'home' | 'search' | 'files' | 'ingest' | 'chat' | 'settings' | 'journals' | 'references') => {
    navigate(`/${view}`)
    setMobileMenuOpen(false)
  }

  const navItems = [
    {
      view: 'home' as const,
      label: 'Home',
      shortcut: '⌘0',
      icon: <Home className="w-[18px] h-[18px]" strokeWidth={1.75} />,
    },
    {
      view: 'search' as const,
      label: 'Search',
      shortcut: '⌘1',
      icon: <Search className="w-[18px] h-[18px]" strokeWidth={1.75} />,
    },
    {
      view: 'files' as const,
      label: 'Files',
      shortcut: '⌘2',
      icon: <FolderOpen className="w-[18px] h-[18px]" strokeWidth={1.75} />,
    },
    {
      view: 'ingest' as const,
      label: 'Add content',
      shortcut: '⌘I',
      icon: <PlusCircle className="w-[18px] h-[18px]" strokeWidth={1.75} />,
    },
    {
      view: 'chat' as const,
      label: 'Chat',
      shortcut: '⌘4',
      icon: <MessageCircle className="w-[18px] h-[18px]" strokeWidth={1.75} />,
    },
    {
      view: 'journals' as const,
      label: 'Journals',
      shortcut: '⌘3',
      icon: <NotebookPen className="w-[18px] h-[18px]" strokeWidth={1.75} />,
    },
    {
      view: 'references' as const,
      label: 'References',
      shortcut: '⌘5',
      icon: <Bookmark className="w-[18px] h-[18px]" strokeWidth={1.75} />,
    },
  ]

  const settingsItem = {
    view: 'settings' as const,
    label: 'Settings',
    shortcut: '⌘,',
    icon: <Settings className="w-[18px] h-[18px]" strokeWidth={1.75} />,
  }

  return (
    <div className="flex h-screen bg-[hsl(var(--bg))]">
      {/* Mobile Header with Hamburger */}
      <div className="md:hidden fixed top-0 left-0 right-0 h-14 bg-[hsl(var(--surface))] border-b border-[hsl(var(--border-subtle))] flex items-center px-4 z-40">
        <button
          onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
          className="min-w-[44px] min-h-[44px] p-2 text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--surface-raised))] rounded-md transition-colors duration-fast"
          aria-label="Toggle menu"
          aria-expanded={mobileMenuOpen}
        >
          {mobileMenuOpen ? (
            <X className="w-5 h-5" strokeWidth={1.75} />
          ) : (
            <Menu className="w-5 h-5" strokeWidth={1.75} />
          )}
        </button>
        <span className="ml-3 text-[hsl(var(--text-primary))] font-serif font-semibold">Recall</span>
      </div>

      {/* Mobile Menu Backdrop */}
      {mobileMenuOpen && (
        <div
          className="md:hidden fixed inset-0 bg-[hsl(var(--overlay))] z-40 mt-14"
          onClick={() => setMobileMenuOpen(false)}
          aria-hidden="true"
        />
      )}

      {/* Mobile Sidebar */}
      <nav
        className={`
          md:hidden
          fixed
          top-14
          left-0
          h-[calc(100vh-3.5rem)]
          w-64
          bg-[hsl(var(--surface))]
          flex flex-col
          py-4
          space-y-2
          px-4
          transform transition-transform duration-base ease-out
          z-50
          ${mobileMenuOpen ? 'translate-x-0' : '-translate-x-full'}
        `}
        aria-label="Main navigation"
      >
        {navItems.map((item) => (
          <NavButton
            key={item.view}
            view={item.view}
            activeView={activeView}
            onClick={() => handleNavClick(item.view)}
            label={item.label}
            shortcut={item.shortcut}
            icon={item.icon}
            isMobile
          />
        ))}

        <div className="flex-1" />

        <NavButton
          view={settingsItem.view}
          activeView={activeView}
          onClick={() => handleNavClick(settingsItem.view)}
          label={settingsItem.label}
          shortcut={settingsItem.shortcut}
          icon={settingsItem.icon}
          isMobile
        />
      </nav>

      {/* Desktop Sidebar */}
      <nav
        className="hidden md:flex w-[68px] bg-[hsl(var(--surface))] flex-col items-center py-5 space-y-1.5"
        aria-label="Main navigation"
      >
        {navItems.map((item) => (
          <NavButton
            key={item.view}
            view={item.view}
            activeView={activeView}
            onClick={() => handleNavClick(item.view)}
            label={item.label}
            shortcut={item.shortcut}
            icon={item.icon}
            isMobile={false}
          />
        ))}

        <div className="flex-1" />

        <HeaderDownloadsIndicator />

        <NavButton
          view={settingsItem.view}
          activeView={activeView}
          onClick={() => handleNavClick(settingsItem.view)}
          label={settingsItem.label}
          shortcut={settingsItem.shortcut}
          icon={settingsItem.icon}
          isMobile={false}
        />
      </nav>

      {/* Main content */}
      <div className="flex-1 flex flex-col overflow-hidden mt-14 md:mt-0">
        <AnimatePresence mode="wait">
          <Outlet key={location.pathname} />
        </AnimatePresence>
      </div>

      {/* Global Downloads UI */}
      <DrawerTrigger />
      <DownloadsDrawer />
    </div>
  )
}
