import type { Nodes, Root } from "mdast";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { toast } from "sonner";
import type { Processor } from "unified";
import { describeError } from "@/lib/errors";
import { rpc } from "@/lib/rpc";

/** Codex inbox metadata is already shown beside the run; keep it out of the response. */
function remarkRunDirectives(this: Processor) {
  return (tree: Root, file: { toString(): string }) => {
    const codeRanges: [number, number][] = [];
    const collectCode = (node: Nodes) => {
      if (node.type === "code" || node.type === "inlineCode") {
        const start = node.position?.start.offset;
        const end = node.position?.end.offset;
        if (start !== undefined && end !== undefined) codeRanges.push([start, end]);
      } else if ("children" in node) {
        node.children.forEach(collectCode);
      }
    };
    collectCode(tree);
    const source = file.toString();
    const filtered = source.replace(
      /^ {0,3}::(?:inbox-item|archive-thread)\{[^\r\n]*\}[\t ]*\r?$/gm,
      (directive, offset: number) =>
        codeRanges.some(([start, end]) => offset >= start && offset < end) ? directive : "",
    );
    if (filtered !== source) tree.children = (this.parse(filtered) as Root).children;
  };
}

export function RunMarkdown({ text }: { text: string }) {
  return (
    <div className="run-markdown min-w-0 break-words text-sm">
      <Markdown
        remarkPlugins={[remarkGfm, remarkRunDirectives]}
        skipHtml
        components={{
          a: ({ href, children }) =>
            href && /^https?:\/\//i.test(href) ? (
              <a
                href={href}
                onClick={(event) => {
                  event.preventDefault();
                  void rpc("open_url", { url: href }).catch((error) => toast.error(describeError(error)));
                }}
              >
                {children}
              </a>
            ) : (
              <span>{children}</span>
            ),
          img: ({ alt }) => <span className="text-muted-foreground">[Image{alt ? `: ${alt}` : ""}]</span>,
          table: ({ children }) => (
            <div className="overflow-x-auto">
              <table>{children}</table>
            </div>
          ),
        }}
      >
        {text}
      </Markdown>
    </div>
  );
}
