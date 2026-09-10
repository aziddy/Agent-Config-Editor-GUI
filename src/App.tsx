import { useEffect } from "react";
import { DiffDialog } from "@/components/DiffDialog";
import { Sidebar } from "@/components/Sidebar";
import { describeError } from "@/lib/errors";
import { useConfigChangedListener } from "@/lib/events";
import { useAppSettings, useSnapshot } from "@/lib/queries";
import { EntityScreen } from "@/screens/EntityScreen";
import { ProjectsScreen } from "@/screens/ProjectsScreen";
import { SettingsScreen } from "@/screens/SettingsScreen";
import { useUiStore } from "@/state/uiStore";

function useTheme(theme: "system" | "light" | "dark" | undefined) {
  useEffect(() => {
    const root = document.documentElement;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || ((theme ?? "system") === "system" && media.matches);
      root.classList.toggle("dark", dark);
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}

export function App() {
  const settings = useAppSettings();
  const snapshot = useSnapshot();
  const nav = useUiStore((s) => s.nav);
  useTheme(settings.data?.theme);
  useConfigChangedListener();

  return (
    <div className="flex h-full">
      <Sidebar />
      <main className="min-w-0 flex-1">
        {snapshot.error ? (
          <div className="p-6 text-sm text-destructive">{describeError(snapshot.error)}</div>
        ) : nav === "projects" ? (
          <ProjectsScreen />
        ) : nav === "settings" ? (
          <SettingsScreen />
        ) : (
          <EntityScreen kind={nav} />
        )}
      </main>
      <DiffDialog />
    </div>
  );
}
