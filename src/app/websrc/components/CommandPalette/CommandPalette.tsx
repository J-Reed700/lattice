import { useCallback, useState, useEffect, useRef } from 'react'

import { Command } from 'cmdk'
import {
  Search,
  Upload,
  FolderUp,
  FileText,
  Clock,
  Settings as SettingsIcon,
  Keyboard,
  BookOpen,
  Bookmark,
  Trash2,
  Home,
  Sparkles,
  NotebookPen,
} from 'lucide-react'

import type { SearchResult } from '@/types'

import { useCommandPalette } from '../../hooks/useCommandPalette'
import VaultAPI from '../../lib/api'
import { logger } from '../../utils/logger'
import { handleAsyncEvent } from '../../utils/promiseHandlers'
import { CommandItem } from '../CommandItem'
import { KeyboardShortcutsModal } from '../KeyboardShortcutsModal'
import '../../styles/command-palette.css'

interface CommandPaletteProps {
  onNavigate: (view: 'search' | 'files' | 'settings' | 'journals' | 'references') => void
}

/**
 * CommandPalette
 *
 * Purpose: Keyboard-first command palette for quick navigation and actions with live search
 *
 * Features:
 * - Cmd+K / Ctrl+K global shortcut (<50ms to open)
 * - Live search with debouncing (300ms)
 * - Fuzzy search filtering for commands
 * - Real-time document search results with scores
 * - Command groups (Search, Upload, Navigate, Settings, Help)
 * - Recent searches and documents
 * - Folder picker integration for indexing
 * - Glassmorphism design with smooth animations
 * - Dark mode support
 *
 * States: open, closed, search mode, loading, error
 * Accessibility: Keyboard navigation, ARIA labels, escape to close, focus trap
 */
