import { describe, expect, it } from 'vitest';

import { SEVERITIES, severityLabel, severityTone } from './severity';

describe('severity', () => {
  it('lists the severities most severe first', () => {
    expect(SEVERITIES).toEqual(['critical', 'major', 'minor', 'nit']);
  });

  it('names each severity for a badge', () => {
    expect(SEVERITIES.map(severityLabel)).toEqual(['Critical', 'Major', 'Minor', 'Nit']);
  });

  it('gives each severity the classes of its own badge, on the shape of a state chip', () => {
    const tones = SEVERITIES.map(severityTone);
    expect(new Set(tones).size).toBe(SEVERITIES.length);
    expect(severityTone('critical')).toContain('bg-danger-muted');
    expect(severityTone('major')).toContain('bg-warning-muted');
    for (const tone of tones) expect(tone).toContain('rounded-full');
  });
});
