import { cn } from "@/lib/utils";

export interface TabItem<T extends string> {
  value: T;
  label: string;
  disabled?: boolean;
}

interface TabsProps<T extends string> {
  value: T;
  onValueChange: (value: T) => void;
  items: TabItem<T>[];
  className?: string;
}

export function Tabs<T extends string>({ value, onValueChange, items, className }: TabsProps<T>) {
  return (
    <div role="tablist" className={cn("inline-flex items-center gap-1 rounded-md bg-muted p-1", className)}>
      {items.map((item) => (
        <button
          key={item.value}
          type="button"
          role="tab"
          aria-selected={value === item.value}
          disabled={item.disabled}
          onClick={() => onValueChange(item.value)}
          className={cn(
            "rounded-sm px-3 py-1 text-xs font-medium transition-colors disabled:opacity-40",
            value === item.value
              ? "bg-background text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
