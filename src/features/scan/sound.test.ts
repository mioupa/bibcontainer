import { describe, expect, it, vi } from "vitest";
import {
  createSoundPlayer,
  gainScale,
  SOUND_TONES,
  type AudioContextLike,
} from "@/features/scan/sound";

function fakeContext(state = "running") {
  const oscillators: {
    type: string;
    frequency: { value: number };
    start: ReturnType<typeof vi.fn>;
    stop: ReturnType<typeof vi.fn>;
  }[] = [];
  const gains: {
    gain: {
      setValueAtTime: ReturnType<typeof vi.fn>;
      linearRampToValueAtTime: ReturnType<typeof vi.fn>;
      exponentialRampToValueAtTime: ReturnType<typeof vi.fn>;
    };
  }[] = [];
  const context = {
    currentTime: 10,
    destination: {},
    state,
    resume: vi.fn(() => Promise.resolve()),
    createOscillator: vi.fn(() => {
      const osc = {
        type: "sine",
        frequency: { value: 0 },
        connect: vi.fn(),
        start: vi.fn(),
        stop: vi.fn(),
      };
      oscillators.push(osc);
      return osc;
    }),
    createGain: vi.fn(() => {
      const g = {
        gain: {
          setValueAtTime: vi.fn(),
          linearRampToValueAtTime: vi.fn(),
          exponentialRampToValueAtTime: vi.fn(),
        },
        connect: vi.fn(),
      };
      gains.push(g);
      return g;
    }),
  };
  return { context: context as unknown as AudioContextLike & typeof context, oscillators, gains };
}

describe("通知音（FR-SCAN-11）", () => {
  it("3 種類の音がある。聞き分けられるよう、周波数・音色・音数が違う", () => {
    expect(Object.keys(SOUND_TONES).sort()).toEqual(["attention", "error", "success"]);
    const signature = (kind: keyof typeof SOUND_TONES) =>
      JSON.stringify(SOUND_TONES[kind].map((t) => [t.frequency, t.type]));
    expect(new Set([signature("success"), signature("error"), signature("attention")]).size).toBe(
      3,
    );
    // 成功は音が上がる。エラーは低い
    const [s1, s2] = SOUND_TONES.success;
    expect(s2!.frequency).toBeGreaterThan(s1!.frequency);
    expect(SOUND_TONES.error[0]!.frequency).toBeLessThan(s1!.frequency);
  });

  it("音ごとに、トーンの数だけ発振器を作って鳴らす", () => {
    const { context, oscillators } = fakeContext();
    const player = createSoundPlayer(() => context);
    for (const kind of ["success", "error", "attention"] as const) {
      oscillators.length = 0;
      player.play(kind, { volume: 0.5, muted: false });
      expect(oscillators).toHaveLength(SOUND_TONES[kind].length);
      expect(oscillators.map((o) => o.frequency.value)).toEqual(
        SOUND_TONES[kind].map((t) => t.frequency),
      );
      for (const osc of oscillators) {
        expect(osc.start).toHaveBeenCalledOnce();
        expect(osc.stop).toHaveBeenCalledOnce();
      }
    }
  });

  it("ミュートか音量 0 のときは、何も鳴らさず、AudioContext も作らない", () => {
    const create = vi.fn(() => fakeContext().context);
    const player = createSoundPlayer(create);
    player.play("success", { volume: 0.5, muted: true });
    player.play("success", { volume: 0, muted: false });
    expect(create).not.toHaveBeenCalled();
  });

  it("音量は大きさ（gain）に反映される。大きい音量ほど大きい", () => {
    const peak = (volume: number) => {
      const { context, gains } = fakeContext();
      createSoundPlayer(() => context).play("attention", { volume, muted: false });
      return gains[0]!.gain.linearRampToValueAtTime.mock.calls[0]![0] as number;
    };
    expect(peak(1)).toBeGreaterThan(peak(0.5));
    expect(peak(0.5)).toBeGreaterThan(peak(0.1));
    expect(gainScale(0)).toBe(0);
    expect(gainScale(2)).toBe(gainScale(1));
    expect(gainScale(-1)).toBe(0);
  });

  it("AudioContext は 1 度だけ作り、止まっていれば再開する", () => {
    const { context } = fakeContext("suspended");
    const create = vi.fn(() => context);
    const player = createSoundPlayer(create);
    player.play("success", { volume: 1, muted: false });
    player.play("error", { volume: 1, muted: false });
    expect(create).toHaveBeenCalledOnce();
    expect(context.resume).toHaveBeenCalled();
  });

  describe("unlock（自動再生の制限の解除）", () => {
    /** resume() すると running になる AudioContext。 */
    function lockedContext() {
      const fake = fakeContext("suspended");
      fake.context.resume.mockImplementation(() => {
        (fake.context as { state: string }).state = "running";
        return Promise.resolve();
      });
      return fake;
    }

    it("まだ鳴らしていなくても、AudioContext を作って再開する（鳴らす前に、ユーザーの操作の中で）", async () => {
      const { context } = lockedContext();
      const create = vi.fn(() => context);
      const player = createSoundPlayer(create);
      expect(create).not.toHaveBeenCalled();

      await expect(player.unlock()).resolves.toBe(true);
      expect(create).toHaveBeenCalledOnce();
      expect(context.resume).toHaveBeenCalledOnce();
      expect(context.state).toBe("running");
    });

    it("unlock のあとに鳴らすと、同じ AudioContext を使い、もう再開しない", async () => {
      const { context, oscillators } = lockedContext();
      const create = vi.fn(() => context);
      const player = createSoundPlayer(create);
      await player.unlock();
      player.play("success", { volume: 1, muted: false });
      expect(create).toHaveBeenCalledOnce();
      expect(context.resume).toHaveBeenCalledOnce();
      expect(oscillators).toHaveLength(SOUND_TONES.success.length);
    });

    it("ミュートでも unlock する（あとでミュートを外したとき、最初から鳴らせるように）", async () => {
      const { context } = lockedContext();
      const player = createSoundPlayer(() => context);
      await expect(player.unlock()).resolves.toBe(true);
    });

    it("すでに running なら、再開しない。再開できなかったとき（まだ操作として数えられない）は false", async () => {
      const running = fakeContext("running");
      await expect(createSoundPlayer(() => running.context).unlock()).resolves.toBe(true);
      expect(running.context.resume).not.toHaveBeenCalled();

      const stuck = fakeContext("suspended"); // resume() しても suspended のまま
      await expect(createSoundPlayer(() => stuck.context).unlock()).resolves.toBe(false);
      expect(stuck.context.resume).toHaveBeenCalledOnce();
    });

    it("作れない環境・例外でも、例外を投げず false", async () => {
      await expect(createSoundPlayer(() => null).unlock()).resolves.toBe(false);
      const broken = fakeContext("suspended");
      broken.context.resume.mockRejectedValue(new Error("not allowed"));
      await expect(createSoundPlayer(() => broken.context).unlock()).resolves.toBe(false);
      const throwing = vi.fn(() => {
        throw new Error("no audio");
      });
      await expect(createSoundPlayer(throwing).unlock()).resolves.toBe(false);
    });
  });

  it("鳴らせない環境（AudioContext がない・例外）でも、例外を投げない", () => {
    expect(() =>
      createSoundPlayer(() => null).play("success", { volume: 1, muted: false }),
    ).not.toThrow();
    const broken = fakeContext();
    broken.context.createOscillator.mockImplementation(() => {
      throw new Error("boom");
    });
    expect(() =>
      createSoundPlayer(() => broken.context).play("error", { volume: 1, muted: false }),
    ).not.toThrow();
  });
});
