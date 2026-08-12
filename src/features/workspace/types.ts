import type { EditLogEntry, SessionDetail } from "@/features/desktop/types";

export type WorkspaceTabStatus = "running" | "attention" | "idle" | "done";
export type WorkspaceInspector = "changes" | "files" | "memory" | null;

export interface WorkspaceTab {
  id: string;
  kind: "session";
  lastActiveAt: number;
  openedAt: number;
  platform: string;
  sessionKey: string;
  status: WorkspaceTabStatus;
  title: string;
}

export interface SessionTabViewState {
  composerDraft: string;
  detail: SessionDetail | null;
  editLog: EditLogEntry[];
  error: string | null;
  inspector: WorkspaceInspector;
  loading: boolean;
  scrollOffset: number;
  searchQuery: string;
  terminalId: string | null;
  terminalDrawerOpen: boolean;
}

export interface WorkspaceState {
  activeTabId: string | null;
  openTabs: WorkspaceTab[];
  viewByTabId: Record<string, SessionTabViewState>;
}

export interface WorkspaceTabInput {
  id?: string;
  now?: number;
  platform: string;
  sessionKey: string;
  status?: WorkspaceTabStatus;
  title?: string;
}

export type RestorableSessionViewState = Pick<
  SessionTabViewState,
  "scrollOffset" | "composerDraft" | "inspector" | "terminalId" | "terminalDrawerOpen" | "searchQuery"
>;

export type WorkspaceAction =
  | { type: "open"; payload: WorkspaceTabInput }
  | { type: "activate"; payload: { tabId: string; now?: number } }
  | { type: "deactivate" }
  | { type: "close"; payload: { tabId: string } }
  | {
      type: "update-tab";
      payload: {
        tabId: string;
        updates: Partial<
          Pick<WorkspaceTab, "title" | "status" | "lastActiveAt">
        >;
      };
    }
  | {
      type: "update-detail";
      payload: { tabId: string; detail: SessionDetail | null };
    }
  | {
      type: "update-edit-log";
      payload: { tabId: string; editLog: EditLogEntry[] };
    }
  | { type: "set-loading"; payload: { tabId: string; loading: boolean } }
  | { type: "set-error"; payload: { tabId: string; error: string | null } }
  | {
      type: "restore-view-state";
      payload: { tabId: string; state: Partial<RestorableSessionViewState> };
    }
  | { type: "hydrate"; payload: WorkspaceState };
