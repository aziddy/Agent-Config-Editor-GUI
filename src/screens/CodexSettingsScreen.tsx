import {
  Brain,
  Cpu,
  FileCode2,
  type LucideIcon,
  MessageSquareText,
  RotateCcw,
  Save,
  ShieldCheck,
  Sparkles,
  Terminal,
} from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { PathActions } from "@/components/PathActions";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { CodexSettingField } from "@/forms/CodexSettingField";
import {
  APPROVAL_FIELDS,
  codexSettingChanges,
  type CodexSettingField as Field,
  INSTRUCTIONS_FIELD,
  isJsonMap,
  MEMORY_FIELDS,
  MODEL_FIELDS,
  SANDBOX_FIELDS,
  setSetting,
  settingsListsValid,
  settingValue,
  TERMINAL_FIELDS,
  WEB_FIELD,
  WORKSPACE_FIELDS,
} from "@/lib/codexSettings";
import { resolveEditorNavigation } from "@/lib/editorSession";
import { describeError } from "@/lib/errors";
import { useRequestPlan } from "@/lib/mutations";
import { useCodexSettings, useSnapshot } from "@/lib/queries";
import { rpc } from "@/lib/rpc";
import type { CodexSettings, JsonMap, JsonValue } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";
import { useUnsavedChangesStore } from "@/state/unsavedChangesStore";

function SettingsCard({
  title,
  description,
  icon: Icon,
  children,
  wide = false,
}: {
  title: string;
  description: string;
  icon: LucideIcon;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <Card className={wide ? "min-w-0 lg:col-span-2" : "min-w-0"}>
      <CardHeader className="border-b bg-muted/20 pb-3">
        <CardTitle className="flex items-center gap-2">
          <Icon className="h-4 w-4 text-muted-foreground" aria-hidden="true" /> {title}
        </CardTitle>
        <p className="text-xs text-muted-foreground">{description}</p>
      </CardHeader>
      <CardContent className="space-y-4 pt-4">{children}</CardContent>
    </Card>
  );
}

export function CodexSettingsScreen() {
  const homes = useSnapshot().data?.homes;
  const path = homes ? `${homes.codexHome.replace(/\/+$/, "")}/config.toml` : null;
  return path ? (
    <SettingsEditor key={path} path={path} readOnly={homes?.readOnly ?? true} />
  ) : (
    <div className="flex h-full flex-col">
      <TopBar />
      <p role="status" className="p-4 text-sm text-muted-foreground">
        Loading Codex settings…
      </p>
    </div>
  );
}

