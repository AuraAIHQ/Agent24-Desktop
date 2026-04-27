# Agent24-Desktop 产品计划与架构

> 完整设计文档，记录从 xiaoheishu/desktop 借鉴 → Agent24-Desktop 通用框架 → 应用方 fork 这一演进路径上的所有决策。
> 创建日期：2026-04-27

---

## 一、产品定位修正：从"裁剪"到"模块化适配层"

最初设想是从 xiaoheishu/desktop fork 后裁掉特定场景代码（如小红书发布），但这思路是错的。**正确思路是模块化解耦：框架与能力解耦、框架与 AI 模型解耦**。

### 设计原则

1. **框架核心只做演进** — Electron 壳、IPC、模块加载机制、AI 适配层、记忆层、通信层
2. **能力即模块** — blog 发布、小红书发布、微信桥接、文件归档、图像处理 …… 全部抽象为 `CapabilityModule`，按需加载/卸载
3. **AI 解耦** — iDoris 为主 AI 供应商，旁有 Claude / OpenAI / 本地 LLaVA 适配器，业务层不感知具体来源
4. **后台 daemon + 任务自动分解** — desktop 启动即起后台 agent，用户交互后自动拆解任务、调度执行、跨 agent 协调

---

## 二、架构

```
┌──────────────────────────────────────────────────────────────┐
│   Electron Shell（跨平台分发 + UI 一致性）                      │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ Core Loop（永远在线的本地 agent 主循环）                  │  │
│  │  • 任务拆解  • 调度  • 与用户交互  • 与其他 agent 通信     │  │
│  └─────────────────┬──────────────────────────────────────┘  │
│                    │                                         │
│   ┌────────────────┼────────────────────────┐                │
│   ▼                ▼                        ▼                │
│ ┌────────────┐ ┌──────────────┐ ┌─────────────────────────┐  │
│ │ AI Layer   │ │ Memory Layer │ │ Capability Modules      │  │
│ │ (适配器)    │ │              │ │ (可插拔)                 │  │
│ │            │ │ • 短期: SQLite│ │                         │  │
│ │ • iDoris   │ │ • 长期: ATIF  │ │ ▣ blog 发布             │  │
│ │   (主)     │ │   archive    │ │ ▣ 小红书发布             │  │
│ │ • Claude   │ │ • 工作记忆: KV│ │ ▣ 微信桥接 (iDoris-SDK)  │  │
│ │ • OpenAI   │ │ • 跨设备同步  │ │ ▣ 文件归档              │  │
│ │ • Local    │ │   (Nostr)    │ │ ▣ 图像处理 (Vision LLM)  │  │
│ │   (LLaVA)  │ │              │ │ ▣ Claude Code skills     │  │
│ │            │ │              │ │ ▣ ... 用户自定义模块      │  │
│ └────────────┘ └──────────────┘ └─────────────────────────┘  │
│                    │                                         │
│         ┌──────────┴──────────┐                              │
│         ▼                     ▼                              │
│   ┌──────────────┐    ┌──────────────────┐                   │
│   │ MCP Bridge   │    │ Agent-Speaker    │                   │
│   │ • Agent24    │    │ Bridge (Nostr)   │                   │
│   │ • 任意 MCP   │    │ • 跨 agent 通信   │                   │
│   └──────────────┘    └──────────────────┘                   │
└──────────────────────────────────────────────────────────────┘
                            │
            ┌───────────────┼───────────────┐
            ▼               ▼               ▼
       Local FS         Nostr Relay    Other Agents
```

### 任务流

```
用户输入 → Conversation Layer → Task Decomposer → Executor Pool
                                                       │
                                                       ├─ Local: AI Layer 调用
                                                       ├─ Module: 加载 capability 执行
                                                       ├─ Remote: 通过 agent-speaker 派给其他 agent
                                                       └─ Schedule: cron 化定时任务
                                                       
执行完毕 → 写入 Memory Layer → ATIF 轨迹 → 触发 Evolver
```

Desktop 启动时自动起后台 daemon（Electron tray icon + 隐藏窗口），用户随时唤起对话面板。这对应 SkillClaw 论文的 `evolve-server --interval 300` 模式。

---

## 三、核心子系统

