import type { Nodes, Root } from "mdast";
import type { Processor } from "unified";
import { MarkdownContent } from "@/components/MarkdownContent";

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
  return <MarkdownContent text={text} remarkPlugins={[remarkRunDirectives]} />;
}
