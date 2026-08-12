import assert from "node:assert/strict";
import test from "node:test";
import {
  createInitialWorkspaceState,
  createWorkspaceTabId,
  workspaceReducer,
} from "./reducer.ts";

function open(state, platform, sessionKey, now) {
  return workspaceReducer(state, {
    type: "open",
    payload: { platform, sessionKey, title: `${platform}:${sessionKey}`, now },
  });
}

test("opening the same platform/session pair activates one stable tab", () => {
  let state = createInitialWorkspaceState();
  state = open(state, "claude", "session-a", 10);
  const tabId = state.activeTabId;
  state = open(state, "claude", "session-a", 20);

  assert.equal(state.openTabs.length, 1);
  assert.equal(state.activeTabId, tabId);
  assert.equal(state.openTabs[0].lastActiveAt, 20);
});

test("the same session key on different platforms creates different tabs", () => {
  let state = createInitialWorkspaceState();
  state = open(state, "claude", "shared-key", 10);
  state = open(state, "codex", "shared-key", 20);

  assert.equal(state.openTabs.length, 2);
  assert.notEqual(state.openTabs[0].id, state.openTabs[1].id);
});

test("an async detail response updates its target tab without changing the active tab", () => {
  let state = createInitialWorkspaceState();
  state = open(state, "claude", "session-a", 10);
  const tabA = createWorkspaceTabId("claude", "session-a");
  state = open(state, "codex", "session-b", 20);
  const tabB = createWorkspaceTabId("codex", "session-b");

  const detailA = {
    platform: "claude",
    sessionKey: "session-a",
    sessionId: "a",
    title: "A",
    aliasTitle: "",
    cwd: "C:/a",
    commands: {},
    revision: "revision-a",
    blocks: [],
  };
  state = workspaceReducer(state, {
    type: "update-detail",
    payload: { tabId: tabA, detail: detailA },
  });

  assert.equal(state.activeTabId, tabB);
  assert.equal(state.viewByTabId[tabA].detail, detailA);
  assert.equal(state.viewByTabId[tabB].detail, null);
});

test("edit logs are isolated by tab id", () => {
  let state = createInitialWorkspaceState();
  state = open(state, "claude", "session-a", 10);
  const tabA = state.activeTabId;
  state = open(state, "codex", "session-b", 20);
  const tabB = state.activeTabId;
  const log = [{ id: 1, editTarget: "a", oldContent: "old", newContent: "new", createdAt: "now" }];

  state = workspaceReducer(state, {
    type: "update-edit-log",
    payload: { tabId: tabA, editLog: log },
  });

  assert.deepEqual(state.viewByTabId[tabA].editLog, log);
  assert.deepEqual(state.viewByTabId[tabB].editLog, []);
});

test("closing the active tab selects the most recently active remaining tab", () => {
  let state = createInitialWorkspaceState();
  state = open(state, "claude", "session-a", 10);
  const tabA = state.activeTabId;
  state = open(state, "codex", "session-b", 20);
  const tabB = state.activeTabId;
  state = open(state, "pi", "session-c", 30);
  const tabC = state.activeTabId;
  state = workspaceReducer(state, { type: "activate", payload: { tabId: tabA, now: 40 } });
  state = workspaceReducer(state, { type: "close", payload: { tabId: tabA } });

  assert.equal(state.activeTabId, tabC);
  assert.equal(state.openTabs.some((tab) => tab.id === tabA), false);
  assert.equal(state.viewByTabId[tabA], undefined);
  assert.equal(state.openTabs.some((tab) => tab.id === tabB), true);
});

test("restoring view state does not overwrite authoritative detail or edit logs", () => {
  let state = createInitialWorkspaceState();
  state = open(state, "grok", "session-a", 10);
  const tabId = state.activeTabId;
  const detail = {
    platform: "grok",
    sessionKey: "session-a",
    sessionId: "a",
    title: "A",
    aliasTitle: "",
    cwd: "C:/a",
    commands: {},
    revision: "revision-a",
    blocks: [],
  };
  state = workspaceReducer(state, {
    type: "update-detail",
    payload: { tabId, detail },
  });
  state = workspaceReducer(state, {
    type: "restore-view-state",
    payload: {
      tabId,
      state: { composerDraft: "draft", inspector: "memory", scrollOffset: 320 },
    },
  });

  assert.equal(state.viewByTabId[tabId].detail, detail);
  assert.equal(state.viewByTabId[tabId].composerDraft, "draft");
  assert.equal(state.viewByTabId[tabId].inspector, "memory");
  assert.equal(state.viewByTabId[tabId].scrollOffset, 320);
});

test("inspector selection is isolated per workspace tab and can be closed", () => {
  let state = createInitialWorkspaceState();
  state = open(state, "claude", "session-a", 10);
  const tabA = state.activeTabId;
  state = workspaceReducer(state, {
    type: "restore-view-state",
    payload: { tabId: tabA, state: { inspector: "memory" } },
  });
  state = open(state, "codex", "session-b", 20);
  const tabB = state.activeTabId;
  state = workspaceReducer(state, {
    type: "restore-view-state",
    payload: { tabId: tabB, state: { inspector: "files" } },
  });

  assert.equal(state.viewByTabId[tabA].inspector, "memory");
  assert.equal(state.viewByTabId[tabB].inspector, "files");

  state = workspaceReducer(state, {
    type: "restore-view-state",
    payload: { tabId: tabB, state: { inspector: null } },
  });
  assert.equal(state.viewByTabId[tabB].inspector, null);
  assert.equal(state.viewByTabId[tabA].inspector, "memory");
});

test("terminal drawer association is isolated and closing a tab only removes its view state", () => {
  let state = createInitialWorkspaceState();
  state = open(state, "claude", "session-a", 10);
  const tabA = state.activeTabId;
  state = workspaceReducer(state, {
    type: "restore-view-state",
    payload: {
      tabId: tabA,
      state: { terminalId: "terminal_a", terminalDrawerOpen: true },
    },
  });
  state = open(state, "codex", "session-b", 20);
  const tabB = state.activeTabId;

  assert.equal(state.viewByTabId[tabA].terminalId, "terminal_a");
  assert.equal(state.viewByTabId[tabA].terminalDrawerOpen, true);
  assert.equal(state.viewByTabId[tabB].terminalId, null);
  assert.equal(state.viewByTabId[tabB].terminalDrawerOpen, false);

  state = workspaceReducer(state, { type: "close", payload: { tabId: tabA } });
  assert.equal(state.viewByTabId[tabA], undefined);
  assert.equal(state.activeTabId, tabB);
});
