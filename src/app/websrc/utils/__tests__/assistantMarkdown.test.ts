import { describe, it, expect } from 'vitest';

import { normalizeAssistantMarkdown } from '../assistantMarkdown';

describe('normalizeAssistantMarkdown', () => {
  it('normalizes unicode bullet lists', () => {
    const input = 'Choose a variety\n• Pick compact bushes\n• Verify chill hours';

    expect(normalizeAssistantMarkdown(input)).toBe(
      'Choose a variety\n- Pick compact bushes\n- Verify chill hours'
    );
  });

  it('normalizes dash bullet variants', () => {
    const input = 'Planting steps\n– Fill container\n— Water deeply\n− Mulch surface';

    expect(normalizeAssistantMarkdown(input)).toBe(
      'Planting steps\n- Fill container\n- Water deeply\n- Mulch surface'
    );
  });

  it('normalizes parenthesized numbered lists', () => {
    const input = '1) Prepare soil\n2）Test pH';

    expect(normalizeAssistantMarkdown(input)).toBe('1. Prepare soil\n2. Test pH');
  });

  it('does not rewrite list-like symbols inside fenced code blocks', () => {
    const input = '```md\n• leave this as-is\n1) keep this too\n```';

    expect(normalizeAssistantMarkdown(input)).toBe(input);
  });

  it('normalizes line endings and collapses extra blank lines', () => {
    const input = 'Line one\r\n\r\n\r\nLine two\r\n';

    expect(normalizeAssistantMarkdown(input)).toBe('Line one\n\nLine two');
  });
});
