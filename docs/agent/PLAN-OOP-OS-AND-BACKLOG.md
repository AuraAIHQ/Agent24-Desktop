# 进程外领域 OS 的路线，与其余待办清单

> 目标（用户 2026-09-10 定）：**Sin90、Cos72 和别人写的 OS 一样，都脱离内核单独发版**；
> Agent24 提供模板与样例（Sin90 / Cos72），第三方照模板写自己的，就能被 Agent24 加载。
>
> 本文只回答三件事：① 这个目标卡在哪几刀上 ② 到能发版为止还剩什么 ③ 与此无关的待办有哪些。
> **读数与判断分开标**：`【实测】` 是跑出来或读源码得到的，`【判断】` 是我的裁量，`【待测】` 是还没有依据的。

---

## 一、这个目标必须等 ME-3 —— 这是实测，不是推断

【实测】磁盘上的包只要写 `impl_kind: out_of_process`，**今天在挂载时被硬拒**：

- `rust/apps/agent24d/src/domain.rs:727` → `if !manifest.is_mountable_in_process()` 直接拒绝
- `rust/crates/agent24-domain/src/lib.rs:705` → `is_mountable_in_process()` 只对 `InProcessCrate` 返回 true，注释原文：*"ME-3's transport does not exist yet, so the in-process mounter MUST refuse it rather than half-mount a config it cannot honor"*

而 ME-3a（已合）交付的是**发现 + 安装**：`agent24 os install <目录>` 能把包放进 `~/.agent24/packages/`，daemon 下次启动会扫到它。**扫到 ≠ 装得上** —— 扫到的进程内包必须是编译进内核的 crate，扫到的进程外包会被拒。所以今天磁盘安装这条路对第三方是空的。

### Sin90 需要哪几刀 —— 比整个 ME-3 少一刀

【实测】`rust/crates/agent24-sin90-os/domain-os.yml`：

```yaml
kernel_capabilities: [events]      # 只有 events
data_dir: ~/.agent24/os/sin90/     # 自己管数据,不用内核记忆
```

**所以 Sin90 进程外化不需要 ME-3d（记忆回调）。** 这一刀是全 ME-3 里否定用例最多、最难的一刀（租约、配额、分区键、`private/*` 拒绝租约字段……）。

| 刀 | 内容 | Sin90 需要？ | 第三方通用 OS 需要？ |
|---|---|---|---|
| 3a 发现与安装 | ✅ **已合**（#160/#161） | — | — |
| 3b-2a 版本协商 | ✅ **已合**（#162），🟢 library-only | ✔ | ✔ |
| 3b-1 framing | 🔵 **PR #164 复审中** | ✔ | ✔ |
| 3b-2b `initialize` 线格式 | 未开工 | ✔ | ✔ |
| 3b-3 spawn + 进程监督 | 未开工 | ✔ | ✔ |
| 3b-4 受约束代理 | 未开工 | ✔ | ✔ |
| 3b-5 两阶段热 disable | 未开工 | ✔ | ✔ |
| 3c 回调通道其余部分 | 未开工 | ✔ | ✔ |
| **3d 记忆回调** | 未开工 | **✘ 不需要** | 需要（若模块要用内核记忆） |
| 3e 事件 + 审批 | 未开工 | ✔（要 `events/emit`） | ✔ |
| 3f 仓外 mock 包 + 端到端 | 未开工 | ✔ **这条是验收** | ✔ |
| 3g 启用路径准入校验 | 未开工 | ✔ | ✔ |

【判断】**3f 是「第三方能装」这件事唯一算数的判据**：先构建 daemon，之后生成并安装一个**仓库之外**的包，不改源码、不重新构建，重启后完成挂载 → 路由代理 → 事件转发。没跑通 3f，「支持第三方 OS」就只是一句声称。

【判断】**建议把 3d 排在 3f 之后**，而不是按 SPEC 的字母序。理由：Sin90 与 Cos72 都不用内核记忆，把最难的一刀放在验收之前，会让「第三方能装」这个里程碑被一件它不需要的工作推迟。代价是 offer set 的阶梯要改成 `3c 空集 → 3e {Events, Approval} → 3d {+Memory}`，SPEC §8 那张阶梯表要跟着改一行。**这条要 PR-Daemon 先审切法再动。**

### 还需要一件 ME-3 之外的东西：模板与样例

【实测】`feat/me4-cos72-skeleton` 分支上有 Cos72 骨架（16 文件 / +2315 行，末次 2026-08-22），**main 里没有 cos72 crate**，是真未合的在途工作。它今天是进程内骨架；要当「第三方照着写」的模板，得在 3f 之后改成进程外形态。

