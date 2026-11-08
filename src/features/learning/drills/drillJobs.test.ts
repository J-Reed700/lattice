import { describe, expect, it } from 'vitest';

import { DRILL_BUILD, DRILL_GRADING, shelfOf } from './drillJobs';
import { DEFAULT_OPTIONS } from './drillOptions';

describe('the jobs of a drill', () => {
  it('are the build that writes it and the grading of a review', () => {
    expect(DRILL_BUILD).toBe('learning.drill_build');
    expect(DRILL_GRADING).toBe('learning.drill_grading');
  });

  it('build for the shelf of a course: its code in a language', () => {
    expect(shelfOf('course-1', { ...DEFAULT_OPTIONS, material: 'code', language: 'python' })).toBe('drill:course-1:code:python');
  });

  it('build for the shelf of a course: its passages, whatever language was picked', () => {
    expect(shelfOf('course-1', { ...DEFAULT_OPTIONS, material: 'passage' })).toBe('drill:course-1:passage');
    expect(shelfOf('course-2', { ...DEFAULT_OPTIONS, material: 'passage', language: 'rust' })).toBe('drill:course-2:passage');
  });
});
