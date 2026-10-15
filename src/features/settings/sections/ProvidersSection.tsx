// 取得元（FR-CONF-02、FR-META-03）。有効・無効、API キー（OS のキーチェーン）、接続テスト、
// 項目ごとの優先順位、User-Agent の連絡先。
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowDownIcon, ArrowUpIcon, PlugZapIcon } from "lucide-react";
import { useId, useState } from "react";
import { toast } from "sonner";
import type { MetaGroup } from "@/bindings/MetaGroup";
import type { MetaSource } from "@/bindings/MetaSource";
import type { PriorityProfile } from "@/bindings/PriorityProfile";
import type { ProviderSettings } from "@/bindings/ProviderSettings";
import type { ProviderStatus } from "@/bindings/ProviderStatus";
import type { ProviderTestResult } from "@/bindings/ProviderTestResult";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Group } from "@/features/settings/fields";
import { settingsKeys, useProviderSettings } from "@/features/settings/hooks";
import {
  META_GROUPS,
  PRIORITY_PROFILES,
  isFixedSource,
  moveSource,
  orderOf,
  withProfileGroupOrder,
} from "@/features/settings/priorities";
import { type TestTone, describeTestResult } from "@/features/settings/testResult";
import { ja } from "@/i18n/ja";
import { errorMessage } from "@/lib/errors";
import { ipc } from "@/lib/ipc";
import { inTauri } from "@/lib/runtime";
import { cn } from "@/lib/utils";

const TONE_CLASS: Record<TestTone, string> = {
  success: "text-success",
  warning: "text-warning",
  error: "text-destructive",
};

export function ProvidersSection() {
  const settings = useProviderSettings();
  const providers = useQuery({
    queryKey: settingsKeys.providers,
    queryFn: ipc.listProviders,
    enabled: inTauri(),
  });
  const p = ja.settings.providers;

  if (!inTauri()) return <p className="text-sm">{ja.settings.browserNotice}</p>;
  if (settings.isLoading || providers.isLoading) {
    return <p className="text-sm text-muted-foreground">{ja.settings.loading}</p>;
  }
  if (settings.value === undefined || providers.data === undefined) {
    return <p className="text-sm text-destructive">{ja.settings.loadFailed}</p>;
  }
  const value = settings.value;

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">{p.description}</p>

      {providers.data.map((provider) => (
        <ProviderCard
          key={provider.id}
          provider={provider}
          onToggle={(enabled) =>
            settings.save({ ...value, enabled: { ...value.enabled, [provider.id]: enabled } })
          }
          saving={settings.saving}
        />
      ))}

      <PrioritiesGroup
        settings={value}
        onChange={(priorities) => settings.save({ ...value, priorities })}
      />

      <ContactUrlGroup
        // 保存された値が変わったら、入力欄を作り直して合わせる
        key={value.contactUrl}
        contactUrl={value.contactUrl}
        saving={settings.saving}
        onSave={(contactUrl) =>
          settings.save(
            { ...value, contactUrl },
            { onSuccess: () => toast.success(p.contactUrl.savedToast) },
          )
        }
      />
    </div>
  );
}

