import { lazy, Suspense, type ComponentType, type ReactNode } from 'react';

import { motion, useReducedMotion } from 'framer-motion';
import { createBrowserRouter, Navigate } from 'react-router';

import {
  ExplorerSectionErrorBoundary,
  FilesSectionErrorBoundary,
  QASectionErrorBoundary,
  SearchSectionErrorBoundary,
  SettingsSectionErrorBoundary,
  StudioSectionErrorBoundary,
} from './components/ErrorBoundary';
import { Layout } from './components/Layout';
import { RootLayout } from './components/RootLayout';

// Lazy-load every surface so the boot bundle stays small.
const Dashboard = lazy(() => import('./features/dashboard').then((m) => ({ default: m.Dashboard })));
const SearchInterface = lazy(() => import('./features/search/components/SearchInterface').then((m) => ({ default: m.SearchInterface })));
const FileTree = lazy(() => import('./features/files/components/FileTree').then((m) => ({ default: m.FileTree })));
const ChatView = lazy(() => import('@/features/chat').then((m) => ({ default: m.ChatView })));
const IngestHub = lazy(() => import('./features/files/components/IngestHub').then((m) => ({ default: m.IngestHub })));
const JournalWorkspace = lazy(() => import('@/features/journal').then((m) => ({ default: m.JournalWorkspace })));
const ReferenceInbox = lazy(() => import('@/features/references').then((m) => ({ default: m.ReferenceInbox })));
const ExplorerPage = lazy(() => import('./features/explorer').then((m) => ({ default: m.ExplorerPage })));
const LearningStudioPage = lazy(() => import('@/features/learning').then((m) => ({ default: m.LearningStudioPage })));
const ComparePage = lazy(() => import('./features/compare').then((m) => ({ default: m.ComparePage })));
const Settings = lazy(() => import('./features/settings').then((m) => ({ default: m.Settings })));

/** Route transition: a short fade with a few pixels of travel. Nothing bounces. */
const PAGE_VARIANTS = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -4 },
};
const PAGE_TRANSITION = { duration: 0.18, ease: [0.22, 1, 0.36, 1] as const };

const PageLoading = () => (
  <div className="flex h-full items-center justify-center">
    <p className="text-sm text-text-muted">Loading…</p>
  </div>
);

interface PageProps {
  id: string;
  boundary?: ComponentType<{ children: ReactNode }>;
  children: ReactNode;
}

function Page({ id, boundary: Boundary, children }: PageProps) {
  const reduceMotion = useReducedMotion();
  const content = <Suspense fallback={<PageLoading />}>{children}</Suspense>;
  return (
    <motion.div
      key={id}
      variants={PAGE_VARIANTS}
      initial={reduceMotion ? false : "initial"}
      animate="animate"
      exit="exit"
      transition={reduceMotion ? { duration: 0 } : PAGE_TRANSITION}
      className="h-full min-h-0"
    >
      {Boundary ? <Boundary>{content}</Boundary> : content}
    </motion.div>
  );
}

export const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    children: [
      {
        path: '/',
        element: <Layout />,
        children: [
          // The journal is the landing surface; Home stays reachable at /home.
          { index: true, element: <Navigate to="/journals" replace /> },
          { path: 'home', element: <Page id="home"><Dashboard /></Page> },
          {
            path: 'search',
            element: <Page id="search" boundary={SearchSectionErrorBoundary}><SearchInterface /></Page>,
          },
          {
            path: 'files',
            element: <Page id="files" boundary={FilesSectionErrorBoundary}><FileTree /></Page>,
          },
          { path: 'chat', element: <Page id="chat" boundary={QASectionErrorBoundary}><ChatView /></Page> },
          { path: 'ingest', element: <Page id="ingest"><IngestHub /></Page> },
          { path: 'journals', element: <Page id="journals"><JournalWorkspace /></Page> },
          { path: 'daily', element: <Navigate to="/journals" replace /> },
          { path: 'references', element: <Page id="references"><ReferenceInbox /></Page> },
          {
            path: 'explorer',
            element: <Page id="explorer" boundary={ExplorerSectionErrorBoundary}><ExplorerPage /></Page>,
          },
          { path: 'studio', element: <Page id="studio" boundary={StudioSectionErrorBoundary}><LearningStudioPage /></Page> },
          { path: 'compare', element: <Page id="compare"><ComparePage /></Page> },
          {
            path: 'settings',
            element: <Page id="settings" boundary={SettingsSectionErrorBoundary}><Settings /></Page>,
          },
        ],
      },
    ],
  },
]);
