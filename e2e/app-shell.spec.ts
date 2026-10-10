import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { makePreviewPdf } from './fixtures/previewPdf';
import { installCustomCollectionsFixture } from './fixtures/customCollections';
import { installTauriMock, mockCommands, type CommandResult, type Fixture } from './fixtures/tauri';
import type { LearningLessonDto, LearningProgramDto, MessageDto, ModelCategoryDto } from '../src/lib/bindings';
import { openStudioSection } from './helpers/learningStudioNavigation';
import { makeAppSettings } from '../src/tests/fixtures/appSettings';

test('inline citations support keyboard passage navigation and clean reading', async ({ page }) => {
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.setViewportSize({ width: 1440, height: 900 });
  await mockCommands(page, (settings) => {
    localStorage.setItem('chat.sidebar.collapsed', '1');
    const stamp = '2026-10-09T00:00:00Z';
    const conversation = { id: 'citations-chat', title: 'Citation keyboard check', modelName: 'test', createdAt: stamp, updatedAt: stamp, messageCount: 1, totalTokens: 20, spaceId: 'space_general', isArchived: false };
    const text = 'Record the chosen measure. Keep the observation period.';
    const source = {
      documentId: 'web:https://example.org/field-guide', chunkId: 'field-guide-chunk',
      fileName: 'Field guide', filePath: 'https://example.org/field-guide', mimeType: 'text/html',
      category: 'Web Article', content: text, score: 1, fileSizeBytes: 0, modifiedAt: stamp, citationId: 1,
      webSnapshot: { url: 'https://example.org/field-guide', title: 'Field guide', text, fetchedAt: stamp, truncated: false },
    };
    const message = { id: 'citations-answer', conversationId: conversation.id, role: 'assistant', content: 'Record the chosen measure [1]. Keep the observation period [1].', tokens: 20, status: 'completed', createdAt: stamp, metadata: JSON.stringify({ sources: [source] }) };
    return {
      get_settings: () => settings,
      list_conversation_spaces: () => [{ id: 'space_general', name: 'General', isArchived: false }],
      list_conversations_explorer: () => ({ conversations: [conversation], total: 1 }),
      list_conversations: () => ({ conversations: [conversation], total: 1 }),
      get_conversation: () => ({ conversation }),
      get_conversation_messages: () => ({ messages: [message], total: 1 }),
      list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
      list_journals: () => [],
      list_conversation_tangents: () => [],
      list_conversation_linked_documents: () => [],
      list_conversation_web_sources: () => [],
      list_passage_references: () => [],
      list_downloaded_models: () => [],
      list_downloads: () => [],
      get_indexed_folders: () => [],
    };
  }, makeAppSettings());
  await page.goto('/chat?conversationId=citations-chat');
  const answer = page.locator('#message-citations-answer');
  const chips = answer.getByRole('button', { name: 'Citation 1', exact: true });
  await expect(chips).toHaveCount(2);
  await chips.first().focus();
  await chips.first().press('Enter');
  const reader = page.getByRole('complementary', { name: 'Source reader' });
  await expect(reader).toBeVisible();
  await expect(chips.first()).toHaveClass(/is-lit/);
  await page.keyboard.press('Escape');
  await expect(reader).toBeHidden();
  await expect(chips.first()).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(chips.nth(1)).toBeFocused();
  await page.keyboard.press('Space');
  await expect(reader).toBeVisible();
  await expect(chips.nth(1)).toHaveClass(/is-lit/);
  await expect(chips.first()).not.toHaveClass(/is-lit/);
  await page.getByRole('button', { name: 'Hide citations', exact: true }).click();
  await expect(reader).toBeHidden();
  await expect(chips).toHaveCount(0);
  await expect(answer.locator('.citation-hidden')).toHaveCount(2);
  await expect(answer).toContainText('Record the chosen measure');
  expect(errors).toEqual([]);
});

