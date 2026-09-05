import { Check, Copy } from "lucide-react";
import { useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils";

function CodeBlockRenderer({ code, language }: { code: string; language: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  };

  return (
    <div className="my-3 overflow-hidden rounded-xl border border-border/40 bg-[#0b0e14] text-[#c9d1d9] font-mono text-xs shadow-md">
      <div className="flex items-center justify-between bg-[#11151c] px-4 py-2 border-b border-border/30 text-[10px] text-muted-foreground select-none">
        <span className="font-bold uppercase tracking-wider text-[#79c0ff]">{language || "code"}</span>
        <button
          type="button"
          onClick={handleCopy}
          className={cn(
            "flex items-center gap-1.5 hover:text-foreground transition-colors px-2.5 py-1 rounded bg-white/5 border border-border/20",
            copied && "text-green-400 bg-green-500/10 border-green-500/20",
          )}
        >
          {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="p-4 overflow-x-auto whitespace-pre leading-relaxed">{code}</pre>
    </div>
  );
}

function highlightWord(val: string, searchHighlight?: string): ReactNode {
  if (!searchHighlight) return val;
  const regex = new RegExp(`(${searchHighlight.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi");
  const parts = val.split(regex);
  if (parts.length === 1) return val;
  return parts.map((part, i) =>
    regex.test(part) ? (
      <mark key={i} className="bg-amber-400/35 text-foreground rounded-sm px-0.5">
        {part}
      </mark>
    ) : (
      part
    ),
  );
}

export function parseContentWithCodeBlocks(text: string, searchHighlight?: string) {
  if (!text) return "";

  const parts: ReactNode[] = [];
  const regex = /```(\w*)\n([\s\S]*?)```/g;
  let lastIndex = 0;
  let match;
  let key = 0;

  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push(
        <span key={key++} className="whitespace-pre-wrap leading-relaxed text-sm">
          {highlightWord(text.slice(lastIndex, match.index), searchHighlight)}
        </span>,
      );
    }
    parts.push(<CodeBlockRenderer key={key++} language={match[1]} code={match[2]?.trim()} />);
    lastIndex = regex.lastIndex;
  }

  if (lastIndex < text.length) {
    parts.push(
      <span key={key++} className="whitespace-pre-wrap leading-relaxed text-sm">
        {highlightWord(text.slice(lastIndex), searchHighlight)}
      </span>,
    );
  }

  return parts.length > 0 ? parts : text;
}

const markdownClassName = cn(
  "message-markdown text-sm leading-relaxed text-foreground break-words",
  "[&>:first-child]:mt-0 [&>:last-child]:mb-0",
  "[&_p]:my-2 [&_p]:whitespace-pre-wrap",
  "[&_ul]:my-2 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-2 [&_ol]:list-decimal [&_ol]:pl-5",
  "[&_li]:my-0.5",
  "[&_h1]:mt-3 [&_h1]:mb-2 [&_h1]:text-lg [&_h1]:font-bold",
  "[&_h2]:mt-3 [&_h2]:mb-2 [&_h2]:text-base [&_h2]:font-bold",
  "[&_h3]:mt-2.5 [&_h3]:mb-1.5 [&_h3]:text-sm [&_h3]:font-semibold",
  "[&_blockquote]:my-2 [&_blockquote]:border-l-2 [&_blockquote]:border-border/60 [&_blockquote]:pl-3 [&_blockquote]:text-muted-foreground",
  "[&_hr]:my-3 [&_hr]:border-border/40",
  "[&_a]:text-primary [&_a]:underline [&_a]:underline-offset-2",
  "[&_table]:my-3 [&_table]:w-full [&_table]:border-collapse [&_table]:text-xs",
  "[&_th]:border [&_th]:border-border/40 [&_th]:bg-muted/30 [&_th]:px-2 [&_th]:py-1.5 [&_th]:text-left",
  "[&_td]:border [&_td]:border-border/40 [&_td]:px-2 [&_td]:py-1.5",
  "[&_code]:rounded [&_code]:bg-muted/50 [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.85em]",
  "[&_pre_code]:bg-transparent [&_pre_code]:p-0",
);

function MarkdownContent({ text, className }: { text: string; className?: string }) {
  return (
    <div className={cn(markdownClassName, className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noreferrer noopener">
              {children}
            </a>
          ),
          pre: ({ children }) => <>{children}</>,
          code: ({ className: codeClassName, children, ...props }) => {
            const match = /language-(\w+)/.exec(codeClassName || "");
            const raw = String(children).replace(/\n$/, "");
            const isBlock = Boolean(match) || (typeof children === "string" && children.includes("\n"));
            if (isBlock) {
              return <CodeBlockRenderer language={match?.[1] || ""} code={raw} />;
            }
            return (
              <code className={codeClassName} {...props}>
                {children}
              </code>
            );
          },
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}

export function MessageContent({
  text,
  searchHighlight,
  renderMarkdown,
  className,
  plainClassName,
}: {
  text: string;
  searchHighlight?: string;
  renderMarkdown: boolean;
  className?: string;
  plainClassName?: string;
}) {
  if (!text) return null;

  if (renderMarkdown) {
    return <MarkdownContent text={text} className={className} />;
  }

  return (
    <div className={cn("text-sm font-sans leading-relaxed text-foreground whitespace-pre-wrap break-words", plainClassName)}>
      {parseContentWithCodeBlocks(text, searchHighlight)}
    </div>
  );
}
