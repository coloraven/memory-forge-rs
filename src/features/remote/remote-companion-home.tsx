import {
  Bot,
  ChevronRight,
  Code,
  Gem,
  LoaderCircle,
  MousePointer2,
  Orbit,
  Pi,
  Search,
  Sparkles,
  SquareTerminal,
  Terminal,
  Wifi,
  type LucideIcon,
} from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import type { DashboardSummary, Session } from "@/features/desktop/types";
import type { MessageKey } from "@/features/desktop/i18n";
import { useDesktop } from "@/features/desktop/provider";
import { useRemoteTerminal } from "@/features/terminal/remote-terminal-context";

const ACTIVE_TERMINAL_STATUSES = new Set(["starting", "running", "stopping"]);

const PLATFORM_META: Record<string, { icon: LucideIcon; labelKey: MessageKey }> = {
  claude: { icon: Bot, labelKey: "platformClaude" },
  codex: { icon: Terminal, labelKey: "platformCodex" },
  cursor: { icon: MousePointer2, labelKey: "platformCursor" },
  gemini: { icon: Gem, labelKey: "platformGemini" },
  grok: { icon: Orbit, labelKey: "platformGrok" },
  kiro: { icon: Sparkles, labelKey: "platformKiro" },
  "kiro-ide": { icon: Sparkles, labelKey: "platformKiroIde" },
  opencode: { icon: Code, labelKey: "platformOpencode" },
  pi: { icon: Pi, labelKey: "platformPi" },
};

function sessionIdentity(session: Pick<Session, "platform" | "sessionKey">) {
  return JSON.stringify([session.platform, session.sessionKey]);
}

