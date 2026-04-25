/**
 * scrollToMessage
 *
 * Polls for a message DOM node by id, scrolls it into view, and briefly
 * highlights it. Lifted from the former ConversationSpotlight so other
 * surfaces (notably the unified Spotlight) can reuse it.
 */
export function scrollToMessage(messageId: string): void {
  let attempts = 0;
  const maxAttempts = 12;

  const tick = () => {
    const element = document.getElementById(`message-${messageId}`);
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'center' });
      element.classList.add('chat-message-highlighted');
      window.setTimeout(() => {
        element.classList.remove('chat-message-highlighted');
      }, 1500);
      return;
    }

    attempts += 1;
    if (attempts < maxAttempts) {
      window.setTimeout(tick, 120);
    }
  };

  window.setTimeout(tick, 80);
}
