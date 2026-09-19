/** アプリシェル（担当: WS-F）: 左サイドバー + タスクタブ + タスクビュー */
import { useEffect } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { TaskTabs } from "./components/layout/TaskTabs";
import { TaskView } from "./features/tasks/TaskView";
import { useProjectStore } from "./store/projectStore";
import { useTabStore } from "./store/tabStore";
import { initAgentEventBridge } from "./store/agentStore";

export default function App() {
  const loadProjects = useProjectStore((s) => s.loadProjects);
  const { openTaskIds, activeTaskId } = useTabStore();

  useEffect(() => {
    initAgentEventBridge();
    loadProjects().catch(() => {});
  }, [loadProjects]);

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        <TaskTabs />
        {openTaskIds.length === 0 ? (
          <div className="empty">左のプロジェクトからタスクを開いてください</div>
        ) : (
          // 非アクティブなタブもアンマウントせず保持（会話のスクロール位置・入力中テキストを維持）
          openTaskIds.map((id) => (
            <div key={id} style={{ display: id === activeTaskId ? "contents" : "none" }}>
              <TaskView taskId={id} />
            </div>
          ))
        )}
      </main>
    </div>
  );
}
