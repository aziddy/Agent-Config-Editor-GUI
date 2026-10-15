import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { FieldRow } from "@/components/FieldRow";
import { Switch } from "@/components/ui/switch";
import { describeError } from "@/lib/errors";
import { useRequestPlan } from "@/lib/mutations";
import { queryKeys } from "@/lib/queries";
import { rpc } from "@/lib/rpc";
import type { Skill } from "@/lib/types";

export function CodexSkillInvocationField({ skill, disabled }: { skill: Skill; disabled?: boolean }) {
  const [busy, setBusy] = useState(false);
  const requestPlan = useRequestPlan();
  const policy = useQuery({
    // File-change events and successful writes invalidate this file-key prefix.
    queryKey: [...queryKeys.file(`${skill.dir}/agents/openai.yaml`), "invocation"],
    queryFn: () => rpc("get_codex_skill_invocation", { skillId: skill.id }),
  });

  return (
    <FieldRow
      label="Disable model invocation"
      agents={["claude", "codex"]}
      help="Prevents automatic use; $name still works. Inverts allow_implicit_invocation in agents/openai.yaml."
    >
      {policy.error ? (
        <p className="text-xs text-destructive">
          {describeError(policy.error)} Edit the file in the Editor tab.
        </p>
      ) : policy.data ? (
        <div className="space-y-1 pt-1.5">
          <Switch
            aria-label="Disable model invocation"
            checked={!policy.data.allowImplicitInvocation}
            disabled={disabled || busy}
            onCheckedChange={async (disable) => {
              const current = policy.data;
              if (!current) return;
              setBusy(true);
              try {
                await requestPlan(() =>
                  rpc("preview_set_codex_skill_invocation", {
                    skillId: skill.id,
                    allowImplicitInvocation: !disable,
                    baseHash: current.baseHash,
                  }),
                );
              } finally {
                setBusy(false);
              }
            }}
          />
          <p className="break-words font-mono text-[11px] text-muted-foreground">
            allow_implicit_invocation: {String(policy.data.allowImplicitInvocation)}
          </p>
        </div>
      ) : (
        <p className="pt-1.5 text-xs text-muted-foreground">Loading invocation policy…</p>
      )}
    </FieldRow>
  );
}
