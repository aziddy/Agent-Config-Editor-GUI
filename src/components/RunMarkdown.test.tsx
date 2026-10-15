import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { RunMarkdown } from "./RunMarkdown";

describe("run conversation Markdown", () => {
  it("renders results with tables, lists and fenced code", () => {
    const html = renderToStaticMarkup(
      <RunMarkdown
        text={"| Result | Value |\n| --- | --- |\n| Run | OK |\n\n- Checked\n\n```sh\ngit status\n```"}
      />,
    );
    expect(html).toContain("<table>");
    expect(html).toContain("<ul>");
    expect(html).toContain("git status");
  });

  it("does not render raw HTML, remote images or executable links", () => {
    const html = renderToStaticMarkup(
      <RunMarkdown
        text={
          "<script>bad()</script>\n\n![result](https://example.com/private.png)\n\n[run](javascript:bad())\n\n[report](file:///private)"
        }
      />,
    );
    expect(html).not.toContain("<script");
    expect(html).not.toContain("<img");
    expect(html).not.toContain("javascript:");
    expect(html).not.toContain("file://");
    expect(html).toContain("[Image: result]");
  });

  it("hides standalone Codex inbox directives without changing the final response", () => {
    const text =
      'No changes since the previous run.\n::inbox-item{title="No changes" summary="Main remains at abc123"}\n::archive-thread{}';
    const html = renderToStaticMarkup(<RunMarkdown text={text} />);
    expect(html).toContain("No changes since the previous run.");
    expect(html).not.toContain("::inbox-item");
    expect(html).not.toContain("::archive-thread");
    expect(text).toContain("::inbox-item");
  });

  it("preserves directives in code examples, quoted prose and unknown directives", () => {
    const directive = '::inbox-item{title="Example" summary="Example"}';
    const text = [
      `\`\`\`text\n${directive}\n\`\`\``,
      `    ${directive}`,
      `\`${directive}\``,
      `\`multiline\n${directive}\nexample\``,
      `> ${directive}`,
      `Explain ${directive} here.`,
      "::unknown-directive{}",
    ].join("\n\n");
    const html = renderToStaticMarkup(<RunMarkdown text={text} />);
    expect(html.match(/::inbox-item/g)).toHaveLength(6);
    expect(html).toContain("::unknown-directive{}");
  });
});
