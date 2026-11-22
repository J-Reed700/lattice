import { useCallback, useEffect, useMemo, useState } from 'react'

import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { Command } from 'cmdk'
import {
  Bookmark,
  BookOpen,
  Clock,
  FilePlus,
  FileSearch,
  FileText,
  FolderOpen,
  FolderPlus,
  FolderTree,
  Globe,
  Home,
  Keyboard,
  Layers,
  MessageCircle,
  MessageSquarePlus,
  NotebookPen,
  Plus,
  Search,
  Settings as SettingsIcon,
  Trash2,
  Zap,
} from 'lucide-react'

import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import { useApi } from '@/contexts/ApiContext'
import { useClearCacheMutation } from '@/features/cache/hooks/useClearCacheMutation'
import { chooseFiles, chooseFolder } from '@/features/files/hooks/filePickers'
import { useAddToLibrary } from '@/features/files/hooks/useAddToLibrary'
import { fetchDocument, openFilePath } from '@/features/files/hooks/useLibraryDocumentActions'
import type { RecentItem } from '@/features/palette/hooks/useCommandPalette'
import { selectPaletteGroups, usePaletteCommandsStore } from '@/features/palette/stores/paletteCommandsStore'
import { quickSearchDocuments } from '@/features/search/hooks/useSearchQuery'
import { useAppNavigate } from '@/hooks/useAppNavigate'
import { useDebounce } from '@/hooks/useDebounce'
import { describeFailure } from '@/lib/errorHandling'
import { getErrorMessage } from '@/lib/errorUtils'
import { toast } from '@/stores/toastStore'
import type { SearchResult } from '@/types'
import { logger } from '@/utils/logger'
import { handleAsyncEvent } from '@/utils/promiseHandlers'

import { CommandItem } from '../CommandItem'

import '@/styles/command-palette.css'

const NO_RESULTS: SearchResult[] = []

/** Two characters, not all spaces, before the library is searched. */
const isSearchable = (query: string) => query.length >= 2 && query.trim().length > 0

export interface CommandPaletteDialogProps {
  isOpen: boolean
  searchMode: boolean
  recentSearches: RecentItem[]
  recentDocuments: RecentItem[]
  close: () => void
  enterSearchMode: () => void
  addRecentSearch: (search: string) => void
  addRecentDocument: (id: string, label: string) => void
  clearRecentSearches: () => void
  onShowShortcuts: () => void
}