---

## 二、到能发版为止，在途的东西

【实测】**代码类在途只有两处**，其余未合分支都是 squash 合并后的残留（文件内容与 main 相同）：

| 分支 / PR | 状态 | 处置 |
|---|---|---|
| **PR #164** ME-3b-1 framing | 复审中，六条已改，新 head `951a364` | 合掉 |
| **`feat/me4-cos72-skeleton`** | 2 commits，4 个文件 main 里没有 | 3f 之后重做成进程外样例；**现在不合** |
| 其余 7 个分支 | 文件内容已在 main | 可删 |

### 建议的发版切法【判断】

不要等整条 ME-3 才发版。按「能对外说什么」切三个版本：

- **v0.4.0 — 握手层**：3b-1 + 3b-2b + 3b-3 + 3b-4 + 3b-5 + 3c。
  能说的：内核能起子进程、握手、代理路由、热停。**不能说**：能装第三方 OS（offer set 是空集，模块还没有任何业务回调）。
- **v0.5.0 — 第三方可装**：+ 3e + 3f + 3g + Cos72 进程外样例。
  能说的：**照模板写的 OS 能被 Agent24 加载**，这是你要的那句话。Sin90 可在此版本脱离内核。
- **v0.6.0 — 内核记忆开放给模块**：+ 3d。给需要内核记忆的第三方用。

【待测】我不给天数。可参照的实测节奏：ME-3a 四块，每块 2–4 轮复审；3b-2a 一刀走了 4 轮；3b-1 到目前 2 轮。**每一刀的复审轮数比编码时间更能决定周期**，而复审轮数取决于判据写得多准，不取决于我打字多快。

---

## 三、与此无关的待办（37 条未完成）

来源 `docs/agent/followups.md`。按「挡什么」分类，不按编号。

### A. 挡 F5 泡测（外部依赖 / 环境，非本仓代码）
- **FU-33** 上游 hyphae：发布早于落库的竞态。【实测】本地 minirelay 上 `sent=105 confirmed=2 lost=94`，赢面约 2% → **F5 判据 6 必然失败**。详见下方决策上下文。
- **FU-38** `relay.aastar.io` 下线（HTTP 530，Cloudflare 源站不可达）。【实测】DNS/TCP 正常，正对照 `relay.damus.io` 返回 200。**它是 iDoris 自己的 relay，INTERFACES.md 列为「核心」依赖 —— 下线本身要报上游。**
- **FU-34** 上游二进制改名 `agent-speaker` → `hyphae`；`--json` 契约四条已逐条验过没漂移；剩 keystore 非交互解锁（R3）。
- **FU-39** 两个桥进程抢同一个健康快照文件，症状会把人引向完全错误的诊断。

### B. CI 盲区（今天没有任何东西会红）
- **FU-40** 没有 CI 跑 `cargo doc` → 文档链接断裂对每道闸门不可见。【实测】13 条既有警告，口径已写明。
- **FU-10** 契约漂移：`os list` 输出形状变了不会红。
- **FU-28** Skill 版本递增门：改了 `skills/<name>/` 但 version 没变高应当红。

### C. 记忆 / 空间层的正确性与安全（F8 域）
FU-1 rekey 后 `ON CONFLICT` 目标错、FU-2 记忆库打不开只 warn、FU-3 sweep 的安全性论证不覆盖跨进程、**FU-4 首次插入不校验 `owner_key` 是否真编码了所声明的 (org,space) → 抢占**、FU-5 冲突日志不打已存身份、FU-6 前缀不相交依赖 ASCII、FU-8 `Decision` 可被 `let _ =` 绕过、FU-9 「模块拿不到 KvStore」靠私有字段 + 纪律而非构建保证。

### D. ME-3 自身的挂账
- **FU-41** ephemeral packages root 不可猜了，但仍无所有权/权限校验。**ME-3b 让 manifest 能指定 spawn 进程之后，它就是执行边界 —— 必须在 3b-3 前闭。**
- **FU-42** `MAX_FRAME_BYTES = 1 MiB` 在消费方存在之前定的，且超限**断连不降级**。必须在 3b-2b 落地**之前**用它真实的 wire shape 钉一遍。
- FU-14 ACP vs 自定义协议 —— **已裁决**（ADR-031，不采用 ACP），待 PR 号落定翻 `[x]`。

### E. Nostr / 桥
FU-29 回复 at-most-once 且超时投递状态不确定、FU-30 两个 bridge 抢 `rename`、FU-31 macOS `fsync` 不等于 `F_FULLFSYNC`、FU-35 泡测自动 verdict 只覆盖判据 1/2/6 且是 5 分钟离散采样、FU-36 判据 2 用的是存在性判据不是活性判据。

