# Memory Forge 工作区重构 Goal 执行手册

用途：把 [工作区体验重构规划](workspace-redesign-plan.md) 拆成可以逐轮开启、逐轮验收的 Goal。  
建议顺序：Goal 1 → Goal 2 → Goal 3 → Goal 4 → Goal 5 → Goal 6。  
可选扩展：Goal 7（真正的历史消息前插/后插）。

## 实施状态

> 2026-07-22 补充：Goal 1-6 的工程实现与主机侧自动化测试已完成，终端命令安全、LAN 多网卡发现和远程 HTTP 冒烟也已收尾。用户当前没有手机，真实手机 LAN/防火墙/触控闭环延期，详见 [下一轮人工验收与重构记录](workspace-next-iteration.md)。在真机验收和会话 UI 方向确认前，不开启可选 Goal 7。

- Goal 1：已完成（工作区状态基础、兼容 reducer、持久化和单元测试）。
- Goal 2：已完成（桌面工作区外壳、跨平台页签、页签级搜索/滚动状态和异步隔离）。
- Goal 3：已完成（桌面 Memory Inspector、审计 Diff、双向定位、再次编辑、恢复与 revision conflict 保护）。
- Goal 4：已完成（桌面原始终端抽屉、页签级终端关联、Provider 生命周期复用和终端工作区联动）。
- Goal 5：已完成（移动端会话总览、结构化会话页、底部抽屉编辑、独立 Memory 审计页和 Terminal 底部导航）。
- Goal 6：已完成（活动页签权威快照刷新、SessionDetail capability 声明、能力驱动 UI 和四平台契约回归）。
- 必做 Goal 已全部完成；下一轮仅剩可选 Goal 7（真正的历史消息前插/后插）。
- 最新主机侧基线：Rust 83/83、workspace 10/10、typecheck/build/remote smoke/Rust fmt/git diff check 均通过；真实手机验收仍单独待办。
- Goal 1 验收：`npm run test:workspace`、`npm run typecheck`、`npm run build` 均通过。
- Goal 2 验收：`npm run test:workspace`、`npm run typecheck`、`npm run build` 均通过；Playwright 已验证 1280x800、1440x900、1920x1080、打开/切换/关闭/刷新恢复和键盘关闭。
- Goal 3 验收：workspace 测试 9/9、Rust 测试 79/79、`npm run typecheck`、`npm run build` 均通过；Playwright 已验证 1280x800、1440x900、Diff 展开、Changes/Files 占位、Memory 双向定位和再次编辑，页面无横纵溢出。
- Goal 4 验收：workspace 测试 10/10、嵌入式终端 Rust 测试 12/12、`npm run typecheck`、`npm run build` 均通过；Playwright 已验证 1280x800、1440x900、抽屉与 Inspector 并存、失败/完成输出、页签关闭保活和重新打开页签关联，页面无横向溢出。
- Goal 5 验收：workspace 测试 10/10、远程服务端 Rust 测试 8/8、`npm run typecheck`、`npm run build` 均通过；Playwright 已验证 375x812、390x844、430x932 和 844x390，无横向溢出、console error 或小于 44px 的主要触控目标；另已验证会话切换、Memory 双向定位、底部抽屉在 430x600 可见、滚动位置恢复、只读 capability 隐藏 mutation 操作，以及运行中远程终端在刷新后重连。
- Goal 6 验收：平台适配器 Rust 测试 32/32、远程服务端 Rust 测试 8/8、workspace 测试 10/10、`npm run typecheck`、`npm run build` 均通过；Playwright 已验证 8 秒刷新只请求活动页签、切换后后台页签请求为 0、编辑抽屉打开时暂停刷新、权威 revision 更新后结构化消息从 12 条更新为 13 条，以及 edit/erase/restore/rawTerminal capability 独立控制入口，console error 为 0。

## 使用规则

