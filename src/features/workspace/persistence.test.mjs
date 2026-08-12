import assert from "node:assert/strict";
import test from "node:test";
import { parseWorkspaceState, serializeWorkspaceState } from "./persistence.ts";
import { createInitialWorkspaceState, workspaceReducer } from "./reducer.ts";

test("persistence excludes session details, audit content, terminal state, and scroll offsets", () => {
  let state = createInitialWorkspaceState();
  state = workspaceReducer(state, {
    type: "open",
    payload: { platform: "claude", sessionKey: "session-a", title: "Session A", now: 10 },
  });
  const tabId = state.activeTabId;
  state = workspaceReducer(state, {
    type: "update-detail",
    payload: {
      tabId,
      detail: {
        platform: "claude",
        sessionKey: "session-a",
        sessionId: "a",
        title: "A",
        aliasTitle: "",
        cwd: "C:/secret",
        commands: {},
        revision: "revision-a",
        blocks: [{ id: "1", role: "user", content: "SECRET_SESSION_BODY", editable: true, editTarget: "1", sourceMeta: {} }],
      },
    },
  });
  state = workspaceReducer(state, {
    type: "update-edit-log",
    payload: {
      tabId,
      editLog: [{ id: 1, editTarget: "1", oldContent: "SECRET_OLD", newContent: "SECRET_NEW", createdAt: "now" }],
    },
  });
  state = workspaceReducer(state, {
    type: "restore-view-state",
    payload: {
      tabId,
      state: {
        composerDraft: "persist me",
        inspector: "memory",
        terminalId: "terminal-secret",
        terminalDrawerOpen: true,
        scrollOffset: 900,
      },
    },
  });

  const raw = serializeWorkspaceState(state);
  const serialized = JSON.parse(raw);
  assert.equal(raw.includes("SECRET_"), false);
  assert.equal(raw.includes("terminal-secret"), false);
  assert.equal(raw.includes("900"), false);
  assert.equal("terminalDrawerOpen" in serialized.viewByTabId[tabId], false);

  const restored = parseWorkspaceState(raw);
  assert.ok(restored);
  assert.equal(restored.viewByTabId[tabId].detail, null);
  assert.deepEqual(restored.viewByTabId[tabId].editLog, []);
  assert.equal(restored.viewByTabId[tabId].composerDraft, "persist me");
  assert.equal(restored.viewByTabId[tabId].inspector, "memory");
  assert.equal(restored.viewByTabId[tabId].terminalId, null);
  assert.equal(restored.viewByTabId[tabId].terminalDrawerOpen, false);
  assert.equal(restored.viewByTabId[tabId].scrollOffset, 0);
});

test("invalid, unknown-version, and duplicate persisted state is handled safely", () => {
  assert.equal(parseWorkspaceState("not-json"), null);
  assert.equal(parseWorkspaceState('{"version":999,"openTabs":[],"viewByTabId":{}}'), null);

  const tab = {
    id: "tab-a",
    kind: "session",
    platform: "claude",
    sessionKey: "session-a",
    title: "A",
    status: "idle",
    openedAt: 1,
    lastActiveAt: 2,
  };
  const restored = parseWorkspaceState(JSON.stringify({
    version: 1,
    openTabs: [tab, { ...tab, id: "tab-duplicate" }],
    activeTabId: "missing",
    viewByTabId: {},
  }));

  assert.ok(restored);
  assert.equal(restored.openTabs.length, 1);
  assert.equal(restored.activeTabId, "tab-a");
});
