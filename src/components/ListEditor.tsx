import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface Props {
  items: string[];
  onChange: (items: string[]) => void;
  placeholder?: string;
  readOnly?: boolean;
  mono?: boolean;
}

export function ListEditor({ items, onChange, placeholder, readOnly, mono = true }: Props) {
  return (
    <div className="space-y-1">
      {items.map((item, i) => (
        <div key={`${i}-${item}`} className="flex items-center gap-1">
          <Input
            value={item}
            readOnly={readOnly}
            placeholder={placeholder}
            onChange={(ev) => onChange(items.map((x, j) => (j === i ? ev.target.value : x)))}
            className={mono ? "h-7 font-mono text-xs" : "h-7 text-xs"}
          />
          {!readOnly && (
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              title="Remove"
              onClick={() => onChange(items.filter((_, j) => j !== i))}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      ))}
      {!readOnly && (
        <Button
          variant="outline"
          size="sm"
          className="h-7 gap-1 text-xs"
          onClick={() => onChange([...items, ""])}
        >
          <Plus className="h-3 w-3" /> Add
        </Button>
      )}
    </div>
  );
}
