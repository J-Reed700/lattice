import micCaptureWorklet from './micCapture.worklet?worker&url';
import { MIC_CAPTURE_PROCESSOR } from './micProcessor';

export type MicFailure = 'denied' | 'missing' | 'busy' | 'unsupported' | 'failed';

const FAILURE_MESSAGES: Record<MicFailure, string> = {
  denied:
    'Lattice isn’t allowed to use the microphone. Turn it on in your system’s privacy settings (on Windows, Settings › Privacy & security › Microphone), then press the mic again.',
  missing: 'No microphone was found. Connect one and press the mic again.',
  busy: 'The system wouldn’t hand over the microphone. Another app may have it, or it may be off in your privacy settings (on Windows, Settings › Privacy & security › Microphone).',
  unsupported: 'This window can’t reach a microphone on this computer.',
  failed: 'The microphone couldn’t start. Press the mic to try again.',
};

/** Why the microphone can't be used, in words for the reader. */
export class MicError extends Error {
  constructor(readonly failure: MicFailure) {
    super(FAILURE_MESSAGES[failure]);
    this.name = 'MicError';
  }
}

function failureOf(error: unknown): MicFailure {
  const name = error instanceof Error ? error.name : '';
  if (name === 'NotAllowedError' || name === 'SecurityError') return 'denied';
  if (name === 'NotFoundError' || name === 'OverconstrainedError') return 'missing';
  if (name === 'NotReadableError' || name === 'AbortError') return 'busy';
  return 'failed';
}

/** The echo canceller is what lets the mic stay open while a reply plays. */
const SPEECH_CONSTRAINTS: MediaStreamConstraints = {
  audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 },
  video: false,
};

export interface MicHandlers {
  /** One frame of 16 kHz mono PCM. */
  onFrame: (_frame: Float32Array) => void;
  /** The microphone went away: unplugged, or taken back in the system's settings. */
  onLost: () => void;
}

export interface OpenMic {
  stop: () => void;
}

interface MicEnvironment {
  media?: Pick<MediaDevices, 'getUserMedia'>;
  createContext: () => AudioContext;
  createCapture: (_context: AudioContext) => AudioWorkletNode;
}

const browser = (): MicEnvironment => ({
  media: navigator.mediaDevices,
  createContext: () => new AudioContext(),
  createCapture: (context) => new AudioWorkletNode(context, MIC_CAPTURE_PROCESSOR),
});

/** Opens the microphone, or throws a {@link MicError} saying why it can't. */
export async function openMic({ onFrame, onLost }: MicHandlers, environment: MicEnvironment = browser()): Promise<OpenMic> {
  if (!environment.media) throw new MicError('unsupported');
  let stream: MediaStream;
  try {
    stream = await environment.media.getUserMedia(SPEECH_CONSTRAINTS);
  } catch (error) {
    throw new MicError(failureOf(error));
  }

  const tracks = stream.getAudioTracks();
  let context: AudioContext | null = null;
  const release = () => {
    for (const track of tracks) {
      track.onended = null;
      track.stop();
    }
    void context?.close();
  };

  try {
    context = environment.createContext();
    await context.audioWorklet.addModule(micCaptureWorklet);
    if (context.state === 'suspended') await context.resume();
    const source = context.createMediaStreamSource(stream);
    const capture = environment.createCapture(context);
    capture.port.onmessage = (event: MessageEvent<Float32Array>) => onFrame(event.data);
    source.connect(capture);
    // A node with nothing downstream is never pulled. It writes no output, so this is silent.
    capture.connect(context.destination);
    for (const track of tracks) track.onended = onLost;

    return {
      stop: () => {
        capture.port.onmessage = null;
        source.disconnect();
        capture.disconnect();
        release();
      },
    };
  } catch {
    release();
    throw new MicError('failed');
  }
}
