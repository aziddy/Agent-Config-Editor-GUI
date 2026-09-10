import { forwardRef, type SelectHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

const selectClassName =
  "flex h-8 min-w-0 rounded-md border border-input bg-background px-2 py-1 text-xs shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50";

export type SelectProps = SelectHTMLAttributes<HTMLSelectElement>;

export const Select = forwardRef<HTMLSelectElement, SelectProps>(({ className, ...props }, ref) => (
  <select ref={ref} className={cn(selectClassName, className)} {...props} />
));
Select.displayName = "Select";
