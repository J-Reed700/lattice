import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';


import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import type { DashboardData } from '@/features/dashboard/hooks/useDashboardQuery';
import { openDocumentFile } from '@/features/files/hooks/useLibraryDocumentActions';
import { OPEN_PALETTE_EVENT } from '@/features/palette/hooks/useCommandPalette';
import VaultAPI from '@/lib/api';

import { Dashboard } from './Dashboard';

const queries = vi.hoisted(() => ({
  dashboard: vi.fn(),
  week: vi.fn(),
  corpus: vi.fn(),
}));
vi.mock('@/features/dashboard/hooks/useDashboardQuery', () => ({ useDashboardQuery: queries.dashboard }));
vi.mock('@/features/journal/hooks/useWeeklySynthesisCandidatesQuery', () => ({ useWeeklySynthesisCandidatesQuery: queries.week }));
vi.mock('@/features/corpus-shape/hooks/useCorpusShapeQuery', () => ({ useCorpusShapeQuery: queries.corpus }));
vi.mock('@/features/files/hooks/useLibraryDocumentActions', () => ({ openDocumentFile: vi.fn() }));
vi.mock('@/features/reading/components/FilePreviewModal', () => ({
  FilePreviewModal: ({ isOpen, source, onClose }: { isOpen: boolean; source: { filePath: string } | null; onClose: () => void }) =>
    isOpen ? <div role="dialog" aria-label="Reader">{source?.filePath}<button type="button" onClick={onClose}>Close reader</button></div> : null,
}));

const NOW = new Date(2026, 9, 21, 9, 30); // a Wednesday

function data(overrides: Partial<DashboardData> = {}): DashboardData {
  return {
    stats: { documentCount: 1234, storageBytes: 2048, folderCount: 1, lastIndexedAt: null, fileTypes: [] },
    recentDocuments: [], recentConversations: [], todayJournalEntry: null, preferredJournalSpaceId: null,
    recentReferences: [], journalSpaceIds: [], courses: [], spaces: [], ...overrides,
  };
}

function Location() {
  const location = useLocation();
  return <output aria-label="location">{location.pathname + location.search}</output>;
}

function renderHome(value: Partial<{ data: DashboardData; isLoading: boolean; error: Error | null }> = { data: data() }) {
  queries.dashboard.mockReturnValue({ data: undefined, isLoading: false, error: null, ...value });
  render(<MemoryRouter initialEntries={['/home']}><Dashboard /><Location /></MemoryRouter>);
}

const location = () => screen.getByRole('status', { name: 'location' }).textContent;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(NOW);
  localStorage.removeItem('home.weeklySynthesis.dismissedWeek');
  queries.week.mockReturnValue({ data: undefined });
  queries.corpus.mockReturnValue({ data: undefined });
  vi.mocked(openDocumentFile).mockReset();
});
afterEach(() => vi.useRealTimers());