function ProviderCard({
  provider,
  onToggle,
  saving,
}: {
  provider: ProviderStatus;
  onToggle: (enabled: boolean) => void;
  saving: boolean;
}) {
  const client = useQueryClient();
  const p = ja.settings.providers;
  const [keyText, setKeyText] = useState("");
  const [testResult, setTestResult] = useState<ProviderTestResult | null>(null);
  const keyId = useId();
  const needsKey = provider.auth.kind !== "none";
  const keyRequired = provider.auth.kind === "required";

  const refresh = () => client.invalidateQueries({ queryKey: settingsKeys.providers });
  const described = testResult === null ? null : describeTestResult(testResult, needsKey);

  const saveKey = useMutation({
    mutationFn: (key: string) => ipc.setApiKey(provider.id, key),
    onSuccess: async () => {
      // 入力欄は、保存したら空に戻す（キーを画面に残さない）
      setKeyText("");
      // 前のキーでの接続テストの結果は、もう当てはまらない
      setTestResult(null);
      toast.success(p.apiKey.savedToast);
      await refresh();
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
  const deleteKey = useMutation({
    mutationFn: () => ipc.deleteApiKey(provider.id),
    onSuccess: async () => {
      setKeyText("");
      setTestResult(null);
      toast.success(p.apiKey.deletedToast);
      await refresh();
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
  const test = useMutation({
    mutationFn: () => ipc.testProvider(provider.id),
    onSuccess: (result) => setTestResult(result),
    onError: (error) => {
      setTestResult(null);
      toast.error(errorMessage(error));
    },
  });

  return (
    <Group title={provider.displayName}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <label className="flex items-center gap-2 text-sm">
          <Checkbox
            checked={provider.enabled}
            disabled={saving}
            onCheckedChange={(checked) => onToggle(checked === true)}
            aria-label={p.enabled(provider.displayName)}
          />
          {p.enabled(provider.displayName)}
        </label>
        <span
          className={cn(
            "rounded-full px-2.5 py-0.5 text-xs",
            provider.active ? "bg-success/15 text-foreground" : "bg-muted text-muted-foreground",
          )}
        >
          {provider.active ? p.active : p.inactive}
        </span>
      </div>

      {keyRequired && !provider.hasKey && <p className="text-sm text-warning">{p.googleNote}</p>}

      {needsKey && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-3 text-sm">
            <label htmlFor={keyId}>
              {p.apiKey.label(provider.displayName)}
              {provider.auth.kind !== "none" && (
                <span className="ml-1 text-xs text-muted-foreground">
                  （{provider.auth.label}）
                </span>
              )}
            </label>
            <span className={provider.hasKey ? "text-success" : "text-muted-foreground"}>
              {provider.hasKey ? p.apiKey.saved : p.apiKey.notSet}
            </span>
          </div>
          <div className="flex gap-2">
            <Input
              id={keyId}
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={keyText}
              placeholder={p.apiKey.placeholder}
              onChange={(event) => setKeyText(event.target.value)}
            />
            <Button
              variant="outline"
              disabled={keyText.trim() === "" || saveKey.isPending}
              onClick={() => saveKey.mutate(keyText)}
            >
              {p.apiKey.save}
            </Button>
            <Button
              variant="outline"
              disabled={!provider.hasKey || deleteKey.isPending}
              onClick={() => deleteKey.mutate()}
            >
              {p.apiKey.delete}
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">{p.apiKey.storedNote}</p>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-3">
        <Button variant="outline" size="sm" disabled={test.isPending} onClick={() => test.mutate()}>
          <PlugZapIcon />
          {test.isPending ? p.test.running : p.test.button}
        </Button>
        {described !== null && (
          <p
            className={cn("text-sm", TONE_CLASS[described.tone])}
            role="status"
            data-testid={`test-result-${provider.id}`}
            data-tone={described.tone}
          >
            {described.text}
          </p>
        )}
      </div>
    </Group>
  );
}

function PrioritiesGroup({
  settings,
  onChange,
}: {
  settings: ProviderSettings;
  onChange: (priorities: ProviderSettings["priorities"]) => void;
}) {
  const p = ja.settings.providers;
  // 見ている組（和書・洋書など）。覚えておくのはこの画面を開いている間だけ
  const [profile, setProfile] = useState<PriorityProfile>("japanese");
  return (
    <Group title={p.priorities.heading} description={p.priorities.description}>
      <Tabs value={profile} onValueChange={(value) => setProfile(value as PriorityProfile)}>
        <TabsList aria-label={p.priorities.profileLabel}>
          {PRIORITY_PROFILES.map((id) => (
            <TabsTrigger key={id} value={id}>
              {p.priorities.profiles[id]}
            </TabsTrigger>
          ))}
        </TabsList>
        <p className="text-xs text-muted-foreground">{p.priorities.profileNote}</p>
        {PRIORITY_PROFILES.map((id) => (
          <TabsContent key={id} value={id}>
            <PriorityList
              priorities={settings.priorities[id]}
              onReorder={(group, order) =>
                onChange(withProfileGroupOrder(settings.priorities, id, group, order))
              }
            />
          </TabsContent>
        ))}
      </Tabs>
      <p className="text-xs text-muted-foreground">{p.priorities.fixedBarcode}</p>
    </Group>
  );
}

/** 1 つの組（和書か洋書など）の、項目の行ごとの優先順位。矢印で並べ替える。 */
function PriorityList({
  priorities,
  onReorder,
}: {
  priorities: ProviderSettings["priorities"][PriorityProfile];
  onReorder: (group: MetaGroup, order: MetaSource[]) => void;
}) {
  const p = ja.settings.providers;
  return (
    <ul className="flex flex-col divide-y">
      {META_GROUPS.map((group) => {
        const order = orderOf(priorities, group);
        const groupName = p.groups[group];
        return (
          <li
            key={group}
            className="flex flex-wrap items-center gap-x-4 gap-y-1 py-2 text-sm"
            data-testid={`priority-${group}`}
          >
            <span className="w-40 shrink-0 font-medium">{groupName}</span>
            <ol className="flex flex-1 flex-wrap items-center gap-2">
              {order.map((source, index) => {
                const name = p.sources[source];
                const fixed = isFixedSource(source);
                return (
                  <li
                    key={source}
                    className={cn(
                      "flex items-center gap-0.5 rounded-md border py-0.5 pr-0.5 pl-2",
                      fixed && "bg-muted text-muted-foreground",
                    )}
                  >
                    <span className="mr-1 text-xs text-muted-foreground tabular-nums">
                      {index + 1}
                    </span>
                    {name}
                    {!fixed && (
                      <>
                        <Button
                          variant="ghost"
                          size="icon-xs"
                          aria-label={p.priorities.moveUp(groupName, name)}
                          disabled={index === 0 || isFixedSource(order[index - 1] ?? source)}
                          onClick={() => onReorder(group, moveSource(order, index, -1))}
                        >
                          <ArrowUpIcon />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon-xs"
                          aria-label={p.priorities.moveDown(groupName, name)}
                          disabled={index === order.length - 1}
                          onClick={() => onReorder(group, moveSource(order, index, 1))}
                        >
                          <ArrowDownIcon />
                        </Button>
                      </>
                    )}
                  </li>
                );
              })}
            </ol>
          </li>
        );
      })}
    </ul>
  );
}

function ContactUrlGroup({
  contactUrl,
  saving,
  onSave,
}: {
  contactUrl: string;
  saving: boolean;
  onSave: (contactUrl: string) => void;
}) {
  const [text, setText] = useState(contactUrl);
  const id = useId();
  const c = ja.settings.providers.contactUrl;
  return (
    <Group title={c.label} description={c.description}>
      <div className="flex gap-2">
        <Input
          id={id}
          aria-label={c.label}
          value={text}
          inputMode="url"
          spellCheck={false}
          onChange={(event) => setText(event.target.value)}
        />
        <Button
          variant="outline"
          disabled={saving || text.trim() === contactUrl}
          onClick={() => onSave(text.trim())}
        >
          {ja.settings.save}
        </Button>
      </div>
    </Group>
  );
}
