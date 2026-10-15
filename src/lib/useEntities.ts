import { useMemo } from "react";
import {
  countEntitiesByKind,
  entitiesOfKind,
  type FilterOptions,
  filterEntities,
  findEntity,
} from "@/lib/entities";
import { useAppSettings, useSnapshot } from "@/lib/queries";
import type { EntityKind } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";

function useEntityFilterOptions(): FilterOptions {
  const snapshot = useSnapshot();
  const settings = useAppSettings();
  const agentFilter = useUiStore((s) => s.agentFilter);
  const locationFilter = useUiStore((s) => s.locationFilter);
  const search = useUiStore((s) => s.search);
  const showDuplicates = useUiStore((s) => s.showDuplicates);
  return useMemo(
    () => ({
      agent: agentFilter,
      location: locationFilter,
      search,
      collapseDuplicates: !showDuplicates,
      showSystemSkills: settings.data?.showSystemSkills ?? false,
      showAgentsDirSkills: settings.data?.showAgentsDirSkills ?? true,
      agentsDir: snapshot.data?.homes?.agentsDir ?? null,
    }),
    [snapshot.data, settings.data, agentFilter, locationFilter, search, showDuplicates],
  );
}

export function useFilteredEntities(kind: EntityKind) {
  const snapshot = useSnapshot();
  const options = useEntityFilterOptions();
  return useMemo(
    () => (snapshot.data ? filterEntities(entitiesOfKind(snapshot.data, kind), options) : []),
    [snapshot.data, kind, options],
  );
}

export function useSelectedEntity() {
  const snapshot = useSnapshot();
  const selectedId = useUiStore((s) => s.selectedId);
  return useMemo(
    () => (snapshot.data && selectedId ? findEntity(snapshot.data, selectedId) : null),
    [snapshot.data, selectedId],
  );
}

/** Counts per kind honouring the list filters except text search (for the sidebar). */
export function useKindCounts(): Record<EntityKind, number> {
  const snapshot = useSnapshot();
  const options = useEntityFilterOptions();
  return useMemo(() => {
    if (snapshot.data) return countEntitiesByKind(snapshot.data, options);
    return {
      skill: 0,
      mcpServer: 0,
      plugin: 0,
      subAgent: 0,
      slashCommand: 0,
      hook: 0,
      automation: 0,
    };
  }, [snapshot.data, options]);
}
