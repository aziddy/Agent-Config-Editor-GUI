import { useState } from "react";
import { EnabledSwitch } from "@/components/EnabledSwitch";
import { InfoRow } from "@/components/FieldRow";
import { PluginScopeGate } from "@/components/PluginScopeGate";
import { Badge } from "@/components/ui/badge";
import { FrontmatterForm } from "@/forms/FrontmatterForm";
import { FIELD_SPECS, type FrontmatterKind } from "@/lib/frontmatter";
import { useRequestPlan } from "@/lib/mutations";
import { useSnapshot } from "@/lib/queries";
import { rpc } from "@/lib/rpc";
import type { Skill, SlashCommand, SubAgent } from "@/lib/types";

type Entity =
  | { kind: "skill"; data: Skill }
  | { kind: "subAgent"; data: SubAgent }
  | { kind: "slashCommand"; data: SlashCommand };

export function FrontmatterEntityDetail({ entity }: { entity: Entity }) {
  const { data } = entity;
  const snapshot = useSnapshot();
  const requestPlan = useRequestPlan();
  const [unlocked, setUnlocked] = useState(false);
  const pluginScoped = data.scope.type === "plugin";
  const isMarkdown = data.origin.locator.type === "markdownFile";
  const kind: FrontmatterKind = entity.kind;
  const baseHash = snapshot.data?.fileHashes[data.origin.file] ?? null;
  const dupGroup = snapshot.data?.dupGroups.find((g) => g.key === data.dupGroup);

  return (
    <div className="space-y-4">
      <div className="space-y-1.5">
        {entity.kind === "skill" && (
          <InfoRow label="Enabled">
            <div className="flex items-center gap-2">
              <EnabledSwitch
                state={entity.data.enabled}
                onChange={(enabled) =>
                  void requestPlan(() => rpc("preview_set_skill_enabled", { skillId: data.id, enabled }))
                }
              />
              {entity.data.enabled.reason && (
                <span className="text-xs text-muted-foreground">{entity.data.enabled.reason}</span>
              )}
            </div>
          </InfoRow>
        )}
        <InfoRow label="File">
          <span className="font-mono text-xs">{data.origin.file}</span>
        </InfoRow>
        {entity.kind === "skill" && (entity.data.isSystem || entity.data.lockManaged) && (
          <InfoRow label="Managed">
            <div className="flex gap-1">
              {entity.data.isSystem && <Badge variant="muted">built-in</Badge>}
              {entity.data.lockManaged && (
                <Badge variant="muted" title="Tracked in ~/.agents/.skill-lock.json">
                  installer-managed
                </Badge>
              )}
            </div>
          </InfoRow>
        )}
        {dupGroup && dupGroup.projectRoots.length > 1 && (
          <InfoRow label="Identical copies">
            <ul className="space-y-0.5 font-mono text-xs">
              {dupGroup.projectRoots.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
          </InfoRow>
        )}
        {!data.frontmatter.present && isMarkdown && (
          <p className="text-xs text-muted-foreground">
            This file has no frontmatter block. Saving a field below will add one at the top.
          </p>
        )}
      </div>

      {pluginScoped && data.scope.type === "plugin" && (
        <PluginScopeGate
          pluginId={data.scope.pluginId}
          unlocked={unlocked}
          onUnlock={() => setUnlocked(true)}
        />
      )}

      {isMarkdown ? (
        <FrontmatterForm
          frontmatter={data.frontmatter}
          specs={FIELD_SPECS[kind]}
          disabled={pluginScoped && !unlocked}
          onSave={(changes) =>
            requestPlan(() =>
              rpc("preview_save_frontmatter", { path: data.origin.file, changes, body: null, baseHash }),
            )
          }
        />
      ) : (
        <div className="space-y-1.5">
          <InfoRow label="Description">
            {data.description ?? <span className="text-muted-foreground">—</span>}
          </InfoRow>
          {entity.kind === "subAgent" && (
            <>
              <InfoRow label="Model">{entity.data.model ?? "—"}</InfoRow>
              <InfoRow label="Reasoning effort">{entity.data.effort ?? "—"}</InfoRow>
            </>
          )}
          <p className="text-xs text-muted-foreground">TOML-backed entity: edit it in the Raw tab.</p>
        </div>
      )}

      {data.bodyPreview && (
        <div>
          <p className="mb-1 text-xs font-medium text-muted-foreground">Body preview</p>
          <p className="rounded border bg-muted/30 p-2 text-xs leading-relaxed text-muted-foreground">
            {data.bodyPreview}
          </p>
        </div>
      )}
    </div>
  );
}