每一轮只开启一个 Goal。当前 Goal 完成并关闭后，再开启下一轮。不要把所有提示词一次性发给同一个 Goal。

每一轮开始前：

1. 确认上一轮已经完成并通过验收。
2. 保留现有用户改动，不要使用破坏性 git 命令。
3. 先阅读本手册对应轮次和总规划文档。
4. 在 Goal 结束时更新文档中的完成状态，汇报测试结果和未解决风险。

这些 Goal 默认都在仓库 `F:\workspacevk\109-0712\memory-forge-rs`、分支 `feature/remote-foundation` 上执行。每轮都应保持现有 Rust 平台适配器和远程协议兼容，除非该轮明确要求修改。

## 轮次总览

| Goal | 主题 | 主要范围 | 不包含 |
| --- | --- | --- | --- |
| 1 | 工作区状态基础 | 页签类型、reducer、兼容 selector、持久化 | 新 UI、移动端、终端重构 |
| 2 | 桌面多会话页签 | 工作区外壳、侧栏、页签、跨平台切换 | Memory Inspector、移动端、实时流 |
| 3 | Memory Inspector | 消息操作、审计、Diff、恢复、冲突 | 真正新增历史消息 |
| 4 | 桌面终端抽屉 | PTY 抽屉、页签关联、重连、生命周期 | 结构化实时解析 |
| 5 | 移动端伴侣 | 会话总览、会话页、移动 Memory 审计 | 原生 App、消息插入 |
| 6 | 实时刷新与能力声明 | 快照刷新、invalidation、capability、平台回归 | 完全统一的实时事件流 |
| 7 | 可选：真正插入消息 | `insertBefore/After` 后端契约和适配器 | 不应提前做 |

---

## Goal 1：工作区状态基础

### 目标

建立多会话工作区的状态模型，但暂时不改变主要视觉布局。当前单一的 `selectedSessionKey/sessionDetail/editLog` 继续通过兼容 selector 工作，确保旧 UI 不回归。

### 可复制提示词

```text
请在 F:\workspacevk\109-0712\memory-forge-rs 完成 Goal 1：工作区状态基础。

先阅读：
- docs/workspace-redesign-plan.md
- src/features/desktop/types.ts
- src/features/desktop/provider.tsx
- src/app/routes/platform.tsx

目标：
1. 新增 WorkspaceTab、SessionTabViewState、WorkspaceState 类型。
2. 实现 open/activate/close/update-detail/update-edit-log/restore-view-state reducer 操作。
3. 同一 (platform, sessionKey) 默认只打开一个页签。
4. 保留 selectedSessionKey、sessionDetail、editLog 的兼容 selector，旧 SessionList、SessionDetail、EditMessageDialog 不需要大规模重写。
5. 为页签元数据、激活页签、草稿和 Inspector 偏好设计带 schema version 的本地持久化；不要把会话正文和审计正文写入 localStorage。
6. 页签关闭不删除会话、不归档会话、不停止终端。

约束：
- 不修改各平台 JSONL/SQLite 写回格式。
- 不做移动端 UI。
- 不做终端生命周期重构。
- 不做真正的历史消息插入。
- 不使用 git reset --hard、git checkout -- 等破坏性命令。
- 继续保留用户现有改动。

验收标准：
- reducer 单元测试覆盖打开、重复打开、激活、关闭和异步响应按 tabId 定向更新。
- 旧单会话 UI 行为保持正常。
- 持久化损坏或版本未知时可安全回到空工作区。
- npm run typecheck 通过。
- 汇报修改文件、测试命令、剩余风险，不要提前实现 Goal 2。
```

### 完成标志

- 有独立的工作区状态类型和 reducer。
- 至少一个测试验证异步结果不会写入错误页签。
- 旧页面仍能正常打开、编辑和恢复会话。

---

## Goal 2：桌面多会话工作区

### 目标

