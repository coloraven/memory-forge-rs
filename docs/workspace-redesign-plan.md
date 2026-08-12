# Memory Forge 工作区体验重构规划

状态：Draft，等待按阶段实施  
目标分支：`feature/remote-foundation` 及其后续功能分支  
更新日期：2026-07-19

## 1. 背景与结论

Memory Forge 当前已经具备多平台会话读取、结构化时间线、历史消息编辑、擦除、审计恢复、嵌入式终端和 LAN 远程伴侣等基础能力，但桌面端仍以“平台页 + 单个选中会话”为主要状态模型，移动端也更接近远程功能集合，而不是完整的会话工作台。

本次重构的目标是把产品升级为：

- 桌面端以工作区和多会话页签为核心。
- 结构化会话是主要阅读与操作界面。
- 原始 PTY 终端保留为独立入口和兼容层，不与 Memory 审计混合。
- 历史记忆编辑成为消息内的一级操作，并在 Memory 面板集中审计。
- 移动端沿用同一套会话与修改能力，但采用适合触控的单任务导航。

结论：该方案可以在现有架构上渐进实现，不需要重写 Rust 平台适配器或远程协议。主要改造集中在 React 工作区状态、布局组合和跨页签生命周期管理。

## 2. 原型基线

交互原型位于仓库的 `.protos` 目录：

- [桌面工作区](../.protos/memory-forge-desktop-workspace.html)
- [桌面历史编辑态](../.protos/memory-forge-desktop-memory-edit.png)
- [移动端伴侣](../.protos/memory-forge-mobile-companion.html)
- [移动端历史编辑态](../.protos/memory-forge-mobile-memory-edit.png)

原型用于确认信息架构、主要状态和交互方向，不要求正式实现逐像素复制。正式界面继续使用项目现有主题、组件库、国际化和可访问性约定。

## 3. 产品原则

### 3.1 结构化会话优先

用户默认看到标准化后的用户消息、助手回复、thinking 和工具调用。原始终端不承担主要阅读界面的职责。

### 3.2 Memory 是产品核心，不是附加设置

历史消息操作应出现在消息附近；完整修改历史、前后差异和恢复能力进入同一会话的 Memory Inspector。

### 3.3 终端与 Memory 分离

- 桌面：原始终端位于底部可展开抽屉，或进入已有终端工作区。
- 移动：终端保留为底部主导航中的独立入口。
- Memory 审计页面不再包含 Files/Terminal 页签。

### 3.4 一套 UI，多个平台适配器

UI 只消费 `SessionDetail`、`TimelineBlock`、`ToolCallBlock` 等统一 DTO。Claude Code、Codex、Pi、Grok 等格式差异继续由 Rust `PlatformAdapter` 处理。

### 3.5 快照权威，实时事件只做加速

会话详情和 revision 快照始终是权威数据。后续 SSE/WebSocket 可以触发刷新，但不能成为唯一真相来源。

### 3.6 渐进迁移

先保留当前单会话行为作为兼容路径，再逐步引入页签、Inspector 和缓存。避免同时重写路由、平台适配、终端和远程协议。

## 4. 范围

### 4.1 本次包含

- 桌面工作区侧栏和多会话页签。
- 每个页签独立的会话详情、审计记录、草稿和视图状态。
- 消息内编辑、擦除、复制、定位操作。
- 右侧 Changes / Files / Memory Inspector。
- 桌面底部原始终端抽屉。
- 移动端会话总览、会话切换、编辑底部抽屉和 Memory 审计页。
- revision 冲突反馈和权威快照刷新。
- Claude Code、Codex、Pi、Grok 的适配回归验证。

### 4.2 本次不包含

- 复制 Paseo 的 AGPL 源代码。
- 一开始就实现原生 iOS/Android App；第一阶段仍使用现有远程 Web 伴侣。
- 允许远程客户端执行任意 shell 命令。
- 为所有 CLI 实现完全一致的实时结构化流。
- 将所有历史会话内容长期写入浏览器存储。
- 第一阶段新增复杂的工作区/git 管理后端。

## 5. 当前能力盘点

### 5.1 可以直接复用

