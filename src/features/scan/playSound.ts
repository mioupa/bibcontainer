// 設定ストアの音量・ミュート・種類ごとの有効を使って、通知音を鳴らす入口。ペアリングの効果（sound）の実行などから呼ぶ。
import { effectiveSound, type SoundKind } from "@/features/scan/config";
import { soundPlayer } from "@/features/scan/sound";
import { useSettingsStore } from "@/stores/settingsStore";

export function playSound(kind: SoundKind): void {
  soundPlayer.play(kind, effectiveSound(useSettingsStore.getState().sound, kind));
}