在 Goal 1 状态基础上实现桌面工作区外壳和多会话页签。复用现有 `SessionList`、`SessionDetail` 和编辑对话框，不在这一轮搬动 Memory 审计和终端。

### 可复制提示词

```text
请在 F:\workspacevk\109-0712\memory-forge-rs 完成 Goal 2：桌面多会话工作区。

前置条件：Goal 1 已完成并通过验收。先阅读：
- docs/workspace-redesign-plan.md
- docs/workspace-goal-prompts.md
- Goal 1 的实际改动和测试结果
- src/app/routes/platform.tsx
- src/components/layout/shell-layout.tsx
- src/features/session/session-list.tsx
- src/features/session/session-detail.tsx

目标：
1. 增加桌面工作区外壳：左侧全局入口和会话列表，中间页签条，主区域显示活动会话。
2. 支持至少 3 个跨平台会话同时打开、切换和关闭。
3. 从全局历史、侧栏、搜索结果打开会话时，创建或激活普通工作区页签。
4. 页签标题、平台、运行状态和待确认状态使用统一数据源。
5. 切换页签时保留对应的草稿、搜索状态和滚动位置。
6. 关闭页签不得删除会话、归档会话或停止终端。
7. 保留现有编辑、擦除、恢复、导出、收藏、归档和远程会话行为。

约束：
- 不在这一轮重构 EditLogPanel 为 Inspector。
- 不在这一轮实现桌面终端抽屉。
- 不做移动端布局。
- 不为不同平台复制一套页面组件。
- UI 使用现有主题、Lucide 图标、国际化和可访问性模式。

验收标准：
- 3 个不同 platform/sessionKey 的页签可以独立切换。
- 异步加载不会把 A 页签的数据写进 B 页签。
- 1280x800、1440x900、1920x1080 无横向溢出。
- 键盘可以聚焦和关闭页签，关闭按钮有 aria-label。
- npm run typecheck 和 npm run build 通过。
- 用 Playwright 验证打开、切换、关闭页签和现有编辑入口。
```

### 完成标志

- 桌面端已经从单选会话变为多页签工作区。
- 旧功能没有被迁移过程破坏。
- 页签状态和会话身份不会混淆。

---

## Goal 3：Memory Inspector

### 目标

把现有 EditLogPanel 变成右侧 Memory Inspector，并在消息附近提供编辑、擦除、复制和定位操作。这个 Goal 只使用当前已有的编辑、擦除和恢复 API。

### 可复制提示词

```text
请在 F:\workspacevk\109-0712\memory-forge-rs 完成 Goal 3：Memory Inspector。

前置条件：Goal 1 和 Goal 2 已完成。先阅读：
- docs/workspace-redesign-plan.md 的第 5.3、6.1、8.2、8.3 节
- src/features/session/edit-log-panel.tsx
- src/features/session/edit-message-dialog.tsx
- src/features/session/session-detail.tsx
- src/features/desktop/api.ts
- src-tauri/src/session_service.rs
- src-tauri/src/remote_server.rs

目标：
1. 在桌面工作区右侧增加 Changes、Files、Memory Inspector，其中 Memory 是本轮重点。
2. 将现有 edit log 按当前活动页签加载和显示。
3. 可编辑消息显示 revision 标记和消息内操作：编辑、擦除、复制、定位。
4. 编辑成功后只刷新目标页签的 SessionDetail 和 edit log。
5. 支持 before/after Diff、再次编辑、定位原消息和恢复旧版本。
6. 处理 session revision conflict：重新加载权威详情，展示冲突，不静默覆盖。
7. 移动端本轮不实现；可以保留现有 remote-web 行为。

明确限制：
- 不要实现真正的 insertBefore/insertAfter。
- 当前“注入”如果只是编辑现有消息，可以在文案中说明；不要展示一个会执行但没有后端契约的新增消息按钮。
- 不删除 edit_log，不改变 SQLite 审计语义。
- 不把会话正文缓存到 localStorage。

验收标准：
- 编辑、擦除、恢复均生成或保留正确审计记录。
- 恢复操作本身也会被记录。
- 冲突时原内容和用户待保存内容都不会丢失。
- Memory Inspector 与消息时间线可以互相定位。
- 桌面宽度 1280px 时操作按钮和 Diff 不溢出。
- 现有 Rust session_service 测试和前端 typecheck/build 通过。
```

