# Agent24

> 面向 24/7 AI agent 的 **shell-agnostic 框架**——Rust 内核 + daemon 提供统一的"个人 AI 助手"承载能力，外壳与核心解耦：自带 Electron 参考外壳，任何外壳都能挂上来（如 Tauri 的 Pet0）；支持可插拔能力模块、本地 & API 多模型适配、分层记忆、跨 agent 通信。

## 定位

**Agent24 是框架，不是应用。** 我们提供：

- **外壳无关（shell-agnostic）**：Rust 内核 + daemon 与前端解耦，壳只经 HTTP/WS 协议连接——自带 Electron 参考外壳，Tauri 外壳（如 Pet0 桌宠）等同样可挂载
- **多端**：桌面已落地（Electron 参考壳，macOS / Windows / Linux 分发）；移动（iOS / Android）与 Web 规划中，同属 shell-agnostic——移动端计划各提供 Tauri 与 Expo / React Native 瘦壳示例（见 [ADR-027](docs/decision.md)）。daemon 与模型可不在端上（如跑在你的 Mac），移动 / Web 端做瘦壳、只经 HTTP/WS 协议远程消费
- 后台 daemon + 用户交互一致性
- 标准化能力模块接口（`@auraaihq/sdk` `defineModule`）
- AI 适配层（本地 & API 多模型：本地小模型 + Claude / OpenAI / iDoris 等 API，可切换）
- 分层记忆（L0 KV → L3 ATIF 轨迹 + SkillBank）+ 自进化框架
- 通过 **Hyphae 菌丝网络**（Nostr）与其他 agent 通信

**应用方**（如小黑书、博客、社区工具等）从本框架 fork，搭载具体场景的能力模块。

> **仓库位置**：本框架仓库现位于 `iDoris-ai/Agent24`（组织由早期文档中的 `AuraAIHQ` 迁至 `iDoris-ai`；历史 rename 背景见 [ADR-015](docs/decision.md)）。

---

## 架构（Rust Core + Polyglot，见 [ADR-026](docs/ADR-026-rust-core-polyglot.md)）

内核是 Rust daemon `agent24d`——唯一核心运行时，也是桌面端默认后端（`AGENT24_BACKEND=rust`，v0.1.0 起）。所有外壳只经 **v1 REST + WebSocket** 协议接入，互不感知实现。

```
┌────────────────────────────────────────────────────────────────────┐
│  外壳（shell-agnostic，只经 v1 REST/WS 接入）                          │
│  Electron+React 桌面（默认/参考壳）· Tauri（如 Pet0）·                 │
│  移动 iOS/Android + Web（规划，瘦壳）· TUI（ratatui）· CLI             │
└───────────────────────────────┬────────────────────────────────────┘
                  HTTP REST + WebSocket（bearer token，动态端口）
┌───────────────────────────────▼────────────────────────────────────┐
│  Agent24 Core = Rust daemon  agent24d                                │
│  /api/v1: sessions · runs · events(WS) · approvals · schedules ·     │
│           models · chat · usage · tools · tool-overrides · sin90 …   │
│  crates: core · agent(Loop) · models(网关+三级路由) · scheduler ·    │
│          store · memory · policy · tools · mcp · protocol · sin90    │
└──────────┬───────────────────────────────────────┬─────────────────┘
     契约：protocol/openapi.yaml + events.schema     │ REST
     （单一来源 → packages/api-client TS SDK 自动生成，CI 校验漂移）
┌──────────▼──────────────────┐        ┌───────────▼──────────────────┐
│ TS 能力模块 / 协议参考实现    │        │ Python ML Worker（规划）       │
│ packages/node-daemon         │        │ Embedding · Whisper ·          │
│ （v1 协议 mock/参考实现，     │        │ 图像 · LoRA 训练                │
│  CapabilityModule 承载）     │        │ （agent24-ml-worker）          │
└──────────────────────────────┘        └────────────────────────────────┘
```

> **为什么不是 Node/Python 主后端**（[ADR-026](docs/ADR-026-rust-core-polyglot.md)，取代 ADR-023 的「M3 切 Python FastAPI」）：新内核能力（Agent Loop / 调度器 / 记忆 / 工作流 / 权限）从第一行起写在 Rust，不在 Node 或 Python 主后端里先写一遍。`packages/node-daemon` 保留为 v1 协议的 **mock/参考实现**（`AGENT24_BACKEND=node` 可切），保障协议演进期日常开发不阻塞；Python **仅**用于 ML Worker（不承担会话/权限/持久化/审计）。

