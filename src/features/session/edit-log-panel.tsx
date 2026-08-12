import { ScrollArea } from '@/components/ui/scroll-area'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { useDesktop } from '@/features/desktop/provider'
import { cn } from '@/lib/utils'
import { FileText, ArrowLeft, ArrowRight, Clock, Eye, EyeOff, RefreshCw, CheckCircle, Undo2, Trash2, FileDiff, Files, BrainCircuit, LocateFixed, Pencil, Copy, Check, PanelRightClose } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router'
import type { MessageKey } from '@/features/desktop/i18n'
import type { EditLogEntry } from '@/features/desktop/types'
import { api, isSessionRevisionConflict } from '@/features/desktop/api'
import type { WorkspaceInspector } from '@/features/workspace/types'
import { resolveSessionCapabilities } from '@/features/session/capabilities'

function DiffView({ oldText, newText, t }: { oldText: string; newText: string; t: (key: MessageKey) => string }) {
  const [expanded, setExpanded] = useState(false)
  const maxLen = 120
  const oldPreview = oldText.length > maxLen && !expanded ? oldText.slice(0, maxLen) + '...' : oldText
  const newPreview = newText.length > maxLen && !expanded ? newText.slice(0, maxLen) + '...' : newText

  return (
    <div className="space-y-2">
      <div className="rounded-lg bg-red-500/5 border border-red-500/20 p-3">
        <div className="flex items-center gap-1.5 mb-1.5">
          <span className="text-[10px] font-medium text-red-400/80 uppercase tracking-wider">{t('editLog.before')}</span>
          {!expanded && oldText.length > maxLen && (
            <button onClick={() => setExpanded(true)} className="text-[10px] text-muted-foreground/50 hover:text-foreground">{t('editLog.expand')}</button>
          )}
        </div>
        <pre className="text-xs text-red-300/80 whitespace-pre-wrap font-mono leading-relaxed">{oldPreview}</pre>
      </div>
      <div className="flex justify-center"><ArrowRight className="w-3.5 h-3.5 text-muted-foreground/30" /></div>
      <div className="rounded-lg bg-green-500/5 border border-green-500/20 p-3">
        <div className="flex items-center gap-1.5 mb-1.5">
          <span className="text-[10px] font-medium text-green-400/80 uppercase tracking-wider">{t('editLog.after')}</span>
          {!expanded && newText.length > maxLen && (
            <button onClick={() => setExpanded(true)} className="text-[10px] text-muted-foreground/50 hover:text-foreground">{t('editLog.expand')}</button>
          )}
        </div>
        <pre className="text-xs text-green-300/80 whitespace-pre-wrap font-mono leading-relaxed">{newPreview}</pre>
      </div>
    </div>
  )
}

