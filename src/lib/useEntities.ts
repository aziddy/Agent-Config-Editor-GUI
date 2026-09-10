import { useMemo } from "react";
import { entitiesOfKind, filterEntities, findEntity } from "@/lib/entities";
import { useAppSettings, useSnapshot } from "@/lib/queries";
import type { EntityKind } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";

export function useFilteredEntities(kind: EntityKind) {
  const snapshot = useSnapshot();
  const settings = useAppSettings();
  const agentFilter = useUiStore((s) => s.agentFilter);
  const search = useUiStore((s) => s.search);
  const showDuplicates = useUiStore((s) => s.showDuplicates);
  return useMemo(() => {
    if (!snapshot.data) return [];
    return filterEntities(entitiesOfKind(snapshot.data, kind), {
      agent: agentFilter,
      search,
      collapseDuplicates: !showDuplicates,
      showSystemSkills: settings.data?.showSystemSkills ?? false,
      showAgentsDirSkills: settings.data?.showAgentsDirSkills ?? true,
      agentsDir: snapshot.data.homes?.agentsDir ?? null,
    });
  }, [snapshot.data, settings.data, kind, agentFilter, search, showDuplicates]);
}

export function useSelectedEntity() {
  const snapshot = useSnapshot();
  const selectedId = useUiStore((s) => s.selectedId);
  return useMemo(
    () => (snapshot.data && selectedId ? findEntity(snapshot.data, selectedId) : null),
    [snapshot.data, selectedId],
  );
}

/** Counts per kind honouring the agent filter only (for the sidebar). */
export function useKindCounts(): Record<EntityKind, number> {
  const snapshot = useSnapshot();
  const agentFilter = useUiStore((s) => s.agentFilter);
  const settings = useAppSettings();
  return useMemo(() => {
    const counts = {
      skill: 0,
      mcpServer: 0,
      plugin: 0,
      subAgent: 0,
      slashCommand: 0,
      hook: 0,
      automation: 0,
    } satisfies Record<EntityKind, number>;
    if (!snapshot.data) return counts;
    for (const kind of Object.keys(counts) as EntityKind[]) {
      counts[kind] = filterEntities(entitiesOfKind(snapshot.data, kind), {
        agent: agentFilter,
        search: "",
        collapseDuplicates: true,
        showSystemSkills: settings.data?.showSystemSkills ?? false,
        showAgentsDirSkills: settings.data?.showAgentsDirSkills ?? true,
        agentsDir: snapshot.data.homes?.agentsDir ?? null,
      }).length;
    }
    return counts;
  }, [snapshot.data, agentFilter, settings.data]);
}