### 核心组件

> 状态图例：✅ 已落地（有测试）· 🟡 部分 · 🔲 未建成。职责列只写已落地能力，目标态见 ADR/ROADMAP。

| 组件 | 路径 | 状态 | 职责 |
|------|------|------|------|
| **agent24d**（Rust daemon） | `rust/apps/agent24d` | ✅ | v1 REST+WS 核心运行时；桌面默认后端 |
| **agent24-cli / TUI** | `rust/apps/agent24-cli` | ✅ CLI · ✅ TUI 最小版 · 🔲 chat | Attached/Standalone；TUI（ratatui）runs/事件流/审批队列，headless 运维 |
| **agent24-core** | `rust/crates/agent24-core` | ✅ | 稳定领域模型（Session/Run/Task/ToolCall/Approval/Event/Usage…），零框架依赖 |
| **agent24-agent** | `rust/crates/agent24-agent` | ✅ | Agent Loop：上下文 → 调模型 → 解析 ToolCall → 权限 → 执行 → 续 |
| **agent24-models** | `rust/crates/agent24-models` | ✅ | Model Gateway + 三级路由（本地小模型 / 远程 API / 自训领域 LoRA；敏感任务强制本地或 LoRA，数据不出设备） |
| **agent24-scheduler** | `rust/crates/agent24-scheduler` | ✅ | cron 式日常工作流调度器 |
| **agent24-store / memory / policy** | `rust/crates/agent24-{store,memory,policy}` | ✅ | 持久化 / 分层记忆 / 权限审批 |
| **agent24-sin90 (+store)** | `rust/crates/agent24-sin90*` | ✅ | 内置 Personal-OS 领域模块（独立 `sin90.db`） |
| **api-client**（生成物） | `packages/api-client` | ✅ | openapi + events schema → TS SDK（CI 校验零漂移） |
| **node-daemon**（参考实现） | `packages/node-daemon` | ✅ | v1 协议 mock/参考；TS CapabilityModule 承载 |
| **desktop**（Electron 壳） | `apps/desktop` | ✅ | spawn agent24d + 端口/token/托盘/preload；React UI |
| **agent24-worker → Python ML Worker** | `rust/crates/agent24-worker` | ✅ 契约/客户端 · 🔲 Python 侧 | Rust 侧 wire 契约 + HTTP 客户端（embed/transcribe/health）；Python 服务 `agent24-ml-worker`（Embedding/Whisper/图像/LoRA）规划 |

### 两套扩展机制：领域 OS vs 能力模块

Agent24 有两条互不替代的扩展路径。一句话：**领域 OS 是「换主板」，能力模块是「插一张卡」。**

```
外壳（Pet0 / desktop / 微信 / Nostr / TUI / MCP client）
   │  只经 v1 REST + WebSocket，互不感知实现
   ▼
内核 agent24d（Rust，唯一核心运行时）
   core · agent(Loop) · models · scheduler · store · memory · policy · protocol · mcp
   │                                      │
   │ ① 领域 OS（M-E，重）                  │ ② 能力模块 / 插件（M2 · ADR-016，轻）
   ▼                                      ▼
DomainModule（Rust trait）             CapabilityModule（TS）
 = 这台 agent 是「什么产品」             = 这台 agent「多会一件事」
 sin90 / cos72…                        ping / summarize / codebox…
```

|  | 领域 OS（`DomainModule`） | 能力模块（`CapabilityModule`） |
|---|---|---|
| 回答的问题 | 这台 agent 是**什么产品** | 这台 agent **多会一件事** |
| 数量 | 同时只有一个 active（`active_domain_os`） | 可装多个 |
| 语言 / 宿主 | Rust，挂进 `agent24d` | TS，跑在 `packages/node-daemon` |
| 自带数据库 | ✅ 独立 DB + 独立迁移（如 `sin90.db`） | ❌ |
| 记忆分区 | ✅ 共享记忆底座里的私有分区 `(org, os:<name>)`，模块间不可互读 | ❌ |
| 路由命名空间 | `/api/v1/<name>/*`（有保留字防撞内核路由） | `/api/capabilities/<id>` |
| 事件 | `EventBody::Module{module, kind, payload}` | — |
| 隔离 | 进程内（ME-3 后可进程外）；**非沙箱** | **BoxLite 微 VM 沙箱**（Hypervisor.framework / KVM） |
| 分发 | `agent24 os enable/disable`（目录当前编译进 daemon，ME-3a 解决） | npm registry + 市场浏览 + 安装同意摘要 |
| 清单 | `domain-os.yml` → `DomainOsManifest` | [`protocol/module.schema.json`](protocol/module.schema.json) → `ModuleManifest` |

