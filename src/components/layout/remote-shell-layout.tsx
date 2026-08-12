import {
  Bot,
  CheckCircle2,
  Code,
  Eye,
  Gem,
  KeyRound,
  LoaderCircle,
  Library,
  Menu,
  MessagesSquare,
  MousePointer2,
  Orbit,
  Pi,
  Radio,
  ShieldCheck,
  Sparkles,
  Settings2,
  Terminal,
  SquareTerminal,
  Wifi,
  X,
  type LucideIcon,
} from "lucide-react";
import { Suspense, useEffect, useMemo, useState } from "react";
import { NavLink, Outlet, useLocation } from "react-router";
import { AppLogo } from "@/components/logo";
import { api, hasRemoteAccessToken, setRemoteAccessToken } from "@/features/desktop/api";
import type { MessageKey } from "@/features/desktop/i18n";
import { useDesktop } from "@/features/desktop/provider";
import { cn } from "@/lib/utils";

type RemotePlatformItem = {
  id: string;
  labelKey: MessageKey;
  icon: LucideIcon;
};

const REMOTE_PLATFORMS: RemotePlatformItem[] = [
  { id: "claude", labelKey: "platformClaude", icon: Bot },
  { id: "codex", labelKey: "platformCodex", icon: Terminal },
  { id: "cursor", labelKey: "platformCursor", icon: MousePointer2 },
  { id: "opencode", labelKey: "platformOpencode", icon: Code },
  { id: "kiro", labelKey: "platformKiro", icon: Sparkles },
  { id: "kiro-ide", labelKey: "platformKiroIde", icon: Sparkles },
  { id: "gemini", labelKey: "platformGemini", icon: Gem },
  { id: "grok", labelKey: "platformGrok", icon: Orbit },
  { id: "pi", labelKey: "platformPi", icon: Pi },
  { id: "terminal-sessions", labelKey: "remoteNavTerminal", icon: SquareTerminal },
];

