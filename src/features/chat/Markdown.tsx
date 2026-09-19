/** Markdown の描画（担当: WS-G）。パースは markdownParser.ts、ここは React 要素への変換だけ */
import { memo, useMemo, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isSafeHref, parseInline, parseMarkdown, type Block, type Inline } from "./markdownParser";

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  const blocks = useMemo(() => parseMarkdown(text), [text]);
  return <div className="md">{renderBlocks(blocks)}</div>;
});

function renderBlocks(blocks: Block[]): ReactNode[] {
  return blocks.map((b, i) => renderBlock(b, i));
}

function renderBlock(b: Block, key: number): ReactNode {
  switch (b.type) {
    case "heading": {
      const H = `h${Math.min(b.level + 2, 6)}` as "h3" | "h4" | "h5" | "h6";
      return <H key={key}>{renderInline(parseInline(b.text))}</H>;
    }
    case "paragraph":
      return <p key={key}>{renderInline(parseInline(b.text))}</p>;
    case "code":
      return <CodeBlock key={key} lang={b.lang} text={b.text} />;
    case "quote":
      return <blockquote key={key}>{renderBlocks(b.blocks)}</blockquote>;
    case "list": {
      const items = b.items.map((it, i) => (
        <li key={i} className={it.checked !== null ? "md-task" : undefined}>
          {it.checked !== null && <input type="checkbox" checked={it.checked} readOnly tabIndex={-1} />}
          {renderListItemBlocks(it.blocks)}
        </li>
      ));
      return b.ordered ? (
        <ol key={key} start={b.start}>
          {items}
        </ol>
      ) : (
        <ul key={key}>{items}</ul>
      );
    }
    case "table":
      return (
        <div key={key} className="md-table-wrap">
          <table>
            <thead>
              <tr>
                {b.header.map((h, i) => (
                  <th key={i} style={{ textAlign: b.align[i] ?? undefined }}>
                    {renderInline(parseInline(h))}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {b.rows.map((r, ri) => (
                <tr key={ri}>
                  {b.header.map((_, ci) => (
                    <td key={ci} style={{ textAlign: b.align[ci] ?? undefined }}>
                      {renderInline(parseInline(r[ci] ?? ""))}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "hr":
      return <hr key={key} />;
  }
}

/** リスト項目が段落 1 つだけなら <p> で包まない（詰めて表示する） */
function renderListItemBlocks(blocks: Block[]): ReactNode {
  if (blocks.length >= 1 && blocks[0].type === "paragraph") {
    return (
      <>
        {renderInline(parseInline(blocks[0].text))}
        {renderBlocks(blocks.slice(1))}
      </>
    );
  }
  return renderBlocks(blocks);
}

function renderInline(nodes: Inline[]): ReactNode[] {
  return nodes.map((n, i) => {
    switch (n.type) {
      case "text":
        return n.text;
      case "code":
        return <code key={i}>{n.text}</code>;
      case "strong":
        return <strong key={i}>{renderInline(n.children)}</strong>;
      case "em":
        return <em key={i}>{renderInline(n.children)}</em>;
      case "del":
        return <del key={i}>{renderInline(n.children)}</del>;
      case "br":
        return <br key={i} />;
      case "link":
        return (
          <ExternalLink key={i} href={n.href}>
            {renderInline(n.children)}
          </ExternalLink>
        );
    }
  });
}

/** WebView 内で遷移させず、既定ブラウザで開く */
export function ExternalLink({ href, children }: { href: string; children: ReactNode }) {
  if (!isSafeHref(href)) return <span title={href}>{children}</span>;
  return (
    <a
      href={href}
      title={href}
      onClick={(e) => {
        e.preventDefault();
        openUrl(href).catch((err) => console.error("リンクを開けませんでした", err));
      }}
    >
      {children}
    </a>
  );
}

export function CodeBlock({ lang, text }: { lang?: string; text: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    navigator.clipboard
      .writeText(text)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      })
      .catch(() => undefined);
  };
  return (
    <div className="md-code">
      <div className="md-code-bar">
        <span>{lang || "text"}</span>
        <button type="button" className="chat-link-button" onClick={copy}>
          {copied ? "コピーしました" : "コピー"}
        </button>
      </div>
      <pre>
        <code>{text}</code>
      </pre>
    </div>
  );
}