| 能力 | 当前实现 | 重构策略 |
| --- | --- | --- |
| 统一平台 DTO | `src-tauri/src/platforms/mod.rs` | 保持 DTO 作为 UI 边界 |
| Claude/Codex/Pi/Grok 适配器 | `src-tauri/src/platforms/*.rs` | 不重写，增加契约回归测试 |
| 会话详情 | `SessionDetail.blocks` | 作为每个页签的权威快照 |
| 历史消息编辑 | `session_edit_message` | 接入消息内操作和编辑抽屉 |
| 擦除消息 | 空内容调用编辑接口 | 保留确认和审计 |
| 恢复历史 | `session_restore_message` | 移入 Memory Inspector |
| 修改日志 | SQLite `edit_log` | 按页签/会话加载 |
| revision 冲突 | SHA-256 revision + 409 | 新 UI 展示冲突恢复路径 |
| 嵌入式终端 | Desktop PTY manager | 作为底部抽屉复用 |
| 远程终端 | `/api/v1/terminals` | 移动终端入口复用 |
| 远程修改 | `/api/v1/mutations/*` | 移动编辑抽屉复用 |

### 5.2 需要改造

当前 `AppState` 使用单一的：

```ts
selectedSessionKey: string | null
sessionDetail: SessionDetail | null
editLog: EditLogEntry[]
```

这会导致切换会话时覆盖详情、审计和滚动状态，无法支撑多页签。需要改为“工作区级页签 + 页签级视图状态”。

### 5.3 需要单独定义的新能力

原型中的“注入”需要区分两种语义：

1. 在现有历史消息中补充上下文：当前编辑接口已经支持。
2. 在两条历史消息之间新增一条独立消息：当前 `PlatformAdapter::update_message` 不支持。

第一阶段只提供编辑、擦除和恢复。若要支持真正的“前插/后插”，必须新增平台级插入契约、原子写入、revision 校验、审计类型和各适配器测试。正式 UI 在后端能力完成前不得展示可用的独立注入按钮。

## 6. 目标信息架构

### 6.1 桌面端

#### 左侧：工作区与会话入口

- 新建工作区。
- 全局历史。
- 提示词库。
- 按状态组织的工作区/会话列表：运行中、待确认、完成。
- 搜索、筛选、收藏和归档继续复用现有能力。

第一阶段可以继续使用当前会话列表数据，不要求立即实现真实 git workspace 聚合。列表项身份必须使用 `(platform, sessionKey)`，不能只使用 `sessionKey`。

#### 顶部：会话页签

- 一个页签代表一个会话。
- 页签可跨平台存在。
- 关闭页签不删除、归档或终止会话。
- 运行状态、未读/待确认状态以图标和文字共同表达。
- 页签溢出时使用滚动或菜单，不无限压缩标题。

#### 中间：结构化时间线

- 用户消息、助手回复、thinking、工具调用采用统一时间线。
- thinking 和工具调用可折叠。
- 修改过的消息显示琥珀色 revision 标记，并提供“查看修改”入口。
- 消息操作包括编辑、擦除、复制和在 Memory 面板中定位。
- 不依赖 hover 才能完成关键操作；键盘和触控设备需要可见替代入口。

#### 右侧：Inspector

- `Changes`：当前工作区文件变化。
- `Files`：当前工作区文件树或相关文件。
- `Memory`：当前会话的修改历史、Diff、定位和恢复。

Inspector 的打开状态按页签保存。第一阶段若 Changes/Files 数据尚未统一，可以先交付 Memory，并保留另外两个入口为后续阶段，不伪造数据。

#### 底部：原始终端抽屉

- 使用现有 PTY manager 和 xterm 能力。
- 默认收起，不抢占结构化会话空间。
- 展开后显示当前页签关联的 resume/fork 终端。
- 终端生命周期独立于 React 页签；关闭会话页签不自动杀死进程。
- 终端无法可靠结构化时，原始输出仍完整可用。

### 6.2 移动端

#### 会话总览

- 展示主机在线状态和会话分组。
- 底部主导航保留：会话、终端、提示词、设置。
- 终端是独立入口，不放入 Memory 审计页。

#### 会话主界面

- 顶部显示项目/主机和会话切换器。
- 结构化时间线与桌面使用同一 DTO。
- 历史消息只显示一个明确的编辑入口，避免操作按钮拥挤。
- 编辑使用底部抽屉，操作目标不小于 44x44 CSS px。

#### Memory 审计

- 独立页面展示当前会话全部修改。
- 支持前后 Diff、定位原消息、再次编辑和恢复。
- 不包含 Files 或 Terminal 页签。
- 返回行为回到原会话，并恢复之前滚动位置。

## 7. 状态模型

