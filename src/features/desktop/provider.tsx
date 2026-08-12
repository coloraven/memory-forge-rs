import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { getThemeSpec } from "@/features/desktop/catalog";
import { api, loadDesktopSnapshot, updateDesktopSettings } from "@/features/desktop/api";
import {
  appReducer,
  createInitialAppState,
} from "@/features/desktop/app-reducer";
import { translate, type MessageKey } from "@/features/desktop/i18n";
import type {
  AppAction,
  AppState,
  DesktopSettingsPatch,
  DesktopSnapshot,
  LocaleId,
  ThemeId,
} from "@/features/desktop/types";
import type { RemoteCapabilities } from "@/features/remote/protocol";
import { persistWorkspaceState } from "@/features/workspace/persistence";

const ACTIVE_SESSION_REFRESH_INTERVAL_MS = 8_000;

type DesktopContextValue = {
  snapshot: DesktopSnapshot | null;
  loading: boolean;
  saving: boolean;
  notice: string | null;
  error: string | null;
  settings: DesktopSnapshot["settings"] | null;
  remoteBootstrap: DesktopSnapshot["remote"] | null;
  remoteCapabilities: RemoteCapabilities | null;
  isRemote: boolean;
  isReadOnlyRemote: boolean;
  state: AppState;
  dispatch: React.Dispatch<AppAction>;
  t: (key: MessageKey, params?: Record<string, string | number>) => string;
  refresh: () => Promise<void>;
  updateSettings: (patch: DesktopSettingsPatch) => Promise<void>;
  setTheme: (theme: ThemeId) => Promise<void>;
  setLocale: (locale: LocaleId) => Promise<void>;
  setCloseToTrayOnClose: (enabled: boolean) => Promise<void>;
  setLaunchOnStartup: (enabled: boolean) => Promise<void>;
  setReduceMotion: (enabled: boolean) => Promise<void>;
};

const DesktopContext = createContext<DesktopContextValue | null>(null);

