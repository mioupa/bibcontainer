// 通知音（FR-SCAN-11）。Web Audio API で合成する（音声ファイルを同梱しない。0005 §9）。
// 3 種類: 成功（高い 2 音が上がる）、エラー（低い音を 2 回）、注意（中くらいの音を 2 回）。聞き分けやすいよう、音色と長さも変える。
import type { SoundKind, SoundPlayback } from "@/features/scan/config";

export interface Tone {
  /** 周波数（Hz）。 */
  frequency: number;
  /** 鳴り始めの時刻（秒。再生開始からの相対）。 */
  start: number;
  /** 長さ（秒）。 */
  duration: number;
  type: OscillatorType;
  /** 最大の大きさ（0〜1）。音量の設定を掛ける前の値。 */
  gain: number;
}

export const SOUND_TONES: Record<SoundKind, readonly Tone[]> = {
  success: [
    { frequency: 880, start: 0, duration: 0.09, type: "sine", gain: 0.5 },
    { frequency: 1319, start: 0.09, duration: 0.14, type: "sine", gain: 0.5 },
  ],
  error: [
    { frequency: 196, start: 0, duration: 0.18, type: "sawtooth", gain: 0.35 },
    { frequency: 196, start: 0.24, duration: 0.3, type: "sawtooth", gain: 0.35 },
  ],
  attention: [
    { frequency: 659, start: 0, duration: 0.1, type: "triangle", gain: 0.5 },
    { frequency: 659, start: 0.16, duration: 0.1, type: "triangle", gain: 0.5 },
  ],
};

/** 音量の設定（0〜1）から、実際の大きさの倍率を求める。人の耳に合わせて 2 乗にし、最大でも 0.8 倍にする（割れを避ける）。 */
export function gainScale(volume: number): number {
  const v = Math.min(1, Math.max(0, volume));
  return v * v * 0.8;
}

/** Web Audio の、使う部分だけの型（テストで差し替えやすくする）。 */
export interface AudioContextLike {
  readonly currentTime: number;
  readonly destination: unknown;
  readonly state: string;
  resume: () => Promise<void>;
  createOscillator: () => {
    type: OscillatorType;
    frequency: { value: number };
    connect: (node: unknown) => void;
    start: (when: number) => void;
    stop: (when: number) => void;
  };
  createGain: () => {
    gain: {
      setValueAtTime: (value: number, when: number) => void;
      linearRampToValueAtTime: (value: number, when: number) => void;
      exponentialRampToValueAtTime: (value: number, when: number) => void;
    };
    connect: (node: unknown) => void;
  };
}

export interface SoundPlayer {
  /** 音を鳴らす。ミュートか音量 0 なら何もしない。鳴らせない環境でも例外は投げない。 */
  play: (kind: SoundKind, settings: SoundPlayback) => void;
}

/** AudioContext は最初に鳴らすとき（ユーザーの操作の中）に作る。作れない環境では黙る。 */
export function createSoundPlayer(
  createContext: () => AudioContextLike | null = defaultCreateContext,
): SoundPlayer {
  let context: AudioContextLike | null | undefined;

  return {
    play(kind, settings) {
      if (settings.muted) return;
      const scale = gainScale(settings.volume);
      if (scale <= 0) return;
      try {
        context ??= createContext();
        if (!context) return;
        // 自動再生の制限で止まっているときは再開する（スキャンのキー入力はユーザーの操作として扱われる）
        if (context.state === "suspended") void context.resume();
        const base = context.currentTime;
        for (const tone of SOUND_TONES[kind]) {
          const oscillator = context.createOscillator();
          const envelope = context.createGain();
          const at = base + tone.start;
          const peak = tone.gain * scale;
          oscillator.type = tone.type;
          oscillator.frequency.value = tone.frequency;
          // 立ち上がりと終わりを滑らかにして、プツッというノイズを避ける
          envelope.gain.setValueAtTime(0.0001, at);
          envelope.gain.linearRampToValueAtTime(Math.max(peak, 0.0001), at + 0.01);
          envelope.gain.exponentialRampToValueAtTime(0.0001, at + tone.duration);
          oscillator.connect(envelope);
          envelope.connect(context.destination);
          oscillator.start(at);
          oscillator.stop(at + tone.duration + 0.02);
        }
      } catch {
        // 音が鳴らせなくても、登録の処理は止めない
      }
    },
  };
}

function defaultCreateContext(): AudioContextLike | null {
  const Ctor = globalThis.AudioContext as (new () => AudioContextLike) | undefined;
  return Ctor ? new Ctor() : null;
}

/** アプリ全体で 1 つ使う再生器。 */
export const soundPlayer = createSoundPlayer();