> 两者的权限词表今天是两套（`module.schema.json` 明确记着「词表统一推迟到 M-E」），这笔债未还。

**隔离是两层，不是二选一**：领域数据的隔离靠**独立 DB**（`sin90.db`，内核的 `agent24.db` 不认识这些表）；而共享**记忆底座**（M-D 的 EventLog / AssertionLedger / ArtifactStore）是所有模块共用的，那里的归属靠 **`(组织, 空间)` 所有权维度**（[ADR-030](docs/decision.md)）——模块拿到的句柄被钉死在自己的分区上，键做长度前缀编码，两个模块不可能读到对方的、也读不到用户自己的。

### 领域 OS 开发（Rust `DomainModule`，[ADR-029](docs/decision.md)）

```rust
trait DomainModule {                                  // 单向：模块用内核，内核不认识模块
    fn name(&self) -> &str;
    fn manifest(&self) -> &DomainOsManifest;          // domain-os.yml
    async fn open_store(&self, dir: &Path) -> R<()>;  // 自己的 DB + 迁移
    fn routes(&self, ctx: KernelCtx) -> axum::Router; // 自己的命名空间
    fn event_module(&self) -> &str;
}

trait KernelCtx {
    fn events(&self) -> EventSink;                     // ✅ 已授予（只能发自己 module 的事件）
    fn memory(&self, scope, grants) -> ScopedMemory;   // ✅ 已授予（只能碰自己分区）
    // models() / scheduler() / policy()               // 🔲 已声明，尚未授予：没有 handle 可给
}
```

> 内核实授能力集是 `{Events, Memory}`（`rust/apps/agent24d/src/domain.rs` 的 `KERNEL_GRANTS`）。模块可以在清单里多要，`Grants::granting` 取交集——**多要无益**。

### 能力模块开发（TS CapabilityModule，由 `node-daemon` 承载）

```ts
// 实现 CapabilityModule 接口
export const myModule: CapabilityModule = {
  id: 'my-capability',
  register(router, ctx) {
    router.get('/api/capabilities/my-capability', (req, res) => {
      // ctx.llm 可调用 LLM Gateway
      res.end(JSON.stringify({ ok: true }))
    })
  },
}
```

### LLM 运行时（可在设置页切换）

| 运行时 | 端点 | 说明 |
|--------|------|------|
| **oMLX**（默认） | `localhost:8088/v1` | Apple Silicon 原生，最低延迟 |
| Ollama | `localhost:11434` | 跨平台，模型丰富 |
| LM Studio | `localhost:1234/v1` | 图形界面管理 |
| Remote API | 自定义 | OpenAI 兼容接口 |

---

## CLI 快速开始（Rust daemon，M-B 起）

```bash
# 构建
cd rust && cargo build -p agent24d -p agent24-cli

# 常驻模式：启动 daemon（~/.agent24/daemon.json 供发现）
./target/debug/agent24 daemon start
./target/debug/agent24 daemon status     # running · pid … · backend rust
./target/debug/agent24 models            # 需本地 oMLX(8088)/Ollama(11434)
./target/debug/agent24 chat "你好"        # attached：连上已运行的 daemon
./target/debug/agent24 daemon stop

# 无 daemon 时直接 chat：自动拉起临时 daemon，用完即走
./target/debug/agent24 chat "hi"
```

端到端冒烟：`scripts/cli-smoke.sh`。Electron 壳切换 Rust 后端：`AGENT24_BACKEND=rust pnpm dev`。

## 二次开发者接口（现在就有的）

**协议层是唯一真源，CI 有零漂移门**

| 契约 | 位置 | 说明 |
|---|---|---|
| v1 REST | [`protocol/openapi.yaml`](protocol/openapi.yaml) | health · chat · models · usage · sessions · runs · approvals · schedules · tools · standing-grants · tool-overrides（+ 领域 OS 自己的命名空间） |
| WS 事件 | [`protocol/events.schema.json`](protocol/events.schema.json) | 含 `ModuleEventPayload{module, kind, payload}` —— 领域模块触达事件流的唯一一条缝 |
| 插件清单 | [`protocol/module.schema.json`](protocol/module.schema.json) | `ModuleManifest` |
| 生成物 | `packages/api-client` · `packages/contract-tests` | TS SDK（CI 校验零漂移）；契约测试任何实现都能拿去跑 |

