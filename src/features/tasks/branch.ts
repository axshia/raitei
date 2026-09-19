/** ブランチ名・worktree パスの補助（担当: WS-F）。Rust 側 worktree::worktree_path_for の規約に合わせる */

/** タイトルからブランチ名の候補を作る（ASCII 部分のみ。無ければ日時） */
export function suggestBranch(title: string, now = new Date()): string {
  const slug = title
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40)
    .replace(/-+$/g, "");
  if (slug) return `task/${slug}`;
  const p = (n: number) => String(n).padStart(2, "0");
  return `task/${now.getFullYear()}${p(now.getMonth() + 1)}${p(now.getDate())}-${p(now.getHours())}${p(now.getMinutes())}`;
}

/** git check-ref-format の主要ルールだけを簡易チェック。問題があれば理由を返す */
export function validateBranch(name: string): string | null {
  if (!name) return null;
  if (/\s/.test(name)) return "空白は使えません";
  if (/[~^:?*[\\]/.test(name)) return "~ ^ : ? * [ \\ は使えません";
  if (name.includes("..")) return "「..」は使えません";
  if (name.includes("@{")) return "「@{」は使えません";
  if (name.startsWith("-") || name.startsWith("/") || name.endsWith("/")) return "先頭の - や、先頭・末尾の / は使えません";
  if (name.endsWith(".") || name.endsWith(".lock")) return "末尾の . や .lock は使えません";
  if (name.includes("//") || name.split("/").some((c) => c.startsWith("."))) return "空の階層や . で始まる階層は使えません";
  return null;
}

/** `<repo の親>/<repo 名>.worktrees/<branch の / を - に>` */
export function worktreePathFor(repoPath: string, branch: string): string {
  const trimmed = repoPath.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  const parent = idx > 0 ? trimmed.slice(0, idx) : "";
  const name = trimmed.slice(idx + 1);
  return `${parent}/${name}.worktrees/${branch.replace(/\//g, "-")}`;
}

/** ホームディレクトリを ~ に縮める（表示用） */
export function shortenHome(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, "~");
}