### F. 调研转化（读了别人的东西，还没变成本仓的东西）
FU-12 frecency 排序、FU-13 模型权重 lockfile、FU-15 建 `docs/laws/`、FU-18 agent loop 打转检测、FU-19 `_open_nofollow`、FU-20/21 LLM 畸形输出落在哪一侧（**需先核实**）、FU-22 角色-工具矩阵、FU-25 Skill 分发两半、FU-26 记忆的可见性与可纠正性、FU-27 TaskTrace 与 ArtifactStore 的边界。

### G. 待你裁决（不是技术问题）
- **FU-23** 开源 / 商业能力边界表：MISSION.md 写了 MushroomDAO 开源 + HyperCapital 商业，但**哪些能力属于哪一侧没有表**。
- **FU-24** 要不要引入独立 Auditor 角色。
- **FU-11** berd 的「CLI 校验只是便利」是否也适用于 agent24d 的 HTTP 面（**需先核实**，核实前不得当成已知缺陷陈述）。

---

## 四、两个决策的上下文

### 决策一：hyphae 的那行 SQL

【实测】机制：`agent.go:208-238` 先 `relay.Publish` 后 `StoreOutgoingMessage`。若 daemon 恰在 3s 订阅窗口内收到自己发的消息并写成 `is_incoming=1`，发送方随后的 `INSERT OR REPLACE` 会把它**覆盖回 0**，而 daemon 的 `seen` 已记下该 id 不会再处理 → 这条消息在本地库里永远是「出站」。

【实测】本地 minirelay：`sent=105 confirmed=2 lost=94`（≈2%）；`messages.db` 里含 canary 的行 `is_incoming=0` 有 130 条、`=1` 有 11 条。后果：`degraded_transitions` 持续增长，**F5 判据 6（入站通路活性）不可能通过**。

修法本身：让 upsert 的 `is_incoming` 单调（`ON CONFLICT(id) DO UPDATE SET is_incoming = messages.is_incoming OR excluded.is_incoming`），或发布前先落库。**注意「一行」是低估** —— `INSERT OR REPLACE` 是 delete-then-insert，其他列也会一起丢。

| 选项 | 做什么 | 代价 |
|---|---|---|
| **A** | 本地打补丁 + 重编 + 验证，不提交上游 | F5 立刻能跑；修复只活在我们机器上，上游仍坏，别人复现不了我们的结果 |
| **B** | 打补丁 + 给上游开 PR | 做对了；F5 要等另一个仓库的评审来回 |
| **C** | 不打补丁，换公共 relay + 拉长 canary 间隔 | **⚠️ 我此前把 C 说得可行，那是没有依据的**（见下） |

**对 C 的更正【判断】**：我之前建议「用公共 relay + canary 间隔拉到 15 分钟」，理由是远端 relay 往返慢、竞态窗口小。**这条从来没有被测过。** 而且按现有数字算：单次确认率 2%，`staleAfterMs = 3×间隔` 意味着要三次里至少中一次，`1-0.98³ ≈ 6%` —— **拉长间隔并不改变单次赢面，C 按我原来的说法站不住。** 若要保留 C，先做一次实验：在公共 relay 上发 100 条 canary 量确认率（约 10 分钟），**确认率高才谈 C**。

【判断】我的建议是 **A**，理由是它把「F5 能不能跑」和「上游什么时候接受补丁」解耦；随后再走 B，把补丁和那条契约测试一起提上去（FU-33 里已经写明：那条契约上游没有任何测试守着，一次重构就能静默作废 Agent24 的活性判据）。

### 决策二：Mac mini 什么时候起

- 它做什么：F5 是 **7 天泡测**，两台机器同时跑能把「只在这台机器上成立」的结论排除掉。runbook 已在 `docs/SOAK-F5.md`，本机那套准备（二进制、LaunchAgent、无密码 identity、oMLX、两条 schedule、端到端 `agent24 chat` 2.4s）已经跑通过一遍。
- 硬约束：**两台必须用不同的 Nostr 身份**，否则各自的桥会确认对方的 canary，活性判据变成互相背书。
- 它被什么挡住：**同一件事** —— 决策一没定，两台都会在第一分钟进 degraded，而且原因和 FU-32 要抓的失效长得一模一样。
- 【判断】所以顺序是：先定决策一 → 本机跑通冒烟 → 再起 mini，两台同时开始计 7 天。**先起 mini 只会得到两份同样无效的数据。**