export default function RemoteShellLayout() {
  const {
    snapshot,
    notice,
    error,
    t,
    isReadOnlyRemote,
    remoteBootstrap,
    remoteCapabilities,
    state,
    dispatch,
  } = useDesktop();
  const location = useLocation();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [remoteToken, setRemoteToken] = useState("");
  const [remoteAccessReady, setRemoteAccessReady] = useState(false);
  const [remoteAccessChecking, setRemoteAccessChecking] = useState(() => hasRemoteAccessToken());
  const [remoteConnecting, setRemoteConnecting] = useState(false);
  const [remoteTokenError, setRemoteTokenError] = useState(false);

  const visiblePlatforms = useMemo(() => {
    const available = new Set(
      remoteBootstrap?.platforms
        .filter((platform) => platform.available)
         .map((platform) => platform.id) ?? [],
    );
    if (remoteBootstrap?.capabilities.terminal === true) {
      available.add("terminal-sessions");
    }
    const configured = snapshot?.settings.navigationItems ?? [];
    const orderedIds = [
      ...configured.filter((id) => available.has(id)),
      ...Array.from(available).filter((id) => !configured.includes(id)),
    ];
    return orderedIds.flatMap((id) => {
      const item = REMOTE_PLATFORMS.find((candidate) => candidate.id === id);
      return item ? [item] : [];
    });
  }, [
    remoteBootstrap?.capabilities.terminal,
    remoteBootstrap?.platforms,
    snapshot?.settings.navigationItems,
  ]);

  const activePlatform = visiblePlatforms.find((item) => location.pathname === `/${item.id}`)
    ?? visiblePlatforms.find((item) => item.id === state.currentPlatform)
    ?? visiblePlatforms[0];
  const isPlatformSessionRoute = Boolean(
    state.selectedSessionKey
      && visiblePlatforms.some(
        (item) => item.id !== "terminal-sessions" && location.pathname === `/${item.id}`
      )
  );
  const isMemoryRoute = location.pathname === "/memory";
  const hasFocusedContent = isPlatformSessionRoute || isMemoryRoute;
  const bottomNavigation = [
    { to: "/", labelKey: "remoteNavSessions" as const, icon: MessagesSquare, kind: "sessions" as const },
    ...(remoteCapabilities?.terminal === true
      ? [{ to: "/terminal-sessions", labelKey: "remoteNavTerminal" as const, icon: SquareTerminal, kind: "terminal" as const }]
      : []),
    { to: "/prompts", labelKey: "prompts" as const, icon: Library, kind: "prompts" as const },
    { to: "/settings", labelKey: "settings" as const, icon: Settings2, kind: "settings" as const },
  ];

  useEffect(() => {
    setDrawerOpen(false);
  }, [location.pathname]);

  useEffect(() => {
    if (!remoteBootstrap?.auth.required) {
      setRemoteAccessReady(true);
      setRemoteAccessChecking(false);
      return;
    }
    setRemoteAccessReady(false);
    if (!hasRemoteAccessToken()) {
      setRemoteAccessChecking(false);
      return;
    }
    let cancelled = false;
    setRemoteAccessChecking(true);
    api.getDashboard()
      .then((dashboard) => {
        if (cancelled) return;
        dispatch({ type: "setDashboard", payload: dashboard });
        setRemoteAccessReady(true);
        setRemoteTokenError(false);
      })
      .catch(() => {
        if (cancelled) return;
        setRemoteAccessToken("");
        setRemoteTokenError(true);
      })
      .finally(() => {
        if (!cancelled) setRemoteAccessChecking(false);
      });
    return () => {
      cancelled = true;
    };
  }, [dispatch, remoteBootstrap?.auth.required]);

  const connectRemote = async () => {
    if (!remoteToken.trim()) return;
    setRemoteConnecting(true);
    setRemoteTokenError(false);
    setRemoteAccessToken(remoteToken);
    try {
      const dashboard = await api.getDashboard();
      dispatch({ type: "setDashboard", payload: dashboard });
      setRemoteAccessReady(true);
      setRemoteToken("");
    } catch {
      setRemoteAccessToken("");
      setRemoteTokenError(true);
    } finally {
      setRemoteConnecting(false);
    }
  };

  const contentReady = remoteBootstrap?.auth.required !== true || remoteAccessReady;

  return (
    <div className="remote-shell h-[100dvh] overflow-hidden bg-background text-foreground">
      {notice && (
        <div className="remote-toast remote-toast-success" role="status">
          <CheckCircle2 className="size-4" />
          <span>{notice}</span>
        </div>
      )}
      {error && (
        <div className="remote-toast remote-toast-error" role="alert">
          <span>{t("saveError")}: {error}</span>
        </div>
      )}

      {remoteBootstrap?.auth.required && !remoteAccessReady && !remoteAccessChecking && (
        <div className="remote-auth" role="dialog" aria-modal="true" aria-labelledby="remote-access-title">
          <form
            className="remote-auth-form"
            onSubmit={(event) => {
              event.preventDefault();
              void connectRemote();
            }}
          >
            <AppLogo className="size-12" />
            <p className="remote-kicker">Memory Forge Remote</p>
            <h1 id="remote-access-title">{t("remoteAccessTitle")}</h1>
            <div className="remote-host-line">
              <Wifi className="size-4" />
              <span>{remoteBootstrap.serverName}</span>
            </div>
            <label htmlFor="remote-access-token">{t("remoteAccessToken")}</label>
            <div className="remote-token-field">
              <KeyRound className="size-4" />
              <input
                id="remote-access-token"
                type="password"
                autoComplete="off"
                value={remoteToken}
                onChange={(event) => setRemoteToken(event.target.value)}
                autoFocus
              />
            </div>
            {remoteTokenError && <p className="remote-auth-error" role="alert">{t("remoteAccessInvalid")}</p>}
            <button type="submit" disabled={remoteConnecting || !remoteToken.trim()}>
              {remoteConnecting && <LoaderCircle className="size-4 animate-spin" />}
              {t("remoteConnect")}
            </button>
          </form>
        </div>
      )}

      <header className={cn("remote-topbar lg:hidden", hasFocusedContent && "remote-topbar-focused max-md:hidden")}>
        <button
          type="button"
          className="remote-icon-button"
          onClick={() => setDrawerOpen(true)}
          aria-label={t("remoteOpenNavigation")}
          title={t("remoteOpenNavigation")}
        >
          <Menu className="size-5" />
        </button>
        <div className="remote-topbar-title">
          <AppLogo className="size-6" />
          <div>
            <span>{location.pathname === "/" ? t("appName") : activePlatform ? t(activePlatform.labelKey) : t("appName")}</span>
            <small>{location.pathname === "/" ? t("remoteCompanion") : t("remoteSessions")}</small>
          </div>
        </div>
        <span className="remote-online" title={`${t("remoteServer")}: ${remoteBootstrap?.serverName ?? "Memory Forge"}`}>
          <Radio className="size-3.5" />
          <span>{t("remoteOnline")}</span>
        </span>
      </header>

      {drawerOpen && (
        <button
          type="button"
          className="remote-drawer-scrim lg:hidden"
          onClick={() => setDrawerOpen(false)}
          aria-label={t("remoteCloseNavigation")}
        />
      )}

      <div className={cn("remote-layout", hasFocusedContent && "remote-layout-detail")}>
        <aside className={cn("remote-drawer", drawerOpen && "remote-drawer-open")}>
          <div className="remote-drawer-brand">
            <AppLogo className="size-9" />
            <div>
              <strong>{t("appName")}</strong>
              <span>{t("remoteCompanion")}</span>
            </div>
            <button
              type="button"
              className="remote-icon-button ml-auto lg:hidden"
              onClick={() => setDrawerOpen(false)}
              aria-label={t("remoteCloseNavigation")}
              title={t("remoteCloseNavigation")}
            >
              <X className="size-5" />
            </button>
          </div>

          <div className="remote-server-card">
            <span className="remote-server-icon"><Wifi className="size-4" /></span>
            <div>
              <strong>{remoteBootstrap?.serverName ?? "Memory Forge"}</strong>
              <span>{t("remoteLocalConnection")}</span>
            </div>
            <span className="remote-status-dot" />
          </div>

          <p className="remote-nav-label">{t("remoteWorkspaces")}</p>
          <nav className="remote-platform-nav" aria-label={t("remoteWorkspaces")}>
            <NavLink
              end
              to="/"
              className={({ isActive }) => cn("remote-platform-link", isActive && "remote-platform-link-active")}
            >
              <MessagesSquare className="size-4" />
              <span>{t("remoteSessions")}</span>
              <span className="remote-platform-chevron">›</span>
            </NavLink>
            {visiblePlatforms.map((item) => {
              const Icon = item.icon;
              return (
                <NavLink
                  key={item.id}
                  to={`/${item.id}`}
                  className={({ isActive }) => cn("remote-platform-link", isActive && "remote-platform-link-active")}
                >
                  <Icon className="size-4" />
                  <span>{t(item.labelKey)}</span>
                  <span className="remote-platform-chevron">›</span>
                </NavLink>
              );
            })}
          </nav>

          <div className="remote-drawer-footer">
            {remoteCapabilities?.terminal === true
              ? <Terminal className="size-4" />
              : isReadOnlyRemote
                ? <Eye className="size-4" />
                : <ShieldCheck className="size-4" />}
            <div>
              <strong>
                {remoteCapabilities?.terminal === true
                  ? t("remoteTerminalControl")
                  : isReadOnlyRemote
                    ? t("remoteReadOnly")
                    : t("remoteEditsEnabled")}
              </strong>
              <span>
                {remoteCapabilities?.terminal === true
                  ? t("remoteTerminalControlHint")
                  : isReadOnlyRemote
                    ? t("remoteSourceOnHost")
                    : t("remoteRevisionProtected")}
              </span>
            </div>
          </div>
        </aside>

        <main className="remote-main">
          <Suspense fallback={<RemoteLoading label={t("loading")} />}>
            {contentReady ? <Outlet /> : <RemoteLoading label={t("loading")} />}
          </Suspense>
        </main>
      </div>

      {contentReady && (
        <nav className="remote-bottom-nav lg:hidden" aria-label={t("remotePrimaryNavigation")}>
          {bottomNavigation.map((item) => {
            const Icon = item.icon;
            const platformPath = visiblePlatforms.some(
              (platform) => platform.id !== "terminal-sessions" && location.pathname === `/${platform.id}`
            );
            const active = item.kind === "sessions"
              ? location.pathname === "/" || location.pathname === "/memory" || platformPath
              : location.pathname === item.to;
            return (
              <NavLink
                key={item.to}
                end={item.to === "/"}
                to={item.to}
                className={cn("remote-bottom-link", active && "remote-bottom-link-active")}
                aria-current={active ? "page" : undefined}
              >
                <Icon className="size-4" />
                <span>{t(item.labelKey)}</span>
              </NavLink>
            );
          })}
        </nav>
      )}

      {remoteAccessChecking && <div className="remote-loading-overlay"><RemoteLoading label={t("loading")} /></div>}
    </div>
  );
}

function RemoteLoading({ label }: { label: string }) {
  return (
    <div className="remote-loading" role="status" aria-live="polite">
      <LoaderCircle className="size-5 animate-spin motion-reduce:animate-none" />
      <span>{label}</span>
    </div>
  );
}
