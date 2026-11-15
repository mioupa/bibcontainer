// カメラの映像を読んで、バーコードの値をスキャン登録の流れに渡すフック（FR-SCAN-20。0029）。
//
// - `on` が真で、ウィンドウが見えている間だけ、カメラを開く。切ったとき・隠れたとき・画面を離れたときは、
//   すべての track を stop して、カメラを放す（隠れて止めたあと、戻ったら、オンのままなら再開する）。
// - getUserMedia が、止めたあとに（アンマウント後など）届いたら、すぐ stop する。StrictMode の二重マウントも同じ。
// - 読み取りは、前のが終わるまで次を始めず、およそ 150ms おきに行う（フレームは requestVideoFrameCallback、
//   なければ requestAnimationFrame で取る）。値を渡すかどうかは dedupe.ts。
import { useCallback, useEffect, useRef, useState } from "react";
import { INITIAL_DEDUPE, stepDedupe } from "@/features/scanScreen/camera/dedupe";
import { loadDecoder, readFrame } from "@/features/scanScreen/camera/decode";
import {
  cameraSupported,
  classifyCameraError,
  isMissingDeviceError,
  type CameraErrorKind,
} from "@/features/scanScreen/camera/errors";

export type CameraStatus = "off" | "starting" | "running" | "error";

export interface CameraDevice {
  deviceId: string;
  label: string;
}

/** 読み取りの間隔（ミリ秒）。 */
const SCAN_INTERVAL_MS = 150;
/** 続けて読み取りに失敗したら、読む部品が壊れたものとして止める回数。 */
const MAX_READ_FAILURES = 5;
/** 値を渡したときの枠の色を戻すまでの時間（ミリ秒）。 */
const FLASH_MS = 400;

interface UseCameraScanOptions {
  /** 映像を出す `<video>`（呼び出し側が持つ。いつも画面に置いておく）。 */
  videoRef: React.RefObject<HTMLVideoElement | null>;
  on: boolean;
  /** 覚えていたカメラ。null は既定のカメラ。 */
  deviceId: string | null;
  /** 値を渡してよいか。偽の間に読めた値は捨てる。 */
  accepting: boolean;
  onDigits: (digits: string) => void;
}

export interface CameraScan {
  status: CameraStatus;
  error: CameraErrorKind | null;
  devices: CameraDevice[];
  /** いま映しているカメラの deviceId。 */
  activeDeviceId: string | null;
  /** 値を渡した直後の短い間だけ真（枠の色を変える）。 */
  flash: boolean;
}

function useDocumentHidden(): boolean {
  const [hidden, setHidden] = useState(() => document.hidden);
  useEffect(() => {
    const update = () => setHidden(document.hidden);
    document.addEventListener("visibilitychange", update);
    return () => document.removeEventListener("visibilitychange", update);
  }, []);
  return hidden;
}

function stopStream(stream: MediaStream | null): void {
  stream?.getTracks().forEach((track) => track.stop());
}

/** 覚えていたカメラで開く。そのカメラがなければ、既定のカメラでやり直す。 */
async function openStream(deviceId: string | null): Promise<MediaStream> {
  const base = { width: { ideal: 1280 }, height: { ideal: 720 } };
  const devices = navigator.mediaDevices;
  if (deviceId === null) return devices.getUserMedia({ video: base, audio: false });
  try {
    return await devices.getUserMedia({
      video: { ...base, deviceId: { exact: deviceId } },
      audio: false,
    });
  } catch (error) {
    if (!isMissingDeviceError(error)) throw error;
    return devices.getUserMedia({ video: base, audio: false });
  }
}

function requestFrame(video: HTMLVideoElement, callback: () => void): () => void {
  if (typeof video.requestVideoFrameCallback === "function") {
    const id = video.requestVideoFrameCallback(callback);
    return () => video.cancelVideoFrameCallback(id);
  }
  const id = requestAnimationFrame(callback);
  return () => cancelAnimationFrame(id);
}

