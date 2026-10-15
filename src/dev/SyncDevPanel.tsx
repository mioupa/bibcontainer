// 開発用: 同期のモック（src/dev/mockSync.ts）の状態を切り替えるパネル。設定の「同期」の節の下に、開発中だけ出る。
// 状態（未設定・同期中・完了・エラー・不一致・更新が必要・時計のずれ・未書き出しあり）と、フォルダを選んだときの判定を、
// ボタンで切り替えて画面を確かめる。
import { Button } from "@/components/ui/button";
import { Group } from "@/features/settings/fields";
import { useSyncStatus } from "@/features/sync/hooks";
import {
  MOCK_LIBRARY_ID,
  MOCK_SYNC_SCENARIOS,
  mockSync,
  type MockSyncScenario,
} from "@/dev/mockSync";
import type { FolderKind } from "@/features/sync/types";
import { ja } from "@/i18n/ja";
import { useState } from "react";

const OTHER_LIBRARY_ID = "0192bb00-0000-7000-8000-000000000000";

const KINDS: readonly FolderKind[] = [
  "empty",
  "library",
  "otherLibrary",
  "notWritable",
  "updateRequired",
];

export function SyncDevPanel() {
  const t = ja.sync.dev;
  const { data } = useSyncStatus();
  const [inspect, setInspect] = useState(mockSync.control.getInspect());

  const updateInspect = (partial: Partial<typeof inspect>) => {
    mockSync.control.setInspect(partial);
    setInspect(mockSync.control.getInspect());
  };
  // 見た目で選ばれている状態の見本（時計のずれ・未書き出しは、状態が idle のときだけ見分けがつく）
  const current: MockSyncScenario | null =
    data === undefined
      ? null
      : data.state === "idle"
        ? data.clockSkew !== null
          ? "skew"
          : data.pendingOps > 0
            ? "pending"
            : "idle"
        : data.state;

  return (
    <Group title={t.heading} description={t.description}>
      <div className="flex flex-col gap-3" data-testid="sync-dev-panel">
        <div className="flex flex-col gap-1.5">
          <p className="text-xs font-medium">{t.scenarioLabel}</p>
          <div className="flex flex-wrap gap-1.5">
            {MOCK_SYNC_SCENARIOS.map((scenario) => (
              <Button
                key={scenario}
                size="xs"
                variant={current === scenario ? "default" : "outline"}
                onClick={() => mockSync.control.setScenario(scenario)}
              >
                {t.scenarios[scenario]}
              </Button>
            ))}
            <Button
              size="xs"
              variant="ghost"
              onClick={() => void mockSync.api.syncNow().catch(() => {})}
            >
              {t.play}
            </Button>
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          <p className="text-xs font-medium">{t.inspectLabel}</p>
          <div className="flex flex-wrap gap-1.5">
            {KINDS.map((kind) => (
              <Button
                key={kind}
                size="xs"
                variant={inspect.kind === kind ? "default" : "outline"}
                onClick={() =>
                  updateInspect({
                    kind,
                    localLibraryId: kind === "otherLibrary" ? OTHER_LIBRARY_ID : null,
                  })
                }
              >
                {t.inspectKinds[kind]}
              </Button>
            ))}
          </div>
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={inspect.localHasData}
              onChange={(event) => updateInspect({ localHasData: event.target.checked })}
            />
            {t.localHasData}
          </label>
          <p className="text-xs font-medium">{t.localLibraryLabel}</p>
          <div className="flex flex-wrap gap-1.5">
            {(
              [
                ["none", null],
                ["same", MOCK_LIBRARY_ID],
                ["other", OTHER_LIBRARY_ID],
              ] as const
            ).map(([key, id]) => (
              <Button
                key={key}
                size="xs"
                variant={inspect.localLibraryId === id ? "default" : "outline"}
                onClick={() => updateInspect({ localLibraryId: id })}
              >
                {t.localLibrary[key]}
              </Button>
            ))}
          </div>
        </div>
      </div>
    </Group>
  );
}
