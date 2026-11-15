// カメラの映像 1 フレームから、EAN-13 のバーコードの値を読む（zxing-wasm）。薄い関数に分けてあり、テストでは差し替える。
//
// - zxing の JS と wasm は、カメラをオンにしたとき（`loadDecoder`）に初めて読み込む（動的 import）。
//   起動時のバンドルには入れない。
// - wasm はアプリに同梱する（`?url` で Vite がファイルにして dist に出す）。zxing-wasm の既定は
//   jsDelivr から取りに行くので、`locateFile` で必ず上書きする。外部には通信しない（0029 §2）。
// - メインスレッドで動かす。Worker は使わない（CSP の worker-src を足さずに済ませる）。
import type * as ZXingReader from "zxing-wasm/reader";

/** 映像を縮めたあとの幅の上限（ピクセル）。EAN-13 の読み取りには十分で、読み取りが軽くなる。 */
const MAX_WIDTH = 960;
/** 映像の高さのうち、読む中央の帯の割合。上下 2 段（ISBN と 2段目）が入る高さを残す。 */
const BAND_RATIO = 0.6;

type Reader = typeof ZXingReader;

let loading: Promise<Reader> | null = null;

/** zxing と wasm を読み込む。2 回目以降は同じ結果を返す。失敗したら、次の呼び出しでやり直す。 */
export function loadDecoder(): Promise<Reader> {
  loading ??= (async () => {
    const [reader, wasm] = await Promise.all([
      import("zxing-wasm/reader"),
      import("zxing-wasm/reader/zxing_reader.wasm?url"),
    ]);
    await reader.prepareZXingModule({
      overrides: {
        locateFile: (path: string, prefix: string) =>
          path.endsWith(".wasm") ? wasm.default : prefix + path,
      },
      fireImmediately: true,
    });
    return reader;
  })().catch((error: unknown) => {
    loading = null;
    throw error;
  });
  return loading;
}

let canvas: HTMLCanvasElement | null = null;

/** 映像の中央の帯を、縮めて ImageData にする。映像がまだ来ていなければ null。 */
function captureBand(video: HTMLVideoElement): ImageData | null {
  const { videoWidth, videoHeight } = video;
  if (videoWidth === 0 || videoHeight === 0) return null;
  const scale = Math.min(1, MAX_WIDTH / videoWidth);
  const width = Math.round(videoWidth * scale);
  const bandHeight = videoHeight * BAND_RATIO;
  const height = Math.round(bandHeight * scale);

  canvas ??= document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (context === null) return null;
  const top = (videoHeight - bandHeight) / 2;
  context.drawImage(video, 0, top, videoWidth, bandHeight, 0, 0, width, height);
  return context.getImageData(0, 0, width, height);
}

/** 映像の今のフレームから、読めた EAN-13 の値（13 桁の数字）を返す。読めなければ空。 */
export async function readFrame(video: HTMLVideoElement): Promise<string[]> {
  const image = captureBand(video);
  if (image === null) return [];
  const reader = await loadDecoder();
  const results = await reader.readBarcodes(image, {
    formats: ["EAN13"],
    tryHarder: true,
    // 上段と下段が同時に写る場合のため、2 以上にする。余裕を見て 4
    maxNumberOfSymbols: 4,
    // 2段目（192）は別の EAN-13。EAN-2・5 のアドオンは使わない
    eanAddOnSymbol: "Ignore",
  });
  return results.filter((result) => result.isValid).map((result) => result.text);
}
