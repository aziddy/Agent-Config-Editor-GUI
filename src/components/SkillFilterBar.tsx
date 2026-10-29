import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { DEFAULT_SKILL_FILTERS, hasSkillFilters, type SkillFilters } from "@/lib/entities";
import { useAppSettings } from "@/lib/queries";
import { useUiStore } from "@/state/uiStore";

export function SkillFilterBar({ count, loading }: { count: number; loading: boolean }) {
  const filters = useUiStore((s) => s.skillFilters);
  const setFilters = useUiStore((s) => s.setSkillFilters);
  const navigationPending = useUiStore((s) => s.navigationPending);
  const setNav = useUiStore((s) => s.setNav);
  const settings = useAppSettings();
  const builtInsHidden = filters.builtIn === "yes" && settings.data?.showSystemSkills === false;
  const managedHidden = filters.installerManaged === "yes" && settings.data?.showAgentsDirSkills === false;
  const update = <K extends keyof SkillFilters>(key: K, value: SkillFilters[K]) =>
    void setFilters({ ...filters, [key]: value });

  return (
    <div className="shrink-0 space-y-2 border-b px-3 py-2">
      <fieldset
        aria-label="Skill filters"
        disabled={navigationPending}
        className="flex min-w-0 flex-wrap items-end gap-2"
      >
        <label className="space-y-1 text-xs">
          <span
            className="block text-muted-foreground"
            title="Whether the agent may automatically use the skill, independently of its enabled state."
          >
            Implicit invocation
          </span>
          <Select
            value={filters.invocation}
            onChange={(event) => update("invocation", event.target.value as SkillFilters["invocation"])}
          >
            <option value="all">All</option>
            <option value="allowed">Allowed</option>
            <option value="blocked">Blocked</option>
            <option value="unknown">Unknown</option>
          </Select>
        </label>
        <label className="space-y-1 text-xs">
          <span className="block text-muted-foreground" title="Claude skills have no enable switch.">
            Enabled state
          </span>
          <Select
            value={filters.enabled}
            onChange={(event) => update("enabled", event.target.value as SkillFilters["enabled"])}
          >
            <option value="all">All</option>
            <option value="enabled">Enabled</option>
            <option value="disabled">Disabled</option>
            <option value="notApplicable">No enable switch</option>
          </Select>
        </label>
        <label className="space-y-1 text-xs">
          <span className="block text-muted-foreground">Built-in</span>
          <Select
            value={filters.builtIn}
            onChange={(event) => update("builtIn", event.target.value as SkillFilters["builtIn"])}
          >
            <option value="all">All</option>
            <option value="yes">Yes</option>
            <option value="no">No</option>
          </Select>
        </label>
        <label className="space-y-1 text-xs">
          <span
            className="block text-muted-foreground"
            title="Skills tracked by the ~/.agents skill installer."
          >
            Installer-managed
          </span>
          <Select
            value={filters.installerManaged}
            onChange={(event) =>
              update("installerManaged", event.target.value as SkillFilters["installerManaged"])
            }
          >
            <option value="all">All</option>
            <option value="yes">Yes</option>
            <option value="no">No</option>
          </Select>
        </label>
        <div className="ml-auto flex min-h-8 flex-wrap items-center gap-2">
          <span role="status" className="text-xs tabular-nums text-muted-foreground">
            {loading ? "Loading skills…" : count === 1 ? "1 skill matches" : `${count} skills match`}
          </span>
          {hasSkillFilters(filters) && (
            <Button
              variant="ghost"
              size="sm"
              className="h-8 text-xs"
              onClick={() => void setFilters(DEFAULT_SKILL_FILTERS)}
            >
              Clear skill filters
            </Button>
          )}
        </div>
      </fieldset>
      {(builtInsHidden || managedHidden) && (
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
          {builtInsHidden && (
            <span>Built-in skills are hidden. Enable “Show Codex built-in skills” in Settings.</span>
          )}
          {managedHidden && (
            <span>Installer-managed skills are hidden. Enable “Show ~/.agents skills” in Settings.</span>
          )}
          <Button
            variant="link"
            size="sm"
            className="h-auto p-0 text-xs"
            onClick={() => void setNav("settings")}
          >
            Open Settings
          </Button>
        </div>
      )}
    </div>
  );
}