### 完成标志

- 用户可以在消息处开始编辑，也可以在右侧 Memory 中审计和恢复。
- 真实后端能力与 UI 文案一致，没有假注入功能。

---

## Goal 4：桌面原始终端抽屉

### 目标

把现有桌面终端接入工作区底部抽屉。终端是独立 PTY 视图，不替代结构化会话，也不与 Memory Inspector 混合。

### 可复制提示词

```text
请在 F:\workspacevk\109-0712\memory-forge-rs 完成 Goal 4：桌面原始终端抽屉。

前置条件：Goal 1、Goal 2、Goal 3 已完成。先阅读：
- docs/workspace-redesign-plan.md 的第 6.1、8.4 节
- docs/embedded-terminal-implementation-plan.md
- src/features/terminal/terminal-context.tsx
- src/features/terminal/remote-terminal-context.tsx
- src/app/routes/terminal-sessions.tsx
- src/features/session/session-detail.tsx

目标：
1. 在桌面活动会话底部增加可展开/收起的原始终端抽屉。
2. 复用现有 xterm、TerminalProvider 和 PTY 生命周期，不新建第二套终端管理器。
3. 每个工作区页签只保存关联 terminalId，进程生命周期仍由 terminal provider/后端拥有。
4. 支持展开、收起、resize、重连、显式停止和完成状态查看。
5. 页签切换时不得串流终端输出；关闭页签不得隐式杀死终端。
6. 远程终端继续只接受后端解析出的 resume/fork 命令，不接受任意 shell 命令。

约束：
- 不做实时结构化消息解析。
- 不把终端放进 Memory Inspector。
- 不修改远程协议 v1 的安全边界。

验收标准：
- 抽屉收起、切换页签、刷新页面后，运行中的终端仍可按既有规则重连。
- 完成的 PTY 输出仍可查看。
- 终端 resize 和停止行为没有回归。
- Desktop 和 remote-web 两种运行时的不可用/只读状态有明确反馈。
- typecheck/build 和现有终端测试通过。
```

### 完成标志

- 结构化会话是主视图，原始终端成为可靠兼容抽屉。
- 不再需要在 Memory 页面放一个重复的“终端”入口。

### 实施记录与边界

- 桌面端收起抽屉、切换工作区页签、关闭并重新打开会话页签时，终端继续由 `TerminalProvider` 持有；关闭页签不会停止 PTY。
- 工作区持久化不保存 `terminalId` 或抽屉展开状态。重新打开会话页签时会关联 Provider 中同平台、同会话的最新终端，抽屉默认保持收起。
- remote-web 继续通过既有终端列表和输出接口支持浏览器刷新后的重连。
- 桌面本地终端当前不承诺 WebView 整页重载或应用重启后的重连：本地 Provider 状态在内存中，Rust PTY 管理器也没有终端枚举、快照和输出回放 IPC。补齐该能力需要单独扩展本地终端契约，不能只靠持久化 `terminalId` 伪造恢复。

---

## Goal 5：移动端伴侣

### 目标

将现有 LAN remote-web 体验调整为原型中的移动端伴侣：会话总览、单个会话、移动 Memory 审计。终端继续位于独立底部导航。

### 可复制提示词

