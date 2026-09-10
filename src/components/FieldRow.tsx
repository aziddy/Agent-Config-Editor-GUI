import type { ReactNode } from "react";
import { Label } from "@/components/ui/label";

export function FieldRow({ label, help, children }: { label: string; help?: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[140px_1fr] items-start gap-3">
      <div className="pt-1.5">
        <Label>{label}</Label>
        {help && <p className="mt-0.5 text-[11px] leading-snug text-muted-foreground/80">{help}</p>}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

export function InfoRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[140px_1fr] items-baseline gap-3 text-sm">
      <Label>{label}</Label>
      <div className="min-w-0 break-words">{children}</div>
    </div>
  );
}
