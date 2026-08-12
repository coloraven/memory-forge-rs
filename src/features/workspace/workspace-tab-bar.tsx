import { X } from "lucide-react";
import { useNavigate } from "react-router";
import { useDesktop } from "@/features/desktop/provider";
import type { WorkspaceTab } from "@/features/workspace/types";
import { cn } from "@/lib/utils";

function platformLabel(platform: string) {
  if (platform === "kiro-ide") {
    return "Kiro IDE";
  }
  if (platform === "opencode") {
    return "OpenCode";
  }
  if (platform === "pi") {
    return "Pi";
  }
  if (platform === "grok") {
    return "Grok";
  }
  return platform.charAt(0).toUpperCase() + platform.slice(1);
}

function sessionPath(tab: WorkspaceTab) {
  return {
    pathname: `/${tab.platform}`,
    search: `?session=${encodeURIComponent(tab.sessionKey)}`,
  };
}

function nextTabAfterClose(
  tabs: WorkspaceTab[],
  activeTabId: string | null,
  closedTabId: string
) {
  if (activeTabId !== closedTabId) {
    return null;
  }
  return (
    [...tabs]
      .filter((tab) => tab.id !== closedTabId)
      .sort((left, right) => right.lastActiveAt - left.lastActiveAt)[0] ?? null
  );
}

export function WorkspaceTabBar() {
  const { state, dispatch, t } = useDesktop();
  const navigate = useNavigate();

  if (state.workspace.openTabs.length === 0) {
    return null;
  }

  const activateTab = (tab: WorkspaceTab) => {
    dispatch({
      type: "workspace",
      payload: { type: "activate", payload: { tabId: tab.id } },
    });
    navigate(sessionPath(tab));
  };

  const closeTab = (tab: WorkspaceTab) => {
    const nextTab = nextTabAfterClose(
      state.workspace.openTabs,
      state.workspace.activeTabId,
      tab.id
    );
    const wasActive = state.workspace.activeTabId === tab.id;
    dispatch({
      type: "workspace",
      payload: { type: "close", payload: { tabId: tab.id } },
    });
    if (wasActive) {
      navigate(
        nextTab
          ? sessionPath(nextTab)
          : { pathname: `/${state.currentPlatform}` }
      );
    }
  };

  return (
    <div className="flex min-h-12 shrink-0 items-center gap-2 border-border/50 border-b bg-card/55 px-2 md:px-3">
      <div
        aria-label={t("session.tabs")}
        className="scrollbar-none flex min-w-0 flex-1 items-center gap-1 overflow-x-auto"
        role="tablist"
      >
        {state.workspace.openTabs.map((tab) => {
          const active = tab.id === state.workspace.activeTabId;
          return (
            <div
              aria-selected={active}
              className={cn(
                "group flex min-w-0 max-w-[18rem] shrink-0 items-center gap-2 border-b-2 px-3 py-2 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/60",
                active
                  ? "border-primary text-foreground"
                  : "border-transparent text-muted-foreground hover:border-border hover:text-foreground"
              )}
              key={tab.id}
              onClick={() => activateTab(tab)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  activateTab(tab);
                }
              }}
              role="tab"
              tabIndex={active ? 0 : -1}
              title={`${platformLabel(tab.platform)} · ${tab.title}`}
            >
              <span
                aria-hidden="true"
                className={cn(
                  "size-2 shrink-0 rounded-full",
                  tab.status === "running" && "bg-emerald-400",
                  tab.status === "attention" && "bg-amber-400",
                  tab.status === "done" && "bg-sky-400",
                  tab.status === "idle" && "bg-muted-foreground/50"
                )}
              />
              <span className="shrink-0 font-medium text-muted-foreground/80">
                {platformLabel(tab.platform)}
              </span>
              <span className="min-w-0 truncate">{tab.title}</span>
              <button
                aria-label={`${t("session.closeTab")}: ${tab.title}`}
                className="ml-1 flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground/60 opacity-70 transition-colors hover:bg-muted hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/60 group-hover:opacity-100"
                onClick={(event) => {
                  event.stopPropagation();
                  closeTab(tab);
                }}
                title={t("session.closeTab")}
                type="button"
              >
                <X className="size-3.5" />
              </button>
            </div>
          );
        })}
      </div>
      <span className="hidden shrink-0 font-medium text-[10px] text-muted-foreground/45 uppercase tracking-[0.16em] xl:inline">
        {state.workspace.openTabs.length}/12
      </span>
    </div>
  );
}
