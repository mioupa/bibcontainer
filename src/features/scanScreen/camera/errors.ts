// カメラを開くときの失敗の種類。getUserMedia が投げる DOMException の name から分ける（0029 §7）。

export type CameraErrorKind = "unsupported" | "denied" | "notFound" | "inUse" | "decoder" | "other";

function errorName(error: unknown): string {
  return typeof error === "object" && error !== null && "name" in error ? String(error.name) : "";
}

/** getUserMedia の失敗を、画面に出す種類に分ける。 */
export function classifyCameraError(error: unknown): CameraErrorKind {
  switch (errorName(error)) {
    case "NotAllowedError":
    case "SecurityError":
    case "PermissionDeniedError":
      return "denied";
    case "NotFoundError":
    case "DevicesNotFoundError":
      return "notFound";
    case "NotReadableError":
    case "TrackStartError":
    case "AbortError":
      return "inUse";
    default:
      return "other";
  }
}

/** 覚えていたカメラが使えない失敗か（そのカメラを外した、など）。既定のカメラでやり直す。 */
export function isMissingDeviceError(error: unknown): boolean {
  const name = errorName(error);
  return name === "OverconstrainedError" || name === "NotFoundError";
}

/** このウィンドウでカメラを使える作りか。安全な接続（secure context）で、メディアの API があること。 */
export function cameraSupported(): boolean {
  if (typeof window !== "undefined" && window.isSecureContext === false) return false;
  return (
    typeof navigator !== "undefined" && typeof navigator.mediaDevices?.getUserMedia === "function"
  );
}
