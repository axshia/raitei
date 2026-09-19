/**
 * チャット用の最小 Markdown パーサ（担当: WS-G）。副作用なし。
 *
 * 依存追加（package.json / lockfile の変更）を避けるため自前実装にしている。
 * HTML は解釈せずテキストとして扱い、描画は React 要素で行う（dangerouslySetInnerHTML は使わない）。
 *
 * 対応: 見出し / 段落 / 改行 / コードフェンス / 引用 / 箇条書き・番号付きリスト（入れ子はインデントで 1 段まで）/
 *       タスクリスト / 表 / 水平線 / インライン（コード・太字・斜体・取り消し線・リンク・自動リンク）
 */

export type Block =
  | { type: "heading"; level: number; text: string }
  | { type: "paragraph"; text: string }
  | { type: "code"; lang: string; text: string }
  | { type: "quote"; blocks: Block[] }
  | { type: "list"; ordered: boolean; start: number; items: ListItem[] }
  | { type: "table"; header: string[]; align: ("left" | "center" | "right" | null)[]; rows: string[][] }
  | { type: "hr" };

export interface ListItem {
  checked: boolean | null;
  blocks: Block[];
}

export type Inline =
  | { type: "text"; text: string }
  | { type: "code"; text: string }
  | { type: "strong"; children: Inline[] }
  | { type: "em"; children: Inline[] }
  | { type: "del"; children: Inline[] }
  | { type: "link"; href: string; children: Inline[] }
  | { type: "br" };

const FENCE = /^ {0,3}(`{3,}|~{3,})\s*([^`\s]*)[^`]*$/;
const HEADING = /^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$/;
const HR = /^ {0,3}([-*_])(\s*\1){2,}\s*$/;
const QUOTE = /^ {0,3}>\s?(.*)$/;
const LIST_ITEM = /^( {0,3})([-*+]|\d{1,9}[.)])\s+(.*)$/;
const TABLE_SEP = /^\s*\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?\s*$/;

export function parseMarkdown(src: string): Block[] {
  return parseBlocks(src.replace(/\r\n?/g, "\n").split("\n"));
}

function parseBlocks(lines: string[]): Block[] {
  const blocks: Block[] = [];
  let i = 0;
  let para: string[] = [];
  const flush = () => {
    if (para.length) {
      blocks.push({ type: "paragraph", text: para.join("\n") });
      para = [];
    }
  };

  while (i < lines.length) {
    const line = lines[i];

    if (!line.trim()) {
      flush();
      i++;
      continue;
    }

    const fence = line.match(FENCE);
    if (fence) {
      flush();
      const marker = fence[1];
      const body: string[] = [];
      i++;
      while (i < lines.length && !isFenceClose(lines[i], marker)) body.push(lines[i++]);
      i++; // 閉じフェンス（なければ末尾まで）
      blocks.push({ type: "code", lang: fence[2] ?? "", text: body.join("\n") });
      continue;
    }

    const heading = line.match(HEADING);
    if (heading) {
      flush();
      blocks.push({ type: "heading", level: heading[1].length, text: heading[2] });
      i++;
      continue;
    }

    if (HR.test(line) && !(para.length && /^\s*-+\s*$/.test(line))) {
      flush();
      blocks.push({ type: "hr" });
      i++;
      continue;
    }

    if (QUOTE.test(line)) {
      flush();
      const body: string[] = [];
      while (i < lines.length && lines[i].trim() && QUOTE.test(lines[i])) body.push(lines[i++].match(QUOTE)![1]);
      blocks.push({ type: "quote", blocks: parseBlocks(body) });
      continue;
    }

    // 表: ヘッダ行 + 区切り行
    if (line.includes("|") && i + 1 < lines.length && TABLE_SEP.test(lines[i + 1]) && lines[i + 1].includes("-")) {
      flush();
      const header = splitRow(line);
      const align = splitRow(lines[i + 1]).map((c) => {
        const l = c.startsWith(":");
        const r = c.endsWith(":");
        return l && r ? "center" : r ? "right" : l ? "left" : null;
      });
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && lines[i].trim() && lines[i].includes("|")) rows.push(splitRow(lines[i++]));
      blocks.push({ type: "table", header, align, rows });
      continue;
    }

    const li = line.match(LIST_ITEM);
    if (li && (!para.length || !/^\d/.test(li[2]) || li[2].startsWith("1"))) {
      flush();
      const [list, next] = parseList(lines, i);
      blocks.push(list);
      i = next;
      continue;
    }

    para.push(line);
    i++;
  }
  flush();
  return blocks;
}

function isFenceClose(line: string, marker: string): boolean {
  const t = line.trim();
  return t.startsWith(marker[0].repeat(marker.length)) && /^([`~])\1*$/.test(t);
}

