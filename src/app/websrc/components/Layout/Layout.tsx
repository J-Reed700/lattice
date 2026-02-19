/**
 * Layout Component (Enhanced for v1.0 - Responsive)
 *
 * Main application layout with sidebar navigation
 * - Icon-based sidebar with tooltips
 * - Keyboard shortcut hints
 * - Active state highlighting
 * - Smooth transitions
 * - Mobile-first responsive design
 * - Hamburger menu for mobile
 * - Touch-friendly targets (44x44px minimum)
 */

import { useState, type ReactNode } from 'react'

import { AnimatePresence } from 'framer-motion'
import { Home, Search, FolderOpen, PlusCircle, MessageCircle, CalendarDays, Settings, Menu, X } from 'lucide-react'
import { Outlet, useNavigate, useLocation } from 'react-router-dom'

import { DownloadsDrawer } from '../Downloads/DownloadsDrawer'
import { DrawerTrigger } from '../Downloads/DrawerTrigger'
import { HeaderDownloadsIndicator } from '../Downloads/HeaderDownloadsIndicator'

interface NavButtonProps {
  view: 'home' | 'search' | 'files' | 'ingest' | 'chat' | 'settings' | 'daily'
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
        className={`relative min-w-[44px] min-h-[44px] p-3 rounded-lg transition-all duration-200 flex items-center justify-center ${
          isActive
            ? 'bg-[var(--accent-primary)]/10 text-[var(--accent-primary)]'
            : 'text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-hover)]'
        } ${isMobile ? 'w-full justify-start gap-3' : ''}`}
        title={`${label} (${shortcut})`}
        aria-label={label}
        aria-current={isActive ? 'page' : undefined}
      >
        {/* Active left bar indicator */}
        {isActive && !isMobile && (
          <span className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-5 bg-[var(--accent-primary)] rounded-r-full" />
        )}
        {icon}
        {isMobile && <span className="font-medium">{label}</span>}
      </button>

      {/* Tooltip - Only on desktop */}
      {!isMobile && (
        <div className="hidden md:block absolute left-full ml-4 px-2.5 py-1.5 bg-[var(--bg-tertiary)]/95 backdrop-blur-sm text-white text-xs rounded-lg shadow-xl opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all duration-150 whitespace-nowrap z-50 pointer-events-none border border-[var(--border-color)]">
          <div className="font-medium">{label}</div>
          <div className="text-[10px] text-[var(--text-tertiary)] mt-0.5">{shortcut}</div>
          {/* Tooltip arrow */}
          <div className="absolute right-full top-1/2 -translate-y-1/2 border-4 border-transparent border-r-[var(--bg-tertiary)]" />
        </div>
      )}
    </div>
  )
}

export function Layout() {
  console.log('[LAYOUT] Layout component rendering...');
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)
  const navigate = useNavigate()
  const location = useLocation()
  console.log('[LAYOUT] Current path:', location.pathname);

  // Extract active view from current path
  const activeView = location.pathname.slice(1) || 'home'

  const handleNavClick = (view: 'home' | 'search' | 'files' | 'ingest' | 'chat' | 'settings' | 'daily') => {
    navigate(`/${view}`)
    setMobileMenuOpen(false)
  }

  const navItems = [
    {
      view: 'home' as const,
      label: 'Home',
      shortcut: '⌘0',
      icon: <Home className="w-5 h-5" />,
    },
    {
      view: 'search' as const,
      label: 'Search',
      shortcut: '⌘1',
      icon: <Search className="w-5 h-5" />,
    },
    {
      view: 'files' as const,
      label: 'Files',
      shortcut: '⌘2',
      icon: <FolderOpen className="w-5 h-5" />,
    },
    {
      view: 'ingest' as const,
      label: 'Add Content',
      shortcut: '⌘I',
      icon: <PlusCircle className="w-5 h-5" />,
    },
    {
      view: 'chat' as const,
      label: 'Chat',
      shortcut: '⌘4',
      icon: <MessageCircle className="w-5 h-5" />,
    },
    {
      view: 'daily' as const,
      label: 'Daily Notes',
      shortcut: '⌘3',
      icon: <CalendarDays className="w-5 h-5" />,
    },
  ]

  const settingsItem = {
    view: 'settings' as const,
    label: 'Settings',
    shortcut: '⌘,',
    icon: <Settings className="w-5 h-5" />,
  }

  return (
    <div className="flex h-screen bg-[var(--bg-primary)]">
      {/* Mobile Header with Hamburger */}
      <div className="md:hidden fixed top-0 left-0 right-0 h-14 bg-[var(--bg-secondary)]/95 backdrop-blur-xl border-b border-[var(--border-color)]/50 flex items-center px-4 z-40">
        <button
          onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
          className="min-w-[44px] min-h-[44px] p-2 text-[var(--text-primary)] hover:bg-[var(--surface-hover)] rounded-lg transition-colors"
          aria-label="Toggle menu"
          aria-expanded={mobileMenuOpen}
        >
          {mobileMenuOpen ? (
            <X className="w-5 h-5" />
          ) : (
            <Menu className="w-5 h-5" />
          )}
        </button>
        <span className="ml-3 text-[var(--text-primary)] font-semibold tracking-heading">Recall Vault</span>
      </div>

      {/* Mobile Menu Backdrop */}
      {mobileMenuOpen && (
        <div
          className="md:hidden fixed inset-0 bg-black bg-opacity-50 z-40 mt-14"
          onClick={() => setMobileMenuOpen(false)}
          aria-hidden="true"
        />
      )}

      {/* Mobile Sidebar - Slide-in drawer with labels */}
      <nav
        className={`
          md:hidden
          fixed
          top-14
          left-0
          h-[calc(100vh-3.5rem)]
          w-64
          bg-[var(--bg-secondary)]
          flex flex-col
          py-4
          space-y-2
          px-4
          border-r border-[var(--border-color)]
          transform transition-transform duration-300 ease-in-out
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

        {/* Spacer to push settings to bottom */}
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

      {/* Desktop Sidebar - Icon-only, always visible */}
      <nav
        className="hidden md:flex w-[68px] bg-[var(--bg-secondary)] flex-col items-center py-5 space-y-1.5 border-r border-[var(--border-color)]"
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

        {/* Spacer to push bottom items */}
        <div className="flex-1" />

        {/* Downloads indicator */}
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