### 3.1 Capability Module 接口（最关键的解耦点）

每个模块需实现：

```ts
interface CapabilityModule {
  id: string                                // 全局唯一 ID, e.g. "wechat-storage"
  version: string
  name: string                              // 用户可见名称
  description: string

  // 生命周期
  load(ctx: ModuleContext): Promise<void>
  unload(): Promise<void>

  // 能力声明（供 task decomposer 路由）
  capabilities: Capability[]                // e.g. [{ kind: "publish", target: "wechat" }]

  // 主入口
  invoke(intent: Intent, ctx: InvocationContext): Promise<Result>
}
```

`ModuleContext` 包含：AI Layer 句柄、Memory Layer 句柄、IPC 桥、文件系统受限访问。模块以独立进程或 worker 运行（隔离，安全）。

### 3.2 AI Layer 适配器

```
AILayer
├── iDoris    (主 AI — 个人全景洞察系统，本地运行)
├── Claude    (云端 fallback)
├── OpenAI    (云端 fallback)
└── Local     (LLaVA / Qwen2-VL — 离线视觉)
```

业务层只调用 `ai.complete(prompt, modality)`，AI Layer 根据策略路由（隐私敏感 → iDoris；推理强度高 → Claude）。

### 3.3 Memory Layer

借鉴 MemPalace 的分层 + temporal validity，加入 SkillRL 的 SkillBank：

| 层 | 内容 | 存储 |
|---|------|------|
| L0 | 用户身份 + 当前会话上下文 | KV |
| L1 | 重要事实（essential.md 风格） | SQLite |
| L2 | 主题相关记忆（按需加载） | SQLite + 全文索引 |
| L3 | ATIF 轨迹归档（DGM-style） | YAML 多文档 |
| **Skill** | **从 archive 蒸馏的 skill（SkillRL/SkillClaw 风格）** | **SKILL.md + index** |

跨设备同步：通过 Nostr relay 加密同步（NIP-44 + agent-speaker）。

### 3.4 Evolver（自进化引擎）

借鉴 SkillClaw：

```
ATIF Archive (results.log)
    │
    ▼
Pattern Miner ─── 识别 N 次重复出现的行为模式
    │
    ▼
Evolver Decision (LLM)
    ├─ Refine 已有 SKILL.md
    └─ Create 新 SKILL.md
    │
    ▼
Validation Worker (Codex MCP) ─── 校验质量
    │
    ▼ (通过)
SkillBank 主分支
    │
    ▼
所有 desktop 实例同步（通过 Nostr）
```

---

## 四、SkillClaw 借鉴评估

**核心思路**：每个用户的轨迹 → Evolver 提炼 → SKILL.md 更新 → 全员受益。

**对 Agent24 适配性**：极高。我们已有 DGM-style archive (`results.log`) + MemPalace 分层记忆 + Codex 外部评估，**缺的就是从 archive → SKILL 的自动炼化环节**。

**直接借鉴**（来自 SkillClaw `skillclaw/` 模块）：
1. **`skill_manager.py` Refine vs Create 决策** — 现 Phase 4 只写 memory，从不改 SKILL.md。加 `meta-evolve` skill：扫描 archive 重复 pattern，决定 refine 现有还是 create 新 skill
2. **`validation_worker.py` + `prm_scorer.py`** — Codex 评估器作为 validated publish gate，新 SKILL 必须通过 review 才合并
3. **OpenAI-compatible proxy 模式（`api_server.py`）** — desktop 暴露此接口，让任何外部工具透明获得"自进化"能力

**License**：MIT，无限制借鉴。

---

## 五、Top-5 自进化/长记忆 repos 调研

