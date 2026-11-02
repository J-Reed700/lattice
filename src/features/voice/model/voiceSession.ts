import type { GenerationOutcome } from '@/shared/conversations/conversationsStore.types';

import { MicError, type MicHandlers, type OpenMic } from './micInput';
import { ReplyReader, SpokenPieces } from './replySpeech';
import { SpeechDetector } from './speechDetector';

export type VoicePhase = 'off' | 'starting' | 'listening' | 'hearing' | 'transcribing' | 'thinking' | 'speaking';

/** What a voice conversation does in the chat and on the computer around it. */
export interface VoiceSessionDeps {
  /** False stops the session; whatever said why has already said so. */
  ready: () => Promise<boolean>;
  openMic: (_handlers: MicHandlers) => Promise<OpenMic>;
  transcribe: (_samples: Float32Array) => Promise<string>;
  /** Sends a turn and settles when it is over. Throws when it could not be sent. */
  send: (_text: string) => Promise<GenerationOutcome>;
  /** The reply as saved, once a turn has answered. */
  finalReply: () => string;
  /** Asks the turn in flight to stop. */
  interruptTurn: () => void;
  /** Settles when this chat can take a turn. */
  whenFree: () => Promise<void>;
  /** Says `pieces` and settles when they have been heard, or were stopped. */
  speak: (_pieces: AsyncIterable<string>) => Promise<void>;
  stopSpeaking: () => void;
  /** Anything is playing, the reader's own Listen button included. */
  isSpeaking: () => boolean;
  onPhase: (_phase: VoicePhase) => void;
  onFailure: (_title: string, _message: string) => void;
}

interface Turn {
  reader: ReplyReader;
  pieces: SpokenPieces;
  speaking: Promise<void> | null;
  /** The turn has gone out, so text arriving now is its reply. */
  sent: boolean;
}

const reason = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** Whisper writes what isn't speech in brackets: `[BLANK_AUDIO]`, `(wind blowing)`. */
const NOTHING_SAID = /^(?:\s|\[[^\]]*\]|\([^)]*\)|♪)*$/;

/**
 * One voice conversation, from the mic being pressed to it being pressed again:
 * listen, hear an utterance, turn it into text, send the turn, say the reply as
 * it is written, and listen again. Speaking over a reply stops it and starts
 * hearing what was said.
 */
export class VoiceSession {
  private phase: VoicePhase = 'off';
  private mic: OpenMic | null = null;
  private turn: Turn | null = null;
  private readonly detector = new SpeechDetector();

  constructor(private readonly deps: VoiceSessionDeps) {}

  private enter(phase: VoicePhase) {
    this.phase = phase;
    this.deps.onPhase(phase);
  }

  /** Whether stop or a failure ended things while something was being awaited. */
  private get ended() {
    return this.phase === 'off';
  }

  private listen() {
    this.detector.reset();
    this.enter('listening');
  }

  async start() {
    this.enter('starting');
    try {
      if (!(await this.deps.ready())) {
        this.stop();
        return;
      }
      if (this.ended) return;
      const mic = await this.deps.openMic({ onFrame: (frame) => this.hear(frame), onLost: () => this.lostMic() });
      if (this.ended) {
        mic.stop();
        return;
      }
      this.mic = mic;
      this.listen();
    } catch (error) {
      if (this.ended) return;
      this.fail(error instanceof MicError ? "Couldn't use the microphone" : "Couldn't start the voice conversation", reason(error));
    }
  }

  /** Ends the conversation. A reply still being written carries on in the chat, unspoken. */
  stop() {
    if (this.phase === 'off') return;
    this.enter('off');
    this.mic?.stop();
    this.mic = null;
    this.detector.reset();
    this.dropTurn();
  }

  private fail(title: string, message: string) {
    this.stop();
    this.deps.onFailure(title, message);
  }

  private lostMic() {
    this.fail('The microphone stopped', 'Check it is connected, then press the mic to start again.');
  }

  private dropTurn() {
    if (!this.turn) return;
    this.turn.pieces.abandon();
    this.turn = null;
    this.deps.stopSpeaking();
  }

  private hear(frame: Float32Array) {
    if (this.phase === 'listening' || this.phase === 'speaking') {
      if (this.detector.push(frame, this.phase === 'speaking' || this.deps.isSpeaking()) !== 'started') return;
      if (this.phase === 'speaking') {
        this.dropTurn();
        this.deps.interruptTurn();
      }
      this.enter('hearing');
    } else if (this.phase === 'hearing' && this.detector.push(frame) === 'ended') {
      void this.transcribe(this.detector.take());
    }
  }

  private async transcribe(samples: Float32Array) {
    this.enter('transcribing');
    let text: string;
    try {
      text = await this.deps.transcribe(samples);
    } catch (error) {
      if (this.phase === 'transcribing') this.fail("Couldn't work out what you said", reason(error));
      return;
    }
    if (this.phase !== 'transcribing') return;
    if (NOTHING_SAID.test(text)) {
      this.listen();
      return;
    }
    await this.answer(text.trim());
  }

  private async answer(text: string) {
    const turn: Turn = { reader: new ReplyReader(), pieces: new SpokenPieces(), speaking: null, sent: false };
    this.turn = turn;
    this.enter('thinking');
    let outcome: GenerationOutcome;
    try {
      await this.deps.whenFree();
      if (this.turn !== turn) return;
      turn.sent = true;
      outcome = await this.deps.send(text);
    } catch (error) {
      if (this.turn === turn) this.fail("Couldn't send what you said", reason(error));
      return;
    }
    if (this.turn !== turn) return;

    if (outcome === 'failed') {
      this.fail('The reply didn’t finish', 'The chat says what went wrong. Press the mic to carry on.');
      return;
    }
    if (outcome === 'answered') {
      this.say(turn, turn.reader.finish(this.deps.finalReply()));
      turn.pieces.close();
      await turn.speaking;
      if (this.turn !== turn) return;
    } else {
      this.dropTurn();
    }
    this.turn = null;
    this.listen();
  }

  private say(turn: Turn, sentences: string[]) {
    if (sentences.length === 0) return;
    turn.pieces.push(sentences);
    if (turn.speaking) return;
    turn.speaking = this.deps.speak(turn.pieces);
    this.enter('speaking');
  }

  /** The reply being written, as far as it has got. */
  replyGrew(markdown: string) {
    if (this.turn?.sent) this.say(this.turn, this.turn.reader.update(markdown));
  }
}