**运维 / 集成面**

```bash
agent24 daemon start|status|stop      # 进程管理（~/.agent24/daemon.json 供发现）
agent24 service install|status        # macOS LaunchAgent：登录自启 + 自愈（24/7）
agent24 tui                           # runs / 事件流 / 审批队列
agent24 os list|enable|disable        # 领域 OS 处置
agent24 mcp                           # 把 agent24d 自己变成 MCP server —— 外部 agent
                                      # 可以把任务跑在你的 agent24 上，风险动作仍在本机审批
```

渠道：微信桥（`packages/wechat-bridge`）、Nostr 桥（`packages/nostr-bridge`，NIP-44 加密，驱动 agent-speaker 二进制，含入站活性探针）。
约定：能力模块**不直接调 LLM API**，一律经 LLM Gateway（[ADR-019](docs/decision.md)）；权限在清单声明，安装时出同意摘要。

---

## 里程碑进度

> 图例：✅ 完成 · 🟡 部分 · 🔲 未开工。权威状态源见 [`docs/specs/TASKS.md`](docs/specs/TASKS.md)。

| 线 | 内容 | 状态 |
|---|---|---|
| **M-A** 契约冻结 | openapi / events / module schema · contract-tests · api-client 生成管道 | ✅ |
| **M-B** Rust 内核 | agent24d · CLI · core / agent / models / scheduler / store / policy | ✅ |
| **M-C** 发布 | v0.1.0 → v0.2.0 → **v0.3.0**（当前） | ✅ |
| **M-H** 人机边界 | 审批门 · payload 哈希 · durable resume · plan mode · 安装同意摘要 · Fake 渠道 harness | ✅ |
| **M-D** 记忆重做 | MD-1..MD-8 全交付：权威+投影 · 真双时相 · 治理写门 · Condenser（隐藏非删除）· 巩固循环 · FTS/向量缝 | ✅（`OmlxEmbedder` 待 D4b） |
| **M-F** 渠道 | F3 微信 ✅ · F4 Nostr ✅（含入站活性探针）· F1b 托盘常驻 🔲 · **F5 7×24 泡测 🔲** | 🟡 |
| **M-E** 领域 OS | ME-1 `DomainModule`+`KernelCtx` ✅ · ME-2 配置注册表 + `os` CLI ✅ · **ME-3 进程外 Provider 🔲 设计中** · ME-4 第二个领域 OS（Cos72 骨架）🔲 · ME-5 PGL manifest 🔲 · ME-6 签名 + 信任根 🔲 | 🟡 |
| **P4** 生态 / 分发 | 模块市场后端 ✅（npm 发现 + 浏览过滤）· 跨用户分发 / 模块签名 / 跨设备记忆同步 🔲 | 🟡 |

**当前唯一的物理阻塞**：F5 —— 需要 Mac mini + 微信扫码 + Nostr identity 连跑 7 天；代码侧阻塞已清零。
**下一步**：ME-3 设计定稿 → 实现 ME-3a..g → ME-4（用第二个领域 OS 证明「可替换」不是纸面性质）。

**里程碑门**：进入 P4（跨用户分发、模块签名 + 信任根）与发布 tag/Release 需用户确认，不擅自跨。

---

## 文档

- [工作站规划](docs/WORKSTATION_PLAN.md) — oMLX API 调研、64GB Mac 模型清单、能力 TODO
- [决策日志](docs/decision.md) — ADR-001 ~ ADR-030
- [实现蓝图](docs/specs/SPEC-MD-ME.md) — M-D 记忆 + M-E 领域 OS · [任务队列](docs/specs/TASKS.md) — 唯一状态源

## 参考实现

`vendor/xiaoheishu` 是 [MushroomDAO/Xiaoheishu](https://github.com/MushroomDAO/Xiaoheishu) 作为参考引入的 submodule，提供成熟的 Electron + Vite + React 基础。框架演进后，小黑书等应用将从本仓库 fork，只维护自身能力模块。

## License

This project is licensed under the [Apache License, Version 2.0](LICENSE).  
Copyright 2024-present MushroomDAO Contributors.  
See [NOTICE](./NOTICE) · [TRADEMARK.md](./TRADEMARK.md) · [LICENSE-zh.md](./LICENSE-zh.md) · [TRADEMARK-zh.md](./TRADEMARK-zh.md) for details.