建议先在现有 `DesktopProvider` 中渐进引入工作区状态，保留兼容 selector；新界面稳定后再考虑提取独立 `WorkspaceProvider`。不要在第一阶段同时进行 Provider 拆分和 UI 重构。

```ts
type WorkspaceTab = {
  id: string
  kind: "session"
  platform: string
  sessionKey: string
  title: string
  status: "running" | "attention" | "idle" | "done"
  openedAt: number
  lastActiveAt: number
}

type SessionTabViewState = {
  detail: SessionDetail | null
  editLog: EditLogEntry[]
  loading: boolean
  error: string | null
  scrollOffset: number
  composerDraft: string
  inspector: "changes" | "files" | "memory" | null
  terminalId: string | null
}

type WorkspaceState = {
  openTabs: WorkspaceTab[]
  activeTabId: string | null
  viewByTabId: Record<string, SessionTabViewState>
}
```

约束：

- 页签 ID 使用随机稳定 ID；平台和 `sessionKey` 单独存储，避免路径字符导致拼接冲突。
- 同一 `(platform, sessionKey)` 默认只打开一个页签，重复打开时激活已有页签。
- 详情和 edit log 只在内存缓存，不写入 `localStorage`。
- `localStorage` 只保存页签元数据、激活页签、草稿和 Inspector 偏好。
- 恢复应用时重新请求权威详情，不使用旧缓存冒充最新 revision。
- 最多保留 12 个打开页签；超过时提示关闭旧页签。
- 最多同时挂载 3 个重型会话视图，其余页签保留状态但卸载 DOM，避免虚拟列表和 xterm 占用过高。

## 8. 数据流

### 8.1 打开会话

1. 用户从历史、搜索结果或侧栏点击会话。
2. 以 `(platform, sessionKey)` 查找已有页签。
3. 已存在则激活；不存在则创建页签和空视图状态。
4. 请求 `getSessionDetail`。
5. 将结果写入对应 `tabId`，不能覆盖当前其他页签。
6. 恢复该页签草稿、滚动位置和 Inspector 状态。

### 8.2 编辑或擦除

1. 用户从消息内操作打开编辑弹窗/底部抽屉。
2. 请求携带当前页签 `detail.revision`。
3. 后端执行平台适配器写入和审计记录。
4. 成功后并行刷新该页签的 detail 和 edit log。
5. 仅更新目标页签，保持其他页签状态不变。
6. revision 冲突时重新加载权威详情，展示“本地待保存内容”和“主机当前内容”，由用户再次确认。

### 8.3 恢复修改

1. Memory Inspector 选择一条 edit log。
2. 显示恢复确认和目标消息信息。
3. 使用当前 revision 调用 restore。
4. 刷新目标页签详情和审计记录。
5. 将时间线定位到恢复后的消息，并提供短暂成功状态。

### 8.4 终端

1. 页签只保存关联 `terminalId`，不拥有进程生命周期。
2. Desktop/Remote terminal provider 继续拥有 PTY 记录。
3. 展开抽屉时按 ID 连接；不存在时由后端根据 SessionDetail.commands 启动 resume/fork。
4. 页面切换、刷新或页签关闭不隐式停止进程。

## 9. 平台适配策略

`PlatformAdapter` 继续负责：

- 会话发现和摘要。
- 会话详情标准化。
- thinking/tool call 标准化。
- 可编辑目标定位。
- 安全写回和原内容返回。
- 搜索和必要的执行输出解析。

UI 不允许出现 `if (platform === "claude")` 一类消息格式分支。平台差异只用于品牌信息或明确的 capability 展示。

第一批回归平台：

| 平台 | 历史读取 | 文本编辑 | 工具调用 | Resume | 重点风险 |
| --- | --- | --- | --- | --- | --- |
| Claude Code | 已有 | 已有 | 已有 | 已有 | 多内容块 edit target |
| Codex | 已有 | 已有 | 已有 | 已有 | mirrored event/message 同步 |
| Pi | 已有 | 已有 | 已有 | 已有 | parentId 分支和非文本块 |
| Grok | 已有 | 已有 | 已有 | 已有 | 内容形态和 JSONL 行定位 |

建议后续为 `SessionDetail` 增加可选 capability 描述，而不是让前端从平台名称推断：

```ts
type SessionCapabilities = {
  edit: boolean
  erase: boolean
  restore: boolean
  insertBefore: boolean
  insertAfter: boolean
  resume: boolean
  fork: boolean
  rawTerminal: boolean
  liveStructuredEvents: boolean
}
```

