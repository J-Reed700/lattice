/** Bytes per `String.fromCharCode` call; a whole recording would overflow the stack. */
const CHUNK_BYTES = 0x8000;

/** `samples` as mono 16-bit little-endian PCM, base64: what `transcribe_speech` takes. */
export function encodePcm(samples: Float32Array): string {
  const bytes = new Uint8Array(samples.length * 2);
  const view = new DataView(bytes.buffer);
  samples.forEach((sample, i) => {
    const clamped = Math.max(-1, Math.min(1, sample));
    view.setInt16(i * 2, Math.round(clamped * 0x7fff), true);
  });
  let binary = '';
  for (let at = 0; at < bytes.length; at += CHUNK_BYTES) {
    binary += String.fromCharCode(...bytes.subarray(at, at + CHUNK_BYTES));
  }
  return btoa(binary);
}
