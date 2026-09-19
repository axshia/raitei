/**
 * PR 状態のポーリング（担当: WS-H）。
 * タブがアクティブかつウィンドウが表示中の間だけ 30 秒ごとに silent で取得する。
 * 非アクティブなタブは display:none で残るため、activeTaskId で判定する。
 */
import { useEffect } from "react";
import { usePrStore } from "../../store/prStore";
import { useTabStore } from "../../store/tabStore";

export const PR_POLL_INTERVAL_MS = 30_000;

export function usePrPolling(taskId: string) {
  const active = useTabStore((s) => s.activeTaskId === taskId);

  useEffect(() => {
    if (!active) return;
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
  }, [taskId, active]);
}
