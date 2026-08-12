import { LoaderCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Navigate, useSearchParams } from "react-router";
import { api } from "@/features/desktop/api";
import { useDesktop } from "@/features/desktop/provider";
import { EditLogPanel } from "@/features/session/edit-log-panel";
import { EditMessageDialog } from "@/features/session/edit-message-dialog";

export default function RemoteMemoryPage() {
  const { dispatch, isRemote, remoteBootstrap, state, t } = useDesktop();
  const [searchParams] = useSearchParams();
  const [loading, setLoading] = useState(false);
  const platform = searchParams.get("platform") || state.currentPlatform;
  const sessionKey = searchParams.get("session") || state.selectedSessionKey;
  const availablePlatforms = useMemo(
    () => new Set(remoteBootstrap?.platforms.filter((item) => item.available).map((item) => item.id) ?? []),
    [remoteBootstrap?.platforms]
  );
  const validTarget = Boolean(
    sessionKey
      && platform
      && platform !== "dashboard"
      && (availablePlatforms.size === 0 || availablePlatforms.has(platform))
  );

  useEffect(() => {
    if (!isRemote || !validTarget || !sessionKey) return;
    if (state.currentPlatform !== platform) {
      dispatch({ type: "setCurrentPlatform", payload: platform });
      return;
    }
    if (state.selectedSessionKey !== sessionKey) {
      dispatch({ type: "setSelectedSessionKey", payload: sessionKey });
      return;
    }

    let cancelled = false;
    setLoading(true);
    Promise.all([
      api.getSessionDetail(platform, sessionKey),
      api.getEditLog(platform, sessionKey),
    ])
      .then(([detail, editLog]) => {
        if (cancelled) return;
        dispatch({ type: "setSessionDetail", payload: detail });
        dispatch({
          type: "setEditLogForSession",
          payload: { platform, sessionKey, editLog },
        });
      })
      .catch((error) => {
        if (cancelled) return;
        console.error("Failed to load remote memory audit:", error);
        dispatch({
          type: "setSessionStatus",
          payload: { tone: "error", message: t("session.refreshFailed") },
        });
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [
    dispatch,
    isRemote,
    platform,
    sessionKey,
    state.currentPlatform,
    state.selectedSessionKey,
    t,
    validTarget,
  ]);

  if (!isRemote) return <Navigate replace to="/" />;
  if (!validTarget || !sessionKey) return <Navigate replace to="/" />;

  return (
    <div className="remote-memory-route">
      {loading && (
        <div className="remote-detail-loading" role="status" aria-label={t("loading")}>
          <LoaderCircle className="size-5 animate-spin motion-reduce:animate-none" />
        </div>
      )}
      <EditLogPanel />
      {state.editingBlock && <EditMessageDialog />}
    </div>
  );
}