function timestamp(value: string) {
  const numeric = Number(value);
  if (Number.isFinite(numeric) && numeric > 0) {
    if (numeric > 10 ** 17) return numeric / 1_000_000;
    if (numeric > 10 ** 15) return numeric / 1_000;
    if (numeric > 10 ** 12) return numeric;
    return numeric * 1_000;
  }
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function formatSessionTime(value: string) {
  const time = timestamp(value);
  if (!time) return "";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(time);
}

interface RemoteCompanionHomeProps {
  dashboard: DashboardSummary | null;
  error: string | null;
  loading: boolean;
}

export function RemoteCompanionHome({
  dashboard,
  error,
  loading,
}: RemoteCompanionHomeProps) {
  const { remoteBootstrap, t } = useDesktop();
  const { terminals } = useRemoteTerminal();
  const navigate = useNavigate();
  const searchRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");

  const activeTerminals = useMemo(
    () =>
      Object.values(terminals)
        .flat()
        .filter((terminal) => ACTIVE_TERMINAL_STATUSES.has(terminal.status)),
    [terminals]
  );
  const runningSessions = useMemo(
    () =>
      new Set(
        activeTerminals.map((terminal) =>
          JSON.stringify([terminal.platform ?? "", terminal.sessionKey])
        )
      ),
    [activeTerminals]
  );
  const recentSessions = useMemo(() => {
    const candidates = [
      ...(dashboard?.recentSessions ?? []),
      ...(dashboard?.platforms.flatMap((platform) => platform.items ?? []) ?? []),
    ];
    const seen = new Set<string>();
    return candidates
      .filter((session) => {
        const identity = sessionIdentity(session);
        if (seen.has(identity)) return false;
        seen.add(identity);
        return true;
      })
      .sort((left, right) => timestamp(right.updatedAt) - timestamp(left.updatedAt));
  }, [dashboard]);
  const filteredSessions = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return recentSessions;
    return recentSessions.filter((session) =>
      [
        session.displayTitle,
        session.preview,
        session.cwd,
        session.platform,
      ].some((value) => value.toLowerCase().includes(needle))
    );
  }, [query, recentSessions]);
  const running = filteredSessions.filter((session) =>
    runningSessions.has(sessionIdentity(session))
  );
  const recent = filteredSessions.filter(
    (session) => !runningSessions.has(sessionIdentity(session))
  );
  const sessionCount = dashboard?.platforms.reduce(
    (total, platform) => total + platform.count,
    0
  ) ?? 0;

  const openSession = (session: Session) => {
    const params = new URLSearchParams({ session: session.sessionKey });
    navigate(`/${session.platform}?${params}`);
  };

  return (
    <section className="remote-home" aria-label={t("remoteSessions")}>
      <header className="remote-home-toolbar">
        <div className="remote-home-title">
          <p className="remote-kicker">{t("remoteCompanion")}</p>
          <h1>{t("remoteSessions")}</h1>
        </div>
        <button
          type="button"
          className="remote-icon-button"
          onClick={() => searchRef.current?.focus()}
          title={t("session.search")}
          aria-label={t("session.search")}
        >
          <Search className="size-4" />
        </button>
      </header>

      <div className="remote-home-scroll">
        <div className="remote-host-summary">
          <span className="remote-host-summary-icon"><Wifi className="size-4" /></span>
          <div>
            <strong>{remoteBootstrap?.serverName ?? t("appName")}</strong>
            <span>{t("remoteHomeHostSummary", { sessions: sessionCount, terminals: activeTerminals.length })}</span>
          </div>
          <span className="remote-status-dot" aria-label={t("remoteOnline")} />
        </div>

        <label className="remote-home-search">
          <Search className="size-4" />
          <span className="sr-only">{t("session.search")}</span>
          <input
            ref={searchRef}
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("remoteHomeSearch")}
          />
        </label>

        {loading && !dashboard ? (
          <div className="remote-home-loading" role="status">
            <LoaderCircle className="size-5 animate-spin motion-reduce:animate-none" />
            <span>{t("loading")}</span>
          </div>
        ) : (
          <div className="remote-home-groups">
            {running.length > 0 && (
              <SessionGroup
                label={t("remoteHomeRunning")}
                sessions={running}
                runningSessions={runningSessions}
                onOpen={openSession}
                t={t}
              />
            )}
            {recent.length > 0 && (
              <SessionGroup
                label={t("remoteHomeRecent")}
                sessions={recent}
                runningSessions={runningSessions}
                onOpen={openSession}
                t={t}
              />
            )}
            {filteredSessions.length === 0 && (
              <div className="remote-empty-state">
                <SquareTerminal className="size-6" />
                <strong>{t("session.noSessions")}</strong>
              </div>
            )}
          </div>
        )}

        {error && <p className="remote-home-error" role="alert">{error}</p>}
      </div>
    </section>
  );
}

function SessionGroup({
  label,
  sessions,
  runningSessions,
  onOpen,
  t,
}: {
  label: string;
  sessions: Session[];
  runningSessions: Set<string>;
  onOpen: (session: Session) => void;
  t: (key: MessageKey, params?: Record<string, string | number>) => string;
}) {
  return (
    <section className="remote-home-group">
      <h2>{label}</h2>
      <div className="remote-home-session-list">
        {sessions.map((session) => {
          const meta = PLATFORM_META[session.platform];
          const Icon = meta?.icon ?? Bot;
          const running = runningSessions.has(sessionIdentity(session));
          const displayTime = formatSessionTime(session.updatedAt);
          return (
            <button
              key={sessionIdentity(session)}
              type="button"
              className="remote-home-session"
              onClick={() => onOpen(session)}
            >
              <span className="remote-home-session-icon" data-platform={session.platform}>
                <Icon className="size-4" />
              </span>
              <span className="remote-home-session-copy">
                <strong>{session.displayTitle || session.sessionId}</strong>
                <span>{session.preview || session.cwd}</span>
                <small>
                  {meta ? t(meta.labelKey) : session.platform}
                  {displayTime ? ` · ${displayTime}` : ""}
                </small>
              </span>
              <span className={running ? "remote-home-state remote-home-state-running" : "remote-home-state"}>
                <i />
                {t(running ? "remoteHomeRunningState" : "remoteHomeIdleState")}
              </span>
              <ChevronRight className="size-4 text-muted-foreground" />
            </button>
          );
        })}
      </div>
    </section>
  );
}
