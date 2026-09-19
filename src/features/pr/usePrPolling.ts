/**
 * PR 状態のポーリング（担当: WS-H）。
 * PR サブビューが表示中（タブがアクティブかつ PR を選択中）で、ウィンドウが表示中の間だけ 30 秒ごとに silent で取得する。
 * 非アクティブなタブや一度開いたサブビューは hidden でマウントされたままなので、useIsSubViewVisible で判定する。
 * 表示に戻ったときは、前回取得から 30 秒近く経っていればすぐに取り直す。
 */
import { useEffect } from "react";
import { usePrStore } from "../../store/prStore";
import { useIsSubViewVisible } from "../../store/tabStore";

export const PR_POLL_INTERVAL_MS = 30_000;

export function usePrPolling(taskId: string) {
  const visible = useIsSubViewVisible(taskId, "pr");

  useEffect(() => {
    if (!visible) return;
    const { refreshPr } = usePrStore.getState();

    const refreshIfStale = () => {
      if (document.visibilityState !== "visible") return;
      const s = usePrStore.getState();
      const fetchedAt = s.prFetchedAtByTask[taskId];
      if (fetchedAt === undefined) {
        void refreshPr(taskId);
      } else if (Date.now() - fetchedAt >= PR_POLL_INTERVAL_MS - 1000) {
        void refreshPr(taskId, { silent: true });
      }
    };

    refreshIfStale();
    const timer = window.setInterval(refreshIfStale, PR_POLL_INTERVAL_MS);
    document.addEventListener("visibilitychange", refreshIfStale);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", refreshIfStale);
    };
  }, [taskId, visible]);
}
