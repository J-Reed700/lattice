import { describe, expect, it } from 'vitest';

import { fencedCode } from './lessonText';

describe('fencedCode', () => {
  it('gives the code of a fenced block without its fence lines', () => {
    expect(fencedCode('```python\nx = 1\n```')).toBe('x = 1');
  });

  it('keeps the lines and the indentation of the code', () => {
    expect(fencedCode('```csharp\nif (ok)\n{\n    Run();\n}\n```')).toBe('if (ok)\n{\n    Run();\n}');
  });

  it('takes a fence that names no language, and whitespace around the block', () => {
    expect(fencedCode('\n  ```\nSELECT 1;\n```  \n')).toBe('SELECT 1;');
  });

  it('is not a fenced block in prose', () => {
    expect(fencedCode('Spacing helps recall.')).toBeNull();
  });

  it('is not a fenced block in prose with inline code', () => {
    expect(fencedCode('Call `run()` before `stop()`.')).toBeNull();
  });

  it('is not a fenced block when the fence is never closed', () => {
    expect(fencedCode('```python\nx = 1')).toBeNull();
  });

  it('is not a fenced block when prose comes before or after the fence', () => {
    expect(fencedCode('For example:\n```python\nx = 1\n```')).toBeNull();
    expect(fencedCode('```python\nx = 1\n```\nThat sets x.')).toBeNull();
  });

  it('is not one block when there are two', () => {
    expect(fencedCode('```python\nx = 1\n```\n\n```python\ny = 2\n```')).toBeNull();
  });
});
