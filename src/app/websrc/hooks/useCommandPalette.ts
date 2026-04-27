import { useState, useEffect, useCallback } from 'react'

export interface RecentItem {
  id: string
  label: string
  timestamp: number
}

export interface CommandPaletteState {
  isOpen: boolean
  searchMode: boolean
  recentSearches: RecentItem[]
  recentDocuments: RecentItem[]
}

const MAX_RECENT_ITEMS = 5

/**
 * useCommandPalette
 *
 * Purpose: Manages command palette state, keyboard shortcuts, and recent items
 *
 * Features:
 * - Cmd+K / Ctrl+K global keyboard shortcut
 * - Recent searches and documents tracking
 * - Search mode toggle
 * - Local storage persistence
 */
export function useCommandPalette() {
  const [state, setState] = useState<CommandPaletteState>({
    isOpen: false,
    searchMode: false,
    recentSearches: [],
    recentDocuments: [],
  })

  // Load recent items from localStorage on mount
  useEffect(() => {
    try {
      const stored = localStorage.getItem('lattice-command-palette')
      if (stored) {
        const data = JSON.parse(stored)
        setState(prev => ({
          ...prev,
          recentSearches: data.recentSearches || [],
          recentDocuments: data.recentDocuments || [],
        }))
      }
    } catch (error) {
      console.error('Failed to load command palette state:', error)
    }
  }, [])

  // Save recent items to localStorage whenever they change
  useEffect(() => {
    try {
      localStorage.setItem('lattice-command-palette', JSON.stringify({
        recentSearches: state.recentSearches,
        recentDocuments: state.recentDocuments,
      }))
    } catch (error) {
      console.error('Failed to save command palette state:', error)
    }
  }, [state.recentSearches, state.recentDocuments])

  // Keyboard shortcut handler
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Cmd+K (Mac) or Ctrl+K (Windows/Linux)
      if ((event.metaKey || event.ctrlKey) && event.key === 'k') {
        event.preventDefault()
        setState(prev => ({ ...prev, isOpen: !prev.isOpen, searchMode: false }))
      }

      // Escape to close
      if (event.key === 'Escape' && state.isOpen) {
        event.preventDefault()
        setState(prev => ({ ...prev, isOpen: false, searchMode: false }))
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [state.isOpen])

  const open = useCallback(() => {
    setState(prev => ({ ...prev, isOpen: true, searchMode: false }))
  }, [])

  const close = useCallback(() => {
    setState(prev => ({ ...prev, isOpen: false, searchMode: false }))
  }, [])

  const toggle = useCallback(() => {
    setState(prev => ({ ...prev, isOpen: !prev.isOpen, searchMode: false }))
  }, [])

  const enterSearchMode = useCallback(() => {
    setState(prev => ({ ...prev, searchMode: true }))
  }, [])

  const exitSearchMode = useCallback(() => {
    setState(prev => ({ ...prev, searchMode: false }))
  }, [])

  const addRecentSearch = useCallback((search: string) => {
    setState(prev => {
      const newItem: RecentItem = {
        id: `search-${Date.now()}`,
        label: search,
        timestamp: Date.now(),
      }

      // Remove duplicates and add to front
      const filtered = prev.recentSearches.filter(item => item.label !== search)
      const updated = [newItem, ...filtered].slice(0, MAX_RECENT_ITEMS)

      return { ...prev, recentSearches: updated }
    })
  }, [])

  const addRecentDocument = useCallback((docId: string, docName: string) => {
    setState(prev => {
      const newItem: RecentItem = {
        id: docId,
        label: docName,
        timestamp: Date.now(),
      }

      // Remove duplicates and add to front
      const filtered = prev.recentDocuments.filter(item => item.id !== docId)
      const updated = [newItem, ...filtered].slice(0, MAX_RECENT_ITEMS)

      return { ...prev, recentDocuments: updated }
    })
  }, [])

  const clearRecentSearches = useCallback(() => {
    setState(prev => ({ ...prev, recentSearches: [] }))
  }, [])

  const clearRecentDocuments = useCallback(() => {
    setState(prev => ({ ...prev, recentDocuments: [] }))
  }, [])

  return {
    isOpen: state.isOpen,
    searchMode: state.searchMode,
    recentSearches: state.recentSearches,
    recentDocuments: state.recentDocuments,
    open,
    close,
    toggle,
    enterSearchMode,
    exitSearchMode,
    addRecentSearch,
    addRecentDocument,
    clearRecentSearches,
    clearRecentDocuments,
  }
}
