import { RawFileEditor } from "@/components/RawFileEditor";
import { TopBar } from "@/components/TopBar";
import { useSnapshot } from "@/lib/queries";

export function CodexConfigScreen() {
  const homes = useSnapshot().data?.homes;
  const path = homes ? `${homes.codexHome.replace(/\/+$/, "")}/config.toml` : null;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <TopBar />
      <div className="min-h-0 flex-1 p-3">
        {path ? (
          <RawFileEditor
            key={path}
            path={path}
            readOnly={homes?.readOnly}
            readOnlyHint={
              homes?.readOnly
                ? "Read-only mode is enabled. Editing and backup restores are disabled."
                : undefined
            }
          />
        ) : (
          <p role="status" className="p-3 text-sm text-muted-foreground">
            Loading Codex config…
          </p>
        )}
      </div>
    </div>
  );
}
