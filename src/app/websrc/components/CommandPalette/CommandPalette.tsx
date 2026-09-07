import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { Command } from 'cmdk'
import {
  Bookmark,
  Clock,
  FilePlus,
  FileText,
  FolderOpen,
  FolderPlus,
  Globe,
  Home,
  Keyboard,
  MessageCircle,
  MessageSquarePlus,
  NotebookPen,
  Plus,
  Search,
  Settings as SettingsIcon,
  Trash2,
  Zap,
} from 'lucide-react'
import { useNavigate } from 'react-router'

import type { SearchResult } from '@/types'

import { useCommandPalette } from '../../hooks/useCommandPalette'
import VaultAPI from '../../lib/api'
import { selectPaletteGroups, usePaletteCommandsStore } from '../../stores/paletteCommandsStore'
import { logger } from '../../utils/logger'
import { handleAsyncEvent } from '../../utils/promiseHandlers'
import { CommandItem } from '../CommandItem'
import { KeyboardShortcutsModal } from '../KeyboardShortcutsModal'
import '../../styles/command-palette.css'

/**
 * CommandPalette — ⌘K. Live document search plus every navigation target and
 * creation action the app has, with the same shortcuts the rail shows.
 */
export function CommandPalette() {
  const navigate = useNavigate()
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
  const registeredCommands = usePaletteCommandsStore((state) => state.commands)
  const registeredGroups = useMemo(
    () => selectPaletteGroups({ commands: registeredCommands }),
    [registeredCommands],
  )
  const [searchQuery, setSearchQuery] = useState('')
  const [searchResults, setSearchResults] = useState<SearchResult[]>([])
  const [isSearching, setIsSearching] = useState(false)
  const [searchError, setSearchError] = useState<string | null>(null)
  const searchTimeoutRef = useRef<number | null>(null)
  const searchAbortControllerRef = useRef<AbortController | null>(null)

  // Debounced live search with cancellation.
  useEffect(() => {
    if (!searchQuery.trim() || searchQuery.length < 2) {
      setSearchResults([])
      setIsSearching(false)
      setSearchError(null)
      return
    }

    if (searchTimeoutRef.current) clearTimeout(searchTimeoutRef.current)
    if (searchAbortControllerRef.current) searchAbortControllerRef.current.abort()

    setIsSearching(true)
    setSearchError(null)

    const abortController = new AbortController()
    searchAbortControllerRef.current = abortController

    searchTimeoutRef.current = window.setTimeout(() => {
      void (async () => {
        try {
          const result = await VaultAPI.searchDocuments({ query: searchQuery, limit: 10, searchMode: 'hybrid' })
          if (abortController.signal.aborted) return
          if (result.ok) {
            setSearchResults(result.data)
          } else {
            setSearchError(result.error)
          }
          setIsSearching(false)
        } catch (error) {
          if (abortController.signal.aborted) return
          setSearchError(error instanceof Error ? error.message : 'Search failed')
          setIsSearching(false)
        }
      })()
    }, 300)

    return () => {
      if (searchTimeoutRef.current) clearTimeout(searchTimeoutRef.current)
      if (searchAbortControllerRef.current) searchAbortControllerRef.current.abort()
    }
  }, [searchQuery])

  useEffect(() => {
    if (!isOpen) {
      setSearchQuery('')
      setSearchResults([])
      setIsSearching(false)
      setSearchError(null)
    }
  }, [isOpen])

  const run = useCallback(
    (action: () => void | Promise<void>) => {
      const result = action()
      if (result instanceof Promise) {
        void result.then(() => close())
      } else {
        close()
      }
    },
    [close],
  )

  const goTo = useCallback((path: string) => () => run(() => navigate(path)), [navigate, run])

  const goToImport = useCallback(
    (tab: 'single-url' | 'files') => () =>
      run(() => {
        try {
          localStorage.setItem('ingestHub.lastTab', tab)
        } catch {
          // Preference only.
        }
        navigate('/ingest')
      }),
    [navigate, run],
  )

  const handleAddFiles = useCallback(async () => {
    run(async () => {
      try {
        const selected = await VaultAPI.selectMultipleFiles()
        if (!selected || selected.length === 0) return
        for (const file of selected) {
          const result = await VaultAPI.indexFile(file)
          if (!result.ok) logger.error('Failed to index file:', { file, error: result.error })
        }
      } catch (error) {
        logger.error('Failed to add files:', { error })
      }
    })
  }, [run])

  const handleAddFolder = useCallback(async () => {
    run(async () => {
      try {
        const selected = await VaultAPI.selectFolder()
        if (!selected) return
        const result = await VaultAPI.startIndexing(selected, true)
        if (!result.ok) logger.error('Failed to index directory:', { path: selected, error: result.error })
      } catch (error) {
        logger.error('Failed to add folder:', { error })
      }
    })
  }, [run])

  const handleClearCache = useCallback(async () => {
    run(async () => {
      const result = await VaultAPI.clearCache()
      if (!result.ok) logger.error('Failed to clear cache:', { error: result.error })
    })
  }, [run])

  const handleShowKeyboardShortcuts = useCallback(() => {
    close()
    setShowShortcuts(true)
  }, [close])

  const handleRecentSearch = useCallback(
    (search: string) => {
      run(() => {
        addRecentSearch(search)
        enterSearchMode()
        navigate('/search')
      })
    },
    [addRecentSearch, enterSearchMode, navigate, run],
  )

  const handleRecentDocument = useCallback(
    async (docId: string, docName: string) => {
      run(async () => {
        addRecentDocument(docId, docName)
        try {
          const docResult = await VaultAPI.getDocument(docId)
          if (!docResult.ok) {
            logger.error('Failed to retrieve document metadata', { component: 'command-palette', docId, error: docResult.error })
            return
          }
          const { filePath } = docResult.data
          if (!filePath) return
          const openResult = await VaultAPI.openFile(filePath)
          if (!openResult.ok) {
            logger.error('Failed to open document', { component: 'command-palette', docId, path: filePath, error: openResult.error })
          }
        } catch (error) {
          logger.error('Unexpected error opening recent document', {
            component: 'command-palette',
            docId,
            error: error instanceof Error ? error.message : String(error),
          })
        }
      })
    },
    [addRecentDocument, run],
  )

  const handleSearchResultSelect = useCallback(
    async (result: SearchResult) => {
      addRecentSearch(searchQuery)
      addRecentDocument(result.id, result.metadata.filename ?? result.title)
      close()
      try {
        const metadataPath = result.metadata.path
        const filePath = result.path ?? (typeof metadataPath === 'string' ? metadataPath : undefined)
        if (!filePath) return
        const openResult = await VaultAPI.openFile(filePath)
        if (!openResult.ok) logger.error('Failed to open file:', { error: openResult.error })
      } catch (error) {
        logger.error('Failed to open search result:', { error })
      }
    },
    [addRecentDocument, addRecentSearch, close, searchQuery],
  )

  const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().includes('MAC')
  const cmd = isMac ? '⌘' : 'Ctrl+'
  const showCommands = !searchMode && searchQuery.length < 2

  return (
    <>
      {isOpen && (
        <>
          <div className="command-palette-backdrop animate-in fade-in duration-fast" onClick={close} aria-hidden="true" />

          <div className="command-palette-container animate-in fade-in zoom-in-95 duration-fast">
            <Command className="command-palette" label="Command palette" shouldFilter={searchQuery.length < 2}>
              <div className="command-input-wrapper">
                <Search className="command-input-icon" strokeWidth={1.75} />
                <Command.Input
                  placeholder={searchMode ? 'Search documents' : 'Search, or type a command'}
                  className="command-input"
                  autoFocus
                  value={searchQuery}
                  onValueChange={setSearchQuery}
                />
                {isSearching && <span className="command-input-loading">Searching…</span>}
              </div>

              <Command.List className="command-list">
                <Command.Empty className="command-empty">
                  {searchError ? `Couldn't search. ${searchError}` : 'No matches.'}
                </Command.Empty>

                {searchQuery.length >= 2 && searchResults.length > 0 && (
                  <Command.Group heading="Documents" className="command-group">
                    {searchResults.map((result) => {
                      const name = result.metadata.filename ?? result.title
                      const path = result.path ?? (typeof result.metadata.path === 'string' ? result.metadata.path : '')
                      return (
                        <Command.Item
                          key={result.id}
                          value={`search-result-${result.id}`}
                          onSelect={handleAsyncEvent(() => handleSearchResultSelect(result))}
                          className="command-item"
                        >
                          <div className="flex items-start gap-3 px-3 py-2">
                            <FileText className="mt-0.5 h-4 w-4 shrink-0 text-text-muted" strokeWidth={1.75} />
                            <div className="min-w-0 flex-1">
                              <div className="flex items-baseline justify-between gap-3">
                                <span className="truncate text-sm text-text-primary">{name}</span>
                                <span className="shrink-0 font-mono text-xxs tabular-nums text-text-muted">
                                  {result.score.toFixed(2)}
                                </span>
                              </div>
                              {path ? <p className="truncate text-xs text-text-muted">{path}</p> : null}
                              {result.content ? (
                                <p className="mt-0.5 line-clamp-2 text-xs text-text-tertiary">{result.content}</p>
                              ) : null}
                            </div>
                          </div>
                        </Command.Item>
                      )
                    })}
                  </Command.Group>
                )}

                {showCommands && recentSearches.length > 0 && (
                  <Command.Group heading="Recent searches" className="command-group">
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

                {showCommands && recentDocuments.length > 0 && (
                  <Command.Group heading="Recent documents" className="command-group">
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

                {showCommands &&
                  registeredGroups.map((group) => (
                    <Command.Group key={group.heading} heading={group.heading} className="command-group">
                      {group.commands.map((command) => (
                        <CommandItem
                          key={command.id}
                          icon={command.icon ?? Zap}
                          label={command.label}
                          shortcut={command.shortcut}
                          description={command.description}
                          value={[command.label, ...(command.keywords ?? [])].join(' ')}
                          onSelect={() => {
                            close()
                            void Promise.resolve(command.run()).catch((error: unknown) => {
                              logger.error('Palette command failed:', { id: command.id, error })
                            })
                          }}
                        />
                      ))}
                    </Command.Group>
                  ))}

                {showCommands && (
                  <Command.Group heading="New" className="command-group">
                    <CommandItem icon={MessageSquarePlus} label="New conversation" shortcut={`${cmd}N`} onSelect={goTo('/chat?new=1')} />
                    <CommandItem icon={NotebookPen} label="New journal entry" onSelect={goTo('/journals?new=1')} />
                    <CommandItem icon={FilePlus} label="Add files" onSelect={handleAsyncEvent(handleAddFiles)} />
                    <CommandItem icon={FolderPlus} label="Add folder" onSelect={handleAsyncEvent(handleAddFolder)} />
                    <CommandItem icon={Globe} label="Add web page" onSelect={goToImport('single-url')} />
                  </Command.Group>
                )}

                {showCommands && (
                  <Command.Group heading="Go to" className="command-group">
                    <CommandItem icon={Home} label="Home" shortcut={`${cmd}0`} onSelect={goTo('/home')} />
                    <CommandItem icon={Search} label="Search" shortcut={`${cmd}1`} onSelect={goTo('/search')} />
                    <CommandItem icon={FolderOpen} label="Library" shortcut={`${cmd}2`} onSelect={goTo('/files')} />
                    <CommandItem icon={NotebookPen} label="Journal" shortcut={`${cmd}3`} onSelect={goTo('/journals')} />
                    <CommandItem icon={MessageCircle} label="Chat" shortcut={`${cmd}4`} onSelect={goTo('/chat')} />
                    <CommandItem icon={Bookmark} label="References" shortcut={`${cmd}5`} onSelect={goTo('/references')} />
                    <CommandItem icon={Plus} label="Import" shortcut={`${cmd}I`} onSelect={goToImport('files')} />
                    <CommandItem icon={SettingsIcon} label="Settings" shortcut={`${cmd},`} onSelect={goTo('/settings')} />
                  </Command.Group>
                )}

                {showCommands && (
                  <Command.Group heading="More" className="command-group">
                    <CommandItem icon={Keyboard} label="Keyboard shortcuts" onSelect={handleShowKeyboardShortcuts} />
                    <CommandItem icon={Trash2} label="Clear search cache" onSelect={handleAsyncEvent(handleClearCache)} />
                  </Command.Group>
                )}
              </Command.List>

              <div className="command-footer">
                <span className="command-footer-hint">
                  <kbd>↑↓</kbd> Move
                </span>
                <span className="command-footer-hint">
                  <kbd>↵</kbd> Open
                </span>
                <span className="command-footer-hint">
                  <kbd>Esc</kbd> Close
                </span>
              </div>
            </Command>
          </div>
        </>
      )}

      <KeyboardShortcutsModal isOpen={showShortcuts} onClose={() => setShowShortcuts(false)} />
    </>
  )
}