/** 読み取りの繰り返しを始める。返す関数で止める。 */
function startLoop(
  video: HTMLVideoElement,
  onValues: (values: string[]) => void,
  onBroken: () => void,
): () => void {
  let stopped = false;
  let cancelFrame = () => {};
  let timer: ReturnType<typeof setTimeout> | undefined;
  let failures = 0;

  const tick = () => {
    const startedAt = performance.now();
    void (async () => {
      let values: string[] = [];
      try {
        values = await readFrame(video);
        failures = 0;
      } catch {
        failures += 1;
        if (failures >= MAX_READ_FAILURES && !stopped) {
          onBroken();
          return;
        }
      }
      if (stopped) return;
      onValues(values);
      const wait = Math.max(0, SCAN_INTERVAL_MS - (performance.now() - startedAt));
      timer = setTimeout(() => {
        if (!stopped) cancelFrame = requestFrame(video, tick);
      }, wait);
    })();
  };

  cancelFrame = requestFrame(video, tick);
  return () => {
    stopped = true;
    cancelFrame();
    clearTimeout(timer);
  };
}

export function useCameraScan({
  videoRef,
  on,
  deviceId,
  accepting,
  onDigits,
}: UseCameraScanOptions): CameraScan {
  const [status, setStatus] = useState<CameraStatus>("off");
  const [error, setError] = useState<CameraErrorKind | null>(null);
  const [devices, setDevices] = useState<CameraDevice[]>([]);
  const [activeDeviceId, setActiveDeviceId] = useState<string | null>(null);
  const [flash, setFlash] = useState(false);
  const hidden = useDocumentHidden();

  // 読み取りの繰り返しが、いつも最新の値を見るように
  const acceptingRef = useRef(accepting);
  const onDigitsRef = useRef(onDigits);
  useEffect(() => {
    acceptingRef.current = accepting;
    onDigitsRef.current = onDigits;
  });

  const flashTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const flashNow = useCallback(() => {
    setFlash(true);
    clearTimeout(flashTimer.current);
    flashTimer.current = setTimeout(() => setFlash(false), FLASH_MS);
  }, []);
  useEffect(() => () => clearTimeout(flashTimer.current), []);

  useEffect(() => {
    if (!on || hidden) return;
    let cancelled = false;
    let stream: MediaStream | null = null;
    let stopLoop = () => {};
    const video = videoRef.current;

    const release = () => {
      stopLoop();
      stopLoop = () => {};
      stopStream(stream);
      stream = null;
      if (video !== null) video.srcObject = null;
    };
    const fail = (kind: CameraErrorKind) => {
      release();
      if (cancelled) return;
      setError(kind);
      setStatus("error");
    };

    void (async () => {
      setStatus("starting");
      setError(null);
      if (video === null) return fail("other");
      if (!cameraSupported()) return fail("unsupported");

      let opened: MediaStream;
      try {
        opened = await openStream(deviceId);
      } catch (cause) {
        return fail(classifyCameraError(cause));
      }
      // 待っている間に止められていたら、開いたカメラをすぐ放す
      if (cancelled) {
        stopStream(opened);
        return;
      }
      stream = opened;

      // カメラが外されたり、ほかのアプリに取られたりしたとき
      for (const track of opened.getVideoTracks()) {
        track.addEventListener("ended", () => fail("notFound"));
      }

      video.srcObject = opened;
      try {
        await video.play();
      } catch {
        // 自動再生が許されなくても、muted の映像は、たいてい映る。読み取りは映像が来てから動く
      }
      if (cancelled) return release();
      setActiveDeviceId(opened.getVideoTracks()[0]?.getSettings().deviceId ?? null);

      // 一覧は、許可を得たあとでないと、名前が空になる
      try {
        const all = await navigator.mediaDevices.enumerateDevices();
        if (cancelled) return release();
        setDevices(
          all
            .filter((device) => device.kind === "videoinput" && device.deviceId !== "")
            .map((device) => ({ deviceId: device.deviceId, label: device.label })),
        );
      } catch {
        // 一覧を取れなくても、いまのカメラで読める
      }

      try {
        await loadDecoder();
      } catch {
        return fail("decoder");
      }
      if (cancelled) return release();

      // 値を渡すかどうかの状態は、カメラを開くたびに初めから
      let dedupe = INITIAL_DEDUPE;
      stopLoop = startLoop(
        video,
        (values) => {
          const result = stepDedupe(dedupe, values, performance.now(), acceptingRef.current);
          dedupe = result.state;
          for (const digits of result.emit) onDigitsRef.current(digits);
          if (result.emit.length > 0) flashNow();
        },
        () => fail("decoder"),
      );
      setStatus("running");
    })();

    return () => {
      cancelled = true;
      release();
    };
  }, [videoRef, on, hidden, deviceId, flashNow]);

  return {
    status: on ? status : "off",
    error: on ? error : null,
    devices,
    activeDeviceId,
    flash: on && flash,
  };
}
