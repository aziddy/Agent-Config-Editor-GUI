import { Check, Ellipsis, X } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { Select } from "@/components/ui/select";
import {
  basename,
  dirname,
  type LocationFilter,
  locationFilterLabel,
  projectLocations,
} from "@/lib/entities";
import { useAppSettings, useSnapshot } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/state/uiStore";

const OTHER_SCOPES = ["plugin", "system", "managed"] as const;

export function LocationPicker() {
  const location = useUiStore((s) => s.locationFilter);
  const setLocation = useUiStore((s) => s.setLocationFilter);
  const snapshot = useSnapshot();
  const settings = useAppSettings();
  const moreRef = useRef<HTMLDetailsElement>(null);
  const isProject =
    location.type === "project" || (location.type === "scope" && location.scope === "project");
  const isOther = location.type === "scope" && OTHER_SCOPES.some((scope) => scope === location.scope);
  const lastProject = useRef<LocationFilter>(isProject ? location : { type: "scope", scope: "project" });
  const projects = useMemo(
    () => (snapshot.data ? projectLocations(snapshot.data, settings.data?.showMissingProjects ?? false) : []),
    [snapshot.data, settings.data?.showMissingProjects],
  );
  const names = useMemo(() => {
    const counts = new Map<string, number>();
    for (const project of projects) {
      const name = basename(project.root);
      counts.set(name, (counts.get(name) ?? 0) + 1);
    }
    return counts;
  }, [projects]);

  useEffect(() => {
    const closeOutside = (event: PointerEvent) => {
      const more = moreRef.current;
      if (more && event.target instanceof Node && !more.contains(event.target)) more.open = false;
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, []);

  const choose = (filter: LocationFilter) => {
    void setLocation(filter).then((changed) => {
      if (changed && (filter.type === "project" || (filter.type === "scope" && filter.scope === "project"))) {
        lastProject.current = filter;
      }
    });
    if (moreRef.current?.open) {
      moreRef.current.open = false;
      moreRef.current.querySelector("summary")?.focus();
    }
  };
  const quickFilters: { label: string; active: boolean; filter: LocationFilter }[] = [
    { label: "All", active: location.type === "all", filter: { type: "all" } },
    {
      label: "User",
      active: location.type === "scope" && location.scope === "user",
      filter: { type: "scope", scope: "user" },
    },
    { label: "Projects", active: isProject, filter: isProject ? location : lastProject.current },
  ];

  return (
    <fieldset className="mt-2 min-w-0">
      <legend className="sr-only">Location</legend>
      <div className="flex items-center gap-0.5 rounded-md border p-0.5 text-xs">
        {quickFilters.map(({ label, active, filter }) => (
          <button
            key={label}
            type="button"
            aria-pressed={active}
            title={
              label === "All"
                ? "All locations"
                : label === "User"
                  ? "Personal configuration"
                  : "Project configuration"
            }
            onClick={() => choose(filter)}
            className={cn(
              "flex-1 rounded-sm px-1 py-1 font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
              active
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
            )}
          >
            {label}
          </button>
        ))}
        <details
          ref={moreRef}
          className="relative shrink-0"
          onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget)) event.currentTarget.open = false;
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              event.currentTarget.open = false;
              event.currentTarget.querySelector("summary")?.focus();
            }
          }}
        >
          <summary
            aria-label="Other locations"
            title="Other locations"
            className={cn(
              "cursor-pointer list-none rounded-sm p-1 text-muted-foreground hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring [&::-webkit-details-marker]:hidden",
              isOther && "bg-accent text-accent-foreground",
            )}
          >
            <Ellipsis className="h-4 w-4" />
          </summary>
          <div className="absolute right-0 top-full z-20 mt-1 w-40 rounded-md border bg-background p-1 shadow-md">
            {OTHER_SCOPES.map((scope) => (
              <button
                key={scope}
                type="button"
                aria-pressed={location.type === "scope" && location.scope === scope}
                onClick={() => choose({ type: "scope", scope })}
                className="flex w-full items-center justify-between rounded-sm px-2 py-1.5 text-left hover:bg-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              >
                {locationFilterLabel({ type: "scope", scope })}
                {location.type === "scope" && location.scope === scope && <Check className="h-3 w-3" />}
              </button>
            ))}
          </div>
        </details>
      </div>
      {isProject && (
        <Select
          aria-label="Project"
          value={location.type === "project" ? location.root : ""}
          title={location.type === "project" ? location.root : "All projects"}
          onChange={(event) =>
            choose(
              event.target.value
                ? { type: "project", root: event.target.value }
                : { type: "scope", scope: "project" },
            )
          }
          className="mt-1.5 h-7 w-full bg-transparent text-xs shadow-none"
        >
          <option value="">All projects</option>
          {location.type === "project" && !projects.some((project) => project.root === location.root) && (
            <option value={location.root} disabled>
              {basename(location.root) || location.root} (not in latest scan)
            </option>
          )}
          {projects.map((project) => (
            <option key={project.root} value={project.root}>
              {basename(project.root) || project.root}
              {(names.get(basename(project.root)) ?? 0) > 1 ? ` — ${dirname(project.root)}` : ""}
              {project.missing ? " (missing)" : ""}
            </option>
          ))}
        </Select>
      )}
      {isOther && (
        <div className="mt-1 flex items-center justify-between px-2 py-1 text-xs text-muted-foreground">
          {locationFilterLabel(location)}
          <button
            type="button"
            onClick={() => choose({ type: "all" })}
            aria-label="Clear location filter"
            className="rounded p-0.5 hover:bg-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          >
            <X className="h-3 w-3" />
          </button>
        </div>
      )}
    </fieldset>
  );
}