for (const [width, entry] of [[1440, 'menu'], [620, 'selection'], [1440, 'reply'], [620, 'reply']] as const) {
  test(`chat tangents via ${entry} preserve the parent, reopen after reload, and promote at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    const settings = makeAppSettings();
    settings.llm.provider = 'llamacpp';
    settings.llm.llamaCpp.model = 'qwen-test.gguf';
    await mockCommands(page, ({ settings, entry }) => {
      localStorage.setItem('chat.sidebar.collapsed', '1');
      const stamp = '2026-10-06T00:00:00Z';
      const parent = { id: 'tangent-parent', title: 'Main discussion', modelName: '__llamacpp_server__', createdAt: stamp, updatedAt: stamp, messageCount: 2, totalTokens: 50, spaceId: 'space_general', isArchived: false };
      const sourceMessages = [
        { id: 'source-question', conversationId: parent.id, role: 'user', content: 'What is a tangent?', tokens: 10, status: 'completed', createdAt: stamp },
        { id: 'source-answer', conversationId: parent.id, role: 'assistant', content: 'A tangent explores one idea without changing the main conversation.', tokens: 40, status: 'completed', createdAt: stamp },
      ];
      type Message = typeof sourceMessages[number];
      type Tangent = { conversationId: string; parentConversationId: string; sourceConversationId: string; sourceMessageId: string; selectedText: string; title: string; createdAt: string; updatedAt: string; contextMessageCount: number };
      type State = { tangents: Tangent[]; messages: Message[]; promoted: boolean; failPromotion: boolean };
      const saved = localStorage.getItem('test:tangents');
      const state: State = saved ? JSON.parse(saved) : { tangents: [], messages: [], promoted: false, failPromotion: true };
      let cancelPending: ((_reason: unknown) => void) | null = null;
      const persist = () => localStorage.setItem('test:tangents', JSON.stringify(state));
      const expectedPassage = entry === 'reply' ? sourceMessages[1].content : 'explores one idea';
      const conversation = () => ({ ...parent, id: 'tangent-1', title: expectedPassage, messageCount: state.messages.length, tangentParentId: state.promoted ? null : parent.id });
      return {
        get_settings: () => settings,
        list_conversation_spaces: () => [{ id: 'space_general', name: 'General', isArchived: false }],
        list_conversations_explorer: () => ({ conversations: state.promoted ? [conversation(), parent] : [parent], total: state.promoted ? 2 : 1 }),
        list_conversations: () => ({ conversations: state.promoted ? [conversation(), parent] : [parent], total: state.promoted ? 2 : 1 }),
        get_conversation: (args) => {
          const request = (args as { request?: { conversationId?: string; messageId?: string; selectedText?: string } })?.request;
          return { conversation: request?.conversationId === parent.id ? parent : conversation() };
        },
        get_conversation_messages: (args) => {
          const request = (args as { request?: { conversationId?: string; messageId?: string; selectedText?: string } })?.request;
          return { messages: request?.conversationId === parent.id ? sourceMessages : state.messages, total: request?.conversationId === parent.id ? 2 : state.messages.length };
        },
        list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
        list_conversation_tangents: () => state.tangents,
        create_conversation_tangent: (args) => {
          const request = (args as { request?: { conversationId?: string; messageId?: string; selectedText?: string } })?.request;
          if (request?.conversationId !== parent.id || request.messageId !== 'source-answer' || request.selectedText !== expectedPassage) throw new Error('Tangent lost the exact source selection');
          const tangent: Tangent = { conversationId: 'tangent-1', parentConversationId: parent.id, sourceConversationId: parent.id, sourceMessageId: request.messageId, selectedText: request.selectedText, title: request.selectedText, createdAt: stamp, updatedAt: stamp, contextMessageCount: 2 };
          state.tangents.push(tangent);
          state.messages = sourceMessages.map(message => ({ ...message, id: `copy-${message.id}`, conversationId: tangent.conversationId }));
          persist();
          return tangent;
        },
        chat_with_conversation: (args) => {
          const turn = args as { conversationId: string; message: string; cancelOnly?: boolean };
          if (turn.conversationId !== 'tangent-1') throw new Error('Tangent question was sent to the main conversation');
          if (turn.cancelOnly) {
            cancelPending?.({ code: 'INVALID_STATE', message: 'Generation cancelled' });
            cancelPending = null;
            return { conversationId: turn.conversationId, messages: state.messages, contextUsed: 2 };
          }
          if (turn.message === 'Cancel this tangent request') return new Promise((_resolve, reject) => { cancelPending = reject; });
          state.messages.push({ ...sourceMessages[0], id: 'tangent-question', conversationId: turn.conversationId, content: turn.message },
            { ...sourceMessages[1], id: 'tangent-answer', conversationId: turn.conversationId, content: 'You can explore that idea on its own.' });
          persist();
          return { conversationId: turn.conversationId, messages: state.messages, contextUsed: 4 };
        },
        promote_conversation_tangent: () => {
          if (state.failPromotion) { state.failPromotion = false; persist(); throw new Error('Promotion temporarily unavailable'); }
          state.promoted = true; state.tangents = []; persist(); return conversation();
        },
        list_journals: () => [],
        list_conversation_linked_documents: () => [],
        list_conversation_web_sources: () => [],
        list_downloaded_models: () => [],
        list_downloads: () => [],
        get_indexed_folders: () => [],
      };
    }, { settings, entry });
    const errors: Error[] = [];
    page.on('pageerror', error => errors.push(error));
    await page.goto('/chat?conversationId=tangent-parent');
    const composer = page.getByRole('textbox', { name: 'Message composer' });
    await composer.fill('Keep my main draft');
    const answer = page.locator('#message-source-answer .tiptap-viewer p');
    await expect(answer).toBeVisible();
    const replyAction = page.locator('#message-source-answer').getByRole('button', { name: 'Start a tangent from this reply' });
    await expect(replyAction).toBeVisible();
    if (entry === 'reply') {
      await replyAction.focus();
      await replyAction.press('Enter');
    } else {
      await answer.evaluate(element => {
        const text = element.firstChild!;
        const start = text.textContent!.indexOf('explores one idea');
        const range = document.createRange(); range.setStart(text, start); range.setEnd(text, start + 'explores one idea'.length);
        window.getSelection()?.removeAllRanges(); window.getSelection()?.addRange(range);
      });
      if (entry === 'menu') {
        await answer.dispatchEvent('contextmenu', { clientX: 300, clientY: 200 });
        await page.getByRole('menuitem', { name: 'Ask in a tangent' }).click();
      } else {
        const action = page.getByRole('toolbar', { name: 'Selected passage' }).getByRole('button', { name: 'Ask in a tangent' });
        await expect(action).toBeVisible();
        await page.screenshot({ path: '/tmp/lattice-tangent-selection.png', animations: 'disabled' });
        await action.click();
      }
    }
    const panel = page.getByRole('complementary', { name: 'Tangents', exact: true });
    const expectedPassage = entry === 'reply' ? 'A tangent explores one idea without changing the main conversation.' : 'explores one idea';
    await expect(panel.getByRole('blockquote')).toHaveText(expectedPassage);
    await expect(composer).toHaveValue('Keep my main draft');
    const tangentComposer = panel.getByRole('textbox', { name: 'Ask in this tangent' });
    await tangentComposer.fill('Cancel this tangent request');
    await tangentComposer.press('Enter');
    await panel.getByRole('button', { name: 'Stop tangent response' }).click();
    await expect(tangentComposer).toHaveValue('Cancel this tangent request');
    await expect(composer).toHaveValue('Keep my main draft');
    await tangentComposer.fill('What does that mean?');
    await tangentComposer.press('Enter');
    await expect(panel.getByText('You can explore that idea on its own.')).toBeVisible();
    await tangentComposer.fill('Keep this tangent draft');
    await panel.getByRole('button', { name: 'Close tangents' }).click();
    await expect(composer).toHaveValue('Keep my main draft');
    await page.getByRole('button', { name: 'Tangents · 1', exact: true }).click();
    await expect(tangentComposer).toHaveValue('Keep this tangent draft');
    await page.screenshot({ path: `/tmp/lattice-tangents-${width}-${entry}.png`, animations: 'disabled' });
    if (width === 1440) {
      await page.goto('/chat?conversationId=tangent-1&messageId=tangent-answer');
    } else {
      await page.goto('/chat?conversationId=tangent-parent');
      await page.getByRole('button', { name: 'Tangents · 1', exact: true }).click();
      await panel.getByRole('button', { name: `${expectedPassage} ${expectedPassage}` }).click();
    }
    await expect(panel.getByText('You can explore that idea on its own.')).toBeVisible();
    await panel.getByRole('button', { name: 'Make conversation' }).click();
    await expect(page.getByText("Couldn't make this a conversation")).toBeVisible();
    await expect(panel).toBeVisible();
    await panel.getByRole('button', { name: 'Make conversation' }).click();
    await expect(panel).not.toBeVisible();
    const stored = await page.evaluate(() => JSON.parse(localStorage.getItem('test:tangents')!));
    expect(stored.promoted).toBe(true);
    expect(stored.messages).toHaveLength(4);
    await page.getByRole('button', { name: 'Show sidebar' }).click();
    await expect(page.getByText(expectedPassage, { exact: true }).first()).toBeVisible();
    await expect(page.locator('#message-source-answer')).toBeVisible();
    expect(errors).toEqual([]);
  });
}

test.beforeEach(async ({ page }) => {
  await installTauriMock(page);
  await installCustomCollectionsFixture(page);
  // Renderer checks must not wait for external font servers when DNS is offline.
  await page.route('https://fonts.googleapis.com/**', route => route.fulfill({ contentType: 'text/css', body: '' }));
});

for (const width of [1440, 620]) {
  test(`conversation synthesis shows real progress and survives navigation at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    const settings = makeAppSettings();
    settings.llm.provider = 'auto';
    settings.llm.llamaCpp.model = 'qwen-test.gguf';
    await mockCommands(page, (settings, ipc) => {
      const stamp = new Date().toISOString();
      const conversation = { id: 'synthesis-chat', title: 'Forest research', modelName: 'qwen-test.gguf', createdAt: stamp, updatedAt: stamp, spaceId: 'space_general', messageCount: 2, totalTokens: 50 };
      const messages = [
        { id: 'question', conversationId: conversation.id, role: 'user', content: 'What did we learn about the forest?', tokens: 10, status: 'completed', createdAt: stamp },
        { id: 'answer', conversationId: conversation.id, role: 'assistant', content: 'The canopy provides shade and helps retain moisture.', tokens: 40, status: 'completed', createdAt: stamp },
      ];
      let note = { id: 'synthesis-note', title: 'Forest notes', journalId: 'research-journal', content: '', revision: 0, linkedDocumentIds: [], linkedConversationIds: [] as string[], highlights: [], stickyNotes: [], conversationSnapshots: [], sources: [], createdAt: stamp, updatedAt: stamp };
      let captureAttempts = 0;
      let synthesisCalls = 0;
      return {
        get_settings: () => settings,
        list_conversation_spaces: () => [{ id: 'space_general', name: 'General', isArchived: false }],
        list_conversations_explorer: () => ({ conversations: [conversation], total: 1 }),
        list_conversations: () => ({ conversations: [conversation], total: 1 }),
        list_journal_conversations: () => ({ conversations: [conversation], total: 1 }),
        get_conversation: () => ({ conversation }),
        get_conversation_messages: () => ({ messages, total: 2 }),
        list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
        list_journals: () => [{ id: 'research-journal', name: 'Research', isArchived: false, createdAt: stamp, updatedAt: stamp, sortOrder: 0 }],
        list_journal_entry_pins: () => [],
        list_workspace_notes: () => ({ notes: [note] }),
        update_workspace_note: (args) => {
          const next = (args as { note: typeof note }).note;
          if (!next || next.id !== note.id || next.revision !== note.revision) throw new Error('Conflicting page write');
          note = { ...next, revision: next.revision + 1 };
          document.documentElement.dataset.synthesisSavedPage = note.id;
          return note;
        },
        synthesize_journal_entries: async (args) => {
          synthesisCalls += 1;
          document.documentElement.dataset.synthesisCalls = String(synthesisCalls);
          const report = (stage: string, chunkIndex: number | null) => ipc.send(args.onProgress, { stage, entryCount: 1, chunkCount: 2, chunkIndex });
          report('gathering', null);
          window.addEventListener('test:synthesis-reading', () => report('reading', 1), { once: true });
          window.addEventListener('test:synthesis-second', () => report('reading', 2), { once: true });
          window.addEventListener('test:synthesis-writing', () => report('writing', null), { once: true });
          await new Promise<void>(resolve => window.addEventListener('test:synthesis-finish', () => resolve(), { once: true }));
          return { synthesis: synthesisCalls === 1 ? 'Canopy shade helps the forest retain moisture.' : 'The second synthesis adds a comparison of tree species.', entryCount: 1, chunkCount: 2, scope: 'conversation', conversationIds: [conversation.id], citations: [], sources: [] };
        },
        quick_capture: async (args) => {
          captureAttempts += 1;
          if (captureAttempts === 1) throw new Error('The journal could not be saved.');
          await new Promise<void>(resolve => window.addEventListener('test:synthesis-save', () => resolve(), { once: true }));
          const capture = args as { content: string; conversationIds: string[] };
          note = { ...note, content: capture.content, linkedConversationIds: capture.conversationIds, revision: note.revision + 1 };
          return { noteId: note.id, noteTitle: note.title, created: false };
        },
        list_conversation_tangents: () => [],
        list_conversation_linked_documents: () => [],
        list_conversation_web_sources: () => [],
        list_passage_references: () => [],
        list_downloaded_models: () => [],
        list_downloads: () => [],
        get_indexed_folders: () => [],
      };
    }, settings);
    const errors: Error[] = [];
    page.on('pageerror', error => errors.push(error));
    await page.goto('/chat?conversationId=synthesis-chat');
    const composer = page.getByRole('textbox', { name: 'Message composer' });
    await composer.fill('Keep my next question');
    const showSidebar = page.getByRole('button', { name: 'Show sidebar' });
    if (await showSidebar.isVisible()) await showSidebar.click();
    const row = page.getByRole('button', { name: 'Select conversation: Forest research', exact: true });
    await row.hover();
    await row.getByRole('button', { name: 'Actions for Forest research' }).click();
    await page.getByRole('button', { name: 'Synthesize to journal', exact: true }).click();
    const progress = page.getByRole('complementary', { name: 'Conversation synthesis' });
    await expect(progress.getByRole('heading', { name: 'Synthesizing to Journal' })).toBeVisible();
    await expect(row.getByText('Synthesizing to Journal…')).toBeVisible();
    await row.hover();
    await row.getByRole('button', { name: 'Actions for Forest research' }).click();
    await expect(page.getByRole('button', { name: 'Synthesize to journal', exact: true })).toBeDisabled();
    await page.keyboard.press('Escape');
    await page.evaluate(() => window.dispatchEvent(new Event('test:synthesis-reading')));
    await expect(progress.getByText('Reviewing part 1 of 2', { exact: true })).toBeVisible();
    await page.screenshot({ path: `/tmp/lattice-synthesis-${width}.png`, animations: 'disabled' });
    await progress.getByRole('button', { name: 'Keep working' }).click();
    await expect(composer).toHaveValue('Keep my next question');
    // Route changes keep the same running operation and status panel.
    await page.keyboard.press('ControlOrMeta+3');
    await expect(page).toHaveURL(/\/journals/);
    await progress.getByRole('button', { name: /Show synthesis progress/ }).click();
    await page.evaluate(() => window.dispatchEvent(new Event('test:synthesis-second')));
    await expect(progress.getByText('Reviewing part 2 of 2', { exact: true })).toBeVisible();
    await page.evaluate(() => window.dispatchEvent(new Event('test:synthesis-writing')));
    await expect(progress.getByText('Writing your synthesis', { exact: true })).toBeVisible();
    await page.evaluate(() => window.dispatchEvent(new Event('test:synthesis-finish')));
    await expect(progress.getByRole('heading', { name: 'Synthesis needs saving' })).toBeVisible();
    await progress.getByRole('button', { name: 'Retry saving' }).click();
    await expect(progress.getByText('Saving to Journal', { exact: true })).toBeVisible();
    await expect(progress.getByRole('button', { name: 'Open journal page' })).toHaveCount(0);
    expect(await page.locator('html').getAttribute('data-synthesis-calls')).toBe('1');
    await page.evaluate(() => window.dispatchEvent(new Event('test:synthesis-save')));
    await expect(progress.getByRole('heading', { name: 'Synthesis ready' })).toBeVisible();
    await progress.getByRole('button', { name: 'Open journal page' }).click();
    await expect(progress).toHaveCount(0);
    await expect(page.getByText('Canopy shade helps the forest retain moisture.', { exact: true })).toBeVisible();
    // Reopening the current workspace clears the selection from the URL.
    // It must restore the saved journal instead of showing an empty notebook.
    await page.getByRole('button', { name: 'Journal', exact: true }).click();
    await expect(page).toHaveURL(/journalSpaceId=research-journal/);
    await expect(page.getByRole('textbox', { name: 'Page title' })).toHaveValue('Forest notes');
    await expect(page.getByText('Canopy shade helps the forest retain moisture.', { exact: true })).toBeVisible();
    // Journal scopes use the same progress and save to their original page,
    // even after their editor unmounts.
    const showContext = page.getByRole('button', { name: 'Show conversation and highlights' });
    if (await showContext.isVisible()) await showContext.click();
    await page.getByRole('button', { name: 'Synthesize…', exact: true }).click();
    await page.getByRole('radio', { name: /Recent entries/ }).check();
    await page.getByRole('button', { name: 'Synthesize', exact: true }).click();
    await expect(progress.getByRole('heading', { name: 'Synthesizing to Journal' })).toBeVisible();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Synthesizing…', exact: true })).toBeVisible();
    await progress.getByRole('button', { name: 'Keep working' }).click();
    await page.keyboard.press('ControlOrMeta+4');
    await expect(page).toHaveURL(/\/chat/);
    await page.evaluate(() => {
      window.dispatchEvent(new Event('test:synthesis-writing'));
      window.dispatchEvent(new Event('test:synthesis-finish'));
    });
    await progress.getByRole('button', { name: 'Show synthesis progress: Synthesis ready' }).click();
    await expect(page).toHaveURL(/\/chat/);
    expect(await page.locator('html').getAttribute('data-synthesis-saved-page')).toBe('synthesis-note');
    await progress.getByRole('button', { name: 'Open journal page' }).click();
    await expect(page.getByText('The second synthesis adds a comparison of tree species.', { exact: true })).toBeVisible();
    await expect(page.getByText('Canopy shade helps the forest retain moisture.', { exact: true })).toBeVisible();
    expect(errors).toEqual([]);
  });
}

