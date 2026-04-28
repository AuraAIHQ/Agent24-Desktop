# Agent24-Desktop

> 跨平台 Electron 桌面框架——为 Agent24 生态提供统一的"个人 AI 助手"承载壳，支持可插拔能力模块、多 AI 适配、分层记忆、跨 agent 通信。

## 定位

**Agent24-Desktop 是框架，不是应用。** 我们提供：

- 跨平台分发（macOS / Windows）
- 后台 daemon + 用户交互一致性
- 标准化能力模块接口（`CapabilityModule`）
- AI 适配层（iDoris 主、Claude / OpenAI / 本地 LLaVA 备）
- 分层记忆 + 自进化框架
- 通过 agent-speaker / Nostr 与其他 agent 通信

**应用方**（如小黑书、博客、社区工具等）从本框架 fork，搭载具体场景的能力模块。

## 借鉴

`vendor/xiaoheishu` 是 [MushroomDAO/Xiaoheishu](https://github.com/MushroomDAO/Xiaoheishu) 作为参考实现引入的 submodule。其 `desktop/` 子目录提供了成熟的 Electron + Vite + React + node-llama-cpp 架构基础。我们从中提取通用框架部分，将场景特化部分（如小红书发布）抽象为可插拔模块。

未来方向反转：**小黑书等应用从本仓库 fork**，框架由本仓库统一演进。

## 文档

- [产品计划与架构](docs/PLAN.md) — 完整设计、模块化架构、SkillClaw 借鉴、自进化路线
- [Roadmap M1-M5](docs/ROADMAP.md) — 里程碑与交付物
- [Capability Module 接口](docs/MODULE_INTERFACE.md) — 模块开发规范（待写）

## License

Apache 2.0 — see [LICENSE](./LICENSE) and [NOTICE](./NOTICE).
