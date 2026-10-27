import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { toast } from "sonner";
import type { PluggableList } from "unified";
import { describeError } from "@/lib/errors";
import { rpc } from "@/lib/rpc";

export function MarkdownContent({
  text,
  remarkPlugins = [],
}: {
  text: string;
  remarkPlugins?: PluggableList;
}) {
  return (
    <div className="markdown-content min-w-0 break-words text-sm">
      <Markdown
        remarkPlugins={[remarkGfm, ...remarkPlugins]}
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
