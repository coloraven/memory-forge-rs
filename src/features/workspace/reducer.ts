import type {
  SessionTabViewState,
  WorkspaceAction,
  WorkspaceState,
  WorkspaceTab,
  WorkspaceTabInput,
} from "@/features/workspace/types";

export const MAX_WORKSPACE_TABS = 12;

export function createWorkspaceTabId(platform: string, sessionKey: string) {
  return `session:${encodeURIComponent(platform)}:${encodeURIComponent(sessionKey)}`;
}

export function createEmptySessionTabViewState(): SessionTabViewState {
  return {
    detail: null,
    editLog: [],
    loading: false,
    error: null,
    scrollOffset: 0,
    composerDraft: "",
    inspector: null,
    searchQuery: "",
    terminalId: null,
    terminalDrawerOpen: false,
  };
}

export function createInitialWorkspaceState(): WorkspaceState {
  return {
    openTabs: [],
    activeTabId: null,
    viewByTabId: {},
  };
}

export function findWorkspaceTab(
  state: WorkspaceState,
  platform: string,
  sessionKey: string
) {
  return state.openTabs.find(
    (tab) => tab.platform === platform && tab.sessionKey === sessionKey
  );
}

export function selectActiveWorkspaceTab(state: WorkspaceState) {
  if (!state.activeTabId) {
    return null;
  }
  return state.openTabs.find((tab) => tab.id === state.activeTabId) ?? null;
}

export function selectActiveSessionTabView(state: WorkspaceState) {
  if (!state.activeTabId) {
    return null;
  }
  return state.viewByTabId[state.activeTabId] ?? null;
}

function createWorkspaceTab(input: WorkspaceTabInput): WorkspaceTab {
  const now = input.now ?? Date.now();
  return {
    id: input.id ?? createWorkspaceTabId(input.platform, input.sessionKey),
    kind: "session",
    platform: input.platform,
    sessionKey: input.sessionKey,
    title: input.title?.trim() || input.sessionKey,
    status: input.status ?? "idle",
    openedAt: now,
    lastActiveAt: now,
  };
}

function updateView(
  state: WorkspaceState,
  tabId: string,
  updater: (view: SessionTabViewState) => SessionTabViewState
) {
  const current = state.viewByTabId[tabId];
  if (!current) {
    return state;
  }
  return {
    ...state,
    viewByTabId: {
      ...state.viewByTabId,
      [tabId]: updater(current),
    },
  };
}

function openTab(
  state: WorkspaceState,
  input: WorkspaceTabInput
): WorkspaceState {
  const existing = findWorkspaceTab(state, input.platform, input.sessionKey);
  const now = input.now ?? Date.now();
  if (existing) {
    const openTabs = state.openTabs.map((tab) =>
      tab.id === existing.id
        ? {
            ...tab,
            title: input.title?.trim() || tab.title,
            status: input.status ?? tab.status,
            lastActiveAt: now,
          }
        : tab
    );
    return {
      ...state,
      openTabs,
      activeTabId: existing.id,
      viewByTabId: state.viewByTabId[existing.id]
        ? state.viewByTabId
        : {
            ...state.viewByTabId,
            [existing.id]: createEmptySessionTabViewState(),
          },
    };
  }

  if (state.openTabs.length >= MAX_WORKSPACE_TABS) {
    return state;
  }
  const tab = createWorkspaceTab({ ...input, now });
  return {
    ...state,
    openTabs: [...state.openTabs, tab],
    activeTabId: tab.id,
    viewByTabId: {
      ...state.viewByTabId,
      [tab.id]: createEmptySessionTabViewState(),
    },
  };
}

function closeTab(state: WorkspaceState, tabId: string): WorkspaceState {
  if (!state.openTabs.some((tab) => tab.id === tabId)) {
    return state;
  }
  const openTabs = state.openTabs.filter((tab) => tab.id !== tabId);
  const { [tabId]: _closedView, ...viewByTabId } = state.viewByTabId;
  let activeTabId = state.activeTabId;
  if (activeTabId === tabId) {
    activeTabId =
      [...openTabs].sort(
        (left, right) => right.lastActiveAt - left.lastActiveAt
      )[0]?.id ?? null;
  }
  return { ...state, openTabs, activeTabId, viewByTabId };
}

export function workspaceReducer(
  state: WorkspaceState,
  action: WorkspaceAction
): WorkspaceState {
  switch (action.type) {
    case "open":
      return openTab(state, action.payload);
    case "activate": {
      if (!state.openTabs.some((tab) => tab.id === action.payload.tabId)) {
        return state;
      }
      const now = action.payload.now ?? Date.now();
      return {
        ...state,
        activeTabId: action.payload.tabId,
        openTabs: state.openTabs.map((tab) =>
          tab.id === action.payload.tabId ? { ...tab, lastActiveAt: now } : tab
        ),
      };
    }
    case "deactivate":
      return state.activeTabId ? { ...state, activeTabId: null } : state;
    case "close":
      return closeTab(state, action.payload.tabId);
    case "update-tab":
      if (!state.openTabs.some((tab) => tab.id === action.payload.tabId)) {
        return state;
      }
      return {
        ...state,
        openTabs: state.openTabs.map((tab) =>
          tab.id === action.payload.tabId
            ? { ...tab, ...action.payload.updates }
            : tab
        ),
      };
    case "update-detail":
      return updateView(state, action.payload.tabId, (view) => ({
        ...view,
        detail: action.payload.detail,
        error: null,
      }));
    case "update-edit-log":
      return updateView(state, action.payload.tabId, (view) => ({
        ...view,
        editLog: action.payload.editLog,
      }));
    case "set-loading":
      return updateView(state, action.payload.tabId, (view) => ({
        ...view,
        loading: action.payload.loading,
      }));
    case "set-error":
      return updateView(state, action.payload.tabId, (view) => ({
        ...view,
        error: action.payload.error,
      }));
    case "restore-view-state":
      return updateView(state, action.payload.tabId, (view) => ({
        ...view,
        ...action.payload.state,
      }));
    case "hydrate":
      return action.payload;
    default:
      return state;
  }
}