export function CommandPaletteDialog({
  isOpen,
  searchMode,
  recentSearches,
  recentDocuments,
  close,
  enterSearchMode,
  addRecentSearch,
  addRecentDocument,
  clearRecentSearches,
  onShowShortcuts,
}: CommandPaletteDialogProps) {
  const api = useApi()
  const navigate = useAppNavigate()

  const { indexFile, indexFolder } = useAddToLibrary()
  const { mutateAsync: clearCache } = useClearCacheMutation()

  const registeredCommands = usePaletteCommandsStore((state) => state.commands)
  const registeredGroups = useMemo(
    () => selectPaletteGroups({ commands: registeredCommands }),
    [registeredCommands],
  )
  const [searchQuery, setSearchQuery] = useState('')
  const debouncedSearchQuery = useDebounce(searchQuery, 300)
  const searchActive = isSearchable(searchQuery)
  const quickSearch = useQuery({
    queryKey: ['search', 'quick', debouncedSearchQuery] as const,
    queryFn: async () => {
      const result = await quickSearchDocuments(debouncedSearchQuery, api)
      if (!result.ok) throw new Error(describeFailure(result))
      return result.data
    },
    enabled: searchActive && isSearchable(debouncedSearchQuery),
    placeholderData: keepPreviousData,
    retry: false,
    staleTime: 0,
  })
  const searchResults = searchActive ? quickSearch.data ?? NO_RESULTS : NO_RESULTS
  const isSearching = searchActive && (searchQuery !== debouncedSearchQuery || quickSearch.isFetching)
  const searchError =
    searchActive && !isSearching && quickSearch.isError
      ? quickSearch.error instanceof Error ? quickSearch.error.message : 'Search failed'
      : null

  useEffect(() => {
    if (!isOpen) setSearchQuery('')
  }, [isOpen])

  const run = useCallback(
    (action: () => void | Promise<void>): void => {
      close()
      void Promise.resolve(action()).catch((error: unknown) => {
        logger.error('Command palette action failed:', { error })
      })
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

  const handleAddFiles = useCallback(() => {
    run(async () => {
      try {
        const selected = await chooseFiles(api)
        if (!selected || selected.length === 0) return
        const failures: string[] = []
        for (const file of selected) {
          const result = await indexFile(file)
          if (!result.ok) {
            logger.error('Failed to index file:', { file, error: result.error })
            failures.push(result.error)
          }
        }
        if (failures.length > 0) {
          toast.error(
            `Couldn't add ${failures.length} of ${selected.length} file${selected.length !== 1 ? 's' : ''}`,
            { message: failures[0] },
          )
        }
      } catch (error) {
        logger.error('Failed to add files:', { error })
        toast.error("Couldn't add files", { message: getErrorMessage(error) })
      }
    })
  }, [api, indexFile, run])

  const handleAddFolder = useCallback(() => {
    run(async () => {
      try {
        const selected = await chooseFolder(api)
        if (!selected) return
        const result = await indexFolder(selected)
        if (!result.ok) {
          logger.error('Failed to index directory:', { path: selected, error: result.error })
          toast.error("Couldn't add this folder", { message: result.error })
        }
      } catch (error) {
        logger.error('Failed to add folder:', { error })
        toast.error("Couldn't add this folder", { message: getErrorMessage(error) })
      }
    })
  }, [api, indexFolder, run])

  const handleClearCache = useCallback(() => {
    run(async () => {
      const result = await clearCache()
      if (!result.ok) {
        logger.error('Failed to clear cache:', { error: result.error })
        toast.error("Couldn't clear the cache", { message: result.error })
      }
    })
  }, [clearCache, run])

  const handleShowKeyboardShortcuts = useCallback(() => {
    close()
    onShowShortcuts()
  }, [close, onShowShortcuts])

  const handleRecentSearch = useCallback(
    (search: string) => {
      run(() => {
        addRecentSearch(search)
        enterSearchMode()
        navigate(`/search?q=${encodeURIComponent(search)}`)
      })
    },
    [addRecentSearch, enterSearchMode, navigate, run],
  )

  const handleRecentDocument = useCallback(
    (docId: string, docName: string) => {
      run(async () => {
        addRecentDocument(docId, docName)
        try {
          const docResult = await fetchDocument(docId, api)
          if (!docResult.ok) {
            logger.error('Failed to retrieve document metadata', { component: 'command-palette', docId, error: docResult.error })
            return
          }
          const { filePath } = docResult.data
          if (!filePath) return
          const openResult = await openFilePath(filePath, api)
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
    [addRecentDocument, api, run],
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
        const openResult = await openFilePath(filePath, api)
        if (!openResult.ok) logger.error('Failed to open file:', { error: openResult.error })
      } catch (error) {
        logger.error('Failed to open search result:', { error })
      }
    },
    [addRecentDocument, addRecentSearch, api, close, searchQuery],
  )

  const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().includes('MAC')
  const cmd = isMac ? '⌘' : 'Ctrl+'
  const showCommands = !searchMode && searchQuery.length < 2

  return (
    <Dialog open={isOpen} onOpenChange={(open) => { if (!open) close() }}>
      <DialogContent
        unstyled
        hideClose
        overlayClassName="command-palette-backdrop animate-in fade-in duration-fast"
        className="command-palette-container animate-in fade-in zoom-in-95 duration-fast"
        aria-describedby={undefined}
        aria-modal="true"
      >
        <DialogTitle className="sr-only">Command palette</DialogTitle>
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
                    onSelect={() => handleRecentDocument(doc.id, doc.label)}
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
                <CommandItem icon={BookOpen} label="New learning program" onSelect={goTo('/studio?new=1')} />
                <CommandItem icon={MessageSquarePlus} label="New conversation" shortcut={`${cmd}N`} onSelect={goTo('/chat?new=1')} />
                <CommandItem icon={NotebookPen} label="New journal entry" onSelect={goTo('/journals?new=1')} />
                <CommandItem icon={FilePlus} label="Add files" onSelect={handleAddFiles} />
                <CommandItem icon={FolderPlus} label="Add folder" onSelect={handleAddFolder} />
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
                <CommandItem icon={FolderTree} label="Explorer" shortcut={`${cmd}6`} onSelect={goTo('/explorer')} />
                <CommandItem icon={BookOpen} label="Studio" shortcut={`${cmd}7`} onSelect={goTo('/studio')} />
                <CommandItem icon={Layers} label="Flashcards" onSelect={goTo('/studio?section=flashcards')} />
                <CommandItem icon={FileSearch} label="Review drills" onSelect={goTo('/studio?drills=open')} />
                <CommandItem icon={Plus} label="Import" shortcut={`${cmd}I`} onSelect={goToImport('files')} />
                <CommandItem icon={SettingsIcon} label="Settings" shortcut={`${cmd},`} onSelect={goTo('/settings')} />
              </Command.Group>
            )}

            {showCommands && (
              <Command.Group heading="More" className="command-group">
                <CommandItem icon={Keyboard} label="Keyboard shortcuts" onSelect={handleShowKeyboardShortcuts} />
                <CommandItem icon={Trash2} label="Clear search cache" onSelect={handleClearCache} />
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
      </DialogContent>
    </Dialog>
  )
}
