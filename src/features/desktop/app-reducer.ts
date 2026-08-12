import type { AppAction, AppState } from "@/features/desktop/types";
import { loadWorkspaceState } from "@/features/workspace/persistence";
import {
  createInitialWorkspaceState,
  findWorkspaceTab,
  selectActiveSessionTabView,
  selectActiveWorkspaceTab,
  workspaceReducer,
} from "@/features/workspace/reducer";

export function createInitialAppState(): AppState {
  const workspace = loadWorkspaceState() ?? createInitialWorkspaceState();
  const activeTab = selectActiveWorkspaceTab(workspace);
  const activeView = selectActiveSessionTabView(workspace);
  return {
    workspace,
    currentPlatform: activeTab?.platform ?? "dashboard",
    sessions: [],
    selectedSessionKey: activeTab?.sessionKey ?? null,
    sessionDetail: activeView?.detail ?? null,
    dashboard: null,
    roleFilter: "all",
    searchQuery: activeView?.searchQuery ?? "",
    editingBlock: null,
    editLog: activeView?.editLog ?? [],
    showEditLog: activeView?.inspector === "memory",
    locateMessageTarget: null,
    inspectMemoryTarget: null,
    sessionStatus: null,
    mobileSidebarOpen: false,
    showArchived: false,
  };
}

function syncActiveWorkspaceState(
  state: AppState,
  workspace: AppState["workspace"]
): AppState {
  const activeTab = selectActiveWorkspaceTab(workspace);
  const activeView = selectActiveSessionTabView(workspace);
  return {
    ...state,
    workspace,
    currentPlatform: activeTab?.platform ?? state.currentPlatform,
    selectedSessionKey: activeTab?.sessionKey ?? null,
    sessionDetail: activeView?.detail ?? null,
    editLog: activeView?.editLog ?? [],
    showEditLog: activeView?.inspector === "memory",
    locateMessageTarget: null,
    inspectMemoryTarget: null,
    searchQuery: activeView?.searchQuery ?? "",
    sessionStatus: null,
  };
}

function reduceSelectedSessionKey(
  state: AppState,
  sessionKey: string | null
): AppState {
  if (!sessionKey) {
    return {
      ...state,
      workspace: workspaceReducer(state.workspace, { type: "deactivate" }),
      selectedSessionKey: null,
      sessionStatus: null,
    };
  }

  const session = state.sessions.find((item) => item.sessionKey === sessionKey);
  const workspace = workspaceReducer(state.workspace, {
    type: "open",
    payload: {
      platform: state.currentPlatform,
      sessionKey,
      title: session?.displayTitle ?? sessionKey,
    },
  });
  const activeView = selectActiveSessionTabView(workspace);
  return {
    ...state,
    workspace,
    selectedSessionKey: sessionKey,
    sessionDetail: activeView?.detail ?? null,
    editLog: activeView?.editLog ?? [],
    showEditLog: activeView?.inspector === "memory",
    searchQuery: activeView?.searchQuery ?? state.searchQuery,
    sessionStatus: null,
  };
}

function reduceSessionDetail(
  state: AppState,
  detail: AppState["sessionDetail"]
): AppState {
  if (!detail) {
    const activeTabId = state.workspace.activeTabId;
    const workspace = activeTabId
      ? workspaceReducer(state.workspace, {
          type: "update-detail",
          payload: { tabId: activeTabId, detail: null },
        })
      : state.workspace;
    return { ...state, workspace, sessionDetail: null };
  }

  const targetTab = findWorkspaceTab(
    state.workspace,
    detail.platform,
    detail.sessionKey
  );
  if (targetTab) {
    let workspace = workspaceReducer(state.workspace, {
      type: "update-detail",
      payload: { tabId: targetTab.id, detail },
    });
    workspace = workspaceReducer(workspace, {
      type: "update-tab",
      payload: {
        tabId: targetTab.id,
        updates: {
          title: detail.aliasTitle || detail.title || detail.sessionId,
        },
      },
    });
    return targetTab.id === workspace.activeTabId
      ? { ...state, workspace, sessionDetail: detail }
      : { ...state, workspace };
  }

  if (
    state.currentPlatform !== detail.platform ||
    state.selectedSessionKey !== detail.sessionKey
  ) {
    return state;
  }

  let workspace = workspaceReducer(state.workspace, {
    type: "open",
    payload: {
      platform: detail.platform,
      sessionKey: detail.sessionKey,
      title: detail.aliasTitle || detail.title,
    },
  });
  const openedTab = findWorkspaceTab(
    workspace,
    detail.platform,
    detail.sessionKey
  );
  if (openedTab) {
    workspace = workspaceReducer(workspace, {
      type: "update-detail",
      payload: { tabId: openedTab.id, detail },
    });
    workspace = workspaceReducer(workspace, {
      type: "update-tab",
      payload: {
        tabId: openedTab.id,
        updates: {
          title: detail.aliasTitle || detail.title || detail.sessionId,
        },
      },
    });
  }
  return { ...state, workspace, sessionDetail: detail };
}

