import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { yaml } from "@codemirror/lang-yaml";
import { StreamLanguage } from "@codemirror/language";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { githubDark, githubLight } from "@uiw/codemirror-theme-github";
import CodeMirror, { EditorView, type Extension } from "@uiw/react-codemirror";
import { useEffect, useMemo, useState } from "react";
import type { Language } from "@/lib/types";
import { cn } from "@/lib/utils";

function useIsDark(): boolean {
  const [dark, setDark] = useState(() => document.documentElement.classList.contains("dark"));
  useEffect(() => {
    const obs = new MutationObserver(() => setDark(document.documentElement.classList.contains("dark")));
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => obs.disconnect();
  }, []);
  return dark;
}

function languageExtension(language: Language): Extension[] {
  switch (language) {
    case "json":
      return [json()];
    case "toml":
      return [StreamLanguage.define(toml)];
    case "markdown":
      return [markdown()];
    case "yaml":
      return [yaml()];
    default:
      return [];
  }
}

interface Props {
  value: string;
  language: Language;
  onChange?: (value: string) => void;
  readOnly?: boolean;
  className?: string;
  minHeight?: string;
}

export function CodeEditor({ value, language, onChange, readOnly, className, minHeight }: Props) {
  const dark = useIsDark();
  const extensions = useMemo(() => [...languageExtension(language), EditorView.lineWrapping], [language]);
  return (
    <div className={cn("code-editor overflow-hidden rounded-md border", className)}>
      <CodeMirror
        value={value}
        height="100%"
        minHeight={minHeight ?? "200px"}
        theme={dark ? githubDark : githubLight}
        extensions={extensions}
        readOnly={readOnly}
        editable={!readOnly}
        basicSetup={{ foldGutter: false, highlightActiveLine: !readOnly }}
        onChange={(v) => onChange?.(v)}
      />
    </div>
  );
}
