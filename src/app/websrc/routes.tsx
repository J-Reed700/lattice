import { lazy, Suspense } from 'react';

import { motion } from 'framer-motion';
import { createBrowserRouter, Navigate } from 'react-router-dom';

import {
  SearchSectionErrorBoundary,
  FilesSectionErrorBoundary,
  QASectionErrorBoundary,
  SettingsSectionErrorBoundary,
} from './components/ErrorBoundary';
import { Layout } from './components/Layout';
import { RootLayout } from './components/RootLayout';
import { pageTransition } from './lib/animations';

// Lazy load all page components for code splitting
const Dashboard = lazy(() => import('./components/Dashboard').then(m => ({ default: m.Dashboard })));
const SearchInterface = lazy(() => import('./components/SearchInterface').then(m => ({ default: m.SearchInterface })));
const FileTree = lazy(() => import('./components/FileTree').then(m => ({ default: m.FileTree })));
const ChatView = lazy(() => import('./components/Chat').then(m => ({ default: m.ChatView })));
const IngestHub = lazy(() => import('./components/IngestHub').then(m => ({ default: m.IngestHub })));
const DailyNotesWorkspace = lazy(() => import('./components/DailyNotes').then(m => ({ default: m.DailyNotesWorkspace })));
const ReferenceInbox = lazy(() => import('./components/ReferenceInbox').then(m => ({ default: m.ReferenceInbox })));
const Settings = lazy(() => import('./components/Settings').then(m => ({ default: m.Settings })));

// Loading fallback component with smooth animation
const PageLoading = () => (
  <motion.div
    initial={{ opacity: 0 }}
    animate={{ opacity: 1 }}
    exit={{ opacity: 0 }}
    className="flex items-center justify-center h-full"
  >
    <motion.div
      animate={{ rotate: 360 }}
      transition={{ duration: 1, repeat: Infinity, ease: 'linear' }}
      className="rounded-full h-8 w-8 border-b-2 border-[var(--accent-primary)]"
    />
  </motion.div>
);

// Route configuration
export const router = createBrowserRouter([
  {
    path: '/',
    element: <RootLayout />,
    children: [
      {
        path: '/',
        element: <Layout />,
        children: [
          {
            index: true,
            element: <Navigate to="/home" replace />,
          },
          {
            path: 'home',
            element: (
              <motion.div
                key="home"
                variants={pageTransition}
                initial="initial"
                animate="animate"
                exit="exit"
                className="h-full"
              >
                <Suspense fallback={<PageLoading />}>
                  <Dashboard />
                </Suspense>
              </motion.div>
            ),
          },
          {
            path: 'search',
            element: (
              <motion.div
                key="search"
                variants={pageTransition}
                initial="initial"
                animate="animate"
                exit="exit"
                className="h-full"
              >
                <Suspense fallback={<PageLoading />}>
                  <SearchSectionErrorBoundary>
                    <SearchInterface />
                  </SearchSectionErrorBoundary>
                </Suspense>
              </motion.div>
            ),
          },
          {
            path: 'files',
            element: (
              <motion.div
                key="files"
                variants={pageTransition}
                initial="initial"
                animate="animate"
                exit="exit"
                className="h-full"
              >
                <Suspense fallback={<PageLoading />}>
                  <FilesSectionErrorBoundary>
                    <FileTree />
                  </FilesSectionErrorBoundary>
                </Suspense>
              </motion.div>
            ),
          },
          {
            path: 'chat',
            element: (
              <motion.div
                key="chat"
                variants={pageTransition}
                initial="initial"
                animate="animate"
                exit="exit"
                className="h-full"
              >
                <Suspense fallback={<PageLoading />}>
                  <QASectionErrorBoundary>
                    <ChatView />
                  </QASectionErrorBoundary>
                </Suspense>
              </motion.div>
            ),
          },
          {
            path: 'ingest',
            element: (
              <motion.div
                key="ingest"
                variants={pageTransition}
                initial="initial"
                animate="animate"
                exit="exit"
                className="h-full"
              >
                <Suspense fallback={<PageLoading />}>
                  <IngestHub />
                </Suspense>
              </motion.div>
            ),
          },
          {
            path: 'daily',
            element: (
              <motion.div
                key="daily"
                variants={pageTransition}
                initial="initial"
                animate="animate"
                exit="exit"
                className="h-full"
              >
                <Suspense fallback={<PageLoading />}>
                  <DailyNotesWorkspace />
                </Suspense>
              </motion.div>
            ),
          },
          {
            path: 'references',
            element: (
              <motion.div
                key="references"
                variants={pageTransition}
                initial="initial"
                animate="animate"
                exit="exit"
                className="h-full"
              >
                <Suspense fallback={<PageLoading />}>
                  <ReferenceInbox />
                </Suspense>
              </motion.div>
            ),
          },
          {
            path: 'settings',
            element: (
              <motion.div
                key="settings"
                variants={pageTransition}
                initial="initial"
                animate="animate"
                exit="exit"
                className="h-full"
              >
                <Suspense fallback={<PageLoading />}>
                  <SettingsSectionErrorBoundary>
                    <Settings />
                  </SettingsSectionErrorBoundary>
                </Suspense>
              </motion.div>
            ),
          },
        ],
      },
    ],
  },
]);
