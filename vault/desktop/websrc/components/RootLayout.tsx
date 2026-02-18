import { useMemo } from 'react';

import { Outlet, useNavigate } from 'react-router-dom';

import { CommandPalette } from './CommandPalette';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';

/**
 * RootLayout - Wrapper for all routes
 * Contains global UI elements like CommandPalette
 * Handles global keyboard shortcuts
 */
export function RootLayout() {
  console.log('[ROOTLAYOUT] RootLayout component rendering...');
  const navigate = useNavigate();

  const globalShortcuts = useMemo(() => [
    {
      id: 'global.newNote',
      keys: 'Mod+N',
      description: 'Create new note',
      category: 'global' as const,
      handler: () => navigate('/home'),
      global: true,
    },
    {
      id: 'global.settings',
      keys: 'Mod+,',
      description: 'Open settings',
      category: 'global' as const,
      handler: () => navigate('/settings'),
      global: true,
    },
    {
      id: 'navigation.goToHome',
      keys: 'Mod+0',
      description: 'Go to home',
      category: 'navigation' as const,
      handler: () => navigate('/home'),
      global: true,
    },
    {
      id: 'navigation.goToSearch',
      keys: 'Mod+1',
      description: 'Go to search',
      category: 'navigation' as const,
      handler: () => navigate('/search'),
      global: true,
    },
    {
      id: 'navigation.goToFiles',
      keys: 'Mod+2',
      description: 'Go to files',
      category: 'navigation' as const,
      handler: () => navigate('/files'),
      global: true,
    },
    {
      id: 'navigation.goToChat',
      keys: 'Mod+4',
      description: 'Go to Chat',
      category: 'navigation' as const,
      handler: () => navigate('/chat'),
      global: true,
    },
    {
      id: 'navigation.goToDaily',
      keys: 'Mod+3',
      description: 'Go to Daily Notes',
      category: 'navigation' as const,
      handler: () => navigate('/daily'),
      global: true,
    },
    {
      id: 'navigation.goToIngest',
      keys: 'Mod+I',
      description: 'Add content / Import files',
      category: 'navigation' as const,
      handler: () => navigate('/ingest'),
      global: true,
    },
  ], [navigate]);

  useKeyboardShortcuts(globalShortcuts, { enabled: true });

  const handleNavigate = (view: 'search' | 'files' | 'settings' | 'daily') => {
    navigate(`/${view}`);
  };

  return (
    <>
      <Outlet />
      <CommandPalette onNavigate={handleNavigate} />
    </>
  );
}