| 项目 | Stars | 借鉴价值 | 状态 |
|------|------:|---------|------|
| [DGM (jennyzzt/dgm)](https://github.com/jennyzzt/dgm) | 2k | Population archive + 血统追踪 | ✅ 已用 |
| [EvoAgentX](https://github.com/EvoAgentX/EvoAgentX) | 2.9k | MCTS 工作流搜索 | 暂不急 |
| [Voyager](https://github.com/MineDojo/Voyager) | 6.9k | 可执行 skill 库 + 课程学习 | 对非具身过重 |
| [SkillRL](https://github.com/aiming-lab/SkillRL) | 681 | **分层 SkillBank + 自适应检索** | **强烈推荐** |
| [MemSkill](https://github.com/ViktorAxelsen/MemSkill) | 449 | 离线轨迹蒸馏 evolving skills | 中度 |

**最值得借鉴**（独立于 SkillClaw）：
- **SkillRL 的分层 SkillBank** — 在 MemPalace L0-L3 之外加 Skill 层：从 archive 蒸馏 general / task-specific 启发式
- **SkillClaw 的 validated publish gate** — Codex 已在做评估，正好接上

---

## 六、Roadmap（里程碑）

```
M1 (4-6周) — Desktop 壳 + 模块化骨架
  □ AuraAIHQ/Agent24-Desktop 仓库（已建）
  □ 借鉴 vendor/xiaoheishu/desktop 的 Electron + Vite + React 架构
  □ 抽象 CapabilityModule 接口
  □ xiaoheishu 发布逻辑包装为第一批模块（blog / 小红书 / 微信公众号）
  □ AI Layer 适配器：iDoris (placeholder) / Claude / Local LLaVA
  □ 基础对话 UI + 文件浏览器

M2 (6-8周) — Agent 永远在线 + 通信
  □ Tray icon + 后台 daemon
  □ 任务分解器（Conversation → Tasks → Executor Pool）
  □ MCP bridge 接 Agent24 skills
  □ Agent-Speaker bridge 跨 agent 通信
  □ iDoris-SDK 作为模块加载（微信能力）

M3 (8-10周) — Memory + Evolver
  □ 短期/工作/长期记忆分层（参考 MemPalace）
  □ ATIF 轨迹采集（参考原 autoagent）
  □ SkillClaw 风格 Evolver：archive → SKILL refine/create
  □ Codex 评估作为 validated publish gate
  
M4 (10-12周) — 自进化 + 共享
  □ SkillRL 分层 SkillBank 集成
  □ 跨用户 skill 共享（用户自愿，匿名 trajectory）
  □ 通过 Nostr 分发 skill 更新
  □ iDoris 主 AI 接入（替换 placeholder）

M5 (后续) — 生态
  □ Module marketplace
  □ 跨设备记忆同步
  □ 个人 agent ↔ 组织 agent ↔ 公共 agent 三级网络
```

---

## 七、与 xiaoheishu 的兼容与演进

- **当前**：xiaoheishu/desktop 是源参考，作为 submodule 引入
- **M1 完成后**：从 xiaoheishu/desktop 提取通用代码到 Agent24-Desktop 主目录，xiaoheishu 特定能力（小红书发布等）抽离为独立模块
- **M2-M3**：xiaoheishu 切换为基于 Agent24-Desktop fork，自身只维护小红书相关模块和 UI 差异
- **接口契约**：双方共同维护 `CapabilityModule` 接口，保持向后兼容
- **同步策略**：Agent24-Desktop 是源，xiaoheishu 周期性 rebase 上游

---

## 八、相关仓库

| 仓库 | 角色 | URL |
|------|------|-----|
| AuraAIHQ/Agent24 | 自进化 Skills 系统（Claude Code skills）| https://github.com/AuraAIHQ/Agent24 |
| AuraAIHQ/Agent24-Desktop | 跨平台 Electron 框架（本仓库）| https://github.com/AuraAIHQ/Agent24-Desktop |
| AuraAIHQ/iDoris | 个人全景洞察 AI（Prism 启发）| https://github.com/AuraAIHQ/iDoris |
| AuraAIHQ/iDoris-SDK | 微信桥接 SDK（前 MushroomDAO/Agent-WeChat-SDK）| https://github.com/AuraAIHQ/iDoris-SDK |
| AuraAIHQ/agent-speaker | Nostr-based agent 通信 | https://github.com/AuraAIHQ/agent-speaker |
| AuraAIHQ/simple-agent | 微信场景的 Level 1 agent | https://github.com/AuraAIHQ/simple-agent |
| MushroomDAO/Xiaoheishu | 应用参考（M1 后从 Agent24-Desktop fork）| https://github.com/MushroomDAO/Xiaoheishu |
