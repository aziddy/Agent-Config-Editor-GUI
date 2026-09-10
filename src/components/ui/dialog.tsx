import { X } from "lucide-react";
import { type ReactNode, type RefObject, useEffect, useRef } from "react";
import { cn } from "@/lib/utils";

export interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title?: string;
  description?: string;
  children?: ReactNode;
  footer?: ReactNode;
  className?: string;
  initialFocusRef?: RefObject<HTMLElement | null>;
}

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  className,
  initialFocusRef,
}: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (open && !el.open) {
      el.showModal();

      if (initialFocusRef?.current) {
        const frame = requestAnimationFrame(() => {
          initialFocusRef.current?.focus();
        });
        return () => cancelAnimationFrame(frame);
      }
    } else if (!open && el.open) {
      el.close();
    }
  }, [initialFocusRef, open]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onCancel = (e: Event) => {
      e.preventDefault();
      onOpenChange(false);
    };
    el.addEventListener("cancel", onCancel);
    return () => el.removeEventListener("cancel", onCancel);
  }, [onOpenChange]);

  return (
    <dialog
      ref={ref}
      className={cn(
        "backdrop:bg-black/50 backdrop:backdrop-blur-sm rounded-lg border bg-background text-foreground shadow-lg p-0 w-full max-w-md",
        className,
      )}
      onClick={(e) => {
        if (e.target === ref.current) onOpenChange(false);
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onOpenChange(false);
      }}
    >
      <div className="flex items-start justify-between p-4 pb-2">
        <div>
          {title && <h2 className="text-lg font-semibold">{title}</h2>}
          {description && <p className="text-sm text-muted-foreground">{description}</p>}
        </div>
        <button
          type="button"
          onClick={() => onOpenChange(false)}
          className="rounded-md p-1 hover:bg-accent"
          aria-label="Close"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
      <div className="px-4 pb-2">{children}</div>
      {footer && <div className="flex justify-end gap-2 p-4 border-t">{footer}</div>}
    </dialog>
  );
}