export function CommandPalette({ onNavigate }: CommandPaletteProps) {
  const {
    isOpen,
    searchMode,
    recentSearches,
    recentDocuments,
    close,
    enterSearchMode,
    addRecentSearch,
    addRecentDocument,
    clearRecentSearches,
  } = useCommandPalette()

  const [showShortcuts, setShowShortcuts] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [searchResults, setSearchResults] = useState<SearchResult[]>([])
  const [isSearching, setIsSearching] = useState(false)
  const [searchError, setSearchError] = useState<string | null>(null)
  const searchTimeoutRef = useRef<number | null>(null)
  const searchAbortControllerRef = useRef<AbortController | null>(null)

  // Debounced live search with race condition protection
  useEffect(() => {
    if (!searchQuery.trim() || searchQuery.length < 2) {
      setSearchResults([])
      setIsSearching(false)
      setSearchError(null)
      return
    }

    // Clear existing timeout
    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current)
    }

    // Abort any in-flight request
    if (searchAbortControllerRef.current) {
      searchAbortControllerRef.current.abort()
    }

    setIsSearching(true)
    setSearchError(null)

    // Create new abort controller for this search
    const abortController = new AbortController()
    searchAbortControllerRef.current = abortController

    // Debounce search by 300ms
    searchTimeoutRef.current = window.setTimeout(() => {
      void (async () => {
        try {
          const result = await VaultAPI.searchDocuments({
            query: searchQuery,
            limit: 10,
            searchMode: 'hybrid',
          })

          // Only update if this request wasn't cancelled
          if (!abortController.signal.aborted) {
            if (result.ok) {
              setSearchResults(result.data)
              setIsSearching(false)
            } else {
              setSearchError(result.error)
              setIsSearching(false)
            }
          }
        } catch (error) {
          // Only handle error if request wasn't cancelled
          if (!abortController.signal.aborted) {
            const errorMessage = error instanceof Error ? error.message : 'Search failed'
            setSearchError(errorMessage)
            setIsSearching(false)
          }
        }
      })()
    }, 300)

    return () => {
      // Cleanup: cancel timeout and abort request
      if (searchTimeoutRef.current) {
        clearTimeout(searchTimeoutRef.current)
      }
      if (searchAbortControllerRef.current) {
        searchAbortControllerRef.current.abort()
      }
    }
  }, [searchQuery])

  // Reset search when palette closes
  useEffect(() => {
    if (!isOpen) {
      setSearchQuery('')
      setSearchResults([])
      setIsSearching(false)
      setSearchError(null)
    }
  }, [isOpen])

  // Handle command execution
  const executeCommand = useCallback((action: () => void | Promise<void>) => {
    const result = action()
    if (result instanceof Promise) {
      void result.then(() => close())
    } else {
      close()
    }
  }, [close])

  // Command handlers
  const handleSearchDocuments = useCallback(() => {
    executeCommand(() => {
      enterSearchMode()
      onNavigate('search')
    })
  }, [executeCommand, enterSearchMode, onNavigate])

  const handleUploadDocument = useCallback(async () => {
    executeCommand(async () => {
      try {
        // Use VaultAPI selectMultipleFiles
        const selected = await VaultAPI.selectMultipleFiles()

        if (selected && selected.length > 0) {
          for (const file of selected) {
            const result = await VaultAPI.indexFile(file)
            if (!result.ok) {
              logger.error('Failed to index file:', { file, error: result.error })
            }
          }
        }
      } catch (error) {
        logger.error('Failed to upload document:', { error })
      }
    })
  }, [executeCommand])

  const handleUploadFolder = useCallback(async () => {
    executeCommand(async () => {
      try {
        // Use VaultAPI selectFolder
        const selected = await VaultAPI.selectFolder()

        if (selected) {
          const result = await VaultAPI.startIndexing(selected, true)
          if (!result.ok) {
            logger.error('Failed to index directory:', { path: selected, error: result.error })
          }
        }
      } catch (error) {
        logger.error('Failed to index folder:', { error })
      }
    })
  }, [executeCommand])

  const handleViewAllDocuments = useCallback(() => {
    executeCommand(() => {
      onNavigate('files')
    })
  }, [executeCommand, onNavigate])

  const handleOpenSettings = useCallback(() => {
    executeCommand(() => {
      onNavigate('settings')
    })
  }, [executeCommand, onNavigate])

  const handleOpenJournals = useCallback(() => {
    executeCommand(() => {
      onNavigate('journals')
    })
  }, [executeCommand, onNavigate])

  const handleOpenReferenceInbox = useCallback(() => {
    executeCommand(() => {
      onNavigate('references')
    })
  }, [executeCommand, onNavigate])

  const handleClearCache = useCallback(async () => {
    executeCommand(async () => {
      const result = await VaultAPI.clearCache()
      if (!result.ok) {
        logger.error('Failed to clear cache:', { error: result.error })
      }
    })
  }, [executeCommand])

  const handleShowKeyboardShortcuts = useCallback(() => {
    close()
    setShowShortcuts(true)
  }, [close])

  const handleOpenDocumentation = useCallback(() => {
    executeCommand(() => {
      window.open('https://github.com/yourusername/lattice-desktop', '_blank')
    })
  }, [executeCommand])

  const handleRecentSearch = useCallback((search: string) => {
    executeCommand(() => {
      addRecentSearch(search)
      onNavigate('search')
    })
  }, [executeCommand, addRecentSearch, onNavigate])

  const handleRecentDocument = useCallback(async (docId: string, docName: string) => {
    executeCommand(async () => {
      addRecentDocument(docId, docName)

      try {
        // Fetch document metadata to get file path
        const docResult = await VaultAPI.getDocument(docId)

        if (!docResult.ok) {
          logger.error('Failed to retrieve document metadata', {
            component: 'command-palette',
            docId,
            error: docResult.error
          })
          return
        }

        const { filePath } = docResult.data
        if (!filePath) {
          logger.error('Document metadata missing file path', {
            component: 'command-palette',
            docId,
            docName
          })
          return
        }

        // Open document using file path
        const openResult = await VaultAPI.openFile(filePath)

        if (openResult.ok) {
          logger.info('Opened recent document', {
            component: 'command-palette',
            docId,
            docName,
            path: filePath
          })
        } else {
          logger.error('Failed to open document', {
            component: 'command-palette',
            docId,
            path: filePath,
            error: openResult.error
          })
        }
      } catch (error) {
        logger.error('Unexpected error opening recent document', {
          component: 'command-palette',
          docId,
          docName,
          error: error instanceof Error ? error.message : String(error)
        })
      }
    })
  }, [executeCommand, addRecentDocument])

  const handleSearchResultSelect = useCallback(async (result: SearchResult) => {
    addRecentSearch(searchQuery)
    addRecentDocument(result.id, result.metadata.filename)
    close()

    // Open the document file in default application
    try {
      const filePath = result.metadata.filename as string | undefined
      if (filePath) {
        const openResult = await VaultAPI.openFile(filePath)
        if (openResult.ok) {
          logger.info('Opened search result', { component: 'command-palette', id: result.id, path: filePath })
        } else {
          logger.error('Failed to open file:', { error: openResult.error })
        }
      } else {
        logger.error('No file path found for search result', { result })
      }
    } catch (error) {
      logger.error('Failed to open search result:', { error })
    }
  }, [searchQuery, addRecentSearch, addRecentDocument, close])

  const formatScore = (score: number): string => (score * 100).toFixed(1)

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  // Detect platform for keyboard shortcuts
  const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0
  const cmdKey = isMac ? '⌘' : 'Ctrl'

  return (
    <>
      {isOpen && (
        <>
          {/* Backdrop */}
          <div
            className="command-palette-backdrop animate-in fade-in duration-fast"
            onClick={close}
          />

          {/* Command Palette */}
          <div className="command-palette-container animate-in fade-in zoom-in-95 slide-in-from-top-4 duration-base">
            <Command className="command-palette" label="Command palette" shouldFilter={searchQuery.length < 2}>
              <div className="command-input-wrapper">
                <Search className="command-input-icon" strokeWidth={1.75} />
                <Command.Input
                  placeholder={searchMode ? "Search documents..." : "Type a command or search..."}
                  className="command-input"
                  autoFocus
                  value={searchQuery}
                  onValueChange={setSearchQuery}
                />
                {isSearching && (
                  <div className="command-input-loading">
                    <div className="w-4 h-4 border-2 border-[hsl(var(--accent))] border-t-transparent rounded-full animate-spin" />
                  </div>
                )}
              </div>

              <Command.List className="command-list">
                <Command.Empty className="command-empty">
                  {searchError ? (
                    <div className="command-empty-error">
                      <span className="text-[hsl(var(--danger-fg))]">Search failed</span>
                      <p className="text-xs mt-1 text-[hsl(var(--text-tertiary))]">{searchError}</p>
                    </div>
                  ) : (
                    'No results found.'
                  )}
                </Command.Empty>

                {/* Search Results from Live Search */}
                {searchQuery.length >= 2 && searchResults.length > 0 && (
                  <Command.Group heading="Search Results" className="command-group">
                    {searchResults.map((result) => (
                      <Command.Item
                        key={result.id}
                        value={`search-result-${result.id}`}
                        onSelect={handleAsyncEvent(() => handleSearchResultSelect(result))}
                        className="command-item search-result-item"
                      >
                        <div className="flex items-start gap-3 px-4 py-3">
                          <FileText className="w-4 h-4 mt-0.5 text-[hsl(var(--accent))] flex-shrink-0" strokeWidth={1.75} />
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-2 mb-1">
                              <span className="text-sm font-medium text-[hsl(var(--text-primary))] truncate">
                                {result.metadata.filename}
                              </span>
                              {result.metadata.file_type && (
                                <span className="text-xs px-1.5 py-0.5 rounded bg-[hsl(var(--surface))] text-[hsl(var(--text-secondary))] uppercase font-mono">
                                  {result.metadata.file_type}
                                </span>
                              )}
                            </div>
                            <p className="text-xs text-[hsl(var(--text-secondary))] line-clamp-2 mb-1">
                              {result.content}
                            </p>
                            <div className="flex items-center gap-3 text-xs text-[hsl(var(--text-tertiary))]">
                              {result.metadata.file_size && (
                                <span>{formatFileSize(result.metadata.file_size)}</span>
                              )}
                              {result.vectorScore && (
                                <span className="flex items-center gap-1">
                                  <Sparkles className="w-3 h-3" strokeWidth={1.75} />
                                  {formatScore(result.vectorScore)}%
                                </span>
                              )}
                              {result.bm25Score && (
                                <span className="flex items-center gap-1">
                                  <Search className="w-3 h-3" strokeWidth={1.75} />
                                  {formatScore(result.bm25Score)}%
                                </span>
                              )}
                            </div>
                          </div>
                        </div>
                      </Command.Item>
                    ))}
                  </Command.Group>
                )}

                {/* Recent Searches */}
                {!searchMode && searchQuery.length < 2 && recentSearches.length > 0 && (
                  <Command.Group heading="Recent Searches" className="command-group">
                    {recentSearches.map((search) => (
                      <CommandItem
                        key={search.id}
                        icon={Clock}
                        label={search.label}
                        onSelect={() => handleRecentSearch(search.label)}
                        value={`recent-search-${search.label}`}
                      />
                    ))}
                    <CommandItem
                      icon={Trash2}
                      label="Clear recent searches"
                      onSelect={clearRecentSearches}
                      value="clear-recent-searches"
                    />
                  </Command.Group>
                )}

                {/* Recent Documents */}
                {!searchMode && searchQuery.length < 2 && recentDocuments.length > 0 && (
                  <Command.Group heading="Recent Documents" className="command-group">
                    {recentDocuments.map((doc) => (
                      <CommandItem
                        key={doc.id}
                        icon={FileText}
                        label={doc.label}
                        onSelect={handleAsyncEvent(() => handleRecentDocument(doc.id, doc.label))}
                        value={`recent-doc-${doc.id}`}
                      />
                    ))}
                  </Command.Group>
                )}

                {/* Search Commands */}
                {!searchMode && searchQuery.length < 2 && (
                  <Command.Group heading="Search" className="command-group">
                    <CommandItem
                      icon={Search}
                      label="Search documents"
                      description="Search through all your documents"
                      shortcut={`${cmdKey}F`}
                      onSelect={handleSearchDocuments}
                    />
                    <CommandItem
                      icon={FileText}
                      label="Search by filename"
                      description="Find documents by their filename"
                      onSelect={handleSearchDocuments}
                    />
                  </Command.Group>
                )}

                {/* Upload Commands */}
                {!searchMode && searchQuery.length < 2 && (
                  <Command.Group heading="Upload" className="command-group">
                    <CommandItem
                      icon={Upload}
                      label="Upload document"
                      description="Upload a single document"
                      shortcut={`${cmdKey}U`}
                      onSelect={handleAsyncEvent(handleUploadDocument)}
                    />
                    <CommandItem
                      icon={FolderUp}
                      label="Upload folder"
                      description="Upload an entire folder"
                      shortcut={`${cmdKey}⇧U`}
                      onSelect={handleAsyncEvent(handleUploadFolder)}
                    />
                  </Command.Group>
                )}

                {/* Navigate Commands */}
                {!searchMode && searchQuery.length < 2 && (
                  <Command.Group heading="Navigate" className="command-group">
                    <CommandItem
                      icon={Home}
                      label="View all documents"
                      description="Browse all your documents"
                      shortcut={`${cmdKey}1`}
                      onSelect={handleViewAllDocuments}
                    />
                    <CommandItem
                      icon={Clock}
                      label="Recent documents"
                      description="View recently accessed documents"
                      shortcut={`${cmdKey}2`}
                      onSelect={handleViewAllDocuments}
                    />
                    <CommandItem
                      icon={NotebookPen}
                      label="Journals"
                      description="Open your journals workspace"
                      shortcut={`${cmdKey}3`}
                      onSelect={handleOpenJournals}
                    />
                    <CommandItem
                      icon={Bookmark}
                      label="Reference Inbox"
                      description="Open captured and pending references"
                      shortcut={`${cmdKey}5`}
                      onSelect={handleOpenReferenceInbox}
                    />
                  </Command.Group>
                )}

                {/* Settings Commands */}
                {!searchMode && searchQuery.length < 2 && (
                  <Command.Group heading="Settings" className="command-group">
                    <CommandItem
                      icon={SettingsIcon}
                      label="Preferences"
                      description="Open application settings"
                      shortcut={`${cmdKey},`}
                      onSelect={handleOpenSettings}
                    />
                    <CommandItem
                      icon={Trash2}
                      label="Clear cache"
                      description="Clear application cache and temporary files"
                      onSelect={handleAsyncEvent(handleClearCache)}
                    />
                  </Command.Group>
                )}

                {/* Help Commands */}
                {!searchMode && searchQuery.length < 2 && (
                  <Command.Group heading="Help" className="command-group">
                    <CommandItem
                      icon={Keyboard}
                      label="Show keyboard shortcuts"
                      description="View all available keyboard shortcuts"
                      shortcut="?"
                      onSelect={handleShowKeyboardShortcuts}
                    />
                    <CommandItem
                      icon={BookOpen}
                      label="Documentation"
                      description="Open the documentation in your browser"
                      onSelect={handleOpenDocumentation}
                    />
                  </Command.Group>
                )}
              </Command.List>

              {/* Footer */}
              <div className="command-footer">
                <div className="command-footer-hints">
                  <span className="command-footer-hint">
                    <kbd>↑↓</kbd> Navigate
                  </span>
                  <span className="command-footer-hint">
                    <kbd>Enter</kbd> Select
                  </span>
                  <span className="command-footer-hint">
                    <kbd>Esc</kbd> Close
                  </span>
                </div>
              </div>
            </Command>
          </div>
        </>
      )}

    {/* Keyboard Shortcuts Modal */}
    <KeyboardShortcutsModal
      isOpen={showShortcuts}
      onClose={() => setShowShortcuts(false)}
    />
  </>
  )
}