```text
请在 F:\workspacevk\109-0712\memory-forge-rs 完成 Goal 5：移动端伴侣。

前置条件：Goal 1-4 已完成。先阅读：
- docs/workspace-redesign-plan.md 的第 6.2、8 节
- docs/remote-protocol-v1.md
- src/app/routes/platform.tsx
- src/app/routes/terminal-sessions.tsx
- src/features/session/session-detail.tsx
- src/features/remote/protocol.ts
- .protos/memory-forge-mobile-companion.html

目标：
1. 移动端首页展示主机状态、会话分组、运行状态和底部导航。
2. 会话页使用结构化时间线和顶部会话切换器。
3. 历史消息附近只保留一个清晰的编辑入口，详细编辑使用底部抽屉。
4. 增加独立 Memory 审计页：Diff、定位、再次编辑和恢复。
5. 终端保留为独立底部导航，不放入 Memory 审计页。
6. 遵守 remote capabilities、Bearer auth、deviceId 和 expectedRevision。
7. 处理安全区、返回、软键盘、刷新后恢复和运行中终端重连。

约束：
- 第一阶段仍是浏览器 Web companion，不创建原生 iOS/Android 工程。
- 不复制 Paseo 或其他 AGPL 代码。
- 不新增任意 shell 远程执行能力。
- 不实现真正的历史消息前插/后插。

验收标准：
- 375x812、390x844、430x932 无横向溢出。
- 主要触控目标不小于 44x44 CSS px。
- 编辑、擦除、恢复和冲突处理与桌面端一致。
- 刷新手机页面后可以重新连接主机拥有的运行中终端。
- 返回会话后能恢复之前的会话和滚动位置。
- Playwright 移动端截图、console error 检查通过。
```

### 完成标志

- 移动端能完成“打开会话 → 阅读 → 修改 Memory → 恢复/继续终端”的闭环。
- Memory 和 Terminal 的导航职责清晰，不再出现图 3 那种重复终端入口。

### 实施记录与边界

- remote-web 根路由现在是跨平台会话总览；桌面本地工作区入口保持不变。
- 移动端复用现有工作区页签作为会话切换状态，返回总览或 Memory 后可恢复活动会话及滚动位置。
- Memory 使用独立 `/memory` 路由，支持 Diff、定位、再次编辑和带 `expectedRevision` 的恢复；Terminal 仍是独立底部导航。
- 编辑器在移动端使用底部抽屉；只读 remote capability 会隐藏编辑、擦除、再次编辑和恢复操作。
- 本轮没有修改 remote protocol、Rust mutation API、Bearer auth、`deviceId` 或终端所有权边界，也没有新增任意 shell 和历史消息插入能力。

---

## Goal 6：实时刷新与能力声明

### 目标

在前五轮稳定后，改善运行中会话的新消息刷新，并让 UI 通过 capability 数据判断操作可用性，而不是根据平台名称猜测。

### 可复制提示词

```text
请在 F:\workspacevk\109-0712\memory-forge-rs 完成 Goal 6：实时刷新与能力声明。

前置条件：Goal 1-5 已完成。先阅读：
- docs/workspace-redesign-plan.md 的第 5.3、8、9、10 Phase 5 节
- docs/remote-protocol-v1.md
- src-tauri/src/platforms/mod.rs
- src-tauri/src/session_service.rs
- src-tauri/src/remote_server.rs
- src/features/desktop/types.ts

目标：
1. 先实现活动页签的低频权威快照刷新或文件变更 invalidation。
2. 如引入 SSE/WebSocket，只把它作为刷新提示，不能替代权威 SessionDetail snapshot。
3. 后台页签不进行高频大文件解析。
4. 为 SessionDetail 或 bootstrap 增加 append-only 的可选 capabilities：edit、erase、restore、resume、fork、rawTerminal、liveStructuredEvents 等。
5. UI 根据 capability 显示/禁用操作，不根据 platform 字符串复制条件分支。
6. 为 Claude、Codex、Pi、Grok 增加适配器契约回归测试。
7. 保留原始 PTY 作为无法结构化时的完整兜底。

约束：
- 不承诺四个 CLI 具备完全一致的实时事件。
- 不破坏 remote protocol v1 的 append-only 和 revision 约定。
- 不在本轮实现真正的 insertBefore/insertAfter。

验收标准：
- 事件丢失或服务重启后，下一次快照仍能恢复正确内容。
- 活动页签能在合理延迟内看到新的结构化消息。
- capability 与实际平台命令、可编辑字段和 remote capability 一致。
- 四个平台的 list/detail/edit fixture 测试通过。
- 活跃终端和后台页签不存在明显性能回归。
```

