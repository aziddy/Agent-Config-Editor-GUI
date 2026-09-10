import { File, Folder } from "lucide-react";
import { useState } from "react";
import { PathActions } from "@/components/PathActions";
import { Button } from "@/components/ui/button";
import { basename, dirname } from "@/lib/entities";
import { describeError } from "@/lib/errors";
import { useDirListing } from "@/lib/queries";

interface Props {
  root: string;
  onOpenFile: (path: string) => void;
}

/** Browse the entity's directory; click a file to load it in the raw editor. */
export function FilesTab({ root, onOpenFile }: Props) {
  const [dir, setDir] = useState(root);
  const listing = useDirListing(dir);
  if (listing.error) return <p className="text-sm text-destructive">{describeError(listing.error)}</p>;
  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground" title={dir}>
          {dir}
        </span>
        {dir !== root && (
          <Button variant="ghost" size="sm" className="h-7 text-xs" onClick={() => setDir(dirname(dir))}>
            Up
          </Button>
        )}
        <PathActions path={dir} size="icon" />
      </div>
      <ul className="divide-y rounded border text-sm">
        {(listing.data ?? []).map((entry) => (
          <li key={entry.path} className="flex items-center gap-2 px-2 py-1">
            {entry.isDir ? (
              <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            ) : (
              <File className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            )}
            <button
              type="button"
              className="min-w-0 flex-1 truncate text-left hover:underline"
              onClick={() => (entry.isDir ? setDir(entry.path) : onOpenFile(entry.path))}
            >
              {basename(entry.path)}
            </button>
            {!entry.isDir && (
              <span className="text-[11px] tabular-nums text-muted-foreground">
                {entry.size.toLocaleString()} B
              </span>
            )}
            <PathActions path={entry.path} size="icon" />
          </li>
        ))}
        {listing.data?.length === 0 && (
          <li className="px-2 py-2 text-xs text-muted-foreground">Empty directory</li>
        )}
      </ul>
    </div>
  );
}
