# 决策记录（Decision Log）

> 本文档记录 Agent24-Desktop 框架设计中所有关键决策的论证过程、备选方案、决策依据。
> 格式参考 ADR (Architecture Decision Records)，按时间倒序追加，已采纳的决策不删除（仅在被推翻时标注 Superseded）。
> 维护者：jhfnetboy + Claude Code | 起始日期：2026-04-27

---

## ADR-001：从 xiaoheishu/desktop 借鉴而非从零开发

**日期**：2026-04-27
**状态**：✅ 采纳

### 背景

需要一个跨平台（mac/win）的 Electron 桌面应用承载 Agent24 能力。备选方案：
- A. 从零设计 Electron 应用
- B. 从 xiaoheishu/desktop 借鉴
- C. fork 某个开源 desktop agent 框架（如 Open Interpreter desktop）

### 论证

**已勘察 xiaoheishu/desktop（`/Users/jason/Dev/mycelium/blog/submodules/xiaoheishu/desktop`）**：
- Electron 30 + Vite + React 18 + TypeScript（成熟主流栈）
- `node-llama-cpp` 已集成本地 LLM，含模型自动下载、HF endpoint 检测、硬件推荐
- `better-sqlite3` 本地存储
- 干净的 IPC 模块化架构（`electron/ipc/{posts,publish}.ts`）
- 安全的 `localfile://` protocol handler
- Playwright 浏览器自动化（虽然是 xiaohongshu 专用，但架构通用）

放弃 A 的理由：上述技术决策都已经验证过，重做没意义。
放弃 C 的理由：第三方 desktop agent 框架（如 Letta Desktop、AnythingLLM）的核心都是封闭的 chat UI，扩展困难；xiaoheishu 的代码量小、可读性高、无第三方依赖陷阱。

### 决策

**B**。把 xiaoheishu 作为 git submodule 引入 `vendor/xiaoheishu`，提取通用部分到 Agent24-Desktop 主目录，xiaoheishu 自有功能（小红书发布等）后续抽离为独立 npm 包。

---

## ADR-002：从"裁剪"改为"模块化适配层"

**日期**：2026-04-27
**状态**：✅ 采纳（已修正初版方案）

### 背景

最初我（Claude）的设计方案是"裁掉 xiaoheishu 的特定场景代码（小红书发布）"。用户立刻反对：

> "我有一点疑问，就是第一为什么要裁掉原来的一些呃能力。…… 我希望这个 desktop 是一个融合的 desktop 不应该去裁掉原来的能力。换句话说，或者它是一个模块加载的方式。…… 这样的话，我们的框架就跟能力是解耦的，跟 AI 模型也是解耦的，框架只做的核心的迭代。"

### 论证

"裁剪"假设了"我们要做一个新应用"。但用户的真实诉求是"做一个壳，让能力按需加载"——这本质上是平台思维 vs 应用思维。

**裁剪方案的问题**：
- 一旦裁掉，xiaoheishu 后续的更新就再也合不回来
- 框架和场景紧耦合，每加一个新场景（公众号、Twitter）都要改框架本体
- 没法支持"用户自己开发模块"

**模块化方案的优势**：
- 框架只做内核演进（Electron 壳、IPC、AI Layer、Memory Layer）
- 能力变成可插拔的 npm 包，按需 install/uninstall
- xiaoheishu 完成自身调试后，自家功能也成为模块（`@auraaihq/publish-xiaohongshu`），其他人能直接复用

### 决策

**模块化适配层**：内核 + 三层模块（Base/Community/Personal），所有能力都是可插拔模块。

---

## ADR-003：模块按"服务对象"分三层（Base/Community/Personal）

**日期**：2026-04-27
**状态**：✅ 采纳

### 背景

需要给模块分类，备选维度：
- A. 按功能分：发布 / 抓取 / 处理 / 通信 / 身份 ……
- B. 按服务对象分：基础设施 / 社区 / 个人
- C. 按运行时状态分：内核内嵌 / 后台守护 / 按需触发

### 论证

**功能分类（A）的缺点**：
- 维度太多且会膨胀，分到第二第三层就糊
- 一个功能（如"发布"）可能既服务个人也服务社区，归类困难

**服务对象分类（B）的优势**：
- 对应 Mycelium Protocol 的"个人 / 社区 / 城市"三层服务对象（详见 `Brood/protocol/MISSION.md`）
- 清晰映射到我们的产品定位："为个人服务，以社区为协作单位"
- 用户决策路径直观："我要装这个模块吗？" → "它服务于谁？"

**运行时状态分类（C）**：作为正交维度，每个模块自己声明（如 `runtime: "daemon" | "lazy" | "embedded"`），不作为分类轴。

### 决策

主分类用 B，每个模块用前缀编码：
- 基础模块：`module-*`（identity, wallet, comm, storage, shared-memory）
- 社区模块：直接命名（cos72, myshop, mytask, myvote）
- 个人模块：按子类前缀（publish-*, scrape-*, idoris-*）