function SettingsEditor({ path, readOnly }: { path: string; readOnly: boolean }) {
  const file = useCodexSettings(path);
  const [session, setSession] = useState<{ base: CodexSettings | null; draft: JsonMap }>({
    base: null,
    draft: {},
  });
  const { base, draft } = session;
  const [saving, setSaving] = useState(false);
  const requestPlan = useRequestPlan();
  const confirm = useUnsavedChangesStore((s) => s.confirm);
  const setNavigationGuard = useUiStore((s) => s.setNavigationGuard);
  const setNav = useUiStore((s) => s.setNav);
  const externalChange = useUiStore((s) => s.externallyChanged[path]);
  const clearExternalChange = useUiStore((s) => s.clearExternallyChanged);
  const baseHash = base?.hash;

  useEffect(() => {
    if (!file.data) return;
    const loaded = file.data;
    if (baseHash && loaded.hash !== baseHash) return;
    setSession((current) => {
      if (!current.base) return { base: loaded, draft: loaded.values };
      if (current.base.hash === loaded.hash && current.base !== loaded) return { ...current, base: loaded };
      // Keep the draft anchored to its loaded version until an explicit reload.
      return current;
    });
  }, [file.data, baseHash]);

  const changes = useMemo(() => (base ? codexSettingChanges(base.values, draft) : []), [base, draft]);
  const dirty = changes.length > 0;
  const diskChanged = !!base && !!file.data && base.hash !== file.data.hash;
  const conflict = dirty && diskChanged;
  const valid = settingsListsValid(base?.values ?? {}, draft);
  const discard = useCallback(() => {
    setSession((current) => {
      const loaded = file.error ? current.base : (file.data ?? current.base);
      return loaded ? { base: loaded, draft: loaded.values } : current;
    });
    clearExternalChange(path);
  }, [file.data, file.error, clearExternalChange, path]);

  const save = useCallback(async () => {
    if (!base || readOnly || saving || conflict || file.error || !valid || !dirty) return false;
    setSaving(true);
    try {
      const applied = await requestPlan(() =>
        rpc("preview_save_codex_settings", { changes, baseHash: base.hash }),
      );
      if (!applied?.newHash) return false;
      setSession({ base: { ...base, hash: applied.newHash, values: draft }, draft });
      clearExternalChange(path);
      return true;
    } finally {
      setSaving(false);
    }
  }, [
    base,
    readOnly,
    saving,
    conflict,
    file.error,
    valid,
    dirty,
    requestPlan,
    changes,
    draft,
    clearExternalChange,
    path,
  ]);

  useEffect(() => {
    setNavigationGuard(async () => {
      if (saving) return false;
      if (!dirty) return true;
      return resolveEditorNavigation(await confirm(path), save, discard);
    });
    return () => setNavigationGuard(null);
  }, [setNavigationGuard, saving, dirty, confirm, path, save, discard]);

  const reload = async () => {
    const result = await file.refetch();
    if (result.data && !result.error) {
      setSession({ base: result.data, draft: result.data.values });
      clearExternalChange(path);
    }
  };
  const change = (settingPath: string[], value: JsonValue) =>
    setSession((current) => ({ ...current, draft: setSetting(current.draft, settingPath, value) }));
  const fields = (items: Field[]) =>
    items.map((field) => (
      <CodexSettingField
        key={JSON.stringify(field.path)}
        field={field}
        values={draft}
        unsupported={base?.unsupportedPaths ?? []}
        onChange={change}
      />
    ));
  const featureValues = settingValue(base?.values ?? {}, ["features"]);
  const draftFeatures = settingValue(draft, ["features"]);
  const featureNames = [
    ...new Set([
      ...Object.keys(isJsonMap(featureValues) ? featureValues : {}),
      ...Object.keys(isJsonMap(draftFeatures) ? draftFeatures : {}),
    ]),
  ];
  const providers = [
    ...new Set(["openai", "ollama", "lmstudio", "amazon-bedrock", ...(base?.providerNames ?? [])]),
  ];
  const modelFields = MODEL_FIELDS.map((field) =>
    field.path[0] === "model_provider" ? { ...field, options: providers } : field,
  );
  const profileField: Field = {
    path: ["default_permissions"],
    label: "Permissions profile",
    kind: "choice",
    options: [
      ...new Set([":read-only", ":workspace", ":danger-full-access", ...(base?.permissionProfiles ?? [])]),
    ],
    help: "This config uses named permissions. Legacy sandbox settings are left in TOML.",
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <TopBar
        actions={
          <>
            <Button
              variant="outline"
              size="sm"
              className="h-8 gap-1 text-xs"
              onClick={() => void setNav("codexConfig")}
            >
              <FileCode2 className="h-3.5 w-3.5" /> Open TOML
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="h-8 gap-1 text-xs"
              disabled={!dirty || saving}
              onClick={discard}
            >
              <RotateCcw className="h-3.5 w-3.5" /> Discard
            </Button>
            <Button
              size="sm"
              className="h-8 gap-1 text-xs"
              disabled={readOnly || !base || !dirty || saving || conflict || !!file.error || !valid}
              onClick={() => void save()}
            >
              <Save className="h-3.5 w-3.5" /> {saving ? "Saving…" : "Save…"}
            </Button>
          </>
        }
      />
      <div className="min-h-0 flex-1 space-y-4 overflow-auto p-4">
        <div className="flex items-center gap-2">
          <div className="min-w-0 flex-1">
            <p className="mb-1 text-xs text-muted-foreground">
              Edit explicit overrides in your Codex user config.
            </p>
            <p className="truncate font-mono text-xs text-muted-foreground" title={path}>
              {path}
            </p>
          </div>
          <PathActions path={path} size="icon" />
        </div>
        {readOnly && (
          <p className="rounded border border-amber-500/40 bg-amber-500/10 p-2 text-xs">
            Read-only mode is enabled. Settings cannot be changed.
          </p>
        )}
        {file.error && (
          <div role="alert" className="rounded border border-destructive/40 p-3 text-sm text-destructive">
            <p>
              {describeError(file.error)}
              {base && " Your form draft has been kept."}
            </p>
            <Button
              variant="outline"
              size="sm"
              className="mt-2"
              disabled={file.isFetching}
              onClick={() => void reload()}
            >
              Retry
            </Button>
          </div>
        )}
        {(diskChanged || externalChange !== undefined) && (
          <div className="flex items-center justify-between gap-3 rounded border border-destructive/40 bg-destructive/10 p-2 text-xs">
            <p>
              {conflict
                ? "This file changed on disk while you were editing. Saving is blocked until you reload."
                : "This file changed on disk. Reload to see the latest settings."}
            </p>
            <Button
              size="sm"
              variant="outline"
              disabled={saving || file.isFetching}
              onClick={() => void reload()}
            >
              Reload
            </Button>
          </div>
        )}
        {base ? (
          <>
            {base.unsupportedPaths.length > 0 && (
              <p className="text-xs text-muted-foreground">
                Custom TOML values and advanced settings remain available in Codex Config.
              </p>
            )}
            <fieldset
              disabled={readOnly || saving}
              aria-label="Codex settings"
              className="grid min-w-0 grid-cols-1 items-start gap-4 disabled:opacity-70 lg:grid-cols-2"
            >
              <SettingsCard
                title="Model & reasoning"
                description="Choose the model and how it responds."
                icon={Cpu}
              >
                {fields(modelFields)}
              </SettingsCard>
              <SettingsCard
                title="Permissions & web"
                description="Control approvals, sandbox access, and web search."
                icon={ShieldCheck}
              >
                {fields(APPROVAL_FIELDS)}
                {base.namedPermissions ? (
                  fields([profileField])
                ) : (
                  <>
                    {fields(SANDBOX_FIELDS)}
                    {settingValue(draft, ["sandbox_mode"]) === "workspace-write" && fields(WORKSPACE_FIELDS)}
                  </>
                )}
                {fields([WEB_FIELD])}
              </SettingsCard>
              <SettingsCard
                title="Instructions"
                description="Additional guidance for Codex sessions."
                icon={MessageSquareText}
                wide
              >
                {fields([INSTRUCTIONS_FIELD])}
              </SettingsCard>
              <SettingsCard
                title="Features"
                description="Explicit feature flags configured in this file."
                icon={Sparkles}
              >
                {featureNames.length ? (
                  fields(
                    featureNames.map((name) => ({ path: ["features", name], label: name, kind: "boolean" })),
                  )
                ) : (
                  <p className="text-xs text-muted-foreground">
                    No explicit boolean flags. Use Open TOML to add feature settings.
                  </p>
                )}
              </SettingsCard>
              <SettingsCard title="Memories" description="Control memory generation and use." icon={Brain}>
                {fields(MEMORY_FIELDS)}
              </SettingsCard>
              <SettingsCard
                title="Terminal & notifications"
                description="Configure the terminal presentation and external notifier."
                icon={Terminal}
                wide
              >
                {fields(TERMINAL_FIELDS)}
              </SettingsCard>
            </fieldset>
            <p className="text-[11px] text-muted-foreground">
              mode {base.mode.toString(8)}
              {dirty ? " · unsaved changes" : ""} · Reset a field to remove its explicit override. Backups are
              available in Codex Config.
            </p>
          </>
        ) : !file.error ? (
          <p role="status" className="text-sm text-muted-foreground">
            Loading Codex settings…
          </p>
        ) : null}
      </div>
    </div>
  );
}
