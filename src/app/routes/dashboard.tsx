import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowRight, Bot, Brain, Code, Flame, Terminal, Sparkles, MousePointer2, Gem, Orbit, Pi, Zap, Database, Search, User } from "lucide-react";
import { Link, useNavigate } from "react-router";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AppLogo } from "@/components/logo";
import { useDesktop } from "@/features/desktop/provider";
import { api } from "@/features/desktop/api";
import type { Session } from "@/features/desktop/types";
import { cn } from "@/lib/utils";

const GLOBAL_SEARCH_LIMIT = 8;

const platformMeta = [
  {
    key: "claude",
    label: "Claude Code",
    icon: Bot,
    to: "/claude",
    gradient: "from-violet-500/10 to-violet-600/5",
    border: "border-violet-500/20 hover:border-violet-500/40",
    iconBg: "bg-violet-500/15 text-violet-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(139,92,246,0.12)] hover:-translate-y-1"
  },
  {
    key: "codex",
    label: "Codex CLI",
    icon: Terminal,
    to: "/codex",
    gradient: "from-emerald-500/10 to-emerald-600/5",
    border: "border-emerald-500/20 hover:border-emerald-500/40",
    iconBg: "bg-emerald-500/15 text-emerald-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(16,185,129,0.12)] hover:-translate-y-1"
  },
  {
    key: "cursor",
    label: "Cursor",
    icon: MousePointer2,
    to: "/cursor",
    gradient: "from-sky-500/10 to-sky-600/5",
    border: "border-sky-500/20 hover:border-sky-500/40",
    iconBg: "bg-sky-500/15 text-sky-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(14,165,233,0.12)] hover:-translate-y-1"
  },
  {
    key: "opencode",
    label: "OpenCode",
    icon: Code,
    to: "/opencode",
    gradient: "from-sky-500/10 to-sky-600/5",
    border: "border-sky-500/20 hover:border-sky-500/40",
    iconBg: "bg-sky-500/15 text-sky-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(14,165,233,0.12)] hover:-translate-y-1"
  },
  {
    key: "zcode",
    label: "ZCode",
    icon: Zap,
    to: "/zcode",
    gradient: "from-amber-500/10 to-orange-600/5",
    border: "border-amber-500/20 hover:border-orange-500/40",
    iconBg: "bg-amber-500/15 text-amber-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(245,158,11,0.12)] hover:-translate-y-1"
  },
  {
    key: "chat2db-local",
    label: "Chat2DB Local",
    icon: Database,
    to: "/chat2db-local",
    gradient: "from-teal-500/10 to-cyan-600/5",
    border: "border-teal-500/20 hover:border-cyan-500/40",
    iconBg: "bg-teal-500/15 text-teal-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(20,184,166,0.12)] hover:-translate-y-1"
  },
  {
    key: "chat2db-community",
    label: "Chat2DB Community",
    icon: Database,
    to: "/chat2db-community",
    gradient: "from-emerald-500/10 to-teal-600/5",
    border: "border-emerald-500/20 hover:border-teal-500/40",
    iconBg: "bg-emerald-500/15 text-emerald-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(16,185,129,0.12)] hover:-translate-y-1"
  },
  {
    key: "chat2db-pro",
    label: "Chat2DB Pro",
    icon: Database,
    to: "/chat2db-pro",
    gradient: "from-cyan-500/10 to-blue-600/5",
    border: "border-cyan-500/20 hover:border-blue-500/40",
    iconBg: "bg-cyan-500/15 text-cyan-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(6,182,212,0.12)] hover:-translate-y-1"
  },
  {
    key: "kiro",
    label: "Kiro CLI",
    icon: Sparkles,
    to: "/kiro",
    gradient: "from-purple-500/10 to-purple-600/5",
    border: "border-purple-500/20 hover:border-purple-500/40",
    iconBg: "bg-purple-500/15 text-purple-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(168,85,247,0.12)] hover:-translate-y-1"
  },
  {
    key: "kiro-ide",
    label: "Kiro IDE",
    icon: Sparkles,
    to: "/kiro-ide",
    gradient: "from-fuchsia-500/10 to-fuchsia-600/5",
    border: "border-fuchsia-500/20 hover:border-fuchsia-500/40",
    iconBg: "bg-fuchsia-500/15 text-fuchsia-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(217,70,239,0.12)] hover:-translate-y-1"
  },
  {
    key: "gemini",
    label: "Gemini CLI",
    icon: Gem,
    to: "/gemini",
    gradient: "from-blue-500/10 to-indigo-600/5",
    border: "border-blue-500/20 hover:border-indigo-500/40",
    iconBg: "bg-blue-500/15 text-blue-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(59,130,246,0.12)] hover:-translate-y-1"
  },
  {
    key: "pi",
    label: "Pi",
    icon: Pi,
    to: "/pi",
    gradient: "from-rose-500/10 to-cyan-600/5",
    border: "border-rose-500/20 hover:border-cyan-500/40",
    iconBg: "bg-rose-500/15 text-rose-400 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(244,63,94,0.12)] hover:-translate-y-1"
  },
  {
    key: "grok",
    label: "Grok Build",
    icon: Orbit,
    to: "/grok",
    gradient: "from-zinc-400/10 to-orange-500/5",
    border: "border-zinc-400/20 hover:border-orange-400/40",
    iconBg: "bg-zinc-400/15 text-zinc-300 group-hover:scale-110",
    hoverGlow: "hover:shadow-[0_8px_30px_rgba(249,115,22,0.12)] hover:-translate-y-1"
  },
] as const;