### 完成标志

- “实时”是可靠快照刷新，不是依赖某个平台的脆弱输出解析。
- UI 能正确处理只读、不可恢复、不可 fork 等平台差异。

### 实施记录与边界

- `SessionDetail.capabilities` 是 protocol v1 的 append-only 响应字段；旧服务端缺少该字段时，前端兼容回退到 block `editable` 和 `commands`。
- 前端有效能力由 SessionDetail 平台能力与 remote bootstrap 授权取交集，不再通过平台名称决定编辑、擦除、恢复或终端入口。
- 当前活动页签每 8 秒读取一次权威 SessionDetail；隐藏页面、后台页签和打开编辑器的会话不轮询，重新聚焦时立即检查。
- `liveStructuredEvents` 当前对所有平台均为 false，没有新增 SSE/WebSocket；事件丢失或服务重启后仍由下一次完整快照恢复。
- Claude、Codex、Pi、Grok fixture 覆盖 list/detail/edit 与 capability 契约；原始 PTY 继续作为结构化能力之外的完整兜底。
- 本轮没有改变平台文件格式、revision 算法、远程 mutation 请求、Bearer auth、`deviceId`、终端所有权或任意 shell 安全边界。

---

## Goal 7（可选）：真正的历史消息前插/后插

### 为什么单独拆出

当前平台适配器统一提供的是 `update_message(edit_target, new_content)`。它适合编辑和擦除现有消息，不等于可以安全地在任意两条历史消息之间新增一条消息。新增消息会涉及上下文树、顺序、镜像事件、分支、索引和审计语义，必须单独设计。

### 可复制提示词

```text
请在 F:\workspacevk\109-0712\memory-forge-rs 完成 Goal 7：真正的历史消息前插/后插。

前置条件：Goal 1-6 已完成，并且已经明确产品需要新增独立历史消息，而不只是编辑现有消息。

先做设计和样本分析，不要直接写入真实用户会话：
1. 为 Claude Code、Codex、Pi、Grok 分别收集最小 JSONL fixture。
2. 设计 insertBefore/insertAfter 的平台适配器契约和 edit log operation 类型。
3. 定义 revision、原子写入、失败回滚和恢复语义。
4. 分析 Codex mirrored records、Pi parentId 分支和各平台不可编辑事件。
5. 先选择一个平台完成端到端实验，再决定是否推广。
6. UI 只有在对应 capability 为 true 时才显示插入操作。

硬性约束：
- 任何写入都必须使用临时文件、原子替换和备份/回滚策略。
- 不修改真实用户会话作为测试手段。
- 不把“编辑现有消息”错误地包装成“插入新消息”。
- 若某个平台无法保证语义，返回明确的不支持，而不是静默降级。

验收标准：
- 至少一个平台有完整 fixture、写回、重新解析、审计和 restore 测试。
- 冲突和失败写入不会破坏原始会话。
- UI capability 正确隐藏其他不支持平台的插入入口。
```

## 每轮结束时的汇报格式

完成一个 Goal 时，要求输出以下信息：

```text
Goal N 已完成/未完成

已完成：
- ...

修改文件：
- ...

验证：
- 命令：...
- 结果：通过/失败

未完成与风险：
- ...

下一轮前置条件：
- ...
```

如果某轮无法完成，不要直接开启下一轮。先修复失败项，或者把未完成内容明确转入下一轮的范围。