test('renders the application shell and navigates to settings', async ({ page }) => {
  const pageErrors: Error[] = [];
  page.on('pageerror', (error) => pageErrors.push(error));

  await page.goto('/');

  await expect(page.getByRole('navigation', { name: 'Main navigation' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Journal', exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
  expect(pageErrors).toEqual([]);
});

test('opens imported PDF bytes in the library and renders pages after reopening', async ({ page }) => {
  // A native smoke check can supply the exact bytes returned by read_file_bytes.
  const bytes = process.env.LATTICE_PREVIEW_PDF
    ? Array.from(await readFile(process.env.LATTICE_PREVIEW_PDF))
    : makePreviewPdf();
  const filePath = `/home/test/.lattice/files/${'a'.repeat(64)}/preview.pdf`;
  await mockCommands(page, ({ settings, bytes, filePath }) => {
    return {
      get_settings: () => settings,
      list_all_documents: () => [{
        id: 'imported-pdf', fileName: 'preview.pdf', filePath, fileType: 'pdf',
        category: 'Document', language: 'en', wordCount: 100,
        modifiedAt: '2026-09-15T00:00:00Z', indexedAt: '2026-09-15T00:00:00Z',
      }],
      read_file_bytes: (args) => {
        if ((args as { path: string }).path !== filePath) throw new Error('Unexpected PDF path');
        return bytes;
      },
      initialize_database: () => 'Database initialized',
      list_downloads: () => [],
      get_indexed_folders: () => [],
      list_conversation_spaces: () => [],
    };
  }, { settings: makeAppSettings(), bytes, filePath });
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/files');
  await page.getByRole('button', { name: 'Tree view', exact: true }).click();
  await expect(page.getByRole('button', { name: /^Imported files\s*1$/ })).toBeVisible();
  await expect(page.getByText('a'.repeat(64), { exact: true })).toHaveCount(0);
  const document = page.getByRole('button').filter({ has: page.getByText('preview.pdf', { exact: true }) });
  await document.press('Enter');
  const viewer = page.getByRole('dialog', { name: 'preview.pdf' });
  await expect(viewer.getByText(/Page 1 of (?:[2-9]|[1-9]\d+)/)).toBeVisible();
  const canvas = viewer.locator('canvas');
  // Wait for ink, not just an allocated blank canvas or a loaded page count.
  const expectInk = () => expect.poll(() => canvas.evaluate((element: HTMLCanvasElement) => {
    const pixels = element.getContext('2d')?.getImageData(0, 0, element.width, element.height).data;
    return pixels?.some((value, index) => index % 4 === 0 && value < 200 && pixels[index + 3] > 0);
  })).toBe(true);
  await expectInk();
  await viewer.getByRole('button', { name: 'Next page', exact: true }).click();
  await expect(viewer.getByText(/Page 2 of /)).toBeVisible();
  await expect(viewer.locator('.react-pdf__Page[data-page-number="2"] canvas')).toBeVisible();
  await expectInk();
  await viewer.getByRole('button', { name: 'Zoom in', exact: true }).click();
  await expect(viewer.getByText('120%', { exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(viewer).toHaveCount(0);
  await document.press('Enter');
  await expect(viewer.getByText(/Page 1 of (?:[2-9]|[1-9]\d+)/)).toBeVisible();
  await expect(canvas).toBeVisible();
  await expectInk();
  await expect(viewer.getByText('Failed to load PDF', { exact: true })).toHaveCount(0);
  await page.screenshot({ path: '/tmp/lattice-pdf-preview.png' });
  expect(errors).toEqual([]);
});

test.describe('Library collections', () => {
  test.beforeEach(async ({ page }) => {
    await mockCommands(page, settings => {
      const documents = ['Field guide.pdf', 'Interview notes.pdf', 'Planning.pdf'].map((fileName, index) => ({
        id: `collection-doc-${index}`, fileName,
        filePath: `/home/test/.lattice/files/${String(index).repeat(64)}/${fileName}`,
        fileType: 'pdf', category: 'Document', language: 'en', wordCount: 1200 + index * 200,
        modifiedAt: '2026-09-15T00:00:00Z', indexedAt: '2026-09-15T00:00:00Z',
      }));
      return {
        get_settings: () => settings,
        list_all_documents: () => documents,
        initialize_database: () => 'Database initialized',
        list_downloads: () => [],
        get_indexed_folders: () => [],
        list_conversation_spaces: () => [],
        list_document_space_memberships: () => [],
      };
    }, makeAppSettings());
    await page.goto('/files');
    await page.getByRole('button', { name: 'List view', exact: true }).click();
  });

  test('organizes selected documents, edits membership, and persists collections across reload', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.getByRole('checkbox', { name: 'Select Field guide.pdf', exact: true }).click();
    await page.getByRole('checkbox', { name: 'Select Interview notes.pdf', exact: true }).click();
    await page.screenshot({ path: '/tmp/lattice-library-selection.png' });
    await page.getByRole('button', { name: 'Add to collection', exact: true }).click();
    const add = page.getByRole('dialog', { name: 'Add to collection', exact: true });
    await add.getByLabel('New collection', { exact: true }).fill('Fieldwork');
    await add.getByRole('button', { name: 'Create and add' }).click();

    await page.getByRole('button', { name: /^Fieldwork\s*2$/ }).click();
    await expect(page.getByText('Planning.pdf', { exact: true })).toHaveCount(0);
    await page.getByRole('checkbox', { name: 'Select Interview notes.pdf', exact: true }).click();
    await page.getByRole('button', { name: 'Remove from collection', exact: true }).click();
    await expect(page.getByRole('button', { name: /^Fieldwork\s*1$/ })).toBeVisible();

    await page.getByRole('button', { name: 'Add documents', exact: true }).click();
    const picker = page.getByRole('dialog', { name: 'Add documents to Fieldwork' });
    await expect(picker.getByText('Field guide.pdf', { exact: true })).toHaveCount(0);
    await picker.getByRole('button', { name: 'Select shown' }).click();
    await picker.getByRole('button', { name: 'Add 2 documents' }).click();
    await expect(page.getByRole('button', { name: /^Fieldwork\s*3$/ })).toBeVisible();

    await page.getByRole('button', { name: 'Actions for Fieldwork', exact: true }).click();
    await page.getByRole('button', { name: 'Rename collection', exact: true }).click();
    const rename = page.getByRole('dialog', { name: 'Rename collection' });
    await rename.getByRole('textbox', { name: 'Collection name' }).fill('Field notes');
    await rename.getByRole('button', { name: 'Rename', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Field notes', exact: true })).toBeVisible();
    await page.reload();
    await page.getByRole('button', { name: 'List view', exact: true }).click();
    await page.getByRole('button', { name: /^Field notes\s*3$/ }).click();
    await expect(page.getByText('Interview notes.pdf', { exact: true })).toBeVisible();
    await page.screenshot({ path: '/tmp/lattice-library-collections.png' });

    await page.getByRole('button', { name: 'Actions for Planning.pdf', exact: true }).click();
    await page.getByRole('menuitem', { name: 'Add to collection…' }).click();
    await add.getByLabel('New collection', { exact: true }).fill('Reference');
    await add.getByRole('button', { name: 'Create and add' }).click();
    await expect(page.getByRole('button', { name: /^Reference\s*1$/ })).toBeVisible();

    await page.getByRole('button', { name: 'Actions for Field notes', exact: true }).click();
    await page.getByRole('button', { name: 'Delete collection', exact: true }).click();
    await page.getByRole('dialog', { name: 'Delete collection?' }).getByRole('button', { name: /Delete collection/ }).click();
    await expect(page.getByRole('button', { name: /^Field notes\s*3$/ })).toHaveCount(0);
    await expect(page.getByRole('heading', { name: 'All documents', exact: true })).toBeVisible();
    await expect(page.getByText('Field guide.pdf', { exact: true })).toBeVisible();
    await expect(page.getByText('Interview notes.pdf', { exact: true })).toBeVisible();
    await expect(page.getByText('Planning.pdf', { exact: true })).toBeVisible();
  });

  test('fills a new empty collection and adds to an existing collection from the grid', async ({ page }) => {
    await page.getByRole('button', { name: 'New collection', exact: true }).click();
    await page.getByRole('textbox', { name: 'New collection', exact: true }).fill('Reading');
    await page.getByRole('textbox', { name: 'New collection', exact: true }).press('Enter');
    const picker = page.getByRole('dialog', { name: 'Add documents to Reading' });
    await picker.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(page.getByText('This collection is empty.')).toBeVisible();
    await page.getByRole('button', { name: 'Choose documents', exact: true }).click();
    await picker.getByRole('textbox', { name: 'Find documents' }).fill('Field');
    await picker.getByRole('checkbox', { name: 'Select Field guide.pdf', exact: true }).check();
    await page.screenshot({ path: '/tmp/lattice-library-collection-picker.png' });
    await picker.getByRole('button', { name: 'Add 1 document', exact: true }).click();
    await expect(page.getByRole('button', { name: /^Reading\s*1$/ })).toBeVisible();

    await page.getByRole('button', { name: 'All documents', exact: true }).click();
    await page.getByRole('button', { name: 'Grid view', exact: true }).click();
    await page.getByRole('button', { name: 'Actions for Planning.pdf', exact: true }).press('Enter');
    await page.getByRole('menuitem', { name: 'Add to collection…' }).click();
    const add = page.getByRole('dialog', { name: 'Add to collection', exact: true });
    await add.getByRole('button', { name: 'Reading', exact: true }).click();
    await expect(page.getByRole('button', { name: /^Reading\s*2$/ })).toBeVisible();
  });
});

for (const [cached, system, expected] of [
  ['dark', 'light', 'dark'],
  ['light', 'dark', 'light'],
  ['system', 'dark', 'dark'],
  ['invalid', 'light', 'light'],
] as const) {
  test(`applies startup theme ${cached}/${system} before React loads`, async ({ page }) => {
    await page.addInitScript(value => localStorage.setItem('lattice-theme', value), cached);
    await page.emulateMedia({ colorScheme: system });
    // Hold back the production bundle: the blocking bootstrap must work alone.
    await page.route('**/assets/*.js', route => route.abort());
    await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-theme', expected);
    await expect(page.locator('html')).toHaveCSS('color-scheme', expected);
    await expect(page.locator('#root')).toBeEmpty();
  });
}

test('production utilities preserve theme tokens in light and dark mode', async ({ page }) => {
  await page.goto('/settings');
  for (const theme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    const styles = await page.evaluate(() => {
      const actual = document.createElement('div');
      actual.className = 'bg-surface text-text-primary border border-border rounded-sm shadow-sm font-mono text-ui duration-fast';
      const reference = document.createElement('div');
      reference.style.cssText = 'background-color:hsl(var(--surface));color:hsl(var(--text-primary));border:1px solid hsl(var(--border-subtle));border-radius:var(--radius-sm);box-shadow:var(--shadow-sm);font-family:var(--font-mono);font-size:0.8125rem;transition-duration:var(--duration-fast)';
      document.body.append(actual, reference);
      const read = (element: HTMLElement) => {
        const css = getComputedStyle(element);
        return {
          background: css.backgroundColor, color: css.color, border: css.borderColor,
          borderWidth: css.borderWidth, radius: css.borderRadius, shadow: css.boxShadow,
          font: css.fontFamily, size: css.fontSize, duration: css.transitionDuration,
        };
      };
      const result = { actual: read(actual), reference: read(reference) };
      actual.remove(); reference.remove();
      return result;
    });
    const { shadow: actualShadow, ...actual } = styles.actual;
    const { shadow: expectedShadow, ...reference } = styles.reference;
    expect(actual, theme).toEqual(reference);
    expect(styles.actual.radius).toBe('5px');
    // Tailwind composes transparent ring placeholders ahead of the theme shadow.
    expect(actualShadow).toContain(expectedShadow);
    expect(expectedShadow).not.toBe('none');
    expect(styles.actual.font).toContain('JetBrains Mono');
  }
});

test('theme text and destructive actions keep readable contrast', async ({ page }) => {
  await page.goto('/settings');
  for (const theme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    const ratios = await page.evaluate(() => {
      const sample = document.createElement('span');
      document.body.append(sample);
      const color = (token: string) => {
        sample.style.color = `hsl(var(--${token}))`;
        return getComputedStyle(sample).color.match(/[\d.]+/g)!.slice(0, 3).map(Number);
      };
      const luminance = (rgb: number[]) => rgb.reduce((sum, channel, index) => {
        const value = channel / 255;
        return sum + [0.2126, 0.7152, 0.0722][index] * (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
      }, 0);
      const contrast = (fg: number[], bg: number[]) => {
        const [low, high] = [luminance(fg), luminance(bg)].sort((a, b) => a - b);
        return (high + 0.05) / (low + 0.05);
      };
      const results: Record<string, number> = {};
      for (const surface of ['chrome', 'bg', 'surface', 'surface-raised', 'surface-overlay', 'surface-sunken']) {
        results[`muted/${surface}`] = contrast(color('text-muted'), color(surface));
        results[`destructive-hover/${surface}`] = contrast(
          color('accent-fg').map((value, index) => value * 0.9 + color(surface)[index] * 0.1),
          color('danger').map((value, index) => value * 0.9 + color(surface)[index] * 0.1),
        );
      }
      results.destructive = contrast(color('accent-fg'), color('danger'));
      results['destructive-brightness-hover'] = contrast(
        color('accent-fg').map(value => Math.min(255, value * 1.1)),
        color('danger').map(value => Math.min(255, value * 1.1)),
      );
      sample.remove();
      return results;
    });
    for (const [pair, ratio] of Object.entries(ratios)) {
      expect(ratio, `${theme} ${pair}`).toBeGreaterThanOrEqual(4.5);
    }
  }
});

test('HTML and code previews follow light and dark themes', async ({ page }, testInfo) => {
  const fileRoot = `/home/test/.lattice/files/${'b'.repeat(64)}`;
  await mockCommands(page, ({ settings, fileRoot }) => {
    return {
      get_settings: () => settings,
      list_all_documents: () => ['article.html', 'example.ts'].map((fileName, index) => ({
        id: `preview-${index}`, fileName, filePath: `${fileRoot}/${fileName}`, fileType: index ? 'ts' : 'html',
        category: 'Document', language: 'en', wordCount: 100,
        modifiedAt: '2026-09-15T00:00:00Z', indexedAt: '2026-09-15T00:00:00Z',
      })),
      read_file_content: (args) => (args as { path: string }).path.endsWith('.ts')
        ? 'const answer = 42;'
        : '<style>body { color: black; background: white; }</style><h1>Theme preview</h1><p style="color: black; background: white">Readable article</p><pre>const answer = 42;</pre>',
      initialize_database: () => 'Database initialized',
      list_downloads: () => [],
      get_indexed_folders: () => [],
      list_conversation_spaces: () => [],
    };
  }, { settings: makeAppSettings(), fileRoot });
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/files');
  await page.getByRole('button', { name: 'Tree view', exact: true }).click();
  await page.getByRole('button').filter({ has: page.getByText('article.html', { exact: true }) }).press('Enter');
  const article = page.frameLocator('iframe[title="article.html"]');
  await expect(article.getByText('Readable article')).toBeVisible();
  await expect(article.locator('html')).toHaveCSS('color-scheme', 'light');
  const lightText = await article.locator('p').evaluate(element => getComputedStyle(element).color);
  await page.screenshot({ path: testInfo.outputPath('article-light.png') });
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(article.locator('html')).toHaveCSS('color-scheme', 'dark');
  await expect(article.locator('p')).not.toHaveCSS('color', lightText);
  await page.screenshot({ path: testInfo.outputPath('article-dark.png') });
  await page.keyboard.press('Escape');
  await page.getByRole('button').filter({ has: page.getByText('example.ts', { exact: true }) }).press('Enter');
  const code = page.getByRole('dialog', { name: 'example.ts' }).locator('pre');
  await expect(code).toBeVisible();
  const darkCodeBackground = await code.evaluate(element => getComputedStyle(element).backgroundColor);
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(code).not.toHaveCSS('background-color', darkCodeBackground);
});

// These tests exercise the real renderer/settings controls against a simulated
// persistent IPC repository. They do not claim native desktop persistence.
test('switches themes, remembers the choice on reload, and follows system changes', async ({ page }) => {
  await mockCommands(page, (defaults) => {
    return {
      get_settings: () => {
        const saved = JSON.parse(localStorage.getItem('test:settings') ?? JSON.stringify(defaults));
        return saved;
      },
      update_settings: (args) => {
        const saved = JSON.parse(localStorage.getItem('test:settings') ?? JSON.stringify(defaults));
        if (localStorage.getItem('test:fail-save')) throw new Error('Disk is full');
        const { category, updates } = (args as { settings: { category: string | null; updates: Record<string, unknown> } }).settings;
        if (category) Object.assign(saved[category], updates);
        else Object.assign(saved, updates);
        localStorage.setItem('test:settings', JSON.stringify(saved));
        return saved;
      },
    };
  }, makeAppSettings());
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Display', exact: true }).click();
  const dark = page.getByRole('radio', { name: 'Dark', exact: true });
  await dark.click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  expect(await page.evaluate(() => localStorage.getItem('lattice-theme'))).toBe('dark');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('button', { name: 'Display', exact: true }).click();
  await page.getByRole('radio', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.evaluate(() => localStorage.setItem('test:fail-save', 'true'));
  await dark.click();
  await expect(page.getByRole('alert')).toContainText("Couldn't save your theme");
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  expect(await page.evaluate(() => localStorage.getItem('lattice-theme'))).toBe('light');
  await page.evaluate(() => localStorage.removeItem('test:fail-save'));
  await page.getByRole('radio', { name: 'System', exact: true }).click();
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.screenshot({ path: '/tmp/lattice-product-audit/display-light.png' });
  await page.getByRole('radio', { name: 'System', exact: true }).press('ArrowLeft');
  await expect(dark).toHaveAttribute('aria-checked', 'true');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.screenshot({ path: '/tmp/lattice-product-audit/display-dark.png' });
  await page.setViewportSize({ width: 800, height: 600 });
  await expect(dark).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(800);
  await page.screenshot({ path: '/tmp/lattice-product-audit/display-compact.png' });
});

// Simulated native event transport: verifies the renderer becomes inert before exit.
test('acknowledges native quit and prevents further editing while closing', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('navigation', { name: 'Main navigation' })).toBeVisible();
  await page.evaluate(() => window.__LATTICE_IPC__.emit('lattice:shutdown-requested', 42));
  await expect(page.getByRole('status')).toContainText('Saving your work before quitting…');
  await expect.poll(() => page.evaluate(() => window.__LATTICE_IPC__.emitted.filter(({ event }) => event === 'lattice:shutdown-response').map(({ payload }) => payload)))
    .toEqual([{ requestId: 42, saved: true }]);
  await expect(page.locator('[inert]')).toHaveCount(1);
  await page.screenshot({ path: '/tmp/lattice-acceptance/shutdown.png' });
});

// Exercise the decomposed sidebar as one renderer flow, backed by a simulated
// repository. This catches broken prop/hook wiring that isolated hooks miss.
test('conversation outline navigates long virtualized replies and tracks the reading position', async ({ page }) => {
  await mockCommands(page, (settings) => {
    const stamp = '2026-10-03T10:00:00Z';
    const messages = Array.from({ length: 60 }, (_, index) => ({
      id: `outline-${index}`, conversationId: 'outline-chat', role: index % 2 === 0 ? 'user' : 'assistant',
      content: index % 2 === 0 ? `Question ${index / 2 + 1}: Explain this part of the project.`
        : `Answer ${Math.ceil(index / 2)}.\n\n${'This paragraph explains the design in detail, including the choices we made and how they affect the project. '.repeat(12)}\n\n`.repeat(9),
      tokens: 100, status: 'completed', createdAt: stamp,
    }));
    const conversation = { id: 'outline-chat', title: 'Long conversation', modelName: 'test-model', createdAt: stamp, updatedAt: stamp, messageCount: messages.length, totalTokens: 6000, spaceId: 'space_general', isArchived: false };
    return {
      get_settings: () => settings,
      list_conversation_spaces: () => [{ id: 'space_general', name: 'General', isArchived: false }],
      list_conversations_explorer: () => ({ conversations: [conversation], total: 1 }),
      list_conversations: () => ({ conversations: [conversation], total: 1 }),
      get_conversation: () => ({ conversation }),
      get_conversation_messages: () => ({ messages, total: messages.length }),
      list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
      list_journals: () => [],
      list_conversation_tangents: () => [],
      list_conversation_linked_documents: () => [],
      list_conversation_web_sources: () => [],
      list_downloaded_models: () => [],
      list_downloads: () => [],
      get_indexed_folders: () => [],
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/chat?conversationId=outline-chat');
  const navigation = page.getByRole('navigation', { name: 'Conversation navigation' });
  await expect(navigation).toBeVisible();
  await expect(navigation.getByTitle('Message 60 of 60 · Assistant')).toBeVisible();
  await navigation.getByRole('button', { name: 'Expand conversation outline' }).click();
  const checkpoints = navigation.getByLabel('Message checkpoints');
  await checkpoints.evaluate((element) => { element.scrollTop = 11 * 60; });
  await navigation.getByRole('button', { name: /^Message 12, Assistant:/ }).click();
  await expect(navigation.getByRole('button', { name: /^Message 12, Assistant:/ })).toHaveAttribute('aria-current', 'location');

  // A jump to a very long answer must show its beginning, not its middle.
  const answer = page.locator('#message-outline-11');
  await expect.poll(() => answer.evaluate(async (element) => {
    const scroller = element.closest('.overflow-y-auto')!;
    const geometry = () => [
      element.getBoundingClientRect().top - scroller.getBoundingClientRect().top,
      scroller.scrollTop,
      scroller.scrollHeight,
    ];
    const initial = geometry();
    // Selection can update before the virtualizer finishes measuring rows and
    // checking the jump over two animation frames. Require stable geometry
    // across that cycle before starting a separate reading-position scroll.
    for (let frame = 0; frame < 3; frame++) {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      if (geometry().some((value, index) => Math.abs(value - initial[index]) > 1)) return false;
    }
    return Math.abs(initial[0]) < 50;
  })).toBe(true);
  const scroller = page.locator('.chat-panel .overflow-y-auto').first();
  const readingOffset = await scroller.evaluate((element) => {
    element.scrollTop += 1000;
    return element.scrollTop;
  });
  await expect.poll(() => answer.evaluate((element) => {
    return element.getBoundingClientRect().top - element.closest('.overflow-y-auto')!.getBoundingClientRect().top;
  })).toBeLessThan(-900);
  await expect.poll(() => scroller.evaluate((element) => element.scrollTop)).toBeCloseTo(readingOffset, 0);
  await expect(navigation.getByRole('button', { name: /^Message 12, Assistant:/ })).toHaveAttribute('aria-current', 'location');
  await answer.evaluate((element) => {
    const scroll = element.closest('.overflow-y-auto')!;
    const row = element.closest('.chat-message-row')!;
    scroll.scrollTop += row.getBoundingClientRect().bottom - scroll.getBoundingClientRect().top + 1;
  });
  await expect(navigation.getByRole('button', { name: /^Message 13, You:/ })).toHaveAttribute('aria-current', 'location');
  await navigation.getByRole('button', { name: 'Previous message' }).click();
  await expect(navigation.getByRole('button', { name: /^Message 12, Assistant:/ })).toHaveAttribute('aria-current', 'location');
  await page.screenshot({ path: 'e2e-results/artifacts/conversation-outline-desktop.png' });

  const beforeCollapse = await scroller.evaluate((element) => element.scrollTop);
  await navigation.getByRole('button', { name: 'Collapse conversation outline' }).click();
  expect(await scroller.evaluate((element) => element.scrollTop)).toBeCloseTo(beforeCollapse, 0);
  await navigation.getByRole('button', { name: 'Jump to latest message' }).click();
  await expect(page.locator('#message-outline-59')).toBeVisible();
  await expect.poll(() => scroller.evaluate((element) => element.scrollHeight - element.scrollTop - element.clientHeight)).toBeLessThan(100);
  expect(await page.locator('.chat-message-row').count()).toBeLessThan(20);

  // The outline stays usable in the narrower chat panes used by Explorer.
  await page.setViewportSize({ width: 760, height: 720 });
  await navigation.getByRole('button', { name: 'Expand conversation outline' }).click();
  await expect(checkpoints).toBeVisible();
  await page.screenshot({ path: 'e2e-results/artifacts/conversation-outline-narrow.png' });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.reload();
  await page.getByRole('button', { name: 'Select conversation: Long conversation', exact: true }).click();
  await expect(navigation.getByRole('button', { name: 'Collapse conversation outline' })).toBeVisible();
  expect(errors).toEqual([]);
});

test('chat sidebar renames a conversation and opens the spaces editor', async ({ page }) => {
  await mockCommands(page, (settings) => {
    const stamp = new Date().toISOString();
    const conversation = { id: 'conversation-1', title: 'Reading notes', modelName: 'test-model', systemPrompt: null, createdAt: stamp, updatedAt: stamp, messageCount: 0, totalTokens: 0, spaceId: 'space_general', isSaved: false, isBookmarked: false, isPinned: false, isArchived: false };
    const space = { id: 'space_general', name: 'General', description: null, icon: null, accentColor: null, spacePrompt: null, defaultModelName: null, toolPreferencesJson: null, isArchived: false, sortOrder: 0, createdAt: stamp, updatedAt: stamp };
    return {
      get_settings: () => settings,
      list_conversation_spaces: () => [space],
      list_journals: () => [],
      list_conversations_explorer: () => ({ conversations: [conversation], total: 1 }),
      list_conversations: () => ({ conversations: [conversation], total: 1 }),
      get_conversation: () => ({ conversation }),
      get_conversation_messages: () => ({ messages: [], total: 0 }),
      list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
      rename_conversation: (args) => {
        conversation.title = (args as { request: { newTitle: string } }).request.newTitle;
        return { status: 'success' };
      },
      list_conversation_linked_documents: () => [],
      list_conversation_web_sources: () => [],
      list_downloaded_models: () => [],
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/chat');
  const list = page.getByRole('navigation', { name: 'Conversations', exact: true });
  await expect(list.getByText('Reading notes', { exact: true })).toBeVisible();
  await list.getByText('Reading notes', { exact: true }).dblclick();
  const title = page.getByRole('textbox', { name: 'Rename conversation: Reading notes' });
  await title.fill('Updated reading notes');
  await title.press('Enter');
  await expect(list.getByText('Updated reading notes', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Change scope', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Spaces', exact: true })).toBeVisible();
  const panel = page.locator('aside').filter({ has: page.getByRole('heading', { name: 'Spaces', exact: true }) });
  await expect(panel).toHaveCSS('opacity', '1');
  await page.screenshot({ path: '/tmp/lattice-architecture-fix/chat-spaces.png' });
  await panel.getByRole('button', { name: 'New space', exact: true }).click();
  await panel.getByPlaceholder('Space name (e.g. Product, Research, Personal)').fill('Unsaved space draft');
  await page.getByRole('button', { name: 'Close spaces panel', exact: true }).last().click();
  await page.getByRole('button', { name: 'Change scope', exact: true }).click();
  await expect(page.getByPlaceholder('Space name (e.g. Product, Research, Personal)')).toHaveValue('Unsaved space draft');
  await page.getByRole('button', { name: 'Close spaces panel', exact: true }).last().click();
  await expect(page.getByRole('heading', { name: 'Spaces', exact: true })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('creates a space in Settings and opens it in Chat', async ({ page }) => {
  await mockCommands(page, (settings) => {
    const stamp = new Date().toISOString();
    const general = { id: 'space_general', name: 'General', description: null, icon: null, accentColor: null, spacePrompt: null, defaultModelName: null, toolPreferencesJson: null, isArchived: false, sortOrder: 0, createdAt: stamp, updatedAt: stamp };
    let spaces = [general];
    return {
      get_settings: () => settings,
      list_conversation_spaces: () => spaces,
      create_conversation_space: (args) => {
        if (document.documentElement.dataset.spaceCreationAllowed !== 'true') {
          throw new Error('Disk full');
        }
        const request = (args as { request: { name: string } }).request;
        const space = { ...general, ...request, id: 'space_research', sortOrder: 1 };
        spaces = [...spaces, space];
        return space;
      },
      list_conversations_explorer: () => ({ conversations: [], total: 0 }),
      list_conversations: () => ({ conversations: [], total: 0 }),
      list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
      list_journals: () => [],
      list_downloaded_models: () => [],
      list_downloads: () => [],
      get_indexed_folders: () => [],
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));

  await page.goto('/settings');
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Spaces', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Spaces', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Open General in Chat', exact: true })).toBeVisible();
  const name = page.getByRole('textbox', { name: 'Space name', exact: true });
  await name.fill('Research');
  await name.press('Enter');
  await expect(page.getByText('Could not update space', { exact: true })).toBeVisible();
  await expect(name).toHaveValue('Research');
  await expect(page.getByRole('button', { name: 'Open Research in Chat', exact: true })).toHaveCount(0);

  await page.evaluate(() => { document.documentElement.dataset.spaceCreationAllowed = 'true'; });
  await page.getByRole('button', { name: 'Create space', exact: true }).click();
  await expect(page.getByText('Space created', { exact: true })).toBeVisible();
  await expect(name).toHaveValue('');
  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByRole('button', { name: 'Open Research in Chat', exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.screenshot({ path: '/tmp/lattice-settings-spaces-light.png', animations: 'disabled' });
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.screenshot({ path: '/tmp/lattice-settings-spaces-dark.png', animations: 'disabled' });
  await page.getByRole('button', { name: 'Open Research in Chat', exact: true }).click();
  await expect(page).toHaveURL(/\/chat$/);
  await expect(page.getByRole('button', { name: 'Change scope', exact: true })).toHaveText('Research');
  await page.getByRole('button', { name: 'Change scope', exact: true }).click();
  const panel = page.locator('aside').filter({ has: page.getByRole('heading', { name: 'Spaces', exact: true }) });
  await expect(panel.getByText('Research', { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test('Downloaded switches chat between llama.cpp and local while keeping the utility assignment', async ({ page }, testInfo) => {
  await mockCommands(page, defaults => {
    let settings = JSON.parse(localStorage.getItem('test:settings') ?? JSON.stringify(defaults));
    settings.llm.provider = 'llamacpp';
    settings.llm.llamaCpp = {
      url: 'https://llama.example.com', model: 'remote-model.gguf',
      authHeaderName: '', authHeaderValue: '',
    };
    const model = {
      id: 'local-model', model_id: 'local-model', model_name: 'Local model',
      file_path: '/models/local.gguf', file_size_bytes: 1000000000,
      model_type: 'chat', backend: 'local', downloaded_at: '2026-10-01T00:00:00Z',
      last_used_at: null, use_count: 0, is_active_for_chat: true,
      is_active_for_embedding: false, is_active_for_utility: true,
    };
    return {
      get_settings: () => settings,
      update_settings: (args) => {
        const payload = args as { settings: { updates: Record<string, unknown> } };
        settings = { ...settings, llm: { ...settings.llm, ...payload.settings.updates } };
        localStorage.setItem('test:settings', JSON.stringify(settings));
        return settings;
      },
      list_downloaded_models: () => [model],
      set_active_chat_model: (args) => {
        if ((args as { modelId: string }).modelId !== model.model_id) throw new Error('Unexpected model selection');
        return null;
      },
      get_active_chat_model: () => null,
      get_active_embedding_model: () => null,
      get_active_models: () => ({ chat_model: null, embedding_model: null }),
      list_journals: () => [],
      list_downloads: () => [],
      get_indexed_folders: () => [],
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/settings');
  const navigation = page.getByRole('navigation', { name: 'Settings sections' });
  await navigation.getByRole('button', { name: 'Downloaded', exact: true }).click();
  const local = page.getByText('Local model', { exact: true }).locator('../..');
  const remote = page.getByText('llama.cpp connection', { exact: true }).locator('../..');
  await expect(remote.getByRole('button', { name: 'Chat', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(local.getByRole('button', { name: 'Chat', exact: true })).toHaveAttribute('aria-pressed', 'false');
  await expect(local.getByRole('button', { name: 'Utility', exact: true })).toHaveAttribute('aria-pressed', 'true');

  await local.getByRole('button', { name: 'Chat', exact: true }).click();
  await expect(local.getByRole('button', { name: 'Chat', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(remote.getByRole('button', { name: 'Chat', exact: true })).toHaveAttribute('aria-pressed', 'false');
  await remote.getByRole('button', { name: 'Edit connection' }).click();
  await expect(page.getByLabel('Chat provider')).toHaveValue('local');
  await expect(page.getByLabel('llama.cpp URL')).toHaveValue('https://llama.example.com');
  await page.getByRole('button', { name: 'Save connection' }).click();
  await expect(page.getByText('llama.cpp settings saved')).toBeVisible();
  await expect(page.getByLabel('Chat provider')).toHaveValue('local');

  await navigation.getByRole('button', { name: 'Downloaded', exact: true }).click();
  await remote.getByRole('button', { name: 'Chat', exact: true }).click();
  await expect(remote.getByRole('button', { name: 'Chat', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(local.getByRole('button', { name: 'Chat', exact: true })).toHaveAttribute('aria-pressed', 'false');
  await expect(local.getByRole('button', { name: 'Utility', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem('test:settings')!).llm.provider)).toBe('llamacpp');
  await page.screenshot({ path: testInfo.outputPath('downloaded-models-chat.png'), animations: 'disabled' });
  expect(errors).toEqual([]);
});

test('Downloaded only lists configured connections and remembers a saved localhost server', async ({ page }, testInfo) => {
  await mockCommands(page, defaults => {
    defaults.llm.model = 'llama3.2:latest';
    return {
      get_settings: () => {
        const settings = JSON.parse(localStorage.getItem('test:settings') ?? JSON.stringify(defaults));
        return settings;
      },
      update_settings: (args) => {
        const settings = JSON.parse(localStorage.getItem('test:settings') ?? JSON.stringify(defaults));
        const payload = args as { settings: { updates: Record<string, unknown> } };
        Object.assign(settings.llm, payload.settings.updates);
        localStorage.setItem('test:settings', JSON.stringify(settings));
        return settings;
      },
      test_ollama_connection: () => ({ endpoint: '/api/tags', models: ['llama3.2:latest'] }),
      list_downloaded_models: () => [{
        id: 'ollama', model_id: '__ollama_server__', model_name: 'Ollama',
        file_path: '', file_size_bytes: 0, model_type: 'chat', backend: 'ollama',
        downloaded_at: '2026-10-01T00:00:00Z', last_used_at: null, use_count: 0,
        is_active_for_chat: true, is_active_for_embedding: false, is_active_for_utility: true,
      }],
      get_active_chat_model: () => null,
      get_active_embedding_model: () => null,
      get_active_models: () => ({ chat_model: null, embedding_model: null }),
      list_journals: () => [],
      list_downloads: () => [],
      get_indexed_folders: () => [],
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/settings');
  const navigation = page.getByRole('navigation', { name: 'Settings sections' });
  await navigation.getByRole('button', { name: 'Downloaded', exact: true }).click();
  await expect(page.getByText('No models downloaded.')).toBeVisible();
  await expect(page.getByText('Ollama connection', { exact: true })).toHaveCount(0);
  await expect(page.getByText('llama.cpp connection', { exact: true })).toHaveCount(0);

  await navigation.getByRole('button', { name: 'Chat', exact: true }).click();
  await page.getByRole('button', { name: 'Test connection', exact: true }).click();
  await expect(page.getByLabel('Model', { exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Save Ollama connection' }).click();
  await expect(page.getByText('Ollama settings saved')).toBeVisible();
  await expect(page.getByLabel('Chat provider')).toHaveValue('auto');
  await page.getByLabel('Chat provider').selectOption('local');
  await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem('test:settings')!).llm.provider)).toBe('local');

  // Reopen the settings surface from persisted settings, with a different
  // provider selected and the exact same URL/model as the untouched defaults.
  await page.reload();
  await navigation.getByRole('button', { name: 'Downloaded', exact: true }).click();
  const remote = page.getByText('Ollama connection', { exact: true }).locator('../..');
  await expect(remote).toBeVisible();
  await expect(remote.getByRole('button', { name: 'Chat', exact: true })).toHaveAttribute('aria-pressed', 'false');
  await expect(page.getByText('llama.cpp connection', { exact: true })).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath('configured-connections.png'), animations: 'disabled' });
  expect(errors).toEqual([]);
});

test('keeps Ollama and llama.cpp connections separate across provider changes', async ({ page }) => {
  await mockCommands(page, (defaults) => {
    return {
      get_settings: () => {
        const settings = JSON.parse(localStorage.getItem('test:settings') ?? JSON.stringify(defaults));
        return settings;
      },
      update_settings: (args) => {
        const settings = JSON.parse(localStorage.getItem('test:settings') ?? JSON.stringify(defaults));
        const payload = args as { settings: { updates: Record<string, unknown> } };
        Object.assign(settings.llm, payload.settings.updates);
        localStorage.setItem('test:settings', JSON.stringify(settings));
        return settings;
      },
      test_llama_cpp_connection: () => ({ endpoint: '/v1/chat/completions', models: ['test-model.gguf'] }),
      list_downloaded_models: () => [],
      get_active_chat_model: () => null,
      get_active_embedding_model: () => null,
      get_active_models: () => ({ chat_model: null, embedding_model: null }),
    };
  }, makeAppSettings());
  await page.goto('/settings');
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Chat', exact: true }).click();
  await page.getByLabel('Chat provider').selectOption('llamacpp');
  await expect(page.getByRole('heading', { name: 'llama.cpp server' })).toBeVisible();
  await page.getByLabel('llama.cpp URL').fill('https://llama.example.com');
  await page.getByRole('button', { name: 'Test llama.cpp connection', exact: true }).click();
  await expect(page.getByLabel('llama.cpp model', { exact: true })).toHaveValue('test-model.gguf');
  await page.getByRole('button', { name: 'Save connection' }).click();
  await page.getByLabel('Chat provider').selectOption('ollama');
  await expect(page.getByRole('heading', { name: 'Ollama server' })).toBeVisible();
  await expect(page.getByLabel('Server URL')).toHaveValue('http://localhost:11434');
  await page.getByLabel('Chat provider').selectOption('llamacpp');
  await expect(page.getByLabel('llama.cpp URL')).toHaveValue('https://llama.example.com');
  await expect(page.getByLabel('llama.cpp model', { exact: true })).toHaveValue('test-model.gguf');
  await page.screenshot({ path: '/tmp/lattice-llamacpp-settings.png' });
});

// A large catalog must stay navigable without a token or a local chat provider.
test('catalog offers category previews, bounded pages, and honest search results', async ({ page }) => {
  await mockCommands(page, (settings) => {
    settings.llm.provider = 'llamacpp';
    const groups: Array<{ category: ModelCategoryDto; names: string[]; variants: number }> = [
      { category: 'LLM', names: ['Qwen 3 8B', 'Gemma 3 4B', 'Phi 4 Mini', 'Mistral 7B', 'Llama 3.2 3B', 'Qwen 3 14B', 'DeepSeek R1 8B', 'SmolLM 3B'], variants: 3 },
      { category: 'Embedding', names: ['BGE Small English', 'Nomic Embed Text', 'All MiniLM L6', 'BGE Base English', 'E5 Small', 'GTE Base'], variants: 1 },
      { category: 'OCR', names: ['PaddleOCR', 'DeepSeek OCR', 'GOT OCR'], variants: 1 },
      { category: 'Transcription', names: ['Whisper Small', 'Whisper Base', 'Whisper Tiny'], variants: 1 },
    ];
    const models = groups.flatMap(({ category, names, variants }) => names.flatMap((name, index) =>
      Array.from({ length: variants }, (_, variant) => {
        const id = `${category}-${index}-${variant}`;
        const quantization = ['Q4_K_M', 'Q5_K_M', 'Q8_0'][variant];
        return {
          model: {
            id, name: variants > 1 ? `${name} · ${quantization}` : name, category,
            description: 'A model available in the catalog.', size_gb: 1 + index + variant / 10,
            minimum_ram_gb: 4, recommended_ram_gb: 8, context_length: 32768,
            performance_tier: 'Balanced', supported_quantizations: category === 'LLM' ? [quantization] : [],
            capabilities: ['chat'], download_url: `https://huggingface.co/catalog/${id}`,
            license: 'Apache-2.0', requires_auth: false, model_id: `catalog/${id}`,
            default_filename: `${id}.gguf`, files: [], total_size_bytes: 1000000000,
            embedding_dimensions: category === 'Embedding' ? 384 : null, embedding_compatibility: null,
          },
          compatibility: { compatibility_level: 'Good', overall_score: 80, ram_score: 90, gpu_score: 80, disk_score: 90, estimated_tokens_per_second: null, estimated_loading_time_seconds: 3, recommendations: [], blockers: [] },
          ranking_score: 80 - index, popularity_downloads: 100000 - index * 1000 - variant, popularity_likes: 100,
        } satisfies Fixture<CommandResult<'get_all_recommended_models'>[number]>;
      })
    ));
    return {
      get_settings: () => settings,
      get_model_download_path: () => '/Users/example/Models',
      get_huggingface_token_status: () => ({ isSet: false }),
      list_downloaded_models: () => [],
      list_downloads: () => [],
      is_model_already_downloaded: () => false,
      get_all_recommended_models: () => models,
      get_model_variants: (args) => {
        if (localStorage.getItem('test:catalog-versions-error')) throw new Error('Repository temporarily unavailable');
        const base = models.find(({ model }) => model.model_id === (args as { repoId: string }).repoId)!.model;
        return ['Q4_K_M', 'Q5_K_M', 'Q8_0', 'BF16'].map((quant, index) => ({
          ...base, id: `${base.id}-${quant}`, default_filename: `model-${quant}.gguf`,
          supported_quantizations: [quant], size_gb: 4 + index * 3, minimum_ram_gb: (4 + index * 3) * 1.5,
        }));
      },
      download_model: (args) => {
        localStorage.setItem('test:catalog-downloaded-id', (args as { modelId: string }).modelId);
        return { status: 'already_downloaded', download_id: '' };
      },
      detect_system_capabilities: () => ({ total_ram_gb: 32, cpu_cores: 10, cpu_architecture: 'ARM64', gpu_type: 'AppleSilicon', gpu_acceleration: 'Metal', vram_gb: null, available_disk_gb: 500 }),
      get_model_catalog_stats: () => ({ total_entries: 36, valid_entries: 36, expired_entries: 0 }),
      search_model_catalog: (args) => {
        if (localStorage.getItem('test:catalog-search-error')) throw new Error('Catalog search unavailable');
        const query = (args as { request: { query: string } }).request.query.toLowerCase();
        return models.filter(({ model }) => model.name.toLowerCase().includes(query)).map(({ model, popularity_downloads, popularity_likes }) => ({ model, relevance_score: 80, source: 'Curated' as const, popularity_downloads, popularity_likes }));
      },
    };
  }, makeAppSettings());
  await page.goto('/settings');
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Models', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Explore by purpose' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Tools', exact: true })).toBeInViewport({ ratio: 1 });
  const chat = page.getByRole('region', { name: 'Chat & writing' });
  await expect(chat.getByRole('button', { name: 'Download', exact: true })).toHaveCount(3);
  await expect(page.getByRole('button', { name: 'Download', exact: true })).toHaveCount(12);
  await expect(page.getByRole('region', { name: 'Search & retrieval' })).toBeVisible();
  await page.getByRole('heading', { name: 'Catalog', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: '/tmp/lattice-catalog-overview.png' });

  for (const theme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    await expect(page.getByRole('button', { name: 'Filters', exact: true })).toHaveAttribute('aria-expanded', 'false');
    expect(await chat.getByRole('button', { name: /Qwen 3 8B.*Fits/ }).first().evaluate(element => element.getBoundingClientRect().top)).toBeLessThan(650);
    await page.screenshot({ path: test.info().outputPath(`catalog-overview-${theme}.png`), animations: 'disabled' });
  }
  await page.emulateMedia({ colorScheme: 'light' });

  await chat.getByRole('button', { name: 'View all chat & writing models' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 1–8 of 24 models');
  await expect(page.getByRole('button', { name: 'Download', exact: true })).toHaveCount(8);
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 9–16 of 24 models');
  await page.evaluate(() => localStorage.setItem('test:catalog-versions-error', 'true'));
  await page.getByRole('button', { name: 'Versions of Phi 4 Mini · Q8_0', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Phi 4 Mini · Q8_0', exact: true })).toBeVisible();
  await expect(page.getByText(/Couldn’t load versions/)).toBeVisible();
  await page.evaluate(() => localStorage.removeItem('test:catalog-versions-error'));
  await page.getByRole('button', { name: 'Retry versions' }).click();
  const versions = page.getByRole('region', { name: 'Available versions' });
  await expect(versions.getByRole('status')).toHaveText('4 of 4 standalone versions');
  await versions.getByLabel('Filter versions').fill('Q8');
  await expect(versions.getByRole('status')).toHaveText('1 of 4 standalone versions');
  await versions.getByRole('button', { name: /Q8_0/ }).click();
  await expect(versions.getByRole('button', { name: /Q8_0/ })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByText('Selected download', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Download', exact: true }).click();
  await expect.poll(() => page.evaluate(() => localStorage.getItem('test:catalog-downloaded-id'))).toBe('LLM-2-2-Q8_0');
  await versions.getByLabel('Filter versions').fill('');
  await versions.getByRole('button', { name: /Q4_K_M/ }).click();
  await expect(page.getByRole('button', { name: 'Download', exact: true })).toBeEnabled();
  await page.setViewportSize({ width: 800, height: 700 });
  await page.getByRole('heading', { name: 'Choose a version' }).scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(800);
  await page.screenshot({ path: 'e2e-results/catalog-versions-narrow.png' });
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.screenshot({ path: 'e2e-results/catalog-versions.png' });
  await page.getByRole('button', { name: 'Back', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 9–16 of 24 models');
  await page.getByRole('combobox', { name: 'Sort models' }).selectOption('size_asc');
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 1–8 of 24 models');
  await page.getByRole('button', { name: 'Filters', exact: true }).click();
  await page.getByLabel('Listed quantization').selectOption('Q8_0');
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 1–8 of 8 models');
  await page.getByLabel('Listed quantization').selectOption('');
  await page.getByRole('heading', { name: 'Catalog', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: '/tmp/lattice-catalog-pages.png' });

  await page.getByRole('textbox', { name: 'Search models' }).fill('no-such-model');
  await expect(page.getByText('No models match.', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Download', exact: true })).toHaveCount(0);
  await page.evaluate(() => localStorage.setItem('test:catalog-search-error', 'true'));
  await page.getByRole('textbox', { name: 'Search models' }).fill('Qwen');
  await expect(page.getByText(/Couldn't load the catalog/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Download', exact: true })).toHaveCount(0);
  await page.evaluate(() => localStorage.removeItem('test:catalog-search-error'));
  await page.getByRole('button', { name: 'Retry', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 1–6 of 6 models');
  await page.getByRole('button', { name: 'Clear search', exact: true }).click();
  await page.getByRole('button', { name: 'Reset filters', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Explore by purpose' })).toBeVisible();
  await page.getByRole('button', { name: 'Browse all models', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Showing' })).toHaveText('Showing 1–8 of 36 models');
  await page.getByRole('button', { name: 'Explore categories', exact: true }).click();
  await page.setViewportSize({ width: 800, height: 700 });
  await page.getByRole('heading', { name: 'Catalog', exact: true }).scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(800);
  await page.screenshot({ path: '/tmp/lattice-catalog-compact.png' });
  expect(await page.locator('.settings-content').evaluate(element => element.scrollWidth - element.clientWidth)).toBeLessThanOrEqual(1);
});

test('restores a 45-PDF import and updates progress as files finish', async ({ page }) => {
  await mockCommands(page, ({ settings }) => {
    localStorage.setItem('ingestHub.lastTab', 'files');
    const state = window as unknown as { __IMPORT_FINISHED__: number };
    state.__IMPORT_FINISHED__ = 0;
    return {
      get_settings: () => settings,
      get_batch_history: () => ({ jobs: [{ jobId: 'job', jobType: 'file_import', status: 'running', totalItems: 45, completedItems: 0, failedItems: 0 }] }),
      get_batch_status: () => ({
        jobId: 'job', jobType: 'file_import', status: 'running', totalItems: 45,
        completedItems: state.__IMPORT_FINISHED__, failedItems: 0,
        items: Array.from({ length: 45 }, (_, i) => ({
          itemId: `pdf-${i}`, target: `/Downloads/chapter-${i}.pdf`,
          status: i < state.__IMPORT_FINISHED__ ? 'completed' : i === state.__IMPORT_FINISHED__ ? 'processing' : 'pending',
        })),
      }),
      initialize_database: () => 'Database initialized',
      list_downloads: () => [],
      get_indexed_folders: () => [],
      list_conversation_spaces: () => [],
    };
  }, { settings: makeAppSettings() });
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/ingest');
  await expect(page.getByText('0 of 45 files processed · 0%')).toBeVisible();
  await expect(page.getByText('Processing chapter-0.pdf — extracting text and building search index')).toBeVisible();
  // Three files finish; the import's job reports it.
  await page.evaluate(() => {
    (window as unknown as { __IMPORT_FINISHED__: number }).__IMPORT_FINISHED__ = 3;
    window.__LATTICE_IPC__.emit('jobs://status', {
      id: 'job', kind: 'batch.file_import', subjectId: null, status: 'running', progressCurrent: 3, progressTotal: 45,
      progressMessage: 'Imported 3 of 45 files', activity: { completed: 3, failed: 0 }, resultRef: null, errorCode: null,
      error: null, retryOfJobId: null, retryCount: 0, retryNotBefore: null, createdAt: 1, startedAt: 1, finishedAt: null,
    });
  });
  await expect(page.getByText('3 of 45 files processed · 7%')).toBeVisible();
  await expect(page.getByText('Processing chapter-3.pdf — extracting text and building search index')).toBeVisible();
  await expect(page.getByText('Imported', { exact: true })).toHaveCount(3);
  await expect(page.getByText('Queued', { exact: true })).toHaveCount(41);
  await page.screenshot({ path: '/tmp/lattice-import-progress.png' });
  expect(errors).toEqual([]);
});

test('creates a subject-agnostic flashcard deck in Studio, reviews, quizzes, opens sources and saves edits', async ({ page }) => {
  await mockCommands(page, settings => {
    const source = { chunkId: 'biology-chunk', documentId: 'biology', fileName: 'biology.md', filePath: '/library/biology.md', excerpt: 'Chlorophyll absorbs the light used in photosynthesis.' };
    const original = {
      id: 'biology-deck', title: 'Biology review', focus: 'photosynthesis', studyGoal: 'Biology exam', modelName: 'test-model', createdAt: Date.now(),
      cards: [
        { id: 'light', deckId: 'biology-deck', question: 'What absorbs the light used in photosynthesis?', answer: 'Chlorophyll', options: ['Chlorophyll', 'Water', 'Oxygen', 'Glucose', 'Carbon dioxide'], correctIndex: 0, explanation: 'The passage identifies chlorophyll as the light absorber.', topic: 'Light absorption', source, dueAt: 0, intervalDays: 0, reviewCount: 0, lapses: 0 },
        { id: 'energy', deckId: 'biology-deck', question: 'What form of energy does photosynthesis produce?', answer: 'Chemical energy', options: ['Sound', 'Chemical energy', 'Motion', 'Electricity', 'Gravity'], correctIndex: 1, explanation: 'The source describes a conversion from light to chemical energy.', topic: 'Energy conversion', source: { ...source, excerpt: 'Photosynthesis converts light energy into chemical energy.' }, dueAt: 0, intervalDays: 0, reviewCount: 0, lapses: 0 },
      ],
    };
    type Deck = typeof original;
    const load = (): Deck | null => JSON.parse(localStorage.getItem('test:study-deck') ?? 'null');
    const save = (deck: Deck) => localStorage.setItem('test:study-deck', JSON.stringify(deck));
    return {
      get_settings: () => {
        settings.ui.theme = localStorage.getItem('test:study-theme') === 'light' ? 'light' : 'dark';
        return settings;
      },
      initialize_database: () => 'Database initialized',
      list_learning_programs: () => [],
      list_downloads: () => [],
      get_batch_history: () => ({ jobs: [] }),
      list_all_documents: () => [{ id: 'biology', fileName: source.fileName, filePath: source.filePath, fileType: 'md', category: 'Document', wordCount: 20 }],
      read_file_content: (args) => {
        if ((args as { path: string }).path !== source.filePath) throw new Error('Unexpected source');
        return '# Photosynthesis notes\n\nChlorophyll absorbs the light used in photosynthesis.\n\nPhotosynthesis converts light energy into chemical energy.';
      },
      list_study_decks: () => {
        const deck = load();
        if (!deck) return [];
        const { id, title, focus, studyGoal, createdAt, cards } = deck;
        return [{ id, title, focus, studyGoal, createdAt, cardCount: cards.length, dueCount: cards.filter(c => c.dueAt <= Date.now()).length, quizAttempts: 0, quizCorrect: 0 }];
      },
      get_study_deck: () => {
        const deck = load();
        if (!deck) throw new Error('Study deck not found');
        return deck;
      },
      generate_study_deck: async (args) => {
        const request = (args as { request: { title: string; focus: string; studyGoal: string; documentIds: string[] } }).request;
        if (request.documentIds.join(',') !== 'biology') throw new Error('Incorrect source scope');
        Object.assign(original, { title: request.title, focus: request.focus, studyGoal: request.studyGoal });
        await new Promise<void>(resolve => window.addEventListener('test:finish-generation', () => resolve(), { once: true }));
        save(original); return original;
      },
      delete_study_deck: () => { localStorage.removeItem('test:study-deck'); return null; },
      review_study_card: (args) => {
        const deck = load();
        if (!deck) throw new Error('No saved deck');
        if (localStorage.getItem('test:study-fail-review')) throw new Error('Disk full');
        const request = (args as { request: { cardId: string; expectedReviews: number; selectedOption: number | null; rating: string } }).request;
        const card = deck.cards.find(c => c.id === request.cardId)!;
        if (request.expectedReviews !== card.reviewCount) throw new Error('Stale review');
        card.reviewCount++;
        const wrong = request.selectedOption !== null ? request.selectedOption !== card.correctIndex : request.rating === 'again';
        card.lapses += Number(wrong); card.dueAt = Date.now() + (wrong ? 600_000 : 86_400_000); card.intervalDays = wrong ? 0 : 1;
        save(deck); return card;
      },
      update_study_card: (args) => {
        const deck = load();
        if (!deck) throw new Error('No saved deck');
        const request = (args as { request: { cardId: string; question: string; answer: string; explanation: string } }).request;
        const card = deck.cards.find(c => c.id === request.cardId)!;
        Object.assign(card, { question: request.question, answer: request.answer, explanation: request.explanation });
        card.options[card.correctIndex] = card.answer; save(deck); return null;
      },
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/studio');
  await expect(page.getByRole('button', { name: 'Studio', exact: true })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByText('Put what you learn into practice.')).toBeVisible();
  await page.getByRole('button', { name: 'New deck', exact: true }).click();
  await page.getByLabel('Deck title').fill('Biology review');
  await page.getByLabel(/Topic or section/).fill('photosynthesis');
  await page.getByLabel(/Learning goal/).fill('Biology exam');
  await page.getByRole('checkbox', { name: 'biology.md' }).check();
  await page.getByLabel('Questions', { exact: true }).selectOption('2');
  await page.screenshot({ path: '/tmp/lattice-study-new-dark.png' });
  await page.getByRole('button', { name: 'Generate deck' }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Studio', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Generating 2 questions' })).toBeVisible();
  await expect(page.getByText('Put what you learn into practice.')).toHaveCount(0);
  await page.screenshot({ path: '/tmp/lattice-study-background-dark.png' });
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.evaluate(() => window.dispatchEvent(new Event('test:finish-generation')));
  await page.getByRole('button', { name: 'Studio', exact: true }).click();
  await page.getByRole('button').filter({ hasText: 'Biology review' }).click();
  await expect(page.getByRole('heading', { name: 'Biology review', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Review due (2)' }).click();
  await expect(page.getByText('Chlorophyll', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Reveal answer' }).click();
  await page.evaluate(() => localStorage.setItem('test:study-fail-review', '1'));
  await page.getByRole('button', { name: 'Good', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Disk full');
  await page.evaluate(() => localStorage.removeItem('test:study-fail-review'));
  await page.getByRole('button', { name: 'Good', exact: true }).click();
  await expect(page.getByText('2 of 2 · Biology review')).toBeVisible();
  await page.getByRole('button', { name: 'Reveal answer' }).click();
  await page.getByRole('button', { name: 'Again', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Review complete' })).toBeVisible();
  await page.getByRole('button', { name: 'Back to deck' }).click();
  await page.getByRole('button', { name: 'Practice quiz', exact: true }).click();
  await page.getByRole('radio').nth(4).check();
  await page.getByRole('button', { name: 'Check answer' }).click();
  await expect(page.getByText('Review this answer')).toBeVisible();
  await page.getByText('Citation · biology.md', { exact: true }).click();
  await page.getByRole('button', { name: 'Open source' }).click();
  const viewer = page.getByRole('dialog', { name: 'biology.md' });
  await expect(viewer.getByRole('heading', { name: 'Photosynthesis notes' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.screenshot({ path: '/tmp/lattice-study-quiz-dark.png' });
  await page.getByRole('button', { name: 'End session' }).click();
  await page.locator('summary').filter({ hasText: 'What absorbs the light used in photosynthesis?' }).click();
  await page.getByRole('button', { name: 'Edit card' }).click();
  await page.getByLabel('Question', { exact: true }).fill('Which pigment absorbs the light used in photosynthesis?');
  await page.getByRole('button', { name: 'Save card' }).click();
  await page.reload();
  await expect(page.locator('summary').filter({ hasText: 'Which pigment absorbs the light used in photosynthesis?' })).toBeVisible();
  await page.evaluate(() => localStorage.setItem('test:study-theme', 'light'));
  await page.reload();
  await expect(page.locator('html')).not.toHaveClass(/dark/);
  await expect(page.getByRole('heading', { name: 'Biology review', exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/lattice-study-deck-light.png' });
  await page.getByRole('button', { name: 'Delete deck', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete deck', exact: true }).click();
  await expect(page.getByText('Put what you learn into practice.')).toBeVisible();
  expect(errors).toEqual([]);
});


test('keeps both remote connections in Auto settings after saving and reloading', async ({ page }) => {
  await mockCommands(page, defaults => {
    return {
      get_settings: () => {
        const saved = JSON.parse(localStorage.getItem('test:auto-settings') ?? JSON.stringify(defaults));
        return saved;
      },
      update_settings: (args) => {
        const saved = JSON.parse(localStorage.getItem('test:auto-settings') ?? JSON.stringify(defaults));
        Object.assign(saved.llm, (args as { settings: { updates: object } }).settings.updates);
        localStorage.setItem('test:auto-settings', JSON.stringify(saved)); return saved;
      },
      list_downloaded_models: () => [],
      list_downloads: () => [],
      get_batch_history: () => ({ jobs: [] }),
    };
  }, makeAppSettings());
  await page.goto('/settings');
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Chat', exact: true }).click();
  await page.getByLabel('llama.cpp model', { exact: true }).fill('qwen.gguf');
  await page.getByRole('button', { name: 'Save connection' }).click();
  await expect(page.getByLabel('Chat provider')).toHaveValue('auto');
  await page.reload();
  await page.getByRole('navigation', { name: 'Settings sections' }).getByRole('button', { name: 'Chat', exact: true }).click();
  await expect(page.getByLabel('Chat provider')).toHaveValue('auto');
  await expect(page.getByLabel('llama.cpp model', { exact: true })).toHaveValue('qwen.gguf');
  await expect(page.getByText('Ollama server', { exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/lattice-auto-connections.png', fullPage: true });
});

test('chat exposes provider errors and persisted PDF import failures', async ({ page }) => {
  const settings = makeAppSettings();
  settings.llm.provider = 'auto';
  settings.llm.llamaCpp.model = 'qwen-test.gguf';
  await mockCommands(page, (settings, ipc) => {
    const stamp = new Date().toISOString();
    const conversation = { id: 'patent-chat', title: 'Patent Training', modelName: '__ollama_server__', createdAt: stamp, updatedAt: stamp, messageCount: 0, totalTokens: 0, spaceId: 'space_general', isArchived: false };
    const job = { jobId: 'pdf-import', jobType: 'file_import', status: 'completed', totalItems: 45, completedItems: 44, failedItems: 1, createdAt: stamp };
    const earlier = { ...job, jobId: 'earlier-pdf', totalItems: 1, completedItems: 1, failedItems: 0, createdAt: new Date(Date.now() - 3600000).toISOString() };
    let item = { itemId: 'failed-pdf', target: '/Downloads/mpep-2100.pdf', status: 'failed', errorMessage: 'PDF extraction timed out' as string | null };
    const saved = localStorage.getItem('test:pdf-recovery');
    if (saved) { const state = JSON.parse(saved); Object.assign(job, state.job); item = state.item; }
    return {
      get_settings: () => settings,
      list_conversation_spaces: () => [{ id: 'space_general', name: 'General', isArchived: false }],
      list_conversations_explorer: () => ({ conversations: [conversation], total: 1 }),
      list_conversations: () => ({ conversations: [conversation], total: 1 }),
      get_conversation: () => ({ conversation }),
      get_conversation_messages: () => ({ messages: [], total: 0 }),
      list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
      get_batch_history: () => ({ jobs: [job, earlier] }),
      get_batch_status: (args) => (args as { request: { jobId: string } }).request.jobId === earlier.jobId
        ? { ...earlier, completedAt: stamp, items: [{ itemId: 'earlier-item', target: '/Downloads/mpep-9035-appx-p.pdf', status: 'completed' }] }
        : { ...job, completedAt: job.status === 'completed' ? stamp : null, items: [item] },
      'dialog|open': () => '/Downloads/mpep-2100-corrected.pdf',
      retry_failed_items: (args) => {
        const request = args as { jobId: string; itemId: string; replacementPath?: string };
        if (request.jobId !== job.jobId || request.itemId !== item.itemId) throw new Error('Retry must target the exact failed PDF');
        job.status = 'running'; job.failedItems = 0; item.status = 'processing'; item.errorMessage = null;
        if (request.replacementPath) item.target = request.replacementPath;
        setTimeout(() => {
          if (request.replacementPath) { job.status = 'completed'; job.completedItems = 45; item.status = 'completed'; }
          else { job.status = 'failed'; job.failedItems = 1; item.status = 'failed'; item.errorMessage = 'PDF extraction timed out again'; }
          localStorage.setItem('test:pdf-recovery', JSON.stringify({ job, item }));
          // The import's job reports how the retry ended.
          ipc.emit('jobs://status', {
            id: job.jobId, kind: 'batch.file_import', subjectId: null, status: job.status, progressCurrent: 1, progressTotal: 1,
            progressMessage: job.status, activity: null, resultRef: null, errorCode: null, error: null, retryOfJobId: null,
            retryCount: 0, retryNotBefore: null, createdAt: 1, startedAt: 1, finishedAt: 2,
          });
        }, 600);
        return { newJobId: job.jobId, retriedCount: 1 };
      },
      list_journals: () => [],
      list_conversation_tangents: () => [],
      list_conversation_linked_documents: () => [],
      list_conversation_web_sources: () => [],
      list_downloaded_models: () => [],
      list_downloads: () => [],
      get_indexed_folders: () => [],
      chat_with_conversation: () => {
        throw { code: 'NETWORK_ERROR', message: 'Network error', details: 'llama.cpp request timed out' };
      },
    };
  }, settings);
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));
  await page.goto('/chat?conversationId=patent-chat');
  await expect(page.getByText('qwen-test.gguf', { exact: true })).toBeVisible();
  await expect(page.getByRole('status').filter({ hasText: '1 file failed to import: mpep-2100.pdf' })).toBeVisible();
  const composer = page.getByRole('textbox', { name: 'Message composer' });
  await composer.fill('Help me learn the MPEP using only cited PDF passages.');
  await composer.press('Enter');
  await expect(page.getByText('llama.cpp request timed out', { exact: true }).first()).toBeVisible();
  for (const theme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    await page.screenshot({ path: `/tmp/lattice-chat-failures-${theme}.png`, animations: 'disabled' });
  }
  await page.getByRole('link', { name: 'Review import history' }).click();
  await expect(page.getByRole('tab', { name: 'History', exact: true })).toHaveAttribute('data-state', 'active');
  await expect(page.getByText('44 imported, 1 failed', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /45 files Added/ })).toHaveAttribute('aria-expanded', 'true');
  await expect(page.getByRole('button', { name: /mpep-9035-appx-p.pdf Added/ })).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByRole('heading', { name: '2 imports' })).toBeVisible();
  await expect(page.getByText('Failed — PDF extraction timed out', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry mpep-2100.pdf', exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/lattice-import-failures.png', animations: 'disabled' });
  await page.getByRole('button', { name: 'Retry mpep-2100.pdf', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry all failed' })).toHaveCount(0);
  await expect(page.getByText('Failed — PDF extraction timed out again', { exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByText('Failed — PDF extraction timed out again', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Choose replacement for mpep-2100.pdf' }).click();
  await expect(page.getByText('45 imported', { exact: true })).toBeVisible();
  await expect(page.getByText('mpep-2100-corrected.pdf', { exact: true })).toBeVisible();
  await expect(page.getByText('Imported', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '2 imports' })).toBeVisible();
  await expect(page.getByText('Processing — not ready to search yet', { exact: true })).toHaveCount(0);
  await page.screenshot({ path: '/tmp/lattice-import-recovered.png', animations: 'disabled' });
  await page.goto('/chat?conversationId=patent-chat');
  await expect(page.getByText('qwen-test.gguf', { exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Review import history' })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('chat replaces an initial retrieval failure with tool results and keeps it corrected after reload', async ({ page }) => {
  const settings = makeAppSettings();
  settings.llm.provider = 'auto';
  settings.llm.llamaCpp.model = 'qwen-test.gguf';
  await mockCommands(page, (settings, ipc) => {
    const stamp = new Date().toISOString();
    const conversation = { id: 'retrieval-chat', title: 'Patent Training', modelName: 'qwen-test.gguf', createdAt: stamp, updatedAt: stamp, spaceId: 'space_general', messageCount: 0, totalTokens: 0 };
    const recovered = { searchedDocuments: 0, passages: 7, files: 7, scope: 'vault' };
    const oldFailure = { ...recovered, passages: 0, files: 0, unavailableReason: 'the active space has no indexed documents yet' };
    let messages: Fixture<MessageDto>[] = JSON.parse(localStorage.getItem('test:retrieval-messages') ?? '[]');
    return {
      get_settings: () => settings,
      list_conversation_spaces: () => [{ id: 'space_general', name: 'General', isArchived: false }],
      list_conversations_explorer: () => ({ conversations: [conversation], total: 1 }),
      list_conversations: () => ({ conversations: [conversation], total: 1 }),
      get_conversation: () => ({ conversation }),
      get_conversation_messages: () => ({ messages, total: messages.length }),
      list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
      get_batch_history: () => ({ jobs: [] }),
      chat_with_conversation: async (args) => {
        const request = args as { requestId: string; message: string };
        const emit = (retrieval: unknown) => ipc.emit('llm-stream', { conversationId: conversation.id, requestId: request.requestId, status: 'retrieval', retrieval, done: false });
        // QA also publishes on this channel. Its tokens and completion must
        // neither enter this conversation nor detach its stream listener.
        ipc.emit('llm-stream', { type: 'token', content: 'Unrelated QA response', done: false });
        ipc.emit('llm-stream', { type: 'done', done: true });
        emit(oldFailure);
        document.documentElement.dataset.retrievalStage = 'initial';
        await new Promise<void>(resolve => window.addEventListener('test:recover-retrieval', () => resolve(), { once: true }));
        emit(recovered);
        document.documentElement.dataset.retrievalStage = 'recovered';
        await new Promise<void>(resolve => window.addEventListener('test:finish-retrieval', () => resolve(), { once: true }));
        messages = [
          { id: 'user', role: 'user', content: request.message, status: 'completed', createdAt: stamp },
          { id: 'assistant', role: 'assistant', content: 'I retrieved seven passages from your documents.', status: 'completed', createdAt: stamp, metadata: JSON.stringify({ retrieval: recovered }) },
        ];
        localStorage.setItem('test:retrieval-messages', JSON.stringify(messages));
        return { conversationId: conversation.id, messages, contextUsed: 0, sources: [] };
      },
      list_journals: () => [],
      list_conversation_tangents: () => [],
      list_conversation_linked_documents: () => [],
      list_conversation_web_sources: () => [],
      list_downloaded_models: () => [],
      list_downloads: () => [],
      get_indexed_folders: () => [],
    };
  }, settings);
  await page.goto('/chat?conversationId=retrieval-chat');
  const composer = page.getByRole('textbox', { name: 'Message composer' });
  await composer.fill('Help me learn the MPEP from my PDFs.');
  await composer.press('Enter');
  await expect(page.locator('html')).toHaveAttribute('data-retrieval-stage', 'initial');
  await expect(page.getByText('Unrelated QA response', { exact: true })).toHaveCount(0);
  await expect(page.getByText(/no indexed documents|Answered without your documents|Document search was unavailable/)).toHaveCount(0);
  await page.evaluate(() => window.dispatchEvent(new Event('test:recover-retrieval')));
  await expect(page.getByText('Retrieved 7 passages from 7 files', { exact: true })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('test:finish-retrieval')));
  await expect(page.locator('#message-assistant').getByText('I retrieved seven passages from your documents.')).toBeVisible();
  await page.reload();
  await page.getByRole('button', { name: 'Select conversation: Patent Training', exact: true }).press('Enter');
  await expect(page.getByText('Retrieved 7 passages from 7 files', { exact: true })).toBeVisible();
  await expect(page.getByText(/no indexed documents|Answered without your documents|Document search was unavailable/)).toHaveCount(0);
  await page.screenshot({ path: '/tmp/lattice-retrieval-recovered.png', animations: 'disabled' });
});

test('cancels a 43-document deletion after the pending document completes', async ({ page }) => {
  await mockCommands(page, settings => {
    let documents = Array.from({ length: 43 }, (_, index) => ({
      id: `delete-${index}`, fileName: `Delete ${index}.pdf`,
      filePath: `/home/test/.lattice/files/${String(index).padStart(64, '0')}/Delete ${index}.pdf`,
      fileType: 'pdf', category: 'Document', language: 'en', wordCount: 100,
      modifiedAt: '2026-09-15T00:00:00Z', indexedAt: '2026-09-15T00:00:00Z',
    }));
    let calls = 0;
    return {
      get_settings: () => settings,
      list_all_documents: () => documents,
      delete_document: async (args) => {
        document.documentElement.dataset.deleteCalls = String(++calls);
        await new Promise<void>(resolve => window.addEventListener('test:finish-delete', () => resolve(), { once: true }));
        documents = documents.filter(doc => doc.id !== (args as { documentId: string }).documentId);
        document.documentElement.dataset.deleteFinished = 'true';
        return undefined;
      },
      initialize_database: () => 'Database initialized',
      list_downloads: () => [],
      get_indexed_folders: () => [],
      list_conversation_spaces: () => [],
      list_document_space_memberships: () => [],
    };
  }, makeAppSettings());
  await page.goto('/files');
  await page.getByRole('button', { name: 'List view', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Select Delete 0.pdf', exact: true }).click();
  await page.getByRole('button', { name: 'Select all', exact: true }).click();
  await page.getByLabel('Document selection actions').getByRole('button', { name: 'Delete', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Delete documents?' });
  await expect(dialog).toContainText('43 documents');
  await dialog.getByRole('button', { name: /Delete/ }).click();
  await expect(page.locator('html')).toHaveAttribute('data-delete-calls', '1');
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await page.evaluate(() => window.dispatchEvent(new Event('test:finish-delete')));
  await expect(page.getByText('1 document deleted', { exact: true })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-delete-calls', '1');
  await expect(page.getByRole('checkbox', { name: 'Select Delete 0.pdf', exact: true })).toHaveCount(0);
  await expect(page.getByRole('checkbox', { name: 'Select Delete 1.pdf', exact: true })).toBeVisible();
});

test('Learning Studio integrates with the app shell and preserves quick-check work across tabs', async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 1000 });
  await mockCommands(page, settings => {
    const source = {
      id: 'studio-source',
      title: 'Distributed systems field guide',
      url: 'https://example.org/field-guide',
      excerpt: 'Retries are safe only when an operation has a stable identity and repeated delivery preserves the original effect.',
      acquiredAt: 1_790_000_000_000,
    };
    const question = (id: string, kind: 'practice' | 'quiz' | 'test', prompt: string) => ({
      id, kind, prompt, options: ['Use a stable operation identifier', 'Repeat the write without identity', 'Discard every retry', 'Trust the network to deliver once'], sourceIds: [source.id],
    });
    const lesson = (id: string, title: string, suffix: string) => ({
      id, title, objective: `Explain ${title.toLowerCase()} and apply it to a production decision.`, estimatedMinutes: 30,
      preparation: 'ready', completed: false,
      blocks: [
        { kind: 'explanation', title: 'Build the mental model', body: 'Start from the observable failure, then identify which state transition must remain stable when a request is delivered more than once.', sourceIds: [source.id] },
        { kind: 'worked_example', title: 'Trace a duplicate request', body: 'Follow one request through a timeout, retry, and replay. The stable identifier lets the service return the stored result without repeating the effect.', sourceIds: [source.id] },
        { kind: 'reflection', title: 'Name the boundary', body: 'Write down where identity is created, where the result is stored, and which side effects still need their own protection.', sourceIds: [source.id] },
      ],
      questions: [
        question(`practice-${suffix}-1`, 'practice', 'A client retries after losing the response. Which design preserves the original effect?'),
        question(`practice-${suffix}-2`, 'practice', 'Where should the service compare a retry with prior work?'),
        question(`quiz-${suffix}-1`, 'quiz', 'What property makes a repeated request safe?'),
        question(`quiz-${suffix}-2`, 'quiz', 'Which failure requires an idempotency record?'),
        question(`test-${suffix}-1`, 'test', 'Which production change prevents a duplicated charge?'),
        question(`test-${suffix}-2`, 'test', 'Which observation best demonstrates safe replay?'),
      ],
    } satisfies Fixture<LearningLessonDto>);
    const first = lesson('lesson-1', 'Reason about retries', 'a');
    const second = lesson('lesson-2', 'Design an idempotent boundary', 'b');
    const program = {
      summary: {
        id: 'program-1', title: 'Production reasoning after an AI-heavy year',
        goal: 'Rebuild the ability to reason independently about service behavior, tradeoffs, and failure modes.',
        status: 'active', revision: 4, moduleCount: 2, lessonCount: 4, completedLessons: 1,
        currentLessonId: first.id, createdAt: 1_790_000_000_000,
      },
      priorKnowledge: 'Senior engineering experience with C#, Python, AWS, and AI services.',
      minutesPerSession: 35, modelName: 'Local learning model', sources: [source],
      modules: [
        { id: 'module-1', title: 'Reliable service thinking', summary: 'Recover the habit of tracing state, failure, and ownership before reaching for implementation.', outcomes: ['Trace a request across failure', 'Defend an idempotency boundary'], lessons: [first, second] },
        { id: 'module-2', title: 'Independent implementation', summary: 'Move from a justified design into code, tests, and review without surrendering the reasoning step.', outcomes: ['Implement from a written model', 'Explain a tradeoff without assistance'], lessons: [
          { ...lesson('lesson-3', 'Implement from first principles', 'c'), completed: true },
          lesson('lesson-4', 'Review and defend the design', 'd'),
        ] },
      ],
      attempts: [{
        id: 'attempt-1', moduleId: 'module-1', lessonId: first.id, kind: 'quiz', correct: 1, total: 2,
        submittedAt: 1_790_000_600_000,
        results: [
          { questionId: 'quiz-a-1', prompt: 'What property makes a repeated request safe?', options: ['Use a stable operation identifier', 'Repeat the write without identity', 'Discard every retry', 'Trust the network to deliver once'], selectedIndex: 0, correctIndex: 0, explanation: 'The saved identifier connects a retry to the original result.', sourceIds: [source.id] },
          { questionId: 'quiz-a-2', prompt: 'Which failure requires an idempotency record?', options: ['Use a stable operation identifier', 'Repeat the write without identity', 'Discard every retry', 'Trust the network to deliver once'], selectedIndex: 1, correctIndex: 0, explanation: 'A response can be lost after the effect commits, so the next delivery must find the earlier result.', sourceIds: [source.id] },
        ],
      }],
    } satisfies Fixture<LearningProgramDto>;
    return {
      get_settings: () => settings,
      initialize_database: () => 'Database initialized',
      list_downloads: () => [],
      get_indexed_folders: () => [],
      list_conversation_spaces: () => [],
      list_learning_programs: () => [program.summary],
      get_learning_program: () => program,
      get_learning_memory: () => ({ programId: program.summary.id, journalId: null, lessonNotes: [], studyDeck: null, drafts: [], acceptedCards: [], dueCount: 0 }),
      get_learning_practice_workspace: () => ({ programId: program.summary.id, sessions: [] }),
    };
  }, makeAppSettings());
  const errors: Error[] = [];
  page.on('pageerror', error => errors.push(error));

  await page.goto('/studio');
  await expect(page.getByRole('button', { name: 'Studio', exact: true })).toHaveAttribute('aria-current', 'page');
  await page.getByRole('button', { name: /Production reasoning after an AI-heavy year/ }).click();
  await expect(page.getByRole('heading', { name: 'Production reasoning after an AI-heavy year' })).toBeVisible();
  await expect(page.getByLabel('Module', { exact: true })).toHaveValue('module-1');
  const moduleWorkspace = page.getByRole('navigation', { name: 'Module workspace' });
  await expect(moduleWorkspace).toBeVisible();
  await expect(moduleWorkspace.getByRole('tab', { name: 'Lessons', exact: true })).toHaveAttribute('aria-selected', 'true');

  await openStudioSection(page, 'Quick checks');
  const quickCheckType = page.getByRole('group', { name: 'Quick check type' });
  await expect(quickCheckType.getByRole('button', { name: 'practice', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('radio', { name: 'Use a stable operation identifier' }).click();
  await quickCheckType.getByRole('button', { name: 'quiz', exact: true }).click();
  await quickCheckType.getByRole('button', { name: 'practice', exact: true }).click();
  await openStudioSection(page, 'Lessons');
  await openStudioSection(page, 'Quick checks');
  await expect(page.getByRole('radio', { name: 'Use a stable operation identifier' })).toBeChecked();

  await quickCheckType.getByRole('button', { name: 'History (1)', exact: true }).click();
  await expect(page.getByText('Model-authored answer key', { exact: true })).toBeVisible();
  await expect(page.getByText('1 correct of 2', { exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/lattice-learning-studio.png', fullPage: true, animations: 'disabled' });
  expect(errors).toEqual([]);
});
