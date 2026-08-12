import type {
  SessionTabViewState,
  WorkspaceInspector,
  WorkspaceState,
  WorkspaceTab,
  WorkspaceTabStatus,
} from "@/features/workspace/types";

export const WORKSPACE_STORAGE_KEY = "memory-forge.workspace.v1";
export const WORKSPACE_STORAGE_VERSION = 1;
const MAX_WORKSPACE_TABS = 12;

type PersistedSessionView = Pick<
  SessionTabViewState,
  "composerDraft" | "inspector"
>;

interface PersistedWorkspace {
  activeTabId: string | null;
  openTabs: WorkspaceTab[];
  version: typeof WORKSPACE_STORAGE_VERSION;
  viewByTabId: Record<string, PersistedSessionView>;
}

type StorageLike = Pick<Storage, "getItem" | "setItem">;

const TAB_STATUSES = new Set<WorkspaceTabStatus>([
  "running",
  "attention",
  "idle",
  "done",
]);
const INSPECTORS = new Set<Exclude<WorkspaceInspector, null>>([
  "changes",
  "files",
  "memory",
]);

function emptyView(
  persisted?: Partial<PersistedSessionView>
): SessionTabViewState {
  return {
    detail: null,
    editLog: [],
    loading: false,
    error: null,
    scrollOffset: 0,
    composerDraft:
      typeof persisted?.composerDraft === "string"
        ? persisted.composerDraft
        : "",
    inspector:
      persisted?.inspector === null ||
      (typeof persisted?.inspector === "string" &&
        INSPECTORS.has(
          persisted.inspector as Exclude<WorkspaceInspector, null>
        ))
        ? persisted.inspector
        : null,
    searchQuery: "",
    terminalId: null,
    terminalDrawerOpen: false,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readTab(value: unknown): WorkspaceTab | null {
  if (!isRecord(value)) {
    return null;
  }
  if (
    typeof value.id !== "string" ||
    value.id.length === 0 ||
    value.kind !== "session" ||
    typeof value.platform !== "string" ||
    value.platform.length === 0 ||
    typeof value.sessionKey !== "string" ||
    value.sessionKey.length === 0 ||
    typeof value.title !== "string" ||
    !TAB_STATUSES.has(value.status as WorkspaceTabStatus) ||
    typeof value.openedAt !== "number" ||
    !Number.isFinite(value.openedAt) ||
    typeof value.lastActiveAt !== "number" ||
    !Number.isFinite(value.lastActiveAt)
  ) {
    return null;
  }
  return {
    id: value.id,
    kind: "session",
    lastActiveAt: value.lastActiveAt,
    openedAt: value.openedAt,
    platform: value.platform,
    sessionKey: value.sessionKey,
    status: value.status as WorkspaceTabStatus,
    title: value.title,
  };
}

function getDefaultStorage(): StorageLike | null {
  if (typeof window === "undefined") {
    return null;
  }
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function serializeWorkspaceState(state: WorkspaceState): string {
  const persisted: PersistedWorkspace = {
    version: WORKSPACE_STORAGE_VERSION,
    openTabs: state.openTabs.map((tab) => ({ ...tab })),
    activeTabId: state.activeTabId,
    viewByTabId: Object.fromEntries(
      state.openTabs.map((tab) => {
        const view = state.viewByTabId[tab.id];
        return [
          tab.id,
          {
            composerDraft: view?.composerDraft ?? "",
            inspector: view?.inspector ?? null,
          },
        ];
      })
    ),
  };
  return JSON.stringify(persisted);
}

export function parseWorkspaceState(raw: string | null): WorkspaceState | null {
  if (!raw) {
    return null;
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed) || parsed.version !== WORKSPACE_STORAGE_VERSION) {
      return null;
    }
    if (!(Array.isArray(parsed.openTabs) && isRecord(parsed.viewByTabId))) {
      return null;
    }
    const persistedViews = parsed.viewByTabId;

    const seenSessions = new Set<string>();
    const seenIds = new Set<string>();
    const openTabs: WorkspaceTab[] = [];
    for (const value of parsed.openTabs) {
      const tab = readTab(value);
      if (!tab || seenIds.has(tab.id)) {
        continue;
      }
      const sessionIdentity = JSON.stringify([tab.platform, tab.sessionKey]);
      if (seenSessions.has(sessionIdentity)) {
        continue;
      }
      seenIds.add(tab.id);
      seenSessions.add(sessionIdentity);
      openTabs.push(tab);
      if (openTabs.length >= MAX_WORKSPACE_TABS) {
        break;
      }
    }

    const viewByTabId = Object.fromEntries(
      openTabs.map((tab) => {
        const persistedView = persistedViews[tab.id];
        return [
          tab.id,
          emptyView(isRecord(persistedView) ? persistedView : undefined),
        ];
      })
    );
    const requestedActiveTabId =
      typeof parsed.activeTabId === "string" ? parsed.activeTabId : null;
    const activeTabId = openTabs.some((tab) => tab.id === requestedActiveTabId)
      ? requestedActiveTabId
      : ([...openTabs].sort(
          (left, right) => right.lastActiveAt - left.lastActiveAt
        )[0]?.id ?? null);

    return { openTabs, activeTabId, viewByTabId };
  } catch {
    return null;
  }
}

export function loadWorkspaceState(
  storage: StorageLike | null = getDefaultStorage()
) {
  if (!storage) {
    return null;
  }
  try {
    return parseWorkspaceState(storage.getItem(WORKSPACE_STORAGE_KEY));
  } catch {
    return null;
  }
}

export function persistWorkspaceState(
  state: WorkspaceState,
  storage: StorageLike | null = getDefaultStorage()
) {
  if (!storage) {
    return false;
  }
  try {
    storage.setItem(WORKSPACE_STORAGE_KEY, serializeWorkspaceState(state));
    return true;
  } catch {
    return false;
  }
}
