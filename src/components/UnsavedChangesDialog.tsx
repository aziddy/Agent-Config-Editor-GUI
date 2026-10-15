import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useUnsavedChangesStore } from "@/state/unsavedChangesStore";

export function UnsavedChangesDialog() {
  const path = useUnsavedChangesStore((s) => s.path);
  const finish = useUnsavedChangesStore((s) => s.finish);
  return (
    <Dialog
      open={path !== null}
      onOpenChange={(open) => {
        if (!open) finish("cancel");
      }}
      title="Unsaved changes"
      footer={
        <>
          <Button variant="outline" onClick={() => finish("cancel")}>
            Cancel
          </Button>
          <Button variant="outline" onClick={() => finish("discard")}>
            Discard
          </Button>
          <Button onClick={() => finish("save")}>Save…</Button>
        </>
      }
    >
      <p className="mb-2 text-sm">Save your changes before continuing?</p>
      <p className="break-all font-mono text-xs text-muted-foreground">{path}</p>
    </Dialog>
  );
}