---

## ADR-004：cos72 包含 myshop / mytask / myvote 三个核心子模块

**日期**：2026-04-27
**状态**：✅ 采纳（用户明确指定）

### 背景

我最初提议社区层包含"cos72 + governance + team-comm"三个模块。用户修正：

> "cos72 它包含三个基础模块……第一个是 myshop 是做积分兑换的，第二个是 mytask 是完成任务获得积分的。第三是 myvote 是投票的。"

### 论证

**用户的分法符合"完整闭环"**：
- mytask（创造价值，得积分）
- myshop（消费积分，换价值）
- myvote（治理决策，影响规则）

这三个模块构成最小可运转的"社区经济+治理"闭环，缺一不可。比起抽象的"governance"更具体落地。

### 决策

社区模块层包含 4 个：cos72（伞包，依赖 myshop/mytask/myvote）+ communication（复用 base 层的 agent-speaker）+ shared-memory。后续按需扩展。

---

## ADR-005：iDoris-SDK 不收纳 publishers，每个 publisher 独立 npm 包

**日期**：2026-04-27
**状态**：✅ 采纳

### 背景

用户设想："小黑书完成调试后打包为 npm 包到 iDoris-SDK（是这个思路吧）。"

我反对了这个收纳思路。

### 论证

**iDoris-SDK 已有特定职责**：是微信桥接 SDK（前 `MushroomDAO/Agent-WeChat-SDK`），定位是"把任意 Agent 接入个人微信号"。

**混进去的问题**：
1. 职责混淆——"内容发布到各平台" ≠ "微信桥接"
2. 文档/版本/issue 全乱
3. 消费方困惑——只想用微信能力的开发者被迫面对一堆 publisher 包

**正确做法**：每个发布平台独立 npm 包，统一在 `@auraaihq/*` scope 下，命名 `@auraaihq/publish-{platform}`。

### 决策

- iDoris-SDK 保持原职责（微信桥接）
- xiaoheishu 中的 xiaohongshu publisher 抽离为 `@auraaihq/publish-xiaohongshu`
- 所有发布器统一前缀 `publish-`，所有抓取器 `scrape-`

---

## ADR-006：npm scope 用 `@auraaihq`

**日期**：2026-04-27
**状态**：✅ 采纳

### 备选

- `@auraai`：和组织名贴近，发音简短
- `@a24`：超短，跟 Agent24 一致
- `@auraaihq`：和 GitHub 组织 `AuraAIHQ` 一致，npm 上已注册

### 论证

- `@auraai` 在 npm 上没注册，临时改去注册可能与 GitHub 组织名脱节
- `@a24` 太晦涩，外部新用户看不懂
- `@auraaihq` 已在 https://www.npmjs.com/settings/auraaihq/packages 注册

### 决策

**`@auraaihq`**。所有包名 `@auraaihq/{name}`。

---

## ADR-007：混合 Monorepo 策略（pnpm workspace + 按"未来可拆"边界组织目录）

**日期**：2026-04-27
**状态**：✅ 采纳

### 备选

- A. 纯 monorepo：所有包在一个仓库，CI 统一
- B. 纯 multi-repo：每个包一个仓库
- C. 混合：单 repo 但目录结构按可拆分边界组织

### 论证

**纯 monorepo 问题**：
- 想要拆出去时（某个 publisher 由社区独立维护），改造成本高
- issue tracker 容易拥堵

**纯 multi-repo 问题**：
- 早期跨包 PR 协调成本极高
- 几十个包的 release 全手动协调，痛
- 内核迭代快时，每次都要发多个 repo 的版本

**混合方案 C 的关键性质**：
| 性质 | 实现 |
|------|------|
| 包名稳定 | `@auraaihq/publish-blog` 不论在 mono 还是拆出去都是这个名字 |
| 每子目录是完整包 | 自带 package.json + 版本号 + tests |
| workspace 协议 | 内部依赖 `"workspace:*"`，发布时自动转版本号 |
| CI 仅构建变更子树 | 用 turbo/nx/changesets |
| 拆分工具成熟 | `git filter-repo --path X --to-subdirectory-filter -` 一行命令出独立 repo |

### 决策

**混合 monorepo**。仓库 `AuraAIHQ/auraai-packages`，目录按"高耦合内核 / 低耦合扩展"分：
- `packages/`：内核 + AI 适配 + memory + base modules（紧耦合）
- `publishers/`、`scrapers/`、`idoris/`：扩展模块（低耦合，未来易拆）

**何时拆**：当某子目录满足「独立 maintainer 团队 / release cycle 严重不一致 / License 必须不同 / 拥堵到拖累 mono」中的任一条件。**半年内不拆**，先验证 mono 够不够用。

---

## ADR-008：模型包不存权重，只放 metadata

**日期**：2026-04-27
**状态**：✅ 采纳

### 背景

`@auraaihq/models-vision` 这种包要不要把模型权重一起发到 npm？

### 数据