describe('Dashboard', () => {
  it('shows a skeleton while loading and the error with a retry when it fails', () => {
    renderHome({ isLoading: true });
    expect(screen.queryByRole('heading', { level: 1 })).not.toBeInTheDocument();
  });

  it('shows the load error', () => {
    renderHome({ error: new Error('database is busy') });
    expect(screen.getByText("Couldn't load Home.")).toBeInTheDocument();
    expect(screen.getByText('database is busy')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it.each([
    [3, 'Still up.'],
    [9, 'Good morning.'],
    [14, 'Good afternoon.'],
    [21, 'Good evening.'],
  ])('greets by the hour (%i:00)', (hour, text) => {
    vi.setSystemTime(new Date(2026, 9, 21, hour));
    renderHome();
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(text);
  });

  it('reads out only the library numbers the backend reported', () => {
    renderHome({ data: data({ stats: { documentCount: 1234, storageBytes: 0, folderCount: null, lastIndexedAt: null, fileTypes: [] } }) });
    expect(screen.getByText('1,234')).toBeInTheDocument();
    expect(screen.getByText('Documents')).toBeInTheDocument();
    expect(screen.queryByText(/^Folders?$/)).not.toBeInTheDocument();
    expect(screen.queryByText('On disk')).not.toBeInTheDocument();
    expect(screen.getByText('Never')).toBeInTheDocument();
  });

  it('shows folders, size, file types and recent growth when known', () => {
    queries.corpus.mockReturnValue({ data: { grownLast7Days: 1500 } });
    renderHome({
      data: data({
        stats: {
          documentCount: 12, storageBytes: 5 * 1024 * 1024, folderCount: 3, lastIndexedAt: new Date(2026, 9, 21, 9).toISOString(),
          fileTypes: [{ label: 'PDF', count: 8 }, { label: 'Markdown', count: 4 }],
        },
      }),
    });
    expect(screen.getByText('Folders')).toBeInTheDocument();
    expect(screen.getByText('On disk')).toBeInTheDocument();
    expect(screen.getByText(/8 PDF/).closest('p')).toHaveTextContent('8 PDF · 4 Markdown');
    expect(screen.getByText(/added in the last 7 days/)).toHaveTextContent(`${(1500).toLocaleString()} added in the last 7 days`);
    expect(screen.queryByText('Never')).not.toBeInTheDocument();
  });

  it('names a single folder in the singular and leaves out a library with no stats', () => {
    renderHome({ data: data({ stats: { documentCount: 1, storageBytes: null, folderCount: 1, lastIndexedAt: null, fileTypes: [] } }) });
    expect(screen.getByText('Folder')).toBeInTheDocument();
  });

  it('shows empty states for every list', () => {
    renderHome({ data: { ...data(), stats: undefined as never } });
    expect(screen.queryByText('Documents')).not.toBeInTheDocument();
    expect(screen.getByText(/Your conversations will appear here/)).toBeInTheDocument();
    expect(screen.getByText(/Nothing saved yet/)).toBeInTheDocument();
    expect(screen.getByText(/Add your first files/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start today’s entry' })).toBeInTheDocument();
  });

  it('opens the palette from the search bar', async () => {
    const user = userEvent.setup();
    const opened = vi.fn();
    window.addEventListener(OPEN_PALETTE_EVENT, opened);
    renderHome();
    await user.click(screen.getByRole('button', { name: /Find a document/ }));
    expect(opened).toHaveBeenCalledTimes(1);
    window.removeEventListener(OPEN_PALETTE_EVENT, opened);
  });

  it.each([
    ['Search', '/search'],
    ['Ask', '/chat?new=1'],
    ['Studio', '/studio'],
    ['Add', '/ingest'],
    ['Open studio', '/studio'],
    ['All conversations', '/chat'],
    ['All references', '/references'],
    ['Open library', '/files'],
  ])('%s goes to %s', async (name, path) => {
    const user = userEvent.setup();
    renderHome();
    await user.click(screen.getByRole('button', { name: new RegExp(`^${name}`) }));
    expect(location()).toBe(path);
  });

  it('continues today’s journal entry', async () => {
    const user = userEvent.setup();
    renderHome({
      data: data({
        todayJournalEntry: { journalSpaceId: 'j1', entryId: 'e1', title: '', lastMessagePreview: `${'Tomatoes need staking. '.repeat(20)}`, messageCount: 1, updatedAt: NOW.toISOString() },
      }),
    });
    expect(screen.getByRole('button', { name: /^Write/ })).toHaveTextContent('Continue today’s entry');
    expect(screen.getByText('Untitled entry')).toBeInTheDocument();
    expect(screen.getByText('1 message')).toBeInTheDocument();
    expect(screen.getByText(/Tomatoes need staking/).textContent).toMatch(/…$/);
    await user.click(screen.getByRole('button', { name: /Continue writing/ }));
    expect(location()).toBe('/journals?journalSpaceId=j1&entryId=e1');
  });

  it('starts an entry in the preferred journal, or the journal list', async () => {
    const user = userEvent.setup();
    renderHome({ data: data({ preferredJournalSpaceId: 'garden' }) });
    await user.click(screen.getByRole('button', { name: 'Start today’s entry' }));
    expect(location()).toBe('/journals?journalSpaceId=garden');
  });

  it('opens the journal list when there is no journal yet', async () => {
    const user = userEvent.setup();
    renderHome();
    await user.click(screen.getByRole('button', { name: /^Write/ }));
    expect(location()).toBe('/journals');
  });

  it('resumes a recent conversation', async () => {
    const user = userEvent.setup();
    renderHome({
      data: data({
        recentConversations: [
          { id: 'c 1', title: 'Fermenting cabbage', lastMessagePreview: '  Salt ratio matters.  ', updatedAt: NOW.toISOString() },
          { id: 'c2', title: '', lastMessagePreview: null, updatedAt: NOW.toISOString() },
        ] as DashboardData['recentConversations'],
      }),
    });
    expect(screen.getByText('Salt ratio matters.')).toBeInTheDocument();
    expect(screen.getByText('Untitled conversation')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Fermenting cabbage/ }));
    expect(location()).toBe('/chat?conversationId=c+1');
  });

  it('lists saved references with where they came from', async () => {
    const user = userEvent.setup();
    renderHome({
      data: data({
        journalSpaceIds: ['journal-space'],
        recentReferences: [
          { id: 'r1', title: ' The Silk Road ', spaceId: 'journal-space', conversationTitle: 'Trade routes', createdAt: NOW.toISOString() },
          { id: 'r2', title: '', note: 'Check the dates', spaceId: 'chat-space', conversationTitle: '', createdAt: NOW.toISOString() },
          { id: 'r3', title: null, note: null, messagePreview: null, spaceId: null, conversationTitle: 'Spices', createdAt: NOW.toISOString() },
        ] as unknown as DashboardData['recentReferences'],
      }),
    });
    expect(screen.getByRole('button', { name: /The Silk Road/ })).toHaveTextContent('Journal · Trade routes');
    expect(screen.getByRole('button', { name: /Check the dates/ })).toHaveTextContent('Chat · Untitled');
    // A reference with no title, note or preview shows only where it came from.
    expect(screen.getAllByRole('button').some((button) => button.textContent?.startsWith('Chat · Spices'))).toBe(true);
    await user.click(screen.getByRole('button', { name: /The Silk Road/ }));
    expect(location()).toBe('/references?referenceId=r1');
  });

  it('offers last week’s synthesis once, until dismissed by using it', async () => {
    const user = userEvent.setup();
    queries.week.mockReturnValue({ data: { total: 3, conversations: 2, references: 1, notes: 0 } });
    renderHome();
    expect(screen.getByText('2 conversations · 1 saved passage')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Synthesize last week/ }));
    expect(location()).toBe('/journals?synthesize=week');
    expect(localStorage.getItem('home.weeklySynthesis.dismissedWeek')).toBe('Week of Oct 19');
  });

  it('hides the synthesis when this week was already done or nothing happened', () => {
    localStorage.setItem('home.weeklySynthesis.dismissedWeek', 'Week of Oct 19');
    queries.week.mockReturnValue({ data: { total: 3, conversations: 3, references: 0, notes: 0 } });
    renderHome();
    expect(screen.queryByText('Synthesize last week')).not.toBeInTheDocument();
  });

  it('opens a recently indexed web page in the reader', async () => {
    const user = userEvent.setup();
    renderHome({
      data: data({
        recentDocuments: [{ id: 'd1', documentId: 'd1', documentName: 'Bread history', documentPath: 'https://example.org/bread', lastAccessedAt: NOW.toISOString() }] as DashboardData['recentDocuments'],
      }),
    });
    await user.click(screen.getByRole('button', { name: /Bread history/ }));
    expect(screen.getByRole('dialog', { name: 'Reader' })).toHaveTextContent('https://example.org/bread');
    expect(openDocumentFile).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Close reader' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('opens a local document in the reader, at its own path, only when the app renders it', async () => {
    const user = userEvent.setup();
    vi.mocked(openDocumentFile)
      .mockResolvedValueOnce({ ok: true, data: { action: 'opened_external' } } as never)
      .mockResolvedValueOnce({ ok: true, data: { action: 'render_internal', contentPath: '/cache/notes.html' } } as never);
    renderHome({
      data: data({
        recentDocuments: [
          { id: 'd2', documentId: '', documentName: '', documentPath: '/library/notes.docx', lastAccessedAt: NOW.toISOString() },
          { id: '', documentId: '', documentName: '', documentPath: '', lastAccessedAt: NOW.toISOString() },
        ] as unknown as DashboardData['recentDocuments'],
      }),
    });
    await user.click(screen.getByRole('button', { name: /\/library\/notes\.docx/ }));
    expect(openDocumentFile).toHaveBeenCalledWith('d2', VaultAPI);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /\/library\/notes\.docx/ }));
    await waitFor(() => expect(screen.getByRole('dialog', { name: 'Reader' })).toHaveTextContent('/library/notes.docx'));
    expect(screen.getByRole('dialog', { name: 'Reader' })).not.toHaveTextContent('/cache/notes.html');

    // A row with no id at all has nothing to open.
    await user.click(screen.getByRole('button', { name: /^Untitled/ }));
    expect(openDocumentFile).toHaveBeenCalledTimes(2);
  });

  it('surfaces active Learning Studio courses with current module, lesson progress, and 1-click resume', async () => {
    const user = userEvent.setup();
    renderHome({
      data: data({
        courses: [
          {
            id: 'prog-bio',
            title: 'Cellular Energetics',
            goal: 'Master ATP synthesis and respiration',
            status: 'active',
            moduleCount: 3,
            lessonCount: 8,
            completedLessons: 5,
            currentLessonId: 'lesson-oxphos',
            currentLessonTitle: 'Oxidative Phosphorylation',
            currentModuleTitle: 'Module 2 · Mitochondria',
            currentLessonMinutes: 15,
            createdAt: 1700000000,
          },
          {
            id: 'prog-hist',
            title: 'Roman Infrastructure',
            goal: 'Aqueducts and road networks',
            status: 'active',
            moduleCount: 2,
            lessonCount: 4,
            completedLessons: 1,
            currentLessonId: 'lesson-viaducts',
            currentLessonTitle: 'Gravity-fed Aqueducts',
            currentModuleTitle: 'Module 1 · Water',
            currentLessonMinutes: 20,
            createdAt: 1699000000,
          },
        ],
      }),
    });

    expect(screen.getByText('Cellular Energetics')).toBeInTheDocument();
    expect(screen.getByText('Module 2 · Mitochondria')).toBeInTheDocument();
    expect(screen.getByText('Oxidative Phosphorylation')).toBeInTheDocument();
    expect(screen.getByText('5 of 8 lessons · 63%')).toBeInTheDocument();
    expect(screen.getByRole('progressbar', { name: 'Cellular Energetics progress' })).toHaveAttribute('aria-valuenow', '63');
    expect(screen.getByText('Roman Infrastructure')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /Cellular Energetics/ }));
    expect(location()).toBe('/studio?course=prog-bio&lesson=lesson-oxphos');
  });

  it('uses status-aware CTAs on secondary course cards and omits progress bar when lessonCount is 0', () => {
    renderHome({
      data: data({
        courses: [
          {
            id: 'prog-ready',
            title: 'Quantum Mechanics',
            goal: 'Wavefunctions',
            status: 'active',
            moduleCount: 2,
            lessonCount: 4,
            completedLessons: 0,
            currentLessonId: 'l1',
            currentLessonTitle: 'Wave-Particle Duality',
            currentModuleTitle: 'Module 1',
            currentLessonMinutes: 12,
            createdAt: 1700000300,
          },
          {
            id: 'prog-done',
            title: 'Linear Algebra',
            goal: 'Eigenvalues',
            status: 'active',
            moduleCount: 2,
            lessonCount: 4,
            completedLessons: 4,
            currentLessonId: 'l4',
            currentLessonTitle: 'SVD',
            currentModuleTitle: 'Module 2',
            currentLessonMinutes: 10,
            createdAt: 1700000200,
          },
          {
            id: 'prog-draft',
            title: 'Organic Synthesis',
            goal: 'Drafting outline',
            status: 'draft',
            moduleCount: 0,
            lessonCount: 0,
            completedLessons: 0,
            currentLessonId: null,
            currentLessonTitle: null,
            currentModuleTitle: null,
            currentLessonMinutes: null,
            createdAt: 1700000100,
          },
        ],
      }),
    });

    expect(screen.getByRole('button', { name: /Quantum Mechanics/ })).toHaveTextContent('Start lesson');
    expect(screen.getByRole('button', { name: /Linear Algebra/ })).toHaveTextContent('Review →');
    expect(screen.getByRole('button', { name: /Organic Synthesis/ })).toHaveTextContent('Outline →');
    expect(screen.queryByRole('progressbar', { name: 'Organic Synthesis progress' })).not.toBeInTheDocument();
  });

  it('offers a clean Studio invitation when no courses exist yet', async () => {
    const user = userEvent.setup();
    renderHome({ data: data({ courses: [] }) });
    await user.click(screen.getByRole('button', { name: /Start a course in Studio/ }));
    expect(location()).toBe('/studio?new=1');
  });

  it('surfaces active Spaces and jumps directly into a selected Space in Chat', async () => {
    const user = userEvent.setup();
    conversationUiStore.setState({ selectedSpaceId: null });
    renderHome({
      data: data({
        spaces: [
          { id: 'sp-bio', name: 'Cell Biology', description: null, icon: null, accentColor: '#0f766e' },
          { id: 'sp-arch', name: 'Architecture', description: null, icon: null, accentColor: '#b45309' },
        ],
      }),
    });

    await user.click(screen.getByRole('button', { name: 'Cell Biology' }));
    expect(conversationUiStore.getState().selectedSpaceId).toBe('sp-bio');
    expect(location()).toBe('/chat');
  });
});
