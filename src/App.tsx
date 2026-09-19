/**
 * アプリシェル（担当: WS-F）: 左サイドバー + タスクタブ + タスクビュー。
 *
 * - 起動時: agent://event 購読開始（WS-G の initAgentEventBridge）、プロジェクト・タスク・環境情報の読み込み
 * - 復元したタブのうち存在しないタスクのものは閉じる
 * - 非アクティブなタブもアンマウントせず hidden で保持（会話のスクロール位置・入力中テキストを維持）
 * - ショートカット: ⌘1〜9 タブ切替 / ⌘⇧[ ⌘⇧] 左右のタブ / ⌃1〜4 サブビュー切替
 * - macOS はタイトルバーを Overlay にしているため、最上段（サイドバー見出し + タスクタブ）をヘッダーとして
 *   信号機ボタンの分だけ左を空ける。フルスクリーン中は信号機が消えるので余白を外す
 */
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Sidebar } from "./components/layout/Sidebar";
import { TaskTabs } from "./components/layout/TaskTabs";
import { SUB_VIEWS } from "./components/layout/SubViewTabs";
import { TaskView } from "./features/tasks/TaskView";
import { useProjectStore } from "./store/projectStore";
import { useTabStore } from "./store/tabStore";
import { initAgentEventBridge } from "./store/agentStore";
import "./components/layout/layout.css";
import "./features/projects/projects.css";
import "./features/tasks/tasks.css";

function useStartup() {
  const loadProjects = useProjectStore((s) => s.loadProjects);
  const loadEnvironment = useProjectStore((s) => s.loadEnvironment);
  const loaded = useProjectStore((s) => s.loaded);
  const tasksByProject = useProjectStore((s) => s.tasksByProject);
  const projectCount = useProjectStore((s) => s.projects.length);

  useEffect(() => {
    initAgentEventBridge();
    loadProjects().catch(() => {});
    void loadEnvironment();
  }, [loadProjects, loadEnvironment]);

  // タスク一覧が揃ったら、存在しないタスクのタブを閉じる（読み込み失敗したプロジェクトがある間は閉じない）
  useEffect(() => {
    // 読み込みエラー中は（一覧が不完全な可能性があるため）閉じない
    if (!loaded || useProjectStore.getState().error) return;
    const lists = Object.values(tasksByProject);
    if (lists.length < projectCount) return;
    useTabStore.getState().pruneTabs(lists.flat().map((t) => t.id));
  }, [loaded, tasksByProject, projectCount]);
}

const IS_MAC = navigator.userAgent.includes("Mac OS X");

/** macOS でウィンドウがフルスクリーンかどうか（信号機ボタンが表示されない状態）を追跡する */
function useFullscreen(): boolean {
  const [fullscreen, setFullscreen] = useState(false);
  useEffect(() => {
    if (!IS_MAC) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    try {
      const win = getCurrentWindow();
      const sync = () =>
        win
          .isFullscreen()
          .then((v) => !disposed && setFullscreen(v))
          .catch(() => {});
      void sync();
      win
        .onResized(() => void sync())
        .then((fn) => (disposed ? fn() : (unlisten = fn)))
        .catch(() => {});
    } catch {
      // Tauri 外（ブラウザでの表示確認など）では何もしない
    }
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  return fullscreen;
}

function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (document.querySelector(".modal-backdrop")) return;
      const tabs = useTabStore.getState();
      if (e.metaKey && !e.ctrlKey && !e.altKey && /^[1-9]$/.test(e.key)) {
        const id = tabs.openTaskIds[Number(e.key) - 1];
        if (id) {
          e.preventDefault();
          tabs.activateTask(id);
        }
      } else if (e.metaKey && e.shiftKey && (e.code === "BracketLeft" || e.code === "BracketRight")) {
        e.preventDefault();
        tabs.cycleTab(e.code === "BracketLeft" ? -1 : 1);
      } else if (e.ctrlKey && !e.metaKey && !e.altKey && /^[1-4]$/.test(e.key) && tabs.activeTaskId) {
        e.preventDefault();
        tabs.setSubView(tabs.activeTaskId, SUB_VIEWS[Number(e.key) - 1].key);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

function EmptyState() {
  const hasProjects = useProjectStore((s) => s.projects.length > 0);
  return (
    <div className="empty welcome">
      <div className="welcome-title">raitei</div>
      <div>
        {hasProjects
          ? "左のプロジェクトからタスクを開くか、「+」で新しいタスクを作成してください。"
          : "サイドバーの「追加」から git リポジトリを登録してください。"}
      </div>
      <div className="welcome-keys subtle">
        <span>
          <span className="kbd">⌘1</span>〜<span className="kbd">⌘9</span> タブ切替
        </span>
        <span>
          <span className="kbd">⌘⇧[</span> <span className="kbd">⌘⇧]</span> 前後のタブ
        </span>
        <span>
          <span className="kbd">⌃1</span>〜<span className="kbd">⌃4</span> チャット / PR / コンフリクト / 変更
        </span>
      </div>
    </div>
  );
}

export default function App() {
  const openTaskIds = useTabStore((s) => s.openTaskIds);
  const activeTaskId = useTabStore((s) => s.activeTaskId);
  const fullscreen = useFullscreen();
  useStartup();
  useShortcuts();

  return (
    <div className={`app ${IS_MAC && !fullscreen ? "traffic-light-inset" : ""}`}>
      <Sidebar />
      <main className="main">
        <TaskTabs />
        {openTaskIds.length === 0 && <EmptyState />}
        {openTaskIds.map((id) => (
          <div key={id} className="task-host" hidden={id !== activeTaskId}>
            <TaskView taskId={id} />
          </div>
        ))}
      </main>
    </div>
  );
}
