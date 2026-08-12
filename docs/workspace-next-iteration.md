# Memory Forge 下一轮人工验收与重构记录

记录日期：2026-07-21  
仓库：`F:\workspacevk\109-0712\memory-forge-rs`  
分支：`feature/remote-foundation`

## 当前结论

Goal 1-6 的代码实现和自动化验收已经完成，但 2026-07-21 的首次人工验收发现：

1. 手机目前无法完成远程会话闭环。
2. 当前会话页面的前端技术组织和 UI 方向可能需要调整。
3. 本轮先记录，不立即继续修改；下一次先复现、定方案，再实施。

因此，当前状态应理解为“工程基线完成，产品人工验收仍有后续”，不能把手机远程会话视为已经验收通过。

## 2026-07-22 无手机条件下的主机侧收尾

用户当前没有可用手机，真实设备验收明确延期，不阻塞主机侧工作。本轮已完成：

1. 远程终端命令改为按 `platform + sessionId + commandKind` 生成规范命令；包含 shell 元字符、换行或伪装成 CLI 参数的 session ID 不再产生终端能力。远程启动还会重新生成并核对批准命令，不直接信任快照中的命令字符串。
2. LAN 地址发现不再只依赖单次外网路由探测。服务会枚举处于运行状态的私网接口，以默认路由地址优先，保留 Wi-Fi、以太网和虚拟网卡等可选地址；桌面设置页可切换地址后再生成二维码或复制链接。
3. 新增 `npm run test:remote` 主机侧只读冒烟测试，覆盖 `/health`、安全响应头、公开 bootstrap、未授权 `401` 和 Bearer 授权 dashboard；token 只从 `MEMORY_FORGE_REMOTE_TOKEN` 环境变量读取且不会打印。
4. 当前桌面进程监听 `0.0.0.0:7331`，本机 loopback 和首选 Wi-Fi 地址均可访问。当前仍保持远程编辑关闭、远程终端关闭，后续真机验收时再由用户显式开启。

仍未完成且无法由主机模拟替代：手机与电脑同网段访问、Windows 防火墙入站链路、手机浏览器 fragment token 落库，以及真实触控下的阅读/编辑/恢复/Terminal 重连闭环。

## 已观察现象

### 手机远程会话

- 用户在真实设备验收时无法使用手机进行远程会话。
- 本次启动时，桌面进程和本机 Remote 健康检查正常：`http://127.0.0.1:7331/health` 返回 200。
- `127.0.0.1` 只证明服务在电脑本机可用，不证明手机能够通过局域网访问。
- 尚未确认问题发生在 LAN 绑定、Windows 防火墙、手机访问地址、Bearer token、Host 校验、能力设置、路由加载还是会话/终端操作阶段。
- 在没有真实设备请求、桌面日志和浏览器错误信息之前，不预设根因。

### 会话技术栈与 UI

- 当前实现以现有 React、Tauri、Rust platform adapter 和 remote protocol v1 为基础。
- 桌面端采用多页签工作区、Memory Inspector 和原始终端抽屉。
- 移动端采用浏览器 companion、结构化会话页、独立 Memory 与 Terminal 底部导航。
- 用户可能重新调整会话区域的技术组织和 UI；下一轮不应直接在现有组件上继续堆功能。
- Paseo、OpenCode 等项目只作为交互和视觉参考，不复制其受许可证约束的代码。

## 下次优先级

### P0：完成真实手机验收（当前唯一设备阻塞项）

使用一台真实手机和运行中的桌面应用完成以下链路记录：

1. 确认 Remote 设置是 `lan`，记录端口、电脑局域网 IP、手机 URL 和 capability 开关。
2. 确认服务实际监听 `0.0.0.0:<port>` 或对应 LAN 地址，而不是只监听 `127.0.0.1`。
3. 在电脑浏览器和手机浏览器分别请求 `/health` 与 `/api/v1/bootstrap`。
4. 检查 Windows 防火墙入站规则和当前网络配置文件（专用/公用）。
5. 记录手机浏览器的 HTTP 状态、页面提示、console/network 错误，以及 Rust remote-server 日志。
6. 验证 fragment token 是否被写入 localStorage，受保护请求是否携带 Bearer token。
7. 区分“页面打不开”“能打开但无会话”“会话能读但不能编辑”“Terminal 无法启动/重连”四类故障。
8. 修复后用真实手机验证阅读、编辑、复原和 Terminal 重连闭环，不只使用 Playwright mock。

### P1：重新确认会话技术方案

在修改 UI 前先形成一个短 ADR，至少回答：

