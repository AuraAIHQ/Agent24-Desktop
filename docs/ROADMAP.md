# Roadmap

> 主里程碑列表见 [PLAN.md 第六节](PLAN.md#六roadmap里程碑)。本文档维护当前进度。
> 所有重大决策见 [decision.md](decision.md)。

---

## M0（Bootstrap）— 进行中

### 已完成
- [x] 创建 `AuraAIHQ/Agent24-Desktop` 仓库
- [x] 引入 `vendor/xiaoheishu` submodule（参考实现）
- [x] 写产品计划 `docs/PLAN.md`
- [x] 写决策日志 `docs/decision.md`（11 个 ADR）
- [x] 注册 npm scope `@auraaihq`
- [x] 模块分类与命名规则确认（三层 + iDoris 子层）

### 进行中
- [ ] 创建 `AuraAIHQ/auraai-packages` monorepo 仓库
- [ ] 初始化 pnpm workspace 骨架（packages/ + community/ + publishers/ + scrapers/ + idoris/）
- [ ] 第一批占位包（core / sdk / cli），README + package.json，无实现

### 待完成（M0 收尾）
- [ ] CI 框架（pnpm + turbo + changesets）
- [ ] devcontainer / 开发文档

---

## M1（4-6 周）— Desktop 内核 + 第一批模块

### 内核提取（从 vendor/xiaoheishu/desktop）
- [ ] `electron/main.ts` → `Agent24-Desktop/src/main/main.ts`
- [ ] `electron/preload.ts` → `Agent24-Desktop/src/main/preload.ts`
- [ ] `electron/db.ts` → `@auraaihq/memory` (sqlite backend)
- [ ] `electron/ai.ts` → `@auraaihq/ai-local` (拆出 node-llama-cpp 部分)
- [ ] `electron/settings.ts` → `Agent24-Desktop/src/main/config/`

### 第一批占位模块（M1 末有可运行 demo）
- [ ] `@auraaihq/core` — 模块加载器骨架
- [ ] `@auraaihq/sdk` — 模块开发者公共 API
- [ ] `@auraaihq/ai-bridge` — AI Layer 路由
- [ ] `@auraaihq/ai-claude` — Claude 适配
- [ ] `@auraaihq/ai-local` — node-llama-cpp 适配
- [ ] `@auraaihq/memory` — L0-L1 实现（SQLite）

### 第一个真实模块（参考实现）
- [ ] `@auraaihq/publish-blog` — 抽离 xiaoheishu 的 blog 发布逻辑（最简单的 publisher）

### 接口规格 v0.1（M1 末）
- [ ] 总结 publish-blog + memory 的实现共性 → 提出 `CapabilityModule` 接口规格

---

## M2（6-8 周）— Agent 永远在线 + 通信 + iDoris-SDK 合并

- [ ] Tray icon + 后台 daemon
- [ ] Conversation Layer：任务分解 + 调度
- [ ] MCP bridge 接 Agent24 skills
- [ ] `@auraaihq/module-comm` — agent-speaker bridge
- [ ] `@auraaihq/module-identity` — AirAccount 集成
- [ ] **iDoris-SDK 合并到 monorepo（ADR-013）**
  - [ ] 代码迁入 `auraai-packages/communication/wechat-bridge/`
  - [ ] npm 包改名 `@agent-wechat/core` → `@auraaihq/wechat-bridge`
  - [ ] 老包 deprecate
  - [ ] simple-agent 升级依赖
  - [ ] AuraAIHQ/iDoris-SDK 归档
- [ ] `@auraaihq/module-wechat` 适配模块（包装 wechat-bridge 给 desktop 用）
- [ ] 第二批 publisher：xiaohongshu / xiaoheishu / wechat-mp / twitter

---

## M3（8-10 周）— Memory + Evolver + Agent24 替代

- [ ] `@auraaihq/memory` 升级到 L0-L3 完整分层
- [ ] `@auraaihq/skill-bank` —（SkillRL 风格）
- [ ] `@auraaihq/evolver` —（SkillClaw 风格守护进程）
- [ ] ATIF 轨迹采集（Conversation Layer 写入 archive）
- [ ] Codex 评估作为 validated publish gate
- [ ] **Agent24 → npm 包迁移（ADR-014）**
  - [ ] `@auraaihq/skills-evolve` / `skills-evaluate` / `skills-setup` / `skills-org-sync`
  - [ ] `@auraaihq/cli install <skill>` 替代 install.sh
  - [ ] AuraAIHQ/Agent24 标记为 **deprecated**：仓库 archive 只读 + README 顶部加显眼 deprecated banner 引导到新 npm 包
- [ ] **Agent24-Desktop 改名为 Agent24（ADR-015）**
  - [ ] 仓库 rename: AuraAIHQ/Agent24-Desktop → AuraAIHQ/Agent24（旧 Agent24 已归档，名字空出来）
  - [ ] 应用产品名：去掉 "Desktop" 后缀
  - [ ] 为未来 mobile 端做准备（Electron + Capacitor 或 Tauri 路径）

---

## M4（10-12 周）— 自进化 + 共享

- [ ] 跨用户 skill 共享（用户自愿，匿名 trajectory）
- [ ] Nostr 分发 skill 更新
- [ ] iDoris 主 AI 接入（替换 placeholder）
- [ ] 模块市场 UI（用户从 desktop 装/卸 npm 模块）

---

## M5（后续）

- [ ] 模块签名 + AirAccount 信任根
- [ ] 跨设备记忆同步
- [ ] 个人 ↔ 组织 ↔ 公共 三级 agent 网络
- [ ] 拆分时机评估（哪些子目录适合拆出 monorepo）

---

## 拆分决策门槛（参考 ADR-007）

某子目录满足以下任一条件时考虑拆出 monorepo：
- 独立 maintainer 团队
- release cycle 严重不一致（差 10x 以上）
- License 必须不同
- 包数量/issue 量拖累 mono 构建

**底线**：M5 之前不拆，先验证 mono 够不够用。
