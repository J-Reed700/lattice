/**
 * Read-aloud playback. One player for the app, so one thing speaks at a
 * time: starting another source stops the current one.
 *
 * A source is spoken piece by piece. Each piece is synthesised while the one
 * before it plays and is scheduled to start the moment it ends, so a long
 * answer starts after its first sentence and plays without gaps. Synthesis
 * keeps at most {@link LEAD_SECONDS} of audio ahead of the listener.
 */

export interface SpeechClip {
  sampleRate: number;
  /** Mono 16-bit little-endian PCM, base64. */
  pcm: string;
  durationMs: number;
}

export type Synthesize = (text: string) => Promise<SpeechClip>;

export type SpeechStatus = 'idle' | 'loading' | 'playing' | 'paused';

export interface SpeechSnapshot {
  /** The source speaking or paused; `null` when nothing is. */
  sourceId: string | null;
  status: SpeechStatus;
  error: string | null;
}

/** The parts of the Web Audio API the player uses. */
export interface AudioOutput {
  readonly currentTime: number;
  readonly state: string;
  resume(): Promise<void>;
  suspend(): Promise<void>;
  createBuffer(channels: number, length: number, sampleRate: number): AudioBuffer;
  createBufferSource(): AudioBufferSourceNode;
  readonly destination: AudioNode;
}

const LEAD_SECONDS = 20;
const IDLE: SpeechSnapshot = { sourceId: null, status: 'idle', error: null };

export function decodeClip(output: AudioOutput, clip: SpeechClip): AudioBuffer {
  const bytes = Uint8Array.from(atob(clip.pcm), (c) => c.charCodeAt(0));
  const view = new DataView(bytes.buffer);
  const samples = new Float32Array(bytes.length >> 1);
  for (let i = 0; i < samples.length; i += 1) samples[i] = view.getInt16(i * 2, true) / 32768;
  const buffer = output.createBuffer(1, Math.max(samples.length, 1), clip.sampleRate);
  buffer.copyToChannel(samples, 0);
  return buffer;
}

interface Session {
  sourceId: string;
  cancelled: boolean;
  nodes: AudioBufferSourceNode[];
  /** When the last scheduled piece ends, on the output's clock. */
  endsAt: number;
}

export class SpeechPlayer {
  private output: AudioOutput | null = null;
  private session: Session | null = null;
  private snapshot: SpeechSnapshot = IDLE;
  private readonly listeners = new Set<() => void>();

  constructor(private readonly createOutput: () => AudioOutput) {}

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): SpeechSnapshot => this.snapshot;

  private publish(next: SpeechSnapshot) {
    this.snapshot = next;
    this.listeners.forEach((listener) => listener());
  }

  /** Speaks `pieces` as `sourceId`, stopping whatever was speaking. */
  async play(sourceId: string, pieces: readonly string[], synthesize: Synthesize): Promise<void> {
    this.stop();
    if (pieces.length === 0) return;
    this.output ??= this.createOutput();
    const output = this.output;
    const session: Session = { sourceId, cancelled: false, nodes: [], endsAt: 0 };
    this.session = session;
    this.publish({ sourceId, status: 'loading', error: null });
    if (output.state === 'suspended') await output.resume();

    try {
      for (const [index, piece] of pieces.entries()) {
        while (!session.cancelled && session.endsAt - output.currentTime > LEAD_SECONDS) {
          await new Promise((resolve) => setTimeout(resolve, 500));
        }
        if (session.cancelled) return;
        const clip = await synthesize(piece);
        if (session.cancelled) return;
        this.schedule(session, decodeClip(output, clip), index === pieces.length - 1);
      }
    } catch (error) {
      if (session.cancelled) return;
      this.stop();
      this.publish({ ...IDLE, error: error instanceof Error ? error.message : String(error) });
    }
  }

  private schedule(session: Session, buffer: AudioBuffer, last: boolean) {
    const output = this.output;
    if (!output) return;
    const node = output.createBufferSource();
    node.buffer = buffer;
    node.connect(output.destination);
    const startAt = Math.max(output.currentTime, session.endsAt);
    session.endsAt = startAt + buffer.duration;
    session.nodes.push(node);
    node.onended = () => {
      session.nodes = session.nodes.filter((n) => n !== node);
      if (last && this.session === session) {
        this.session = null;
        this.publish(IDLE);
      }
    };
    node.start(startAt);
    if (this.snapshot.status === 'loading') this.publish({ sourceId: session.sourceId, status: 'playing', error: null });
  }

  pause() {
    if (!this.session || !this.output || this.snapshot.status !== 'playing') return;
    void this.output.suspend();
    this.publish({ ...this.snapshot, status: 'paused' });
  }

  resume() {
    if (!this.session || !this.output || this.snapshot.status !== 'paused') return;
    void this.output.resume();
    this.publish({ ...this.snapshot, status: 'playing' });
  }

  /** Stops speaking; a source that was never this player's is left alone. */
  stop(sourceId?: string) {
    const session = this.session;
    if (!session || (sourceId !== undefined && session.sourceId !== sourceId)) return;
    session.cancelled = true;
    for (const node of session.nodes) {
      node.onended = null;
      try {
        node.stop();
      } catch {
        // Already stopped.
      }
    }
    session.nodes = [];
    this.session = null;
    if (this.output?.state === 'suspended') void this.output.resume();
    this.publish(IDLE);
  }
}

/** The app's one player. */
export const speechPlayer = new SpeechPlayer(() => new AudioContext());
