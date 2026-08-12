import { useEffect, useMemo } from "react";
import { useNavigate } from "react-router";
import {
  ChevronDown,
  ChevronUp,
  ExternalLink,
  Maximize2,
  Play,
  RotateCw,
  Square,
  SquareTerminal,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ConfirmDialog,
  useConfirmDialog,
} from "@/components/ui/confirm-dialog";
import { api } from "@/features/desktop/api";
import type { MessageKey } from "@/features/desktop/i18n";
import { useDesktop } from "@/features/desktop/provider";
import { EmbeddedTerminalPanel } from "@/features/terminal/embedded-terminal-panel";
import { useTerminal } from "@/features/terminal/terminal-context";
import { terminalTheme } from "@/features/terminal/terminal-theme";
import type {
  EmbeddedTerminalSession,
  TerminalCommandKind,
} from "@/features/terminal/terminal-types";
import { TerminalViewport } from "@/features/terminal/terminal-viewport";
import { cn } from "@/lib/utils";
import { resolveSessionCapabilities } from "@/features/session/capabilities";

const ACTIVE_STATUSES = new Set(["starting", "running", "stopping"]);

export function WorkspaceTerminalDrawer() {
  const navigate = useNavigate();
  const { t, state, dispatch } = useDesktop();
  const {
    terminals,
    setActiveTerminal,
    startTerminal,
    restartTerminal,
    stopTerminal,
    closeTerminal,
  } = useTerminal();
  const { confirm, dialogProps } = useConfirmDialog();
  const detail = state.sessionDetail;
  const capabilities = resolveSessionCapabilities(detail, false, null);
  const activeTabId = state.workspace.activeTabId;
  const view = activeTabId ? state.workspace.viewByTabId[activeTabId] : null;
  const drawerOpen = view?.terminalDrawerOpen ?? false;

  const commands = useMemo(() => {
    if (!detail || !capabilities.rawTerminal) return [];
    return (["resume", "fork"] as const).flatMap((commandKind) => {
      if (!capabilities[commandKind]) return [];
      const command = detail.commands?.[commandKind];
      return command ? [{ command, commandKind }] : [];
    });
  }, [capabilities.fork, capabilities.rawTerminal, capabilities.resume, detail]);

  const sessionTerminals = useMemo(() => {
    if (!detail) return [];
    return (terminals[detail.sessionKey] ?? [])
      .filter(
        (terminal) =>
          !terminal.platform || terminal.platform === state.currentPlatform
      )
      .sort((left, right) => right.createdAt - left.createdAt);
  }, [detail, state.currentPlatform, terminals]);

  const selectedTerminal =
    sessionTerminals.find((terminal) => terminal.id === view?.terminalId) ??
    null;

  const updateDrawerState = (
    updates: Partial<{ terminalDrawerOpen: boolean; terminalId: string | null }>
  ) => {
    if (!activeTabId) return;
    dispatch({
      type: "workspace",
      payload: {
        type: "restore-view-state",
        payload: { tabId: activeTabId, state: updates },
      },
    });
  };

  useEffect(() => {
    if (!activeTabId || !view) return;
    if (selectedTerminal) return;
    const fallbackId = sessionTerminals[0]?.id ?? null;
    if (view.terminalId === fallbackId) return;
    dispatch({
      type: "workspace",
      payload: {
        type: "restore-view-state",
        payload: { tabId: activeTabId, state: { terminalId: fallbackId } },
      },
    });
  }, [activeTabId, dispatch, selectedTerminal, sessionTerminals, view]);

  if (!detail || !activeTabId || (commands.length === 0 && sessionTerminals.length === 0)) {
    return null;
  }

  const handleStart = async (
    commandKind: Exclude<TerminalCommandKind, "shell">,
    command: string
  ) => {
    const terminalId = await startTerminal(
      detail.sessionKey,
      commandKind,
      command,
      detail.cwd || null,
      {
        platform: state.currentPlatform,
        sessionTitle: detail.aliasTitle || detail.title || detail.sessionId,
      }
    );
    if (!terminalId) {
      dispatch({
        type: "setSessionStatus",
        payload: { tone: "error", message: t("terminal.tabs.maxWarning") },
      });
      return;
    }
    setActiveTerminal(terminalId);
    updateDrawerState({ terminalId, terminalDrawerOpen: true });
  };

  const handleRestart = async (terminal: EmbeddedTerminalSession) => {
    const terminalId = await restartTerminal(terminal.id);
    if (!terminalId) return;
    setActiveTerminal(terminalId);
    updateDrawerState({ terminalId, terminalDrawerOpen: true });
  };

  const handleStop = async (terminal: EmbeddedTerminalSession) => {
    if (terminal.status === "stopping") {
      const accepted = await confirm({
        title: t("terminal.btn.confirmForceStop"),
        description: t("terminal.forceStopDesc"),
        variant: "danger",
      });
      if (!accepted) return;
      await stopTerminal(terminal.id, true);
      return;
    }
    if (terminal.status === "starting" || terminal.status === "running") {
      await stopTerminal(terminal.id, false);
    }
  };

  const handleCloseFinished = async (terminal: EmbeddedTerminalSession) => {
    await closeTerminal(terminal.sessionKey, terminal.id);
    updateDrawerState({ terminalId: null });
  };

  const handleOpenExternal = async (terminal: EmbeddedTerminalSession) => {
    try {
      await api.launchSessionTerminal(terminal.command, terminal.cwd);
    } catch (error) {
      console.error("Failed to open terminal externally:", error);
      try {
        await navigator.clipboard.writeText(terminal.command);
      } catch {
        // The status message remains actionable when clipboard access is unavailable.
      }
      dispatch({
        type: "setSessionStatus",
        payload: {
          tone: "error",
          message: t("terminal.workspace.externalFailed"),
        },
      });
    }
  };

  const statusConfig = selectedTerminal
    ? terminalTheme.statusConfig[selectedTerminal.status]
    : terminalTheme.statusConfig.idle;
  const statusLabel = t(
    `terminal.status.${selectedTerminal?.status ?? "idle"}` as MessageKey
  );

  return (
    <section
      className="shrink-0 border-t border-border/60 bg-background/95"
      aria-label={t("terminal.drawer.title")}
      data-terminal-drawer={drawerOpen ? "open" : "closed"}
    >
      <header className="flex h-11 min-w-0 items-center gap-2 px-3 md:px-4">
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
          onClick={() => updateDrawerState({ terminalDrawerOpen: !drawerOpen })}
          aria-expanded={drawerOpen}
        >
          <SquareTerminal className="size-4 shrink-0 text-emerald-500" />
          <span className="truncate text-xs font-semibold text-foreground">
            {t("terminal.drawer.title")}
          </span>
          <span className={cn("size-2 shrink-0 rounded-full", statusConfig.dot)} />
          <span className="truncate text-[11px] text-muted-foreground">
            {selectedTerminal
              ? `${selectedTerminal.title} · ${statusLabel}`
              : t("terminal.drawer.unlinked")}
          </span>
        </button>

        {sessionTerminals.length > 1 && (
          <select
            value={selectedTerminal?.id ?? ""}
            onChange={(event) => {
              const terminalId = event.target.value;
              setActiveTerminal(terminalId);
              updateDrawerState({ terminalId });
            }}
            className="h-7 max-w-40 rounded-md border border-border/60 bg-background px-2 text-[11px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-primary/45"
            aria-label={t("terminal.drawer.select")}
          >
            {sessionTerminals.map((terminal) => (
              <option key={terminal.id} value={terminal.id}>
                {terminal.title} · {t(`terminal.status.${terminal.status}` as MessageKey)}
              </option>
            ))}
          </select>
        )}

        {selectedTerminal && drawerOpen && (
          <div className="flex shrink-0 items-center gap-0.5">
            {["running", "exited", "failed"].includes(selectedTerminal.status) && (
              <Button
                variant="ghost"
                size="icon"
                className="size-8"
                onClick={() => void handleRestart(selectedTerminal)}
                title={t("terminal.btn.restart")}
                aria-label={t("terminal.btn.restart")}
              >
                <RotateCw className="size-3.5" />
              </Button>
            )}
            {ACTIVE_STATUSES.has(selectedTerminal.status) && (
              <Button
                variant="ghost"
                size="icon"
                className="size-8 text-muted-foreground hover:text-red-400"
                onClick={() => void handleStop(selectedTerminal)}
                title={
                  selectedTerminal.status === "stopping"
                    ? t("terminal.btn.forceStop")
                    : t("terminal.btn.stop")
                }
                aria-label={
                  selectedTerminal.status === "stopping"
                    ? t("terminal.btn.forceStop")
                    : t("terminal.btn.stop")
                }
              >
                <Square className="size-3.5" />
              </Button>
            )}
            <Button
              variant="ghost"
              size="icon"
              className="size-8"
              onClick={() => void handleOpenExternal(selectedTerminal)}
              title={t("terminal.btn.openExternal")}
              aria-label={t("terminal.btn.openExternal")}
            >
              <ExternalLink className="size-3.5" />
            </Button>
            {["exited", "failed"].includes(selectedTerminal.status) && (
              <Button
                variant="ghost"
                size="icon"
                className="size-8 text-muted-foreground hover:text-red-400"
                onClick={() => void handleCloseFinished(selectedTerminal)}
                title={t("terminal.btn.close")}
                aria-label={t("terminal.btn.close")}
              >
                <X className="size-3.5" />
              </Button>
            )}
          </div>
        )}

        <Button
          variant="ghost"
          size="icon"
          className="size-8 shrink-0"
          onClick={() => {
            if (selectedTerminal) setActiveTerminal(selectedTerminal.id);
            navigate("/terminal-sessions");
          }}
          title={t("terminal.drawer.openWorkspace")}
          aria-label={t("terminal.drawer.openWorkspace")}
        >
          <Maximize2 className="size-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-8 shrink-0"
          onClick={() => updateDrawerState({ terminalDrawerOpen: !drawerOpen })}
          title={
            drawerOpen
              ? t("terminal.drawer.collapse")
              : t("terminal.drawer.expand")
          }
          aria-label={
            drawerOpen
              ? t("terminal.drawer.collapse")
              : t("terminal.drawer.expand")
          }
          aria-expanded={drawerOpen}
        >
          {drawerOpen ? (
            <ChevronDown className="size-4" />
          ) : (
            <ChevronUp className="size-4" />
          )}
        </Button>
      </header>

      {drawerOpen && (
        <div className="flex h-[min(38vh,360px)] min-h-[220px] overflow-hidden border-t border-border/50">
          {selectedTerminal ? (
            <EmbeddedTerminalPanel
              status={selectedTerminal.status}
              exitCode={selectedTerminal.exitCode}
              errorMessage={selectedTerminal.errorMessage}
              onStart={() => void handleRestart(selectedTerminal)}
              onRestart={() => void handleRestart(selectedTerminal)}
              onOpenExternal={() => void handleOpenExternal(selectedTerminal)}
              onClose={() => void handleCloseFinished(selectedTerminal)}
            >
              <TerminalViewport terminalId={selectedTerminal.id} isActive />
            </EmbeddedTerminalPanel>
          ) : (
            <div className="flex h-full flex-1 flex-col items-center justify-center px-6 text-center">
              <SquareTerminal className="mb-3 size-7 text-emerald-500/70" />
              <p className="text-sm font-semibold text-foreground">
                {t("terminal.drawer.emptyTitle")}
              </p>
              <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">
                {t("terminal.drawer.emptyDesc")}
              </p>
              <div className="mt-4 flex flex-wrap items-center justify-center gap-2">
                {commands.map(({ command, commandKind }) => (
                  <Button
                    key={commandKind}
                    size="sm"
                    className="gap-1.5"
                    onClick={() => void handleStart(commandKind, command)}
                  >
                    <Play className="size-3.5" />
                    {commandKind === "resume"
                      ? t("terminal.resumeEmbedded")
                      : t("terminal.forkEmbedded")}
                  </Button>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      <ConfirmDialog {...dialogProps} />
    </section>
  );
}