| 模型 | FP16 | Q4 量化 |
|------|------|---------|
| LLaVA-1.5-7B | ~14 GB | ~4 GB |
| Qwen2-VL-7B | ~16 GB | ~4.5 GB |
| MiniCPM-V-2.6 | ~8 GB | ~2.5 GB |
| Whisper-large-v3 | ~3 GB | ~1.5 GB |

### 论证

**npm 限制**：单文件 >100MB 困难，要走 LFS-like 方案，复杂度高
**包大小**：放权重 → 几 GB；不放 → 几 KB
**更新成本**：放权重 → 改一个字段要重传几 GB；不放 → 几行 metadata 改完即发
**离线场景**：不放权重时第一次需要联网下载，下载后永久离线（不是真问题）
**xiaoheishu 现有做法**：`electron/ai.ts` 已经实现了"按需下载到 userData"的完整流程，可直接复用

### 决策

**不存权重**。`@auraaihq/models-*` 包只有：
- 模型 ID
- HuggingFace URL（含镜像 fallback）
- 文件大小、SHA256
- 硬件需求（最小内存、推荐 GPU）
- 推荐量化等级

权重通过 `node-llama-cpp` + HF API 运行时下载到用户 `~/Library/Application Support/{App}/models/`。

---

## ADR-009：SkillBank 与 Evolver 是互补的两个独立包，不合并

**日期**：2026-04-27
**状态**：✅ 采纳

### 背景

用户问："skill-bank 和 evolver 这俩是啥关系，是不同风格的还是互补的？"

### 论证

它们解决"自进化循环"中的不同两半：

| 维度 | SkillBank（SkillRL）| Evolver（SkillClaw）|
|------|---------------------|---------------------|
| 类比 | 图书馆 | 编辑部 |
| 路径 | hot path（每任务）| cold path（周期性）|
| 输入 | 当前任务 context | 历史 ATIF 轨迹 |
| 输出 | 检索出的 top-K skills | 新/refined SKILL.md |
| 优化 | 检索准确率 + 延迟 | skill 质量 + 覆盖率 |

**为什么必须分开**：
1. 关注点分离——检索算法和生产算法独立演化
2. 频次差几个数量级——一个每秒，一个每天
3. 故障隔离——Evolver 挂掉不影响 agent 用现有 skills
4. 可独立替换——换检索引擎不影响 evolver

合并的话内部还是这两个子系统，对外 API 还是分两组，徒增包间依赖。

### 决策

**两个独立包**：`@auraaihq/skill-bank` + `@auraaihq/evolver`，evolver 输出写入 SkillBank 的 storage，agent 检索时读 SkillBank。

---

## ADR-010：先做参考实现 + 渐进提取，不一开始就定接口

**日期**：2026-04-27
**状态**：✅ 采纳

### 背景

用户说："我希望整个结构先讨论，确认清楚，然后再去更新我们的里程碑啊相关的。"

### 论证

接口规格的成熟度依赖于至少 2-3 个真实模块的实现经验。过早冻结接口 = 后期大量 breaking change。

**正确路径**：
1. 内核裸跑（M0-M1 早期）
2. 从 xiaoheishu 提取 1-2 个模块作为参考实现（M1）
3. 总结共性，定 v0.1 接口规格（M1 末）
4. 再加 3-5 个模块（M2），可能需要小调整
5. v1.0 接口规格冻结（M3+）

### 决策

M0 阶段**不写接口**，先：
- 决策记录（本文档）
- 结构图 + 模块清单（PLAN.md）
- 仓库与 npm scope 初始化
- 跟 xiaoheishu 提取的边界划分

接口设计延迟到 M1 中后期。

---

## ADR-011：跨切关注点初步规划

**日期**：2026-04-27
**状态**：✅ 采纳（M1+ 细化）

### 决策清单

| 维度 | 起步策略（M1）| 长期目标（M3+）|
|------|-------------|--------------|
| 模块发现 | 仅内置 npm 包 | + Git URL + IPFS hash |
| 模块信任 | 仅核数字签名校验 | + AirAccount 签发 + 沙箱 |
| 模块权限 | 加载时静态声明 | 首次启用时动态授权 UI |
| 模块配置 | YAML/JSON | UI 自动从 schema 生成表单 |
| 模块状态 | Memory Layer 隔离命名空间 | + 加密 + 跨设备同步 |
| 模块版本 | semver | + 自动更新 + rollback |
| 模块通信 | 事件总线（不直接调用）| + actor model |

---

## 附：决策中我（Claude）犯的错误（用于改进）

| 错误 | 教训 |
|------|------|
| 初版主张"裁剪"xiaoheishu | 应该先理解用户"模块化平台"诉求再设计 |
| 提议把 publishers 收纳进 iDoris-SDK 候选方案前没批驳 | 应该立刻指出职责重叠 |
| 错过用户已注册 `@auraaihq` 的事实，先建议 `@auraai` | 应该先查 npm 状态 |
| 一度想把 skill-bank 和 evolver 合并 | 关注点分离原则不应妥协 |