export function DesktopProvider({ children }: { children: ReactNode }) {
  const [snapshot, setSnapshot] = useState<DesktopSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [state, dispatch] = useReducer(appReducer, undefined, createInitialAppState);
  const noticeTimerRef = useRef<number | null>(null);

  const locale = snapshot?.settings.locale ?? "zh-CN";
  const t = useMemo(
    () => (key: MessageKey, params?: Record<string, string | number>) => {
      let msg = translate(locale, key);
      if (params) {
        for (const [k, v] of Object.entries(params)) {
          msg = msg.replace(`{${k}}`, String(v));
        }
      }
      return msg;
    },
    [locale],
  );

  const settings = snapshot?.settings ?? null;
  const remoteBootstrap = snapshot?.remote ?? null;
  const remoteCapabilities = remoteBootstrap?.capabilities ?? null;
  const isRemote = snapshot?.runtime === "remote-web";
  const isReadOnlyRemote = isRemote && remoteCapabilities?.sessionEdit !== true;
  const activeWorkspaceTab = state.workspace.openTabs.find(
    (tab) => tab.id === state.workspace.activeTabId
  );
  const editingActiveSession = Boolean(
    state.editingBlock &&
      activeWorkspaceTab &&
      state.editingBlock.platform === activeWorkspaceTab.platform &&
      state.editingBlock.sessionKey === activeWorkspaceTab.sessionKey
  );

  useEffect(() => {
    return () => {
      if (noticeTimerRef.current) window.clearTimeout(noticeTimerRef.current);
    };
  }, []);

  useEffect(() => {
    persistWorkspaceState(state.workspace);
  }, [state.workspace]);

  useEffect(() => {
    if (!snapshot || !activeWorkspaceTab || editingActiveSession) return;

    let cancelled = false;
    let inFlight = false;
    const initialDetail =
      state.workspace.viewByTabId[activeWorkspaceTab.id]?.detail ?? null;
    let knownSignature = initialDetail
      ? `${initialDetail.revision}:${JSON.stringify(initialDetail.capabilities ?? null)}`
      : null;

    const refreshActiveSession = async () => {
      if (cancelled || inFlight || document.visibilityState !== "visible") return;
      inFlight = true;
      try {
        const detail = await api.getSessionDetail(
          activeWorkspaceTab.platform,
          activeWorkspaceTab.sessionKey
        );
        if (cancelled) return;
        const nextSignature = `${detail.revision}:${JSON.stringify(detail.capabilities ?? null)}`;
        if (nextSignature !== knownSignature) {
          knownSignature = nextSignature;
          dispatch({ type: "setSessionDetail", payload: detail });
        }
      } catch {
        // Passive refresh failures leave the last authoritative snapshot visible.
      } finally {
        inFlight = false;
      }
    };

    const handleVisibilityOrFocus = () => {
      if (document.visibilityState === "visible") {
        void refreshActiveSession();
      }
    };
    const interval = window.setInterval(
      () => void refreshActiveSession(),
      ACTIVE_SESSION_REFRESH_INTERVAL_MS
    );
    document.addEventListener("visibilitychange", handleVisibilityOrFocus);
    window.addEventListener("focus", handleVisibilityOrFocus);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
      document.removeEventListener("visibilitychange", handleVisibilityOrFocus);
      window.removeEventListener("focus", handleVisibilityOrFocus);
    };
  }, [
    activeWorkspaceTab?.id,
    activeWorkspaceTab?.platform,
    activeWorkspaceTab?.sessionKey,
    editingActiveSession,
    remoteCapabilities?.sessionEdit,
    remoteCapabilities?.terminal,
    snapshot?.runtime,
  ]);

  useEffect(() => {
    if (typeof document === "undefined" || !snapshot) return;
    document.documentElement.dataset.theme = snapshot.settings.theme;
    document.documentElement.dataset.reduceMotion = String(snapshot.settings.reduceMotion);
    document.documentElement.lang = snapshot.settings.locale;
    const theme = getThemeSpec(snapshot.settings.theme);
    document.documentElement.style.colorScheme = theme.mode;
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", theme.preview[0]);
  }, [snapshot]);

  const setTimedNotice = (value: string | null) => {
    setNotice(value);
    if (noticeTimerRef.current) window.clearTimeout(noticeTimerRef.current);
    if (!value) return;
    noticeTimerRef.current = window.setTimeout(() => setNotice(null), 2200);
  };

  const refresh = async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await loadDesktopSnapshot();
      setSnapshot(next);
    } catch (refreshError) {
      const message = refreshError instanceof Error ? refreshError.message : "Unknown error";
      setError(message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const updateSettings = async (patch: DesktopSettingsPatch) => {
    setSaving(true);
    setError(null);
    try {
      const next = await updateDesktopSettings(patch);
      setSnapshot(next);
      setTimedNotice(translate(next.settings.locale, "saveSuccess"));
    } catch (saveError) {
      const message = saveError instanceof Error ? saveError.message : "Unknown save error";
      setError(message);
      setTimedNotice(null);
    } finally {
      setSaving(false);
    }
  };

  const value = useMemo<DesktopContextValue>(
    () => ({
      snapshot,
      loading,
      saving,
      notice,
      error,
      settings,
      remoteBootstrap,
      remoteCapabilities,
      isRemote,
      isReadOnlyRemote,
      state,
      dispatch,
      t,
      refresh,
      updateSettings,
      setTheme: async (theme) => updateSettings({ theme }),
      setLocale: async (nextLocale) => updateSettings({ locale: nextLocale }),
      setCloseToTrayOnClose: async (enabled) => updateSettings({ closeToTrayOnClose: enabled }),
      setLaunchOnStartup: async (enabled) => updateSettings({ launchOnStartup: enabled }),
      setReduceMotion: async (enabled) => updateSettings({ reduceMotion: enabled }),
    }),
    [snapshot, loading, saving, notice, error, state, t, remoteBootstrap, remoteCapabilities, isRemote, isReadOnlyRemote],
  );

  return (
    <DesktopContext.Provider value={value}>{children}</DesktopContext.Provider>
  );
}

export function useDesktop() {
  const context = useContext(DesktopContext);
  if (!context) throw new Error("useDesktop must be used inside DesktopProvider");
  return context;
}