该字段应保持远程协议 v1 的 append-only 兼容性。

## 10. 实施阶段

### Phase 0：状态与交互契约

目标：在不改变现有 UI 的情况下定义页签 reducer 和测试。

任务：

- 新增 WorkspaceTab/SessionTabViewState 类型。
- 实现 open、activate、close、update-detail、update-edit-log、restore-view-state reducer。
- 添加兼容 selector，让当前 SessionDetail 仍可读取 active tab。
- 定义本地持久化 schema 和版本号。

验收：

- 同一会话不会重复打开。
- 关闭激活页签后选择最近使用页签。
- 异步响应只能更新发起请求的 tabId。
- 旧存储数据损坏时可以安全回到空工作区。

预估：1-2 个工程日。

### Phase 1：桌面多会话工作区

目标：交付可用的侧栏、页签和结构化会话主视图。

任务：

- 调整 Shell/PlatformPage 组合关系。
- 新增页签条和溢出处理。
- 复用 SessionList 和 SessionDetail，不重写消息渲染器。
- 保存每个页签的滚动位置和搜索状态。
- 支持从全局历史打开为普通页签。

验收：

- 至少三个跨平台会话可同时打开和切换。
- 切换页签不会丢失草稿、滚动位置和搜索状态。
- 当前编辑、导出、收藏、归档功能无回归。
- 1280x800 和 1440x900 无横向溢出。

预估：3-4 个工程日。

### Phase 2：Memory Inspector

目标：把历史修改能力升级为工作区一级体验。

任务：

- 将 EditLogPanel 改造为 Inspector 内的 Memory panel。
- 为可编辑消息加入稳定的操作入口和 revision 标记。
- 支持定位、再次编辑、擦除和恢复。
- 完成 revision 冲突对话框。
- 不实现真正的前插/后插，直到后端契约完成。

验收：

- 修改成功后仅刷新目标页签。
- Diff 与时间线消息能互相定位。
- 恢复操作产生新的审计记录。
- 冲突不会覆盖另一客户端的新内容。

预估：2-3 个工程日。

### Phase 3：桌面原始终端抽屉

目标：保留完整 CLI 能力，同时让结构化会话保持主导。

任务：

- 将现有 xterm 视图组合进底部抽屉。
- 复用 TerminalProvider 生命周期。
- 建立页签与 terminalId 的关联。
- 支持展开、收起、重连、resize 和显式停止。

验收：

- 抽屉收起不停止进程。
- 切换页签不串终端输出。
- 完成的 PTY 输出仍可查看。
- 不允许远程提交任意 host command。

预估：2-3 个工程日。

### Phase 4：移动端体验

目标：将现有 remote-web 组合成原型所示的移动伴侣。

任务：

- 会话总览和主机状态。
- 会话切换器和结构化时间线。
- 编辑历史记忆底部抽屉。
- 独立 Memory 审计页。
- 保留现有独立终端导航。
- 处理安全区、软键盘和返回行为。

验收：

- 390x844 无横向溢出。
- 所有主要触控目标不小于 44x44 CSS px。
- 编辑、擦除、恢复遵守 remote capabilities 和 revision。
- Memory 页面不出现重复的终端入口。
- 页面刷新后能重新连接主机拥有的运行中终端。

预估：3-4 个工程日。

### Phase 5：实时结构化刷新

目标：减少运行中 CLI 的结构化会话延迟。

任务：

- 先实现活动页签的低频权威快照刷新。
- 评估文件变更通知或 SSE 事件，只发送 invalidation，不发送权威内容。
- 平台会话文件更新后刷新对应页签。
- 保留原始 PTY 作为无法解析事件的兜底。

验收：

- 运行中会话能在合理延迟内出现新结构化消息。
- 事件丢失后下一次快照仍能恢复正确状态。
- 后台页签不会持续高频解析大文件。

预估：3-5 个工程日，需在 Phase 1 后重新校准。

## 11. 文件组织建议

第一阶段建议新增：

```text
src/features/workspace/
├── types.ts
├── reducer.ts
├── persistence.ts
├── session-tab-strip.tsx
├── workspace-sidebar.tsx
├── workspace-inspector.tsx
└── terminal-drawer.tsx
```

重点修改：