function reduceEditLog(
  state: AppState,
  editLog: AppState["editLog"]
): AppState {
  const activeTabId = state.workspace.activeTabId;
  const workspace = activeTabId
    ? workspaceReducer(state.workspace, {
        type: "update-edit-log",
        payload: { tabId: activeTabId, editLog },
      })
    : state.workspace;
  return { ...state, workspace, editLog };
}

function reduceEditLogForSession(
  state: AppState,
  payload: Extract<AppAction, { type: "setEditLogForSession" }>["payload"]
): AppState {
  const targetTab = findWorkspaceTab(
    state.workspace,
    payload.platform,
    payload.sessionKey
  );
  if (!targetTab) {
    return state;
  }
  const workspace = workspaceReducer(state.workspace, {
    type: "update-edit-log",
    payload: { tabId: targetTab.id, editLog: payload.editLog },
  });
  return targetTab.id === workspace.activeTabId
    ? { ...state, workspace, editLog: payload.editLog }
    : { ...state, workspace };
}

function reduceShowEditLog(state: AppState, showEditLog: boolean): AppState {
  const activeTabId = state.workspace.activeTabId;
  const workspace = activeTabId
    ? workspaceReducer(state.workspace, {
        type: "restore-view-state",
        payload: {
          tabId: activeTabId,
          state: { inspector: showEditLog ? "memory" : null },
        },
      })
    : state.workspace;
  return { ...state, workspace, showEditLog };
}

export function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "workspace":
      return syncActiveWorkspaceState(
        state,
        workspaceReducer(state.workspace, action.payload)
      );
    case "setCurrentPlatform":
      return {
        ...state,
        workspace: workspaceReducer(state.workspace, { type: "deactivate" }),
        currentPlatform: action.payload,
        selectedSessionKey: null,
        sessionDetail: null,
        editLog: [],
        showEditLog: false,
        sessionStatus: null,
      };
    case "setSessions":
      return { ...state, sessions: action.payload };
    case "updateSession":
      return {
        ...state,
        sessions: state.sessions.map((session) =>
          session.sessionKey === action.payload.sessionKey
            ? { ...session, ...action.payload.updates }
            : session
        ),
      };
    case "setSelectedSessionKey":
      return reduceSelectedSessionKey(state, action.payload);
    case "setSessionDetail":
      return reduceSessionDetail(state, action.payload);
    case "setDashboard":
      return { ...state, dashboard: action.payload };
    case "setRoleFilter":
      return { ...state, roleFilter: action.payload };
    case "setSearchQuery":
      return {
        ...state,
        workspace: state.workspace.activeTabId
          ? workspaceReducer(state.workspace, {
              type: "restore-view-state",
              payload: {
                tabId: state.workspace.activeTabId,
                state: { searchQuery: action.payload },
              },
            })
          : state.workspace,
        searchQuery: action.payload,
      };
    case "setEditingBlock":
      return { ...state, editingBlock: action.payload };
    case "setEditLog":
      return reduceEditLog(state, action.payload);
    case "setEditLogForSession":
      return reduceEditLogForSession(state, action.payload);
    case "setShowEditLog":
      return reduceShowEditLog(state, action.payload);
    case "setLocateMessageTarget":
      return { ...state, locateMessageTarget: action.payload };
    case "setInspectMemoryTarget":
      return { ...state, inspectMemoryTarget: action.payload };
    case "setSessionStatus":
      return { ...state, sessionStatus: action.payload };
    case "setMobileSidebarOpen":
      return { ...state, mobileSidebarOpen: action.payload };
    case "setShowArchived":
      return {
        ...state,
        workspace: workspaceReducer(state.workspace, { type: "deactivate" }),
        showArchived: action.payload,
        selectedSessionKey: null,
        sessionDetail: null,
      };
    default:
      return state;
  }
}