function splitRow(line: string): string[] {
  let t = line.trim();
  if (t.startsWith("|")) t = t.slice(1);
  if (t.endsWith("|") && !t.endsWith("\\|")) t = t.slice(0, -1);
  const cells: string[] = [];
  let cur = "";
  let inCode = false;
  for (let k = 0; k < t.length; k++) {
    const c = t[k];
    if (c === "\\" && t[k + 1] === "|") {
      cur += "|";
      k++;
    } else if (c === "`") {
      inCode = !inCode;
      cur += c;
    } else if (c === "|" && !inCode) {
      cells.push(cur.trim());
      cur = "";
    } else cur += c;
  }
  cells.push(cur.trim());
  return cells;
}

function parseList(lines: string[], start: number): [Block, number] {
  const first = lines[start].match(LIST_ITEM)!;
  const ordered = /\d/.test(first[2]);
  const baseIndent = first[1].length;
  const items: ListItem[] = [];
  let i = start;
  let current: string[] | null = null;
  let contentIndent = 0;
  const pushItem = () => {
    if (!current) return;
    let checked: boolean | null = null;
    const task = current[0]?.match(/^\[([ xX])\]\s+(.*)$/);
    if (task) {
      checked = task[1] !== " ";
      current[0] = task[2];
    }
    items.push({ checked, blocks: parseBlocks(current) });
  };

  while (i < lines.length) {
    const line = lines[i];
    const m = line.match(LIST_ITEM);
    if (m && m[1].length === baseIndent && /\d/.test(m[2]) === ordered) {
      pushItem();
      current = [m[3]];
      contentIndent = m[1].length + m[2].length + 1;
      i++;
      continue;
    }
    if (!line.trim()) {
      // 空行の後にインデントされた続きがあれば同じ項目
      const nextLine = lines[i + 1];
      if (nextLine !== undefined && indentOf(nextLine) >= contentIndent && nextLine.trim()) {
        current?.push("");
        i++;
        continue;
      }
      break;
    }
    const indent = indentOf(line);
    if (indent > baseIndent) {
      current?.push(line.slice(Math.min(indent, contentIndent)));
      i++;
      continue;
    }
    // 遅延継続行（インデントなしの段落続き）
    if (!m && !FENCE.test(line) && !HEADING.test(line) && !QUOTE.test(line) && current && lines[i - 1]?.trim()) {
      current.push(line.trim());
      i++;
      continue;
    }
    break;
  }
  pushItem();
  const startNum = ordered ? parseInt(first[2], 10) : 1;
  return [{ type: "list", ordered, start: Number.isNaN(startNum) ? 1 : startNum, items }, i];
}

function indentOf(line: string): number {
  const m = line.match(/^[ \t]*/)![0];
  return m.replace(/\t/g, "    ").length;
}

// ---------- インライン ----------