type GlobalHit = {
  platform: string;
  platformLabel: string;
  session: Session;
};

function flattenHits(platform: string, platformLabel: string, sessions: Session[]): GlobalHit[] {
  const hits: GlobalHit[] = [];
  const walk = (items: Session[]) => {
    for (const session of items) {
      hits.push({ platform, platformLabel, session });
      if (session.agentGroup?.children?.length) {
        walk(session.agentGroup.children);
      }
    }
  };
  walk(sessions);
  return hits;
}

export default function DashboardPage() {
  const { snapshot, loading, t, state, dispatch } = useDesktop();
  const navigate = useNavigate();
  const [dashboardLoading, setDashboardLoading] = useState(false);
  const [dashboardError, setDashboardError] = useState<string | null>(null);
  const [searchInput, setSearchInput] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [searchHits, setSearchHits] = useState<GlobalHit[]>([]);
  const [searchTotal, setSearchTotal] = useState(0);
  const [indexIncomplete, setIndexIncomplete] = useState(false);
  const searchRequestRef = useRef(0);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const visiblePlatforms = snapshot?.settings?.visiblePlatforms ?? ["claude", "codex", "cursor", "opencode", "zcode", "chat2db-local", "chat2db-community", "chat2db-pro", "grok", "pi"];
  const visiblePlatformsKey = visiblePlatforms.join("|");
  const snapshotReady = Boolean(snapshot);

  useEffect(() => {
    if (!snapshotReady) return;

    let cancelled = false;
    const label = `[perf] dashboard load (${visiblePlatformsKey || "none"})`;
    setDashboardLoading(true);
    setDashboardError(null);
    console.time(label);

    api.getDashboard()
      .then((data) => {
        if (!cancelled) {
          dispatch({ type: "setDashboard", payload: data });
        }
      })
      .catch((error) => {
        console.error("Failed to load dashboard:", error);
        if (!cancelled) {
          const message = error instanceof Error ? error.message : String(error);
          setDashboardError(message);
        }
      })
      .finally(() => {
        console.timeEnd(label);
        if (!cancelled) {
          setDashboardLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [dispatch, snapshotReady, visiblePlatformsKey]);

  useEffect(() => {
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, []);

  useEffect(() => {
    const q = searchQuery.trim();
    if (!q) {
      searchRequestRef.current += 1;
      setSearching(false);
      setSearchHits([]);
      setSearchTotal(0);
      setIndexIncomplete(false);
      return;
    }

    let cancelled = false;
    const requestId = ++searchRequestRef.current;
    setSearching(true);

    const run = async () => {
      const platforms = visiblePlatforms
        .map((id) => platformMeta.find((item) => item.key === id))
        .filter((item): item is (typeof platformMeta)[number] => Boolean(item));

      const results = await Promise.all(
        platforms.map(async (pm) => {
          try {
            const result = await api.getSessions(pm.key, q, GLOBAL_SEARCH_LIMIT, 0, false);
            return { pm, result };
          } catch (error) {
            console.error(`Global search failed for ${pm.key}:`, error);
            return null;
          }
        }),
      );

      if (cancelled || requestId !== searchRequestRef.current) return;

      const hits: GlobalHit[] = [];
      let total = 0;
      let incomplete = false;
      for (const entry of results) {
        if (!entry) continue;
        const { pm, result } = entry;
        total += result.total;
        if (result.searchIndex.supported && result.searchIndex.indexed < result.searchIndex.total) {
          incomplete = true;
        }
        hits.push(...flattenHits(pm.key, pm.label, result.items).slice(0, GLOBAL_SEARCH_LIMIT));
      }

      hits.sort((a, b) => {
        const aMatches = a.session.totalContentMatches ?? a.session.contentMatches?.length ?? 0;
        const bMatches = b.session.totalContentMatches ?? b.session.contentMatches?.length ?? 0;
        return bMatches - aMatches || b.session.updatedAt.localeCompare(a.session.updatedAt);
      });

      setSearchHits(hits);
      setSearchTotal(total);
      setIndexIncomplete(incomplete);
      setSearching(false);
    };

    void run();
    return () => {
      cancelled = true;
    };
  }, [searchQuery, visiblePlatformsKey]);

  const handleSearchChange = useCallback((value: string) => {
    setSearchInput(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => setSearchQuery(value), 300);
  }, []);

  const openHit = useCallback((hit: GlobalHit) => {
    dispatch({ type: "setSearchQuery", payload: searchQuery.trim() });
    dispatch({ type: "setSelectedSessionKey", payload: hit.session.sessionKey });
    navigate(`/${hit.platform}`);
  }, [dispatch, navigate, searchQuery]);

  const platforms = state.dashboard?.platforms ?? [];
  const displayPlatforms = visiblePlatforms.flatMap((platformId) => {
    const platform = platformMeta.find((item) => item.key === platformId);
    return platform ? [platform] : [];
  });
  const hasSearch = searchQuery.trim().length > 0;

  return (
    <div className="flex h-full flex-col overflow-y-auto pr-2 pb-6">
      {/* Hero with dynamic glowing abstract background */}
      <section className="relative shrink-0 overflow-hidden rounded-[28px] border border-border/80 bg-gradient-to-br from-card/85 via-card/75 to-card/40 px-6 py-7 md:px-8 md:py-8 backdrop-blur-md shadow-xl shadow-black/10">
        {/* Glow Spheres */}
        <div className="absolute -top-12 -left-12 size-48 bg-primary/8 blur-[90px] rounded-full pointer-events-none" />
        <div className="absolute -bottom-16 -right-16 size-56 bg-violet-500/6 blur-[110px] rounded-full pointer-events-none" />
        <div className="absolute inset-y-0 right-0 hidden w-[34%] bg-[radial-gradient(circle_at_center,rgba(255,255,255,0.04),transparent_72%)] lg:block pointer-events-none" />

        <div className="relative flex flex-col gap-6">
          <div className="flex flex-col md:flex-row md:items-center gap-5">
            <div className="inline-flex size-16 shrink-0 items-center justify-center rounded-2xl overflow-hidden shadow-lg shadow-black/25 ring-soft bg-stone-900 border border-white/5 transition-transform duration-300 hover:scale-105 select-none">
              <AppLogo className="size-16" />
            </div>
            <div className="min-w-0">
              <p className="text-fine uppercase tracking-[0.28em] text-primary font-bold">Memory Forge</p>
              <h2 className="mt-1 max-w-3xl text-3xl font-extrabold leading-tight md:text-4xl bg-gradient-to-r from-foreground via-foreground to-foreground/80 bg-clip-text text-transparent">
                {t("welcomeTitle")}
              </h2>
            </div>
          </div>
          <p className="max-w-3xl text-sm md:text-base leading-7 text-quiet">{t("welcomeDesc")}</p>
          <div className="flex flex-wrap items-center gap-3 pt-2">
            <Button asChild size="lg" className="rounded-xl shadow-md shadow-primary/14 hover:shadow-lg hover:shadow-primary/22 cursor-pointer transition-all duration-200">
              <Link to="/prompts">
                {t("prompts")}
                <ArrowRight className="size-4" />
              </Link>
            </Button>
            <div className="rounded-xl border border-border/80 bg-white/5 px-4 py-2.5 text-xs md:text-sm text-quiet backdrop-blur-md select-none font-medium">
              {loading
                ? t("loading")
                : `${snapshot?.appName ?? "Memory Forge"} · v${snapshot?.version ?? "3.3.1"}`}
            </div>
          </div>
        </div>
      </section>

      {/* Global full-text search */}
      <section className="mt-5 relative shrink-0 overflow-hidden rounded-[24px] border border-border/80 bg-gradient-to-br from-card/85 via-card/70 to-card/35 px-4 py-4 md:px-5 md:py-5 backdrop-blur-md shadow-lg shadow-black/5">
        <div className="absolute -top-10 right-8 size-28 bg-primary/6 blur-[70px] rounded-full pointer-events-none" />
        <div className="relative space-y-3">
          <div className="flex flex-wrap items-end justify-between gap-2 px-0.5">
            <p className="text-[10px] font-bold uppercase tracking-[0.22em] text-primary/80">
              {t("dashboard.globalSearchHint")}
            </p>
            {hasSearch && !searching && (
              <p className="text-xs text-quiet">
                {t("dashboard.globalSearchResults", { count: searchTotal })}
              </p>
            )}
          </div>
          <div className="relative">
            <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground/50" />
            <Input
              placeholder={t("dashboard.globalSearch")}
              value={searchInput}
              onChange={(e) => handleSearchChange(e.target.value)}
              className="h-12 pl-11 text-sm bg-muted/20 border-border/40 hover:border-border/80 focus-visible:ring-1 focus-visible:ring-primary/40 focus-visible:border-primary/40 rounded-xl transition-all"
            />
          </div>
          {hasSearch && indexIncomplete && (
            <p className="text-xs text-amber-700 dark:text-amber-300 px-0.5">
              {t("session.searchIndexIncomplete")}
            </p>
          )}
          {hasSearch && (
            <div className="space-y-2 max-h-[320px] overflow-y-auto pr-1">
              {searching ? (
                <div className="rounded-xl border border-border/40 bg-muted/15 px-4 py-6 text-center text-sm text-quiet">
                  {t("dashboard.globalSearching")}
                </div>
              ) : searchHits.length === 0 ? (
                <div className="rounded-xl border border-border/40 bg-muted/15 px-4 py-6 text-center text-sm text-quiet">
                  {t("dashboard.globalSearchEmpty")}
                </div>
              ) : (
                searchHits.map((hit) => (
                  <button
                    key={`${hit.platform}:${hit.session.sessionKey}`}
                    type="button"
                    onClick={() => openHit(hit)}
                    className="group w-full text-left rounded-xl border border-border/40 bg-card/50 hover:bg-card/85 hover:border-primary/30 px-3.5 py-3 transition-all duration-200"
                  >
                    <div className="flex items-center gap-2.5 min-w-0">
                      <span className="shrink-0 rounded-lg border border-border/40 bg-muted/30 px-2 py-0.5 text-[10px] font-semibold text-quiet group-hover:text-foreground">
                        {hit.platformLabel}
                      </span>
                      <span className="min-w-0 flex-1 truncate text-sm font-semibold text-foreground">
                        {hit.session.displayTitle || hit.session.sessionId}
                      </span>
                      <ArrowRight className="size-3.5 shrink-0 text-muted-foreground/40 opacity-0 group-hover:opacity-100 transition-opacity" />
                    </div>
                    {hit.session.preview && (
                      <p className="mt-1.5 line-clamp-1 text-xs text-quiet break-all">
                        {hit.session.preview}
                      </p>
                    )}
                    {hit.session.contentMatches && hit.session.contentMatches.length > 0 && (
                      <div className="mt-2 space-y-1.5">
                        {hit.session.contentMatches.slice(0, 2).map((match, i) => (
                          <div key={i} className="flex items-start gap-1.5 rounded-lg bg-amber-500/5 border border-amber-500/12 px-2.5 py-1.5">
                            {match.role === "user" ? (
                              <User className="size-3 shrink-0 mt-0.5 text-amber-400/60" />
                            ) : (
                              <Bot className="size-3 shrink-0 mt-0.5 text-amber-400/60" />
                            )}
                            <p className="text-[11px] leading-relaxed text-muted-foreground/80 line-clamp-2 break-all font-mono">
                              {match.snippet}
                            </p>
                          </div>
                        ))}
                      </div>
                    )}
                  </button>
                ))
              )}
            </div>
          )}
        </div>
      </section>

      {/* Platform Session Cards */}
      <section className={cn(
        "mt-5 grid gap-4 grid-cols-2",
        displayPlatforms.length === 1 && "xl:grid-cols-1 max-w-sm",
        displayPlatforms.length === 2 && "xl:grid-cols-2 max-w-2xl",
        displayPlatforms.length === 3 && "xl:grid-cols-3 max-w-4xl",
        displayPlatforms.length === 4 && "xl:grid-cols-4",
        displayPlatforms.length >= 5 && "xl:grid-cols-5"
      )}>
        {displayPlatforms.map((pm) => {
          const Icon = pm.icon;
          const summary = platforms.find((p) => p.platform === pm.key);
          const count = summary?.count ?? 0;
          const latest = summary?.latest || "—";
          const isPlatformLoading = dashboardLoading && !summary;
          return (
            <Link
              key={pm.key}
              to={pm.to}
              className={cn(
                "group setting-card rounded-[24px] border bg-gradient-to-b p-5 h-[120px] flex flex-col justify-between transition-all duration-300",
                pm.gradient,
                pm.border,
                pm.hoverGlow
              )}
            >
              <div className="flex items-center gap-3">
                <div className={cn("inline-flex size-11 items-center justify-center rounded-2xl transition-all duration-300", pm.iconBg)}>
                  <Icon className="size-5" />
                </div>
                <div className="min-w-0">
                  <p className="text-sm font-semibold text-quiet group-hover:text-foreground transition-colors">{pm.label}</p>
                  <p className="text-2xl font-bold tracking-tight">{isPlatformLoading ? "…" : count}</p>
                </div>
              </div>
              <p className="truncate text-xs text-quiet border-t border-border/30 pt-2 mt-1">
                最近活跃: {isPlatformLoading ? t("loading") : latest}
              </p>
            </Link>
          );
        })}
      </section>
      {dashboardError && (
        <div className="mt-3 rounded-xl border border-red-500/25 bg-red-500/8 px-4 py-2 text-xs text-red-300">
          Dashboard 加载失败: {dashboardError}
        </div>
      )}

      {/* Feature Cards */}
      <section className="mt-5 grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        <FeatureCard icon={<Brain className="size-5" />} title={t("memoryManipulation")} description={t("memoryManipulationDesc")} />
        <FeatureCard icon={<Flame className="size-5" />} title={t("localFirst")} description="100% 本地运行，零云端依赖。你的数据不会离开你的电脑。" />
        <FeatureCard icon={<ArrowRight className="size-5" />} title={t("multiPlatform")} description="Claude Code / Codex CLI / OpenCode 统一管理，一个界面搞定。" />
      </section>

      {/* Quick Links */}
      <section className="mt-5 setting-card rounded-[24px] p-6 bg-gradient-to-r from-card/50 via-card/30 to-transparent border border-border/40">
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-5">
          <div className="select-none">
            <p className="text-fine uppercase tracking-[0.24em] text-primary font-bold">快捷导航</p>
            <p className="text-xs text-quiet mt-1.5">快速跳转提示词库、全局参数配置或了解本项目</p>
          </div>
          <div className="flex flex-wrap gap-2.5">
            <Link to="/prompts" className="rounded-xl border border-border/80 bg-white/4 px-4 py-2.5 text-xs font-semibold text-foreground/86 hover:bg-primary/12 hover:text-primary hover:border-primary/30 transition-all duration-300">
              {t("promptLibrary")}
            </Link>
            <Link to="/settings" className="rounded-xl border border-border/80 bg-white/4 px-4 py-2.5 text-xs font-semibold text-foreground/86 hover:bg-primary/12 hover:text-primary hover:border-primary/30 transition-all duration-300">
              {t("settings")}
            </Link>
            <Link to="/about" className="rounded-xl border border-border/80 bg-white/4 px-4 py-2.5 text-xs font-semibold text-foreground/86 hover:bg-primary/12 hover:text-primary hover:border-primary/30 transition-all duration-300">
              {t("about")}
            </Link>
          </div>
        </div>
      </section>
    </div>
  );
}

function FeatureCard({ icon, title, description }: { icon: React.ReactNode; title: string; description: string }) {
  return (
    <article className="setting-card rounded-[24px] p-5 hover:-translate-y-1 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5 transition-all duration-300">
      <div className="space-y-3">
        <div className="inline-flex size-11 items-center justify-center rounded-2xl bg-primary/12 text-primary shadow-sm shadow-primary/5 transition-transform duration-300 hover:rotate-6">
          {icon}
        </div>
        <div>
          <h3 className="text-lg font-semibold">{title}</h3>
          <p className="mt-2 text-sm leading-6 text-quiet">{description}</p>
        </div>
      </div>
    </article>
  );
}
