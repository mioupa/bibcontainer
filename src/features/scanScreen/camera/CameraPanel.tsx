// 「カメラで読む」のカード（FR-SCAN-20。0029）。スキャン登録画面の右カラム、手入力欄の上。
//
// オン・オフは画面を開くたびにオフから（保存しない）。選んだカメラだけを覚える（sessionStore の cameraDeviceId）。
// 押したあとは手入力欄にフォーカスを戻す。確定キー（スペース）は画面全体で拾っているので、
// ボタンにフォーカスがあっても、既存のボタンと同じく、ボタンの操作にはならず確定になる（0012 §3・§9）。
// 音はここでは鳴らさない（読んだ値を渡したあとの、ふだんの流れが鳴らす）。値を渡したときは、映像の枠の色を変える。
import { CameraIcon, CameraOffIcon } from "lucide-react";
import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { useCameraScan } from "@/features/scanScreen/camera/useCameraScan";
import { focusManualInput } from "@/features/scanScreen/manualFocus";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { ja } from "@/i18n/ja";
import { useAnyModalOpen } from "@/lib/useModalGuard";
import { cn } from "@/lib/utils";

interface CameraPanelProps {
  /** 読み取りを受け付けているか（フォームや確認を開いている間は偽）。 */
  listening: boolean;
  /** 読めた 13 桁の数字を渡す（`useScanFlow` の `submitDigits`）。 */
  onDigits: (digits: string) => void;
}

export function CameraPanel({ listening, onDigits }: CameraPanelProps) {
  const [on, setOn] = useState(false);
  const deviceId = useScanSessionStore((s) => s.cameraDeviceId);
  const setDeviceId = useScanSessionStore((s) => s.setCameraDeviceId);
  const modalOpen = useAnyModalOpen();
  const accepting = listening && !modalOpen;

  const videoRef = useRef<HTMLVideoElement>(null);
  const camera = useCameraScan({ videoRef, on, deviceId, accepting, onDigits });
  const t = ja.scan.screen.camera;
  const showVideo = camera.status === "starting" || camera.status === "running";

  return (
    <section
      aria-label={t.heading}
      data-testid="camera-panel"
      className="flex flex-col gap-3 rounded-xl border bg-card p-4"
    >
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold text-muted-foreground">{t.heading}</h2>
        <Button
          type="button"
          variant={on ? "default" : "outline"}
          size="sm"
          aria-pressed={on}
          onClick={() => {
            setOn((value) => !value);
            focusManualInput();
          }}
        >
          {on ? <CameraOffIcon /> : <CameraIcon />}
          {on ? t.off : t.on}
        </Button>
      </div>

      {/* 映像の要素は、カメラを開く側が参照するので、いつも置いておく */}
      <div
        className={cn(
          "overflow-hidden rounded-md border-2 bg-muted transition-colors",
          camera.flash ? "border-success" : "border-transparent",
          showVideo ? "aspect-video" : "hidden",
        )}
        data-flash={camera.flash ? "true" : undefined}
        data-testid="camera-frame"
      >
        <video
          ref={videoRef}
          muted
          playsInline
          aria-label={t.previewLabel}
          className="h-full w-full object-cover"
        />
      </div>

      {camera.devices.length >= 2 && on && (
        <label className="flex items-center gap-2 text-sm">
          <span className="shrink-0 text-muted-foreground">{t.deviceLabel}</span>
          <select
            className="min-w-0 flex-1 rounded-md border bg-background px-2 py-1 text-sm"
            value={camera.activeDeviceId ?? ""}
            onChange={(event) => {
              setDeviceId(event.target.value);
              focusManualInput();
            }}
          >
            {camera.devices.map((device, index) => (
              <option key={device.deviceId} value={device.deviceId}>
                {device.label !== "" ? device.label : t.deviceUnnamed(index + 1)}
              </option>
            ))}
          </select>
        </label>
      )}

      {camera.status === "error" && camera.error !== null ? (
        <p role="alert" className="text-sm text-destructive">
          {t.errors[camera.error]}
        </p>
      ) : (
        <p className="text-xs text-muted-foreground" aria-live="polite">
          {camera.status === "starting"
            ? t.starting
            : camera.status === "running"
              ? accepting
                ? t.reading
                : t.paused
              : t.hint}
        </p>
      )}
    </section>
  );
}