1. 桌面和移动端是否继续共享同一套 React 会话组件。
2. 会话数据是否仍以权威 `SessionDetail` 快照为核心。
3. 是否保留 8 秒活动页签轮询，还是增加文件 invalidation/SSE 刷新提示。
4. 是否继续使用 TanStack Virtual 管理长会话时间线。
5. Memory、结构化会话和原始 Terminal 的信息架构是否保持分离。
6. 移动端继续采用 Web companion，还是评估独立原生/跨平台客户端。
7. 哪些现有组件可以保留，哪些需要替换；避免在决定技术方向前大规模重写。

### P2：重新画会话 UI 原型

原型至少覆盖：

- 桌面会话列表、多页签、主时间线、Memory、Terminal 的层级关系。
- 手机会话总览、会话切换、消息操作、编辑抽屉、Memory 与 Terminal。
- 加载、离线、鉴权失败、只读、revision conflict、能力不支持和终端断线状态。
- 375x812、390x844、430x932、1280x800 和 1440x900。

原型确认前不进入大规模实现。

## 必须保留的安全与数据边界

- SessionDetail 快照和 revision 继续是权威数据。
- 每次远程 mutation 继续携带 `deviceId`、`mutationId` 和 `expectedRevision`。
- 远程 Terminal 只能启动主机根据 session 解析出的 `resume/fork` 命令。
- 不开放任意 shell，不把 Bearer token 放进 URL query。
- 不破坏平台 JSONL/SQLite 原子写入、审计和回滚语义。
- 不把“编辑现有消息”包装成“插入历史消息”。
- Goal 7 的真正历史消息前插/后插仍保持可选，不能和本轮连接问题混在一起实现。

## 当前自动化基线

- Rust 全量测试：83/83。
- Workspace 测试：10/10。
- `npm run typecheck`：通过。
- `npm run build`：通过。
- `npm run check`：仍未通过；仓库现有 Ultracite/Biome 格式与风格基线约 1917 条诊断，本轮未做无关的全仓格式化。新增 `remote-smoke.mjs` 单文件 Biome 检查通过。
- Rust fmt 和 `git diff --check`：通过。
- `npm run test:remote`：通过健康检查、bootstrap、未授权拒绝和授权 dashboard。
- 原生 Tauri 设置页：已验证多网卡地址选择器显示首选 Wi-Fi 地址并可用于二维码/复制链接。
- Playwright：远程 Web 在 1280x720 和 390x844 下无横向溢出，console error 为 0。
- Playwright mock 已覆盖活动页签刷新、后台页签零轮询、编辑时暂停、capability 入口控制和移动端布局。
- 上述 mock 结果不能替代真实手机 LAN 验收。

主机侧手工复测（PowerShell，命令不会打印 token）：

```powershell
$tokenPath = Join-Path $env:APPDATA 'com.voidcraft.memoryforge\remote-access-token'
$env:MEMORY_FORGE_REMOTE_TOKEN = (Get-Content -Raw -LiteralPath $tokenPath).Trim()
npm run test:remote -- http://127.0.0.1:7331
Remove-Item Env:MEMORY_FORGE_REMOTE_TOKEN
```

## 下次可复制提示词

```text
请继续 F:\workspacevk\109-0712\memory-forge-rs 的人工验收后续。

先阅读：
- docs/workspace-next-iteration.md
- docs/workspace-redesign-plan.md
- docs/remote-protocol-v1.md
- docs/workspace-goal-prompts.md

本轮先不要直接重写会话 UI，也不要开启可选 Goal 7。

第一阶段：使用真实手机复现“无法远程会话”，记录 Remote 设置、监听地址、Windows 防火墙、手机访问 URL、bootstrap/auth 请求、浏览器错误和 Rust 日志，明确故障属于页面访问、鉴权、会话快照、mutation 还是 Terminal。

第二阶段：基于复现结果提出最小修复并完成电脑加真实手机验收。

第三阶段：和我确认会话技术栈及 UI 方向，先写短 ADR 和桌面/移动端原型，再决定是否重构现有 React 会话组件。

保持 remote protocol v1、revision、Bearer auth、deviceId、expectedRevision、审计写入和 host-derived resume/fork 安全边界；不新增任意 shell，不实现历史消息插入，不复制 AGPL 代码。
```

## 下一轮完成标准

- 真实手机能够稳定访问 Remote 服务并明确显示连接/鉴权状态。
- 至少完成“打开会话 -> 阅读 -> 根据 capability 修改 -> 刷新后仍正确”的真实设备闭环。
- Terminal 若启用，可以启动或重连主机拥有的会话终端；若禁用，UI 给出准确状态。
- 会话技术栈有书面 ADR，桌面和移动 UI 原型得到用户确认。
- 自动化测试与真实设备验收结果分别记录，不再用 mock 结果替代产品验收。