```text
src/features/desktop/types.ts
src/features/desktop/provider.tsx
src/app/provider.tsx
src/app/routes/platform.tsx
src/components/layout/shell-layout.tsx
src/features/session/session-list.tsx
src/features/session/session-detail.tsx
src/features/session/edit-message-dialog.tsx
src/features/session/edit-log-panel.tsx
src/features/terminal/terminal-context.tsx
src/features/terminal/remote-terminal-context.tsx
```

Rust 第一阶段原则上只增加 capability DTO 或测试，不修改各平台文件格式写入逻辑。

## 12. 测试策略

### 前端单元测试

- 页签 reducer 的打开、激活、关闭和 LRU 行为。
- 异步请求响应归属正确 tabId。
- 本地持久化版本升级和损坏恢复。
- capability 对操作按钮的显示控制。

### 前端集成测试

- 多页签切换保持草稿和滚动位置。
- 编辑后目标页签详情和审计同步刷新。
- revision 冲突重新加载并保留用户待保存内容。
- 终端抽屉切换不串 session/terminalId。

### Rust 回归测试

- Claude/Codex/Pi/Grok 的 list/detail/edit。
- 编辑只修改目标字段并返回正确旧内容。
- Codex mirrored records 保持一致。
- 空内容擦除和 restore 仍进入审计日志。
- capability 与实际命令/可编辑状态一致。

### 视觉与端到端测试

- Desktop：1280x800、1440x900、1920x1080。
- Mobile：375x812、390x844、430x932。
- 不出现横向滚动、按钮文字溢出和固定区域遮挡。
- 键盘导航、focus ring、Escape 关闭对话框。
- `prefers-reduced-motion` 下不依赖动画传达状态。
- Playwright 控制台无 error。

## 13. 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 运行中 PTY 与会话文件不同步 | 结构化时间线延迟 | 原始终端兜底，快照轮询，后续 invalidation 事件 |
| 页签状态进入全局 reducer 后过大 | 渲染和维护成本上升 | 以 tabId 定向更新，最多挂载 3 个重型视图 |
| 异步请求写错活动页签 | 显示错误会话内容 | 请求捕获 tabId，不使用返回时的 activeTabId |
| 多端同时修改 | 历史内容被覆盖 | 强制 expectedRevision，冲突后重新加载 |
| 各平台格式升级 | 解析或写回失效 | 适配器 fixture 和契约回归测试 |
| UI 暗示不存在的能力 | 用户误操作 | capability 驱动按钮；真正注入完成前不展示 |
| 终端进程与页签生命周期耦合 | 误杀运行任务 | PTY provider 持有生命周期，页签只保存引用 |
| 移动页面塞入过多桌面工具 | 导航混乱 | Memory、Terminal 分开，移动端一次只显示一个主任务 |

## 14. 发布与回退

- Phase 0-2 可先通过开发设置或内部路由启用 Workspace v2。
- 新旧 UI 必须共用同一 API，禁止维护两套写入逻辑。
- 工作区本地持久化带 schema version，可单独清空，不影响 session 文件和 SQLite 审计。
- 任一阶段出现问题时，可以回退 UI 路由；历史会话文件、终端记录和审计数据不做迁移性破坏。
- Workspace v2 稳定后删除旧单选状态兼容层，避免长期双状态源。

## 15. 粗略工作量

在不加入真正的前插/后插和完整实时事件协议的前提下，单人连续开发的粗略范围为 11-16 个工程日：

| 范围 | 预估 |
| --- | --- |
| 状态模型和桌面多页签 | 4-6 天 |
| Memory Inspector | 2-3 天 |
| 桌面终端抽屉 | 2-3 天 |
| 移动端体验 | 3-4 天 |

实时结构化刷新预计额外 3-5 天，并且需要根据各 CLI 实际落盘时机重新评估。真正的历史消息前插/后插属于单独项目，必须逐个平台验证安全写回。

## 16. 第一实施批次

建议下一步只启动 Phase 0 和 Phase 1：

1. 为工作区页签 reducer 写测试。
2. 将当前单一 `selectedSessionKey` 映射为 active tab selector。
3. 做出桌面页签条，但暂时保持 SessionDetail/EditMessageDialog 行为不变。
4. 验证 Claude、Codex、Pi、Grok 四个平台能在不同页签之间切换。
5. Phase 1 验收后，再开始移动 EditLogPanel 和终端，控制单次改动范围。

这一路径能最早验证最核心、风险最高的状态模型，同时不阻塞现有历史编辑和远程伴侣功能。
