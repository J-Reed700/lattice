import { expect, type Page } from '@playwright/test';
import { ANSWER_CHUNKS, CONVERSATION, PASSAGES, QUESTION, installChatFixture } from './fixtures/chat';
import { argsOf, journeyTest, type ShellBackend } from './fixtures/shell';

const test = journeyTest(installChatFixture);

const composer = (page: Page) => page.getByRole('textbox', { name: 'Message composer' });

/** Sends the question and returns the id the renderer gave the turn. */
async function ask(page: Page, backend: ShellBackend) {
  const before = argsOf(backend, 'chat_with_conversation').length;
  await composer(page).fill(QUESTION);
  await composer(page).press('Enter');
  await expect.poll(() => argsOf(backend, 'chat_with_conversation').length).toBe(before + 1);
  return String(argsOf(backend, 'chat_with_conversation')[before].requestId);
}

/** Delivers one streamed piece of the answer, as the backend's llm-stream event does. */
async function stream(page: Page, requestId: string, content: string) {
  await page.evaluate(({ conversationId, requestId, content }) => {
    window.__LATTICE_IPC__.emit('llm-stream', { conversationId, requestId, content, done: false });
  }, { conversationId: CONVERSATION.id!, requestId, content });
}

test('a question streams its answer in, and each citation opens the passage it cites', async ({ page, backend }) => {
  await page.goto(`/chat?conversationId=${CONVERSATION.id}`);
  const requestId = await ask(page, backend);
  expect(argsOf(backend, 'chat_with_conversation')[0]).toMatchObject({ conversationId: CONVERSATION.id, message: QUESTION });
  await expect(page.getByRole('button', { name: 'Stop generating response' })).toBeVisible();
  await expect(page.getByText(QUESTION, { exact: true })).toBeVisible();

  // The answer appears as it is written, before the turn ends.
  await stream(page, requestId, ANSWER_CHUNKS[0]);
  await expect(page.getByText('It comes down to silica.')).toBeVisible();
  await stream(page, requestId, ANSWER_CHUNKS[1]);
  await expect(page.getByText(/Runny basaltic magma lets gas escape gently/)).toBeVisible();
  await stream(page, requestId, ANSWER_CHUNKS[2]);
  backend.state.finish.release();

  const answer = page.locator('#message-msg-answer');
  await expect(answer).toContainText('Sticky, silica-rich magma traps gas until it bursts out');
  await expect(page.getByRole('button', { name: 'Stop generating response' })).toHaveCount(0);
  await expect(answer.getByRole('button', { name: /^Citation \d$/ })).toHaveCount(2);

  // A cited document opens in the reader beside the chat, at the cited passage.
  const reader = page.getByRole('dialog', { name: 'Volcanoes and magma.md' });
  await answer.getByRole('button', { name: 'Citation 2', exact: true }).click();
  await expect(reader).toContainText('Citation 2 of 2');
  await expect(reader).toContainText(PASSAGES[1].text);
  await expect(reader.getByRole('button', { name: 'Next citation' })).toBeDisabled();
  await reader.getByRole('button', { name: 'Previous citation' }).click();
  await expect(reader).toContainText('Citation 1 of 2');
  await expect(reader).toContainText(PASSAGES[0].text);
  await reader.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(reader).toBeHidden();

  // The saved answer keeps its citations after a reload.
  await page.reload();
  await page.getByRole('button', { name: `Select conversation: ${CONVERSATION.title!}`, exact: true }).click();
  await expect(answer).toContainText('It comes down to silica.');
  await expect(answer.getByRole('button', { name: /^Citation \d$/ })).toHaveCount(2);
});

test('stopping a question before the answer starts hands the question back to the composer', async ({ page, backend }) => {
  await page.goto(`/chat?conversationId=${CONVERSATION.id}`);
  const requestId = await ask(page, backend);
  await expect(composer(page)).toHaveValue('');
  await page.getByRole('button', { name: 'Stop generating response' }).click();

  await expect(composer(page)).toHaveValue(QUESTION);
  await expect(page.getByRole('button', { name: 'Stop generating response' })).toHaveCount(0);
  expect(argsOf(backend, 'chat_with_conversation')[1]).toMatchObject({ conversationId: CONVERSATION.id, requestId, cancelOnly: true });
  expect(backend.state.messages).toEqual([]);

  // Asking again runs a fresh turn to the end.
  const retryId = await ask(page, backend);
  expect(retryId).not.toBe(requestId);
  await stream(page, retryId, ANSWER_CHUNKS.join(''));
  backend.state.finish.release();
  await expect(page.locator('#message-msg-answer')).toContainText('It comes down to silica.');
});
