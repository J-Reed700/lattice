import { fireEvent, render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

import {
  AudioViewer,
  audioViewerPropsFromSource,
  parseSectionStartMs,
} from '../AudioViewer';

// The global setup mocks `@tauri-apps/api/core` as `{ invoke }` only, so
// `convertFileSrc` is otherwise undefined here.
vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => `tauri://localhost${path}`,
}));

function audioElement(container: HTMLElement): HTMLAudioElement {
  const audio = container.querySelector('audio');
  if (!audio) throw new Error('no audio element rendered');
  return audio;
}

describe('AudioViewer', () => {
  it('renders an audio element pointing at the converted path', () => {
    const { container } = render(<AudioViewer filePath="/vault/standup.m4a" />);

    const audio = audioElement(container);
    expect(audio.getAttribute('src')).toBe('tauri://localhost/vault/standup.m4a');
  });

  it('seeks to the cited moment once metadata loads', () => {
    const { container } = render(
      <AudioViewer filePath="/vault/standup.m4a" seekToMs={760000} />
    );

    const audio = audioElement(container);
    fireEvent(audio, new Event('loadedmetadata'));

    expect(audio.currentTime).toBe(760);
  });

  it('seeks when a segment row is clicked', () => {
    const play = vi.spyOn(window.HTMLMediaElement.prototype, 'play');

    const { container } = render(
      <AudioViewer
        filePath="/vault/standup.m4a"
        segments={[
          { startMs: 0, endMs: 45000, text: 'opening remarks' },
          { startMs: 45000, endMs: 90000, text: 'the numbers' },
          { startMs: 90000, endMs: 135000, text: 'next steps' },
        ]}
      />
    );

    const rows = screen.getAllByRole('button');
    expect(rows).toHaveLength(3);

    const third = rows[2];
    if (!third) throw new Error('third row missing');
    fireEvent.click(third);

    expect(audioElement(container).currentTime).toBe(90);
    expect(play).not.toHaveBeenCalled();

    play.mockRestore();
  });

  it('parses section start timestamps', () => {
    expect(parseSectionStartMs('12:40–13:25')).toBe(760000);
    expect(parseSectionStartMs('1:02:07–1:03:00')).toBe(3727000);
    expect(parseSectionStartMs(undefined)).toBeUndefined();
    expect(parseSectionStartMs('p. 12')).toBeUndefined();
  });

  it('maps chunk excerpts to segments', () => {
    const props = audioViewerPropsFromSource({
      section: '0:45–1:30',
      chunkExcerpts: [
        { chunkId: 'b', excerpt: 'later passage', section: '1:30–2:15' },
        { chunkId: 'a', excerpt: 'earlier passage', section: '0:45–1:30' },
      ],
    });

    expect(props.seekToMs).toBe(45000);
    expect(props.segments).toHaveLength(2);
    expect(props.segments?.[0]).toEqual({
      startMs: 45000,
      endMs: 90000,
      text: 'earlier passage',
      chunkId: 'a',
    });
    expect(props.segments?.[1]?.chunkId).toBe('b');
  });

  it('renders no list when there are no parseable segments', () => {
    render(<AudioViewer filePath="/vault/standup.m4a" seekToMs={760000} />);

    expect(screen.queryAllByRole('button')).toHaveLength(0);
    expect(screen.getByText('Cited at 12:40')).toBeTruthy();
  });
});