export function EditLogPanel() {
  const { t, state, dispatch, isRemote, remoteCapabilities } = useDesktop()
  const currentPlatform = state.currentPlatform
  const editLog = state.editLog
  const selectedSessionKey = state.selectedSessionKey
  const sessionDetail = state.sessionDetail
  const activeTabId = state.workspace.activeTabId
  const sessionCapabilities = resolveSessionCapabilities(sessionDetail, isRemote, remoteCapabilities)
  const activeInspector = activeTabId ? state.workspace.viewByTabId[activeTabId]?.inspector ?? null : null
  const [expandedId, setExpandedId] = useState<number | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const [refreshDone, setRefreshDone] = useState(false)
  const [deletingId, setDeletingId] = useState<number | null>(null)
  const [copiedKey, setCopiedKey] = useState<string | null>(null)
  const navigate = useNavigate()

  useEffect(() => {
    const target = state.inspectMemoryTarget
    if (!target || target.platform !== currentPlatform || target.sessionKey !== selectedSessionKey) return
    const entry = editLog.find((item) => item.editTarget === target.editTarget)
    if (entry) setExpandedId(entry.id)
    dispatch({ type: 'setInspectMemoryTarget', payload: null })
  }, [state.inspectMemoryTarget, currentPlatform, selectedSessionKey, editLog, dispatch])

  const setInspector = (inspector: WorkspaceInspector) => {
    if (!activeTabId) return
    dispatch({
      type: 'workspace',
      payload: { type: 'restore-view-state', payload: { tabId: activeTabId, state: { inspector } } },
    })
  }

  const handleLocate = (entry: EditLogEntry) => {
    if (!selectedSessionKey) return
    dispatch({
      type: 'setLocateMessageTarget',
      payload: { platform: currentPlatform, sessionKey: selectedSessionKey, editTarget: entry.editTarget },
    })
    if (isRemote) {
      const params = new URLSearchParams({ session: selectedSessionKey })
      navigate(`/${currentPlatform}?${params}`)
    }
  }

  const handleReEdit = (entry: EditLogEntry) => {
    if (!sessionCapabilities.edit) return
    if (!sessionDetail || !selectedSessionKey || sessionDetail.sessionKey !== selectedSessionKey) return
    const block = sessionDetail.blocks.find((item) => (item.editTarget || item.id) === entry.editTarget)
    const content = block?.content ?? entry.newContent
    dispatch({
      type: 'setEditingBlock',
      payload: {
        id: entry.editTarget,
        platform: currentPlatform,
        sessionKey: selectedSessionKey,
        content,
        originalContent: content,
        role: block?.role ?? 'assistant',
        revision: sessionDetail.revision,
      },
    })
  }

  const handleCopy = async (entry: EditLogEntry, side: 'before' | 'after') => {
    await navigator.clipboard.writeText(side === 'before' ? entry.oldContent : entry.newContent)
    const key = `${entry.id}:${side}`
    setCopiedKey(key)
    window.setTimeout(() => setCopiedKey(current => current === key ? null : current), 1500)
  }

  const handleRefreshLog = async () => {
    if (!selectedSessionKey) return
    setRefreshing(true)
    setRefreshDone(false)
    try {
      const logs = await api.getEditLog(currentPlatform, selectedSessionKey)
      dispatch({ type: 'setEditLogForSession', payload: { platform: currentPlatform, sessionKey: selectedSessionKey, editLog: logs } })
      setRefreshDone(true)
      setTimeout(() => setRefreshDone(false), 1500)
    } catch (err) {
      console.error('Failed to refresh edit log:', err)
    }
    setRefreshing(false)
  }

  const handleRestore = async (entry: EditLogEntry) => {
    if (!sessionCapabilities.restore) return
    if (!selectedSessionKey || !sessionDetail || sessionDetail.sessionKey !== selectedSessionKey) return
    const expectedRevision = sessionDetail.revision
    if (!window.confirm(t('session.restoreConfirm'))) return
    try {
      await api.restoreMessage(currentPlatform, entry.id, selectedSessionKey, expectedRevision)
      const [logs, detail] = await Promise.all([
        api.getEditLog(currentPlatform, selectedSessionKey),
        api.getSessionDetail(currentPlatform, selectedSessionKey),
      ])
      dispatch({ type: 'setEditLogForSession', payload: { platform: currentPlatform, sessionKey: selectedSessionKey, editLog: logs } })
      dispatch({ type: 'setSessionDetail', payload: detail })
      dispatch({
        type: 'setLocateMessageTarget',
        payload: { platform: currentPlatform, sessionKey: selectedSessionKey, editTarget: entry.editTarget },
      })
      dispatch({ type: 'setSessionStatus', payload: { tone: 'success', message: t('session.messageSaved') } })
    } catch (err) {
      console.error('Failed to restore message:', err)
      const conflict = isSessionRevisionConflict(err)
      if (conflict) {
        try {
          const [logs, detail] = await Promise.all([
            api.getEditLog(currentPlatform, selectedSessionKey),
            api.getSessionDetail(currentPlatform, selectedSessionKey),
          ])
          dispatch({ type: 'setEditLogForSession', payload: { platform: currentPlatform, sessionKey: selectedSessionKey, editLog: logs } })
          dispatch({ type: 'setSessionDetail', payload: detail })
        } catch (refreshError) {
          console.error('Failed to refresh after revision conflict:', refreshError)
        }
      }
      dispatch({
        type: 'setSessionStatus',
        payload: { tone: 'error', message: conflict ? t('session.revisionConflict') : t('session.saveFailed') },
      })
    }
  }

  const handleDelete = async (entry: EditLogEntry) => {
    if (isRemote) return
    if (!selectedSessionKey || !window.confirm(t('editLog.deleteConfirm'))) return
    setDeletingId(entry.id)
    try {
      const deleted = await api.deleteEditLog(currentPlatform, entry.id, selectedSessionKey)
      if (!deleted) throw new Error('Edit log does not belong to this session')
      dispatch({ type: 'setEditLogForSession', payload: { platform: currentPlatform, sessionKey: selectedSessionKey, editLog: editLog.filter((item) => item.id !== entry.id) } })
      dispatch({ type: 'setSessionStatus', payload: { tone: 'success', message: t('editLog.deleted') } })
    } catch (err) {
      console.error('Failed to delete edit log:', err)
      dispatch({ type: 'setSessionStatus', payload: { tone: 'error', message: t('session.saveFailed') } })
    } finally {
      setDeletingId(null)
    }
  }

  const handleClear = async () => {
    if (isRemote) return
    if (!selectedSessionKey || editLog.length === 0 || !window.confirm(t('editLog.clearConfirm'))) return
    setDeletingId(-1)
    try {
      await api.clearEditLogs(currentPlatform, selectedSessionKey)
      dispatch({ type: 'setEditLogForSession', payload: { platform: currentPlatform, sessionKey: selectedSessionKey, editLog: [] } })
      dispatch({ type: 'setSessionStatus', payload: { tone: 'success', message: t('editLog.deleted') } })
    } catch (err) {
      console.error('Failed to clear edit logs:', err)
      dispatch({ type: 'setSessionStatus', payload: { tone: 'error', message: t('session.saveFailed') } })
    } finally {
      setDeletingId(null)
    }
  }

  if (currentPlatform === 'dashboard' || currentPlatform === 'about' || currentPlatform === 'prompts' || currentPlatform === 'settings' || !selectedSessionKey) return null
  if (!isRemote && !activeInspector) return null

  if (isRemote) {
    return (
      <section className="remote-memory-page" aria-label={t('inspector.memory')}>
        <header className="remote-change-header">
          <button
            type="button"
            className="remote-icon-button"
            onClick={() => {
              const params = new URLSearchParams({ session: selectedSessionKey })
              navigate(`/${currentPlatform}?${params}`)
            }}
            title={t('mobileBackToSessions')}
            aria-label={t('mobileBackToSessions')}
          >
            <ArrowLeft className="size-5" />
          </button>
          <div className="min-w-0">
            <p className="remote-kicker">{t('remoteRevisionHistory')}</p>
            <h2>{t('remoteMemoryTitle')}</h2>
            <span>{sessionDetail?.aliasTitle || sessionDetail?.title || selectedSessionKey}</span>
          </div>
          <span className="remote-change-count">{editLog.length}</span>
          <button
            type="button"
            className={cn('remote-icon-button ml-auto', refreshDone && 'remote-icon-button-success')}
            onClick={() => void handleRefreshLog()}
            disabled={refreshing}
            title={t('session.refresh')}
            aria-label={t('session.refresh')}
          >
            {refreshDone
              ? <CheckCircle className="size-4" />
              : <RefreshCw className={cn('size-4', refreshing && 'animate-spin')} />}
          </button>
        </header>

        <div className="remote-memory-context">
          <BrainCircuit className="size-4" />
          <div>
            <strong>{t('remoteMemoryContextTitle')}</strong>
            <span>{t('remoteMemoryContextHint')}</span>
          </div>
        </div>

        <div className="remote-change-scroll">
          {editLog.length === 0 ? (
            <div className="remote-empty-state">
              <FileText className="size-6" />
              <strong>{t('editLog.noRecords')}</strong>
            </div>
          ) : (
            <div className="remote-change-list">
              {editLog.map((entry: EditLogEntry) => {
                const expanded = expandedId === entry.id
                return (
                  <article key={entry.id} className="remote-change-entry">
                    <span className="remote-change-track"><i /></span>
                    <div className="remote-change-entry-body">
                      <div className="remote-change-meta">
                        <time dateTime={entry.createdAt}>
                          {new Date(entry.createdAt).toLocaleString(undefined, {
                            month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
                          })}
                        </time>
                        <span>{entry.editTarget.length > 44 ? `${entry.editTarget.slice(0, 44)}...` : entry.editTarget}</span>
                      </div>

                      {expanded ? (
                        <div className="remote-change-diff">
                          <section data-side="before">
                            <strong>{t('editLog.before')}</strong>
                            <pre>{entry.oldContent}</pre>
                          </section>
                          <section data-side="after">
                            <strong>{t('editLog.after')}</strong>
                            <pre>{entry.newContent}</pre>
                          </section>
                        </div>
                      ) : (
                        <div className="remote-change-preview">
                          <p><b>-</b>{entry.oldContent.slice(0, 100)}</p>
                          <p><b>+</b>{entry.newContent.slice(0, 100)}</p>
                        </div>
                      )}

                      <div className="remote-change-actions">
                        <button type="button" onClick={() => setExpandedId(expanded ? null : entry.id)}>
                          {expanded ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                          {expanded ? t('editLog.collapse') : t('editLog.viewDetail')}
                        </button>
                        <button type="button" onClick={() => handleLocate(entry)}>
                          <LocateFixed className="size-4" />
                          {t('editLog.locate')}
                        </button>
                        <button type="button" onClick={() => void handleCopy(entry, 'after')}>
                          {copiedKey === `${entry.id}:after` ? <Check className="size-4" /> : <Copy className="size-4" />}
                          {t(copiedKey === `${entry.id}:after` ? 'copied' : 'copy')}
                        </button>
                        {sessionCapabilities.edit && (
                          <button type="button" onClick={() => handleReEdit(entry)}>
                            <Pencil className="size-4" />
                            {t('editLog.reEdit')}
                          </button>
                        )}
                        {sessionCapabilities.restore && (
                          <button type="button" className="remote-restore-button" onClick={() => void handleRestore(entry)}>
                            <Undo2 className="size-4" />
                            {t('session.restore')}
                          </button>
                        )}
                      </div>
                    </div>
                  </article>
                )
              })}
            </div>
          )}
        </div>
      </section>
    )
  }

  return (
    <>
      <button
        type="button"
        className="edit-log-scrim"
        onClick={() => dispatch({ type: 'setShowEditLog', payload: false })}
        aria-label={t('editLog.collapse')}
      />
      <aside className="edit-log-panel h-full flex-shrink-0 flex-col border-border/60 bg-card/95 backdrop-blur-xl" aria-label={t('inspector.title')}>
        <header className="border-b border-border/60 px-3 pt-3">
          <div className="flex items-center justify-between px-1 pb-3">
            <div className="min-w-0">
              <p className="text-[10px] font-semibold uppercase text-muted-foreground">{t('inspector.title')}</p>
              <h2 className="truncate text-sm font-semibold text-foreground">{sessionDetail?.aliasTitle || sessionDetail?.title || t('editLog.title')}</h2>
            </div>
            <Button variant="ghost" size="icon" className="size-8" onClick={() => dispatch({ type: 'setShowEditLog', payload: false })} title={t('editLog.collapse')} aria-label={t('editLog.collapse')}>
              <PanelRightClose className="size-4" />
            </Button>
          </div>
          <div className="grid grid-cols-3 gap-1" role="tablist" aria-label={t('inspector.title')}>
            {([
              ['changes', FileDiff, t('inspector.changes')],
              ['files', Files, t('inspector.files')],
              ['memory', BrainCircuit, t('inspector.memory')],
            ] as const).map(([value, Icon, label]) => (
              <button
                key={value}
                type="button"
                role="tab"
                aria-selected={activeInspector === value}
                onClick={() => {
                  setInspector(value)
                  if (value === 'memory') void handleRefreshLog()
                }}
                className={cn(
                  'flex min-h-10 items-center justify-center gap-1.5 border-b-2 px-1 text-xs font-medium transition-colors',
                  activeInspector === value
                    ? 'border-amber-500 text-foreground'
                    : 'border-transparent text-muted-foreground hover:text-foreground',
                )}
              >
                <Icon className="size-3.5" />
                <span>{label}</span>
              </button>
            ))}
          </div>
        </header>

        {activeInspector !== 'memory' ? (
          <div className="flex min-h-0 flex-1 flex-col items-center justify-center px-8 text-center">
            {activeInspector === 'changes' ? <FileDiff className="mb-4 size-7 text-muted-foreground/50" /> : <Files className="mb-4 size-7 text-muted-foreground/50" />}
            <p className="text-sm font-medium text-foreground">{t('inspector.unavailable')}</p>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">
              {activeInspector === 'changes' ? t('inspector.changesUnavailable') : t('inspector.filesUnavailable')}
            </p>
          </div>
        ) : (
          <>
            <div className="flex items-center gap-2 border-b border-border/50 px-4 py-2.5">
              <span className="mr-auto text-xs text-muted-foreground">{t('editLog.recordCount', { count: editLog.length })}</span>
              {editLog.length > 0 && (
                <Button variant="ghost" size="icon" className="size-8 text-muted-foreground hover:text-red-400" onClick={() => void handleClear()} disabled={deletingId !== null} title={t('editLog.clear')} aria-label={t('editLog.clear')}>
                  <Trash2 className="size-3.5" />
                </Button>
              )}
              <Button variant="ghost" size="icon" className={cn('size-8', refreshDone && 'text-emerald-400')} onClick={() => void handleRefreshLog()} disabled={refreshing} title={t('session.refresh')} aria-label={t('session.refresh')}>
                {refreshDone ? <CheckCircle className="size-3.5" /> : <RefreshCw className={cn('size-3.5', refreshing && 'animate-spin')} />}
              </Button>
            </div>

            <ScrollArea className="h-0 min-h-0 flex-1">
              {editLog.length === 0 ? (
                <div className="px-6 py-14 text-center">
                  <BrainCircuit className="mx-auto mb-4 size-8 text-amber-500/50" />
                  <p className="text-sm font-medium text-foreground/80">{t('editLog.noRecords')}</p>
                  <p className="mt-2 text-xs leading-5 text-muted-foreground">{t('editLog.afterEditHint')}</p>
                </div>
              ) : (
                <div className="divide-y divide-border/50">
                  {editLog.map((entry) => {
                    const expanded = expandedId === entry.id
                    return (
                      <article key={entry.id} className="px-4 py-4">
                        <div className="mb-2 flex items-center gap-2">
                          <Clock className="size-3 text-muted-foreground/50" />
                          <time className="text-[10px] text-muted-foreground" dateTime={entry.createdAt}>
                            {new Date(entry.createdAt).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}
                          </time>
                        </div>
                        <Badge variant="outline" className="mb-3 block max-w-full truncate text-[10px]" title={entry.editTarget}>
                          {entry.editTarget}
                        </Badge>

                        {expanded ? (
                          <div className="space-y-3">
                            <DiffView oldText={entry.oldContent} newText={entry.newContent} t={t} />
                            <div className="flex flex-wrap items-center gap-1">
                              <Button variant="ghost" size="icon" className="size-8" onClick={() => void handleCopy(entry, 'before')} title={t('editLog.copyBefore')} aria-label={t('editLog.copyBefore')}>
                                {copiedKey === `${entry.id}:before` ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
                              </Button>
                              <Button variant="ghost" size="icon" className="size-8" onClick={() => void handleCopy(entry, 'after')} title={t('editLog.copyAfter')} aria-label={t('editLog.copyAfter')}>
                                {copiedKey === `${entry.id}:after` ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
                              </Button>
                              <Button variant="ghost" size="sm" className="ml-auto h-8 gap-1.5 text-xs" onClick={() => setExpandedId(null)}>
                                <EyeOff className="size-3.5" />{t('editLog.collapse')}
                              </Button>
                            </div>
                          </div>
                        ) : (
                          <button type="button" className="mb-3 block w-full text-left" onClick={() => setExpandedId(entry.id)}>
                            <p className="line-clamp-1 font-mono text-xs text-red-400/70">- {entry.oldContent}</p>
                            <p className="mt-1 line-clamp-1 font-mono text-xs text-emerald-400/70">+ {entry.newContent}</p>
                          </button>
                        )}

                        <div className="grid grid-cols-2 gap-1.5">
                          <Button variant="outline" size="sm" className="h-8 min-w-0 gap-1 text-xs" onClick={() => setExpandedId(expanded ? null : entry.id)}>
                            {expanded ? <EyeOff className="size-3" /> : <Eye className="size-3" />}{expanded ? t('editLog.collapse') : t('editLog.viewDetail')}
                          </Button>
                          <Button variant="outline" size="sm" className="h-8 min-w-0 gap-1 text-xs" onClick={() => handleLocate(entry)}>
                            <LocateFixed className="size-3" />{t('editLog.locate')}
                          </Button>
                          {sessionCapabilities.edit && (
                            <Button variant="outline" size="sm" className="h-8 min-w-0 gap-1 text-xs" onClick={() => handleReEdit(entry)}>
                              <Pencil className="size-3" />{t('editLog.reEdit')}
                            </Button>
                          )}
                          {sessionCapabilities.restore && (
                            <Button variant="outline" size="sm" className="h-8 min-w-0 gap-1 border-blue-500/25 text-xs text-blue-400 hover:bg-blue-500/10" onClick={() => void handleRestore(entry)}>
                              <Undo2 className="size-3" />{t('session.restore')}
                            </Button>
                          )}
                        </div>
                        <div className="mt-2 flex justify-end">
                          <Button variant="ghost" size="icon" className="size-7 text-muted-foreground hover:text-red-400" onClick={() => void handleDelete(entry)} disabled={deletingId === entry.id} title={t('editLog.delete')} aria-label={t('editLog.delete')}>
                            <Trash2 className="size-3" />
                          </Button>
                        </div>
                      </article>
                    )
                  })}
                </div>
              )}
            </ScrollArea>
            <footer className="border-t border-border/50 px-4 py-3 text-[10px] leading-4 text-muted-foreground">
              {t('editLog.traceDesc')}
            </footer>
          </>
        )}
      </aside>
    </>
  )
}
