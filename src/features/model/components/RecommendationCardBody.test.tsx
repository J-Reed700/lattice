import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { RecommendationDto } from '@/lib/bindings';

import { RecommendationCardBody } from './RecommendationCardBody';

const GB = 1_073_741_824;

const recommendation: RecommendationDto = {
  modelId: 'qwen-27b',
  name: 'Qwen 3.5 27B',
  description: 'Dense 27B instruction-tuned model, Q4_K_M quantisation.',
  useFor: 'For complex reasoning and multi-step work, like building and checking courses. Answers more slowly.',
  downloadBytes: 16 * GB,
  verdict: {
    kind: 'fits',
    needsBytes: 20 * GB,
    budgetBytes: 22 * GB,
    device: { name: 'NVIDIA GeForce RTX 4080', kind: 'dedicated', totalBytes: 24 * GB },
  },
  labels: ['best'],
  evidence: { kind: 'untested' },
  speed: 'thorough',
};

describe('RecommendationCardBody', () => {
  it('shows what the model is for, in plain words', () => {
    const { container } = render(<RecommendationCardBody recommendation={recommendation} />);
    expect(container.textContent).toContain(recommendation.useFor);
  });

  it('puts the plain sentence before the technical description, and keeps the description', () => {
    const { container } = render(<RecommendationCardBody recommendation={recommendation} />);
    const text = container.textContent ?? '';
    expect(text).toContain(recommendation.description);
    expect(text.indexOf(recommendation.useFor)).toBeGreaterThan(-1);
    expect(text.indexOf(recommendation.useFor)).toBeLessThan(text.indexOf(recommendation.description));
  });
});
