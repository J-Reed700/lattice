import { useMemo } from 'react';

import { EyeOff, Layers, MessageSquare, PanelRight, RefreshCw } from 'lucide-react';

import { isWebDocument } from '@/features/files/model/documentMetadata';
import { useRegisterPaletteCommands } from '@/features/palette/hooks/useRegisterPaletteCommands';
import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import { type DocumentMetadata } from '@/types/fileBrowser';

/** Registers the Library's command-palette entries for the focused document. */
export function useLibraryPaletteCommands({
  focusedDoc,
  askAbout,
  reindexDocument,
  removeFromIndex,
  isNeighborhoodOpen,
  toggleNeighborhood,
  hasRunThemes,
  themesEnabled,
  isFindingThemes,
  handleFindThemes,
}: {
  focusedDoc: DocumentMetadata | null;
  askAbout: (doc: DocumentMetadata) => void;
  reindexDocument: (doc: DocumentMetadata) => Promise<void>;
  removeFromIndex: (doc: DocumentMetadata) => Promise<void>;
  isNeighborhoodOpen: boolean;
  toggleNeighborhood: () => void;
  hasRunThemes: boolean;
  themesEnabled: boolean;
  isFindingThemes: boolean;
  handleFindThemes: () => void;
}) {
  const paletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'library.askAboutDocument',
        label: 'Ask about this document',
        group: 'Library',
        icon: MessageSquare,
        enabled: focusedDoc !== null,
        run: () => {
          if (focusedDoc) askAbout(focusedDoc);
        },
      },
      {
        id: 'library.reindexFile',
        label: 'Reindex this file',
        group: 'Library',
        icon: RefreshCw,
        enabled: focusedDoc !== null && !isWebDocument(focusedDoc),
        run: () => {
          if (focusedDoc) void reindexDocument(focusedDoc);
        },
      },
      {
        id: 'library.removeFromIndex',
        label: 'Remove this file from the index',
        group: 'Library',
        icon: EyeOff,
        enabled: focusedDoc !== null && !isWebDocument(focusedDoc),
        run: () => {
          if (focusedDoc) void removeFromIndex(focusedDoc);
        },
      },
      {
        id: 'library.toggleRelated',
        label: isNeighborhoodOpen ? 'Hide related documents' : 'Show related documents',
        group: 'Library',
        icon: PanelRight,
        enabled: focusedDoc !== null,
        run: toggleNeighborhood,
      },
      {
        id: 'library.findThemes',
        label: hasRunThemes ? 'Refresh themes' : 'Find themes in vault',
        group: 'Library',
        icon: Layers,
        enabled: themesEnabled && !isFindingThemes,
        run: handleFindThemes,
      },
    ],
    [
      askAbout,
      focusedDoc,
      handleFindThemes,
      hasRunThemes,
      isFindingThemes,
      isNeighborhoodOpen,
      reindexDocument,
      removeFromIndex,
      themesEnabled,
      toggleNeighborhood,
    ]
  );
  useRegisterPaletteCommands(paletteCommands);
}