const URL_RE = /^https?:\/\/[^\s<>"'`]+[^\s<>"'`.,;:!?)\]}]/;

export function parseInline(src: string): Inline[] {
  const out: Inline[] = [];
  let buf = "";
  const pushText = () => {
    if (buf) {
      out.push({ type: "text", text: buf });
      buf = "";
    }
  };
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    const rest = src.slice(i);

    // エスケープ
    if (c === "\\" && i + 1 < src.length && /[\\`*_{}[\]()#+\-.!|~<>]/.test(src[i + 1])) {
      buf += src[i + 1];
      i += 2;
      continue;
    }
    // 改行: 行末スペース 2 つ or バックスラッシュ、チャットでは単独改行も改行として扱う
    if (c === "\n") {
      buf = buf.replace(/ +$/, "");
      pushText();
      out.push({ type: "br" });
      i++;
      continue;
    }
    // インラインコード
    if (c === "`") {
      const ticks = rest.match(/^`+/)![0];
      const end = src.indexOf(ticks, i + ticks.length);
      if (end > 0) {
        pushText();
        let code = src.slice(i + ticks.length, end);
        if (code.startsWith(" ") && code.endsWith(" ") && code.trim()) code = code.slice(1, -1);
        out.push({ type: "code", text: code });
        i = end + ticks.length;
        continue;
      }
      buf += ticks;
      i += ticks.length;
      continue;
    }
    // リンク [text](url)
    if (c === "[") {
      const m = matchLink(src, i);
      if (m) {
        pushText();
        out.push({ type: "link", href: m.href, children: parseInline(m.label) });
        i = m.end;
        continue;
      }
    }
    // 自動リンク <https://...> / 裸の URL
    if (c === "<") {
      const m = rest.match(/^<(https?:\/\/[^>\s]+)>/);
      if (m) {
        pushText();
        out.push({ type: "link", href: m[1], children: [{ type: "text", text: m[1] }] });
        i += m[0].length;
        continue;
      }
    }
    if ((c === "h" || c === "H") && (i === 0 || /[\s(（「]/.test(src[i - 1]))) {
      const m = rest.match(URL_RE);
      if (m) {
        pushText();
        out.push({ type: "link", href: m[0], children: [{ type: "text", text: m[0] }] });
        i += m[0].length;
        continue;
      }
    }
    // 強調
    if (c === "*" || c === "_" || c === "~") {
      const em = matchEmphasis(src, i);
      if (em) {
        pushText();
        out.push({ type: em.type, children: parseInline(em.inner) } as Inline);
        i = em.end;
        continue;
      }
    }
    buf += c;
    i++;
  }
  pushText();
  return out;
}

function matchLink(src: string, i: number): { label: string; href: string; end: number } | null {
  let depth = 0;
  let j = i;
  for (; j < src.length; j++) {
    if (src[j] === "\\") {
      j++;
      continue;
    }
    if (src[j] === "[") depth++;
    else if (src[j] === "]" && --depth === 0) break;
    else if (src[j] === "\n" && src[j + 1] === "\n") return null;
  }
  if (j >= src.length || src[j + 1] !== "(") return null;
  const close = src.indexOf(")", j + 2);
  if (close < 0) return null;
  const target = src.slice(j + 2, close).trim();
  const href = target.split(/\s+/)[0].replace(/^<|>$/g, "");
  if (!href || /\s/.test(href)) return null;
  return { label: src.slice(i + 1, j), href, end: close + 1 };
}

function matchEmphasis(src: string, i: number): { type: "strong" | "em" | "del"; inner: string; end: number } | null {
  const c = src[i];
  const run = src.slice(i).match(c === "~" ? /^~+/ : c === "*" ? /^\*+/ : /^_+/)![0];
  if (c === "~") {
    if (run.length !== 2) return null;
    const end = src.indexOf("~~", i + 2);
    if (end < 0 || end === i + 2) return null;
    return { type: "del", inner: src.slice(i + 2, end), end: end + 2 };
  }
  // 単語中の _ は強調にしない（snake_case 対策）
  if (c === "_" && i > 0 && /[\p{L}\p{N}]/u.test(src[i - 1])) return null;
  const len = run.length >= 2 ? 2 : 1;
  const marker = c.repeat(len);
  const after = src[i + len];
  if (!after || /\s/.test(after)) return null;
  let j = i + len;
  while (true) {
    const end = src.indexOf(marker, j);
    if (end < 0) return null;
    const before = src[end - 1];
    const next = src[end + len];
    const validClose =
      end > i + len &&
      !/\s/.test(before) &&
      // `**a**b` の ** は閉じとして扱うが、`*` 1 つの場合 `**` の一部は閉じにしない
      (len === 2 || next !== c) &&
      (c !== "_" || !next || !/[\p{L}\p{N}]/u.test(next));
    if (validClose) return { type: len === 2 ? "strong" : "em", inner: src.slice(i + len, end), end: end + len };
    j = end + 1;
  }
}

/** 外部で開いてよいリンクか（javascript: などを弾く） */
export function isSafeHref(href: string): boolean {
  return /^(https?:|mailto:)/i.test(href);
}
