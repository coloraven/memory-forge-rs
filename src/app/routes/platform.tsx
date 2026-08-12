import { useEffect } from "react";
import { useParams, useSearchParams } from "react-router";
import { useDesktop } from "@/features/desktop/provider";
import { EditLogPanel } from "@/features/session/edit-log-panel";
import { EditMessageDialog } from "@/features/session/edit-message-dialog";
import { SessionDetail } from "@/features/session/session-detail";
import { SessionList } from "@/features/session/session-list";
import { WorkspaceTabBar } from "@/features/workspace/workspace-tab-bar";
import { WorkspaceTerminalDrawer } from "@/features/workspace/terminal-drawer";
import { cn } from "@/lib/utils";

export default function PlatformPage() {
  const { platform } = useParams<{ platform: string }>();
  const [searchParams] = useSearchParams();
  const { dispatch, state, isRemote } = useDesktop();
  const sessionFromUrl = searchParams.get("session");
  const hasWorkspaceTabs = state.workspace.openTabs.length > 0;
  const activeInspector = state.workspace.activeTabId
    ? (state.workspace.viewByTabId[state.workspace.activeTabId]?.inspector ??
      null)
    : null;

  useEffect(() => {
    const tabFromUrl = state.workspace.openTabs.find(
      (tab) => tab.platform === platform && tab.sessionKey === sessionFromUrl
    );
    if (tabFromUrl) {
      if (state.workspace.activeTabId !== tabFromUrl.id) {
        dispatch({
          type: "workspace",
          payload: { type: "activate", payload: { tabId: tabFromUrl.id } },
        });
      }
      return;
    }
    if (platform && state.currentPlatform !== platform) {
      dispatch({ type: "setCurrentPlatform", payload: platform });
    }
  }, [
    dispatch,
    platform,
    sessionFromUrl,
    state.currentPlatform,
    state.workspace.activeTabId,
    state.workspace.openTabs,
  ]);

  useEffect(() => {
    if (!platform || state.currentPlatform !== platform) {
      return;
    }
    if (sessionFromUrl === state.selectedSessionKey) {
      return;
    }
    dispatch({ type: "setSelectedSessionKey", payload: sessionFromUrl });
    if (!sessionFromUrl) {
      dispatch({ type: "setSessionDetail", payload: null });
      dispatch({ type: "setShowEditLog", payload: false });
    }
  }, [
    dispatch,
    platform,
    sessionFromUrl,
    state.currentPlatform,
    state.selectedSessionKey,
  ]);

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col">
      {!isRemote && <WorkspaceTabBar />}
      <div
        className={cn(
          "relative flex min-h-0 min-w-0 flex-1 overflow-hidden",
          isRemote
            ? "remote-platform-page"
            : cn(
                "border border-border/60 bg-white/4",
                hasWorkspaceTabs
                  ? "rounded-b-2xl md:rounded-b-[24px]"
                  : "rounded-2xl md:rounded-[24px]"
              )
        )}
      >
        <SessionList />
        {isRemote ? (
          <SessionDetail />
        ) : (
          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <SessionDetail />
            <WorkspaceTerminalDrawer />
          </div>
        )}
        {!isRemote && activeInspector && <EditLogPanel />}
      </div>
      {state.editingBlock && <EditMessageDialog />}
    </div>
  );
}
