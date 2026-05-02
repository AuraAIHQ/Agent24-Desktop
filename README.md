# Agent24-Desktop

> 跨平台 Electron 桌面框架——为 Agent24 生态提供统一的"个人 AI 助手"承载壳，支持可插拔能力模块、多 AI 适配、分层记忆、跨 agent 通信。

## 定位

**Agent24-Desktop 是框架，不是应用。** 我们提供：

- 跨平台分发（macOS / Windows）
- 后台 daemon + 用户交互一致性
- 标准化能力模块接口（`@auraaihq/sdk` `defineModule`）
- AI 适配层（iDoris 主、Claude / OpenAI / 本地 LLaVA 备）
- 分层记忆（L0 KV → L3 ATIF 轨迹 + SkillBank）+ 自进化框架
- 通过 agent-speaker / Nostr 与其他 agent 通信

**应用方**（如小黑书、博客、社区工具等）从本框架 fork，搭载具体场景的能力模块。

> **重命名计划**：M3 末 `AuraAIHQ/Agent24-Desktop` → `AuraAIHQ/Agent24`（旧 Agent24 仓库届时归档，名字空出来，详见 [ADR-015](docs/decision.md)）。

---

## 架构

```
┌─────────────────────────────────────────────────────────────────┐
│              Electron Shell（跨平台分发 + UI 一致性）               │
│                                                                 │
│  Main Process (Node.js)              Renderer (React)          │
│  ┌─────────────────────────────┐    ┌────────────────────────┐  │
│  │  kernel.ts                  │IPC │  模块管理面板            │  │
│  │  @auraaihq/core ────────────┼────┤  对话 / 任务状态         │  │
│  │  @auraaihq/memory           │    │  设置                   │  │
│  │  @auraaihq/ai-bridge        │    └────────────────────────┘  │
│  │                             │                                 │
│  │  动态加载能力模块：           │                                 │
│  │  @auraaihq/publish-blog     │                                 │
│  │  @auraaihq/publish-*        │                                 │
│  │  @auraaihq/scrape-*         │                                 │
│  │  @auraaihq/module-*         │                                 │
│  └─────────────────────────────┘                                 │
└──────────────────────────┬──────────────────────────────────────┘
                           │  file: (开发) / npm registry (生产)
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│       AuraAIHQ/auraai-packages（SDK + 能力包 monorepo）            │
│                                                                 │
│  packages/   ← 内核（框架直接依赖，紧耦合）                         │
│  ├── @auraaihq/core        模块加载 / 生命周期 / 依赖排序           │
│  ├── @auraaihq/sdk         模块开发 API（defineModule 等）         │
│  ├── @auraaihq/memory      L0 KV 存储（SQLite，命名空间隔离）       │
│  ├── @auraaihq/ai-bridge   AI 路由（多适配器 + fallback）          │
│  └── @auraaihq/cli         命令行工具                             │
│                                                                 │
│  publishers/ scrapers/ community/ idoris/  ← 可插拔能力模块       │
└─────────────────────────────────────────────────────────────────┘
```

### 核心组件

| 组件 | 包 / 路径 | 职责 |
|------|----------|------|
| **内核** | `@auraaihq/core` | 模块注册、load/unload/invoke、生命周期状态机、依赖图排序 |
| **模块 SDK** | `@auraaihq/sdk` | `defineModule`、`ModuleManifest`、`MemoryHandle`、`AIHandle` |
| **记忆层** | `@auraaihq/memory` | SQLite KV、命名空间隔离、`get/set/has/list/namespace` |
| **AI 路由** | `@auraaihq/ai-bridge` | 多适配器统一接口、优先级 fallback、并发信号量 |
| **Electron 壳** | `src/main/kernel.ts` | 内核单例、SQLite 路径（userData）、Electron 生命周期绑定 |
| **IPC 桥** | `src/main/ipc/` | `KernelListModules` / `KernelLoadModule` / `KernelInvoke` |
| **参考模块** | `@auraaihq/publish-blog` | 第一个能力模块，也是模块开发的参考实现 |

### 能力模块生命周期

```
开发者用 @auraaihq/sdk 的 defineModule() 实现模块
       ↓
kernel.load(moduleId)  →  验证 manifest → module.load(context)
       ↓
context 注入：
  context.memory  = @auraaihq/memory  （SQLite KV，按模块 id 自动隔离）
  context.ai      = @auraaihq/ai-bridge（多模型统一调用）
  context.invoke  = 调用其他模块的跨模块入口
       ↓
kernel.invoke(moduleId, { kind, payload })  →  module.invoke(intent)
       ↓
kernel.unload(moduleId)  →  module.unload() → 释放资源
```

### AI Layer

```
AILayer（@auraaihq/ai-bridge）
├── iDoris    主 AI — 个人全景洞察，本地运行
├── Claude    云端 fallback
├── OpenAI    云端 fallback
└── Local     LLaVA / Qwen2-VL — 离线视觉
```

业务层只调用 `ai.complete(prompt)`，AI Layer 按策略路由（隐私敏感 → iDoris；复杂推理 → Claude）。

### Memory Layer

| 层 | 内容 | 存储 |
|----|------|------|
| L0 | 会话上下文 + 工作记忆 | SQLite KV（`@auraaihq/memory`） |
| L1 | 重要事实 | SQLite 全文索引 |
| L2 | 主题相关记忆（按需加载） | SQLite |
| L3 | ATIF 轨迹归档（DGM-style） | YAML 多文档 |
| **Skill** | **从 archive 蒸馏的可执行 skill** | **SKILL.md + index** |

跨设备同步通过 Nostr relay（NIP-44 加密）。

---

## 文档

- [产品计划与架构](docs/PLAN.md) — 完整设计、模块化架构、SkillClaw 借鉴、自进化路线
- [Roadmap M1-M5](docs/ROADMAP.md) — 里程碑与交付物
- [决策日志](docs/decision.md) — ADR-001 ~ ADR-018

## 参考实现

`vendor/xiaoheishu` 是 [MushroomDAO/Xiaoheishu](https://github.com/MushroomDAO/Xiaoheishu) 作为参考引入的 submodule，提供成熟的 Electron + Vite + React 基础。框架演进后，小黑书等应用将从本仓库 fork，只维护自身能力模块。

## License

This project is licensed under the [Apache License, Version 2.0](LICENSE).  
Copyright 2024-present MushroomDAO Contributors.  
See [NOTICE](./NOTICE) · [TRADEMARK.md](./TRADEMARK.md) · [LICENSE-zh.md](./LICENSE-zh.md) · [TRADEMARK-zh.md](./TRADEMARK-zh.md) for details.
