# SPEC-ME3 — 进程外领域 OS（Out-of-Process Provider）

> 立于 2026-09-02。裁决依据：[ADR-031](../decision.md)（协议按「缝」分别裁决）。
>
> 前置：ME-1（`DomainModule` + `KernelCtx`）、ME-2（配置驱动注册表 + `agent24 os` CLI）均已交付；`ImplKind::OutOfProcessProvider` 这个枚举值 ME-2 时就留好了，本文档负责让它有实现。
>
> 关联：[`SPEC-MD-ME.md`](SPEC-MD-ME.md) §5 ME-3 行、[`SPEC-ME-FOLLOWUPS.md`](SPEC-ME-FOLLOWUPS.md) §「留给 ME-3 的（进程外模块）」、[`SPEC-ORG-SPACE.md`](SPEC-ORG-SPACE.md)（所有权模型）。

## 0. 威胁模型 —— 先说死，因为后面每一条「不可能」都以它为界

**ME-3 防的是：一个合作的模块，它的实现有 bug，或者它在协议层面耍花招。**

**ME-3 不防：一个敌意的本地二进制。**

模块子进程**以当前用户身份运行**（§9）。这意味着它不需要走本文档定义的任何一条协议，就能：

- 直接打开 `~/.agent24/` 下的 `memory.db`，读写任意分区——绕过全部作用域检查；
- 读 daemon 的 state file，拿到 bearer token，然后以内核自己的身份调用全部内核 API；
- 连别的模块的 socket、给 daemon 发信号、`ptrace` 同用户进程；
- 耗尽 CPU / 内存 / 文件描述符 / 磁盘。

所以本文档里的「不可伪造」「隔离」「永远不能」，**全部是 broker API 之内的性质**，不是操作系统层面的保证。一个装了恶意领域 OS 的用户，已经把机器交出去了——这件事**签名（ME-6）也解决不了**：签名回答「这是谁写的」，不提供运行时隔离。

要对敌意模块成立，需要不同 UID / 沙箱 / 容器 / VM，或只传预先打开的 FD。那是另一次立项，不在 ME-3。

**读本文档时请带着这条边界**：下面所有安全性表述都隐含前缀「在模块不绕过 broker 的前提下」。

---

## 1. 这份文档的范围

**这是「让一个第三方进程成为领域 OS」的机制。**

它覆盖：怎么把进程外模块挂上来、路由怎么到它、它怎么回调内核要记忆/事件/审批、它崩了怎么办、它的不可信程度（§0 的意义上）怎么在机制上体现。

它**不**覆盖：进程隔离强度（§0）、模块签名与信任根（ME-6，P4 门后）、跨机分发、模块间通信。§9 逐条列明。

**一句话形状**：内核 spawn 一个子进程，把 `/api/v1/<ns>/*` **受约束地代理**给它；它经一条**窄的 JSON-RPC 回调通道**向内核要记忆、发事件、请审批。

> 初稿在这里写过「入站是代理，**只有出站方向需要定义协议**」——**那句话是错的**，删掉。入站至少还要定义：监听端点由谁选择、怎么传给子进程、ready / health 的契约、请求取消与 shutdown 语义。
>
> 而且这里有一个具体的攻击面：**监听端点必须由内核预先创建并传给子进程（预开 FD / 内核指定路径的 Unix socket），不能让模块在 `initialize` 里回一个任意 URL 或端口。** 否则一个「在协议层面耍花招」的模块（§0 明说要防的那种）可以把内核的反向代理指向本机任意服务，把它变成一个本地 SSRF 跳板。

---

## 2. 入站：受约束的代理，不是「原样转发」

`DomainModule::routes()` 在进程内返回一个 `axum::Router`。这个形状过不了进程边界，但它的**效果**过得去：模块在本地端口上提供 HTTP，内核把自己命名空间下的请求转发过去。内核已经有 axum，为「模块贡献路由」发明协议是把已解决的问题重问一遍。

**但不能原样转发。** 内核今天用 `Authorization: Bearer` 鉴权（`rust/apps/agent24d/src/server.rs` 的 `auth` 中间件）。原样转发等于**把内核的 bearer token 交给模块**——它随后可以用这个 token 调用全部内核 API，而不止自己的命名空间。这条与「模块拿不到内核 token」是直接矛盾的，初稿写错了。

代理必须**保留 method / path / query / body 的语义**，同时：

**剥掉（模块永远看不到）**
- `Authorization`、`Cookie`、以及任何内核私有头；
- hop-by-hop 头（`Connection`、`Keep-Alive`、`TE`、`Transfer-Encoding`、`Upgrade` …）；
- 客户端自己塞的 `X-A24-*` —— **一律先删再由内核写入**，否则模块会收到一个用户伪造的租约（§3）。

**注入（内核写入，模块只能读）**
- `X-A24-Request-Id`：**非秘密的相关性 id**（可进日志、trace、指标）；
- （**将来**，跨空间上线时）`X-A24-Request-Lease`：**秘密的 bearer 租约**，与上面那个分开——见 §3；
- 最小化的调用上下文（不是 bearer，不是原始凭据）。

**响应侧同样受约束**
- 丢弃模块返回的 `Set-Cookie`、认证类头；**以及模块响应里的任何 `X-A24-*`**——它们只能由内核写入，模块回显一个（尤其是 `X-A24-Request-Lease`）就会把秘密送到客户端；
- `Location` 只允许指向本模块命名空间内（否则模块可以把用户重定向到内核的其它路由或外部站点）；
- hop-by-hop 头不透传。

**其余仍归内核**：鉴权在内核（ME-1b 已钉死挂载顺序：模块先 nest，内核 auth layer 最后）；命名空间不可越界；模块不认识内核的其它路由。

---

## 3. 出站：回调通道

模块要的东西对应 `Capability`。**本轮内核的「愿意给」集合（offer set）是 `{Memory, Events, Approval}`**——`Models` / `Scheduler` / `Policy` 都不在本轮（§9）。

> **是 offer set，不是 grant set。** 现有语义是 `manifest 请求 ∩ 内核愿意给`（`Grants::granting`）：没请求就不给。这条必须写死，否则一个老模块只因为用户升级了 daemon 就凭空拿到审批能力。每个会话实际的 grants 一律取交集。

> **`Approval` 是一条新增的独立 capability，不是复用 `Capability::Policy`**（2026-09-08 定案，此前是 §10 的公开问题 1）。两个理由：
>
> 1. 内核今天实授的是 `KERNEL_GRANTS = &[Capability::Events, Capability::Memory]`（`rust/apps/agent24d/src/domain.rs`），注释写明 `Models`/`Scheduler`/`Policy` **故意不授予**，因为「没有 handle 可给，授予一个没有 handle 的能力就是撒谎」。复用 `Policy` 会立刻违反这条自己立的规矩，并长出「§9 说不做 Policy、方法表里却有审批」的自相矛盾。
> 2. `Policy` 的语义（查授权策略、判断 standing-grant 是否自动放行）比「向用户要一次审批」宽得多。为了要审批而顺手把策略查询给出去，是把授予集写松。
>
> **但新增 `Approval` 有一个陷阱，必须一并解决**：`KernelCtx` 今天只有 `events()` 和 `memory()`，**没有审批句柄**。若实现只是把 `Approval` 塞进公共的 `KERNEL_GRANTS`，进程内模块会被报告「已获授 Approval」却没有句柄可用——**恰好重犯上面用来反对 `Policy` 的那条罪**。二选一，实现时必须选定并写进 PR 描述：
> - **(a)** 同时给进程内加一个 `ApprovalRequester` 句柄，保持 capability ↔ trait ↔ wire 三者一一对应（更干净）；
> - **(b)** 拆出一个 `ProviderCapability`，让 `Approval` 只属于 ME-3 的 wire 契约，**不进**当前全局的 `KERNEL_GRANTS`。
>
> **兼容性也要写**：`Capability` 是严格 serde 枚举，旧 daemon 会拒绝含 `approval` 的新 manifest；公共 Rust 枚举不是 `non_exhaustive`，加 variant 会让外部的穷尽 match 源码不兼容。需要定：最低 daemon / 协议版本、unsupported capability 的错误形状、旧 provider 拿到协商结果怎么办，以及对应的测试。

**通道形状**：JSON-RPC 2.0 over Unix domain socket。

**framing 必须钉死。** JSON-RPC 2.0 不定义字节流上的消息边界；不钉死 framing，实现者会各自猜 NDJSON / 长度前缀 / 读到 EOF，而且**没有 framing 就无法在分配与反序列化之前拒绝超大消息**——§5 的「双向上限」会变成空话。本轮选：**行分隔（NDJSON），单行最大长度硬上限，超限即断连**。不支持 batch（支持的话还要单独限 batch 条数）。

同时要定死：请求 ID 类型、并发与乱序响应、未知方法、重复 ID、取消、超时、连接关闭语义；令牌出现在握手的哪个字段、每次 spawn 是否轮换、socket 与目录权限、允许的连接数。这些不是细节，是「实现者不猜」的前提。

**方法集**。初稿说「与进程内 trait 一一对应」——**现在不成立了，别再这么写**：进程内的方法叫 `remember` 不叫 `write`；`Approval` 没有对应的 `KernelCtx` 方法；`scoped/*` 那一族在进程内根本没有对应物（进程内句柄是固定 scope 的）。ADR-031 里那句「薄 SDK 与 trait 一一对应」要么补齐 trait，要么删掉。

| 方法 | 对应进程内 | 说明 |
|---|---|---|
| `initialize` | 挂载握手 | 模块报协议版本；内核回**授予的能力集**。模块报的 manifest 摘要**只用于与内核已加载的 manifest 比对**，不作为身份来源 |
| `_a24/memory/private/{remember,recall,recent}` | `ScopedMemory::{remember,recall,recent}` | 只认连接；**不接受租约字段**；大小上限 + 显式 page size/cursor + 可取消（§5） |
| `_a24/memory/scoped/{remember,recall,recent}` | （进程内无对应物） | 租约**必填**；无效即 forbidden，**绝不回退到 private** |
| `_a24/events/emit` | `EventSink` | 只能发自己 `event_module` 的事件 |
| `_a24/approval/request` | 审批门 | **本轮不需要租约**（单用户/home org,没有「向谁请示」的歧义);必须绑定**内核规范化后的动作 + payload 摘要**(§6),并带 `X-A24-Request-Id` 做相关性。多用户上线时这条要改成需要租约 |

`_` 前缀与 `_meta` 是 ADR-031 定的 ACP 对齐惯例。**`_meta` 永不参与授权与分区**——它是给扩展带附加信息的，不是给模块说自己是谁的。

**凭据与扩展在 JSON-RPC 里的位置,钉死（入站是 HTTP 头，回调是 UDS 上的 JSON-RPC，两边的载体不一样，不写清实现者只能猜）**：

- `request_id`（相关性）与将来的 `request_lease`（秘密）都是 **`params` 的正式字段**，不是顶层成员、不在 `_meta` 里。
- `_meta` 是 **`params._meta`**，每个 params 类型**显式声明**这个可选字段，**不用 `flatten` catch-all**（catch-all 会把「未知字段」重新变成合法输入，正好抵消 `deny_unknown_fields`）。
- `params._meta` 内部宽容、可带未知键；**授权相关的一切只从正式字段读，永不从 `_meta` 读**。

### 身份不由模块提供 —— 但「身份」是两半，不是一半

`SPEC-ME-FOLLOWUPS` 的原话是：

> 远端模块**永远不发**自己的 owner/分区 key，身份来自已认证的连接**或不可伪造的租约**。

那句「或不可伪造的租约」不是修辞。但先纠正初稿的一处事实错误：

> **分区键是 `(org, space)` 两维，不是 `(org, space, module)` 三维。** ADR-030 定的就是二维，`partition_key` 编码的也只有这两个。`module` 今天是被**折进 space 字符串**的（`SpaceId::module_private("sin90")` → `os:sin90`），不是键的第三个分量。分清楚：
>
> ```text
> 目标（写到哪）：  (org, space)
> 主体（谁在做）：  (principal, module)
> ```
>
> 照「三分量」去实现，将来 shared space 很可能被按 module 再切一层——那正好毁掉 shared 的含义。

在这个前提下：

- **`module` 来自连接。** 每个模块有自己的 socket 与令牌，内核按连接查出它是谁。这一维模块伪造不了（在 §0 的边界内）。
- **代表别人做事时，`(org, space)` 来自租约，不来自连接。** 一次被代理的请求服务的是某个组织里某个空间的用户；同一个模块进程会先后服务不同的 org/space。**连接是长期的，那种 scope 是每请求的——用连接去定它是类型错误。**

### 两种授权 —— 但必须是**两套不重叠的方法**，不是一个方法配可选凭据

初稿只有租约一种，于是把「模块写自己的私有分区」也一并要求了租约。那是过严的，会废掉领域 OS 最需要后台能力的那部分（定时清理、夜间巩固、异步计算）。但**分开之后立刻长出一个更糟的东西**，这里说死：

| 授权 | 来源 | 目标 | 谁能用 |
|---|---|---|---|
| **连接授权** | 回调 socket 握手时的令牌（模块身份 = 连接） | **只有自己的私有分区** `(启动时解析的 org, os:<module>)` | 请求内、后台任务都能用 |
| **请求租约**（本轮不签发） | 内核代理入站请求时注入的 **`X-A24-Request-Lease`**（秘密头，与非秘密的 `X-A24-Request-Id` 分开） | 用户空间 / shared space / 一切代表某个 principal 的操作 | **只有请求内** |

**陷阱：凭据降级（credential downgrade）。** 如果私有写和跨空间写共用同一个 `_a24/memory/*` 方法、租约只是一个可选字段，实现者几乎必然写成：

```text
有有效租约 → 用租约的 scope
没有 / 解析失败 → 退回连接的私有 scope
```

那么**过期的、伪造的、来自别的连接的租约就不再 fail-closed，而是静静降级**。这与 FU-32 是同一种病：一个失败被当成了一个合法的空状态。

**所以本设计选：拆成两套方法，不共用。**

```
_a24/memory/private/{remember,recall,recent}   ← 只认连接，不接受任何租约字段
_a24/memory/scoped/{remember,recall,recent}    ← 租约必填；租约无效 = forbidden,绝不回退
```

（等价的替代形状是必填的 tagged union `authority: private | request_lease`；选了 `request_lease` 之后任何错误一律 forbidden。**不接受**「租约可选」这一种。）

「请求处理中写自己的私有分区」走哪条？**走 private。** 这不是模块自报意图能决定的事——目标是不是自己的私有分区，由方法名决定，内核不问模块「你觉得你在代表谁」。

**为什么连接授权是安全的**，理由要写对。初稿写的是「进程外不该比进程内更严」和「它反正能直接开 sqlite，所以不新增风险」——**这两条都不是有效的安全论证，删掉**：

- 进程内模块在本仓的信任模型里被明确标为 trusted code，进程外模块是第三方输入和协议攻击面。同 UID、无沙箱只说明 ME-3 挡不住恶意本地二进制，**不等于 broker API 应该给两者同样的 ambient authority**。
- 「它反正能绕过 broker」用来取消「它可能误用 broker」的风险，等于把 §0 刚立起来的威胁模型自己拆掉。§0 说的是 ME-3 **要**防 buggy 与协议层耍花招的模块。

**真正的理由是这四条**：① 后台能力是明确需求，不是顺手给的；② 目标完全由连接身份**派生**，模块提供不了任何输入（这正是进程内 `ScopedMemory` 的形状——句柄构造时绑死一个 key，三个方法都不收 owner/scope）；③ 授权**可撤销**，见下面的 generation；④ 资源与生命周期受控（§5）。

**代价**：后台任务可以在无人在场时写自己的库。§5 的后台清单是这条代价的对价，不是可选项。

**租约（request lease）——本轮不签发，形状现在定死（§3 裁决 b）**：内核在代理入站请求时生成一个不可猜测的租约，注入为 **`X-A24-Request-Lease`**，并在**内核自己的表**里绑定 `(org, space, principal, module, 到期)`。凡是超出自己私有分区的回调都必须带上它；内核拿它查自己的表得出 scope，**从不采信模块发来的任何 scope 字段**——协议里根本没有这样的字段。

> **租约与 `X-A24-Request-Id` 必须是两个头，不能复用一个。** `X-A24-Request-Id` 是相关性 id，它会进 tracing、错误报告、访问日志、指标标签——这是它的用途。**把同一个字段将来升级成 bearer authority，等于要回头审计每一条日志、每一处回显、每一个 SDK**，那不是纯加法。所以现在就分开：`Request-Id` 非秘密、可记录；`Request-Lease` 是秘密，禁止日志、禁止回显、响应侧剥离、全链路 redact。

租约的性质：

1. **不可猜测**：内核生成的随机值，不是自增、不是请求序号。
2. **随请求结束即失效**，并有独立到期时间。否则模块可以留着 A 用户请求里拿到的租约，事后拿它替 B 操作写记忆——跨 org 越权。
3. **绑定到发它的那条连接**：别的模块拿到也用不了。
4. **模块无法自造**：没有租约的作用域回调直接拒绝。

> **今天的现实，要如实写**：内核当前在 daemon 启动时为 `LOCAL_USER` 解析一次 org，空间是 `module_private(<name>)`。所以 **ME-3 的诚实范围是单用户 / home org / 模块私有空间**——在这个范围内，「请求租约」这条路径实际还没有第二个 space 可指向。它现在就要按**每请求**建，是因为一旦按连接建，将来多用户时改不动；而连接授权只覆盖私有分区、不覆盖任何代表他人的操作，正是为了让那一天到来时不用回头拆。多用户并发与 shared space 的租约语义在 §9 列为不做。

> **由此引出一个必须现在裁决的问题**：本轮**没有任何生产可达的第二个 space**——生产代码里只有 `SpaceId::module_private`，任意 space 的构造器只存在于测试；用户自己的记忆还用裸 user id，尚未进入 `(org, space)` 模型；HTTP 鉴权今天只校验一个 daemon bearer，根本不产出 principal / org / space 上下文。
>
> 那么 §8 里 ME-3d 的「跨空间被拒」「A 请求的租约落到 A 的 scope」就只能靠 test-only 的假 scope 来验——**又一次「测试证明了一个生产路径上不存在的性质」**。

**裁决（2026-09-08，用户）：取 (b)。** 本轮**只实现 `_a24/memory/private/*` 连接句柄**；`_a24/memory/scoped/*`、请求租约的签发与兑换、以及它们的验收，整体推迟到 F8c/F9。理由是本仓自己反复在用的那条：**先有消费者再有提供者**——跨空间授权的消费者是多用户，而多用户这轮不做。

被否掉的是「语义不做，但机制和验收算完成」。

### 本轮不做，但门留在哪（这一节是 (b) 成立的前提，不是安慰话）

推迟不等于以后要拆了重来。将来加 `scoped/*` 是**纯加法**，前提是本轮守住下面**五条**——**每一条都是 MUST，破一条就得回头拆**。（初稿只写了四条；第 5 条是 Codex 第三轮挑出来的，也是最容易漏、漏了「纯加法」就是假的那一条。）

1. **`private/*` 必须*拒绝*租约/scope 字段，不是忽略它。** 收到就报错，不能默默丢掉。这是全部五条里最要紧的一条：只要 `private/*` 曾经接受过一个被忽略的 scope 字段，将来它就可能被「顺手复用」成通用方法，凭据降级（§3）立刻从设计缺陷变成既成事实。

   > **这条不写清未知字段策略就是不可执行的，而文档此前一个字没写。** 补在这里：
   >
   > **方法参数对象一律 `deny_unknown_fields`。** 于是 `private/*` 上出现任何 `lease` / `scope` / `org` / `space` 键都是**解析期失败**，不需要一条专门的运行时检查——「拒绝而非忽略」变成一个结构性质，而不是一句要靠人记得写的话。仓里已有先例与同一条理由：`agent24-sin90/src/types.rs`、`proposal.rs` 对模型产出的输入用 `deny_unknown_fields`，注释写的是「a stray/mistyped key must fail loudly, not be silently dropped」。
   >
   > **这与 §0.2 的「宽容 serde、`#[serde(default)]`、一个坏字段不炸全局」直接冲突——冲突是真的，按位置分开，不要含糊过去**：那条工程标准针对的是**模型产出的内容结构**（LLM 吐出来的东西，抗 schema 漂移比严格更重要）；回调通道传的是**命令**，而且是一条安全边界。同一个仓库里两种策略并存是对的，但必须说明白哪里用哪种，否则实现者会挑一个自己顺手的。
   >
   > **那前向兼容怎么办？** `deny_unknown_fields` 会让协议加不了可选字段——这和 MUST 2/3（将来加 `scoped/*`）是矛盾的。解法用文档里已有的东西：**扩展一律走 `_meta`**（ACP 惯例，§3 上面已定），`_meta` 内部宽容、可带未知键；方法参数本体严格。于是「可扩展」和「拒绝夹带」各得其所。**而 `_meta` 永不参与授权与分区**这条铁律因此更要紧了：它现在是唯一一个宽容的位置，也就是唯一一条可能被拿来夹带 scope 的路径。ME-3d 必须有一条否定用例：`_meta` 里放 `org`/`space`/`lease` 不产生任何效果。
2. **方法名空间现在就分好，`scoped/*` 保留但不实现。** 未实现时返回稳定的 `method not found`，**不是** fallback 到 `private/*`。这样将来上线 `scoped/*` 不改任何既有方法的含义。
3. **`initialize` 的能力协商必须带版本，并且现在就要有。** 将来 `scoped/*` 上线时，新 daemon 会遇到老模块、老 daemon 会遇到新模块——没有版本协商就只能靠猜。协商结果里要能表达「这个方法族本 daemon 不提供」。
4. **租约按*每请求*建的设计现在就写死，即使本轮不签发**，且**用它自己的头 `X-A24-Request-Lease`**，不复用 `X-A24-Request-Id`（理由见上）。一旦按连接建，将来多用户时改不动。

5. **`scoped/*` 将来必须是一条*独立请求、独立授予*的能力，不能落在今天的 `memory` 之下。** 这条是最容易漏、漏了「纯加法」就是假的那一条：
   > 今天 `Capability::Memory` 是**一个粗粒度的布尔**（`rust/crates/agent24-domain/src/lib.rs`），`Grants` 只能表达「有没有 Memory」，表达不了「只有 private」或「允许 scoped」。所以如果将来新 daemon 上线 `scoped/*` 而沿用同一个 `memory`，**所有历史上请求过 `memory` 的模块会自动获得跨空间能力**——旧 grant 的含义被扩大了，那是破坏性变更，不是加法。
   >
   > **方法名分开、版本协商，解决的是「这个方法存不存在」，不是「这个模块该不该有」——那是功能协商，不是授权协商。**
   >
   > 定死：老 manifest 的 `memory` **永远只映射到 `memory.private`**；`memory.scoped` 是一条新的、必须显式请求且显式授予的能力（形状建议 `ProviderCapability::{MemoryPrivate, MemoryScoped, Events, Approval}`，或等价的结构化 memory grant）。**即使模块持有一张有效租约，没有 scoped grant 也必须 forbidden。**
   >
   > 本轮**可以**验的那半:旧 manifest 的 `memory` 只被解析/映射成 `memory.private` entitlement。**不可以**验的那半(需要 `新 daemon 支持 scoped` + `有效租约`,两条都是 deferred 的生产路径)随门 4 一起标 F8c/F9。

6. **manifest / capability schema 本身要有版本与兼容规则——`initialize` 的版本协商救不了它。** 这条是门 5 的前提,漏了门 5 就落不了地:
   > `DomainOsManifest` 今天是**顶层 `deny_unknown_fields` + 严格 capability 枚举**（`rust/crates/agent24-domain/src/lib.rs`）。所以将来一个写了 `memory.scoped` 的新 manifest,碰上老 daemon 会在**解析期**就失败——**早于 spawn、早于 `initialize`**。握手期的版本协商发生得太晚,根本轮不到它。
   >
   > 定死:manifest 里的**最低 daemon / 协议版本**字段;未知 capability 的**稳定错误形状**(而不是一条泛化的解析失败);以及「老 daemon + 新 manifest」的 **pre-handshake 行为**——是拒绝并给出可读原因,还是按最低版本字段提前判定不兼容。

**存储层就绪到哪一层，要说准**——初稿写「存储层已经准备好了，不需要为多用户改」，**那是把一个局部性质说成了全局性质**，收回：

- ✅ **就绪的**：物理 owner-key 编码与底层事件表。`partition_key(org, space)` 是二维、长度前缀、带 `v2\0` 版本前缀，对**任意**字符串都成立。
- ❌ **未就绪的**：生产构造与 catalog 生命周期。任意 `SpaceId` 的构造器是 `#[cfg(test)]` 的；唯一的生产构造器是 `module_private`；`OsMemoryCatalog::record` **无条件**把 manifest 名折成私有 space，并且「记不下就不出借」是它明写的不变量；catalog 把 `module_name` 当作**首次所见、写一次**的列（migration 0013），于是「一个 partition 由一个 module 首次且持续归属」今天是一条不变量——而 shared space 的定义恰恰是多个 provider 访问同一个 `(org, space)`。

所以将来至少还要：加一条受内核控制的生产 `SpaceId` 构造路径；把「创建/登记一个 space」与「某 module 持租约访问一个已有 space」拆开；改掉「每次出借都按当前 module 重新 record」这条不变量；裁决 `module_name` 是首次创建者的 provenance 还是必须从 schema 里拆走。（`UNIQUE(org_id, space_id)` 不是障碍，那是正确的约束。）

今天缺的**不只是授权能力**，还有上面这层 catalog 语义。

**真正挡在多用户前面的也不在 ME-3**，如实列出来免得以后误以为是 ME-3 欠的债：

- 鉴权今天只校验一个 daemon bearer，**不产出 principal / org / space 上下文**（`rust/apps/agent24d/src/server.rs`）；
- 用户自己的记忆仍用**裸 user id**，尚未进入 `(org, space)` 模型（`os_memory.rs` 自陈这是唯一有真实数据、迁移有代价的分区）；
- org 在 daemon 启动时为 `LOCAL_USER` 解析一次，**没有运行时换 org 的受支持路径**；
- 没有 `mem_spaces` 注册表、没有角色/策略——今天的隔离是「你的键或什么都没有」，那是分区，不是访问控制。

这四条是 F8c/F9 的活，与 ME-3 正交。ME-3 取 (b) 既不加速也不拖慢它们。

---

## 4. 生命周期与「安装」这件事的实情

**先纠正一处初稿的事实错误。** 初稿写了 `os install / activate / deactivate / uninstall`。真实的 CLI（`rust/apps/agent24-cli/src/main.rs` 的 `OsAction`）只有：

```
agent24 os list      # 列出 daemon 认识的每个领域 OS 及其处置
agent24 os enable    # 打开一个（下次 daemon 启动生效）
agent24 os disable   # 关掉一个（下次 daemon 启动生效）
```

`install/activate` 是 SPEC-MD-ME §4 描述的**意图**，ME-2 没有交付。更要紧的是：**catalogue 目前是编译进 daemon 的**（`server.rs` 里的 `let catalogue = vec![crate::domain::Installed { … }]`）。

这两条合起来意味着一件事：**如果 ME-3 只做「进程监督 + 代理」，把 mock 模块塞进 catalogue，测试会全绿，而装一个新的第三方 OS 仍然要改源码重编。** 那就是本仓最熟悉的那个毛病——验收比机制强。

所以 **ME-3a 必须交付「发现」这一层**（见 §8）：包目录/registry schema、从磁盘读 manifest、重名处理、安装/卸载的原子性，以及 CLI 行为。若判定这一层太大要拆走，那就必须**同时**把总验收改成诚实的说法（「安装＝人工复制文件并改配置」），不能留着「装第三方 OS 零改内核」这句而不交付它。

运行时：

```
enable + 发现       →  校验 manifest（impl_kind: out-of-process）+ 记录 spawn 命令
daemon 启动          →  spawn 子进程 → 等它连上回调 socket 并 initialize
                        → 健康探测通过 → 挂代理路由
（运行中）            →  子进程崩溃 → 该命名空间换成内核的 503 → 按退避重启（有熔断）
disable / 停机       →  ①原子撤销 generation → 拒绝新 RPC → 取消/等待在途
                        ②关 socket → 发停止信号 → 宽限期 → 杀**整个进程组** → 摘路由
```

> **注意这张图与今天的 CLI 不一致。** `agent24 os disable` 现在只改配置，**下次 daemon 启动才生效**（`os_routes.rs`）。上图的「热 disable」是 ME-3 的**新行为**，必须作为 ME-3 的交付项写明并补竞态测试，不能当成既有的生命周期语义顺手用。
>
> **撤销顺序是安全性质，不是清理顺序**：先撤 generation 拒新 RPC，再关 socket、再杀进程——不能等进程真的死了才撤权，否则宽限期里它还能写。

**一个模块挂了不能带走内核**（在 §0 的边界内：它没在 fork 炸弹或写坏共享库）。ME-1 已为进程内定了这条（`open_store` 返回 `Err` 时挂 503 而不是崩溃）。

**目录条目独立于模块是否在线而持久。** migration 0012 已经做到，`SPEC-ME-FOLLOWUPS` 明确要求 ME-3 不要退回内存态：离线不等于卸载。

---

## 5. 资源上限：不止记忆 RPC

初稿只写了记忆回调的三条，撑不住「一个模块挂掉不影响内核」这句验收。完整清单：

**记忆回调**
- **真配额**，不是地板。`SPEC-ME-FOLLOWUPS` F7 说得直白：单条记忆的大小上限**不是配额**，模块可以用任意多次合法的小写入把共享库撑大。进程内这条被降级为待办；进程外是必须项——按分区的字节/行数上限 + 写入限流，超限返回明确错误。
- **请求与响应双向上限**。只限请求不够：一次 `recall` 可以合法地要求返回很多，响应侧没有上限，内核就替一个不可信模块把自己撑爆了。
- **显式 page size + cursor，可取消**。注意是「在构造超大响应之前限制」，不是「构造完再截断」——初稿的措辞是后者，那是错的。连接断开即取消。

**代理侧**
- 请求/响应的 header 与 body 上限；首字节与总时限；并发上限与背压；
- 无尽 chunked / SSE / WebSocket：**本轮明确拒绝**（见 §10 公开问题 2），而不是含糊放行。

**进程侧**
- stdout/stderr 有上限并限流（一个刷屏的模块不能把磁盘写满）；
- spawn 时清理继承的 FD 与环境变量（否则 token、密钥会漏给子进程）；
- 停止时杀整个**进程组**，处理孙进程残留；
- 健康检查超时、崩溃退避、**最大重启率熔断**（快速崩溃循环不能变成 spawn 风暴）。

**后台路径特有的（连接授权是长寿命的，这一栏不是配额的附注）**
- **每次 spawn 轮换令牌**，不是「实现时定」。单连接 / 单 generation：旧进程、孙进程、断线重连者不得与新实例同时持权。
- **限流桶不能被崩溃重置**：以稳定的 `(module, partition)` 为键、跨连接跨 restart generation 生效，否则模块可以靠自崩溃绕过。
- **读路径也要限流**：后台反复 `recall` 会持续制造 SQLite I/O、CPU 与响应分配；只有分页/响应上限不够。
- **写入幂等性**：DB 已提交但响应丢失时，后台重试会写出第二条记忆——内核今天每次都 mint 新 ULID。需要模块可提供的幂等键，或明确「至少一次」并让模块自己去重（选哪个要写死）。
- **daemon 停机顺序**：callback listener 先停收、旧 socket 清除、在途 RPC 的终止或提交语义定死；新 daemon 必须拒绝旧令牌。
- **daemon 不在线时的重连风暴**：SDK 侧指数退避 + 抖动 + 有界队列。

**仍防不住的**：CPU / 内存 / fork / 磁盘耗尽——没有 OS 沙箱就防不住（§0）。所以 §8 里 ME-3b 的「内核不受影响」与 ME-3d 的「撑不爆内核」都是 **broker 层的**性质，不是绝对的。

---

## 6. 审批：相关性不等于门

模块处理被代理的请求时可能需要审批（比如 Cos72 要动一笔社区资金）。链路是：

```
用户 → 内核（已鉴权）→ 代理 → 模块 → 回调 _a24/approval/request → 内核 → 用户批准 → ???
```

**初稿在这里是错的。** 它只说「带上 `X-A24-Request-Id` 就能归因」。相关性 id 回答的是**「这是哪次请求」**——但一个相关性句柄证明不了因果与 payload 完整性。模块仍然可以：

- 拿 A 操作的租约去为 B 操作请求审批；
- 同一个租约重放，反复要审批；
- 拿到批准后，**执行与审批内容不同的操作**；
- 被拒绝后照样自己执行。

而这条教训本仓**已经付过学费**：现有审批模型把 `run_id + tool_call_id + payload` 绑在一起（`rust/crates/agent24-protocol/src/types.rs`），恢复路径还专门重建 payload 来拒绝「批准 A、执行 B」（`rust/crates/agent24-agent/src/resume.rs`）。ME-3 不能把它丢掉。

**所以**：

1. 审批对象绑定的是**内核规范化后的动作类型、目标、以及不可变 payload 的摘要**，不只是一个相关性 ID。
2. 租约的一次性/可重复使用规则要定死；过期、跨请求、跨连接一律拒绝。
3. **真要成为「门」，批准之后必须由内核执行该动作**，或者内核发一个**一次性、且只对该 payload 有效**的操作凭据。
4. 做不到 3 的话，就**如实叫它「审批 UX / 建议」，不能叫门**——因为模块可以无视结果自己执行（§0：它本来就能直接动数据库）。

> **取 (b) 之后，审批的授权基础要重说一遍（否则文档自相矛盾：裁决说租约推迟，审批却要求持租约）。**
>
> 租约在多用户设计里回答的是「代表**哪个 principal** 请示」。本轮只有一个 OS 级 principal（`LOCAL_USER`），所以**这个窄问题**确实不存在，审批不需要 principal 租约。
>
> **但上一版就停在这里，那是错的。** 「单用户」不等于「单请求来源」：
>
> - 现有审批模型要求 `run_id`（必填）+ `tool_call_id`，可选 `session_id` / `schedule_id`（`rust/crates/agent24-policy/src/lib.rs`）；持久化的 `Approval` 同样强制 `run_id + tool_call_id + payload`（`agent24-protocol/src/types.rs`）。
> - **`run_id` 就是「问谁」的路由键**：微信桥有多个 allowlist 用户、每人一条独立 session，靠 `a.run_id === result.runId` 把审批送回**正确的那个人**（`packages/wechat-bridge/src/bridge.ts`）；Nostr 桥按 npub 分 session 同理。
>
> 所以一个**不查内核映射**的相关性 id 只是模块的自报——它可以来自后台、来自一个已经结束的旧请求，或者在同一个 provider 的并发请求之间被串换。
>
> **本轮要交付的是内核侧的 `RequestContext` 映射**（不是 principal 租约）：内核在代理每个入站请求时登记
>
> ```text
> request_id → { module, generation, 该请求是否仍在活跃, run/session/source(若有) }
> ```
>
> 审批回调必须用它查回上下文；**陈旧的、跨连接的、请求已结束的一律拒绝**。这条不做，审批就没有「由某次代理请求引出」这个保证，只剩下 payload 摘要——那只挡住「批准 A、执行 B」，挡不住「拿一个过期的上下文去要审批」。
>
> **一个必须现在说明、我不替你裁的缺口**：被代理的请求**未必有 `run_id`**——它来自外壳直接调 `/api/v1/<ns>/*`，不是一次 agent run。而内核现有的 `Approval` 类型 `run_id` 是必填。二选一（§10 公开问题）：给模块审批一个自己的类型/表，或为它铸一个合成 run。**不能**含糊过去,否则实现时会有人随手塞一个假 run_id。
>
> **多用户上线时还要再收紧一次**：那时 principal 租约变成必需，叠在 `RequestContext` 之上。那是语义收紧不是加法，现在写下来免得那天被原样沿用。
>
> 与 ACP 的 `session/request_permission` 是同一件事的两个语境：那边归因到 session，这边归因到 request + payload。ADR-031 只核到语义层，**字段级契合度仍未验证**（§10）。

---

## 7. F4e：`os enable` 会从「今天不可达」变成现实问题

`SPEC-ME-FOLLOWUPS` F4e 记着：`PATCH` 放行 `Refused` 条目——`os enable <会被准入拒绝的模块>` 返回成功并落盘，但不存在任何能让它生效的路径。当时判断「今天不可达（生产 catalogue 只有 sin90 且可准入），**ME-3 会让它变成现实问题**」。

它到期了。进程外模块会因为缺依赖、版本不符、清单非法、spawn 目标不存在而被拒，而 `enable` 仍会写下一个永远不会生效的启用位。**ME-3 必须在启用路径上做准入校验**：被拒的模块 `enable` 返回错误且**不落盘**。

---

## 8. 交付 / 测试 / 验收

| ID | 交付 | 依赖 | 测试 | 验收 |
|---|---|---|---|---|
| **ME-3a** | **发现与安装层**：包目录 + registry schema、从磁盘读 manifest、重名处理、安装/卸载原子性、CLI 行为；manifest 支持 `impl_kind: out-of-process` + spawn 命令 | ME-2 | 非法 manifest / 重名 / spawn 目标不存在 / spawn 参数非法 全部被拒且不落盘 | catalogue 不再是编译进去的 |
| **ME-3b** | 受约束代理（§2）+ 进程监督（起/停/崩溃退避/熔断/进程组终止） | ME-3a | mock 后端**回显它实际收到的 header**，断言 bearer 与客户端伪造的 `X-A24-*` 都看不见；无 token 访问模块路由 401；越命名空间的 `Location` 不被跟随；**恶意模块在响应里回显 `X-A24-Request-Lease` / 任意 `X-A24-*`,客户端侧收不到**；子进程被 kill 后自动重启、期间该命名空间 503（不是 500/挂起）；快速崩溃触发熔断 | 模块看不到内核凭据；一个模块挂掉不带走内核（broker 层） |
| **ME-3c** | 回调通道：**NDJSON framing + 单行上限**、令牌握手（每次 spawn 轮换）、`initialize` 能力协商（offer set = `{Memory, Events, Approval}`，实际 grants 取 `manifest 请求 ∩ offer`） | ME-3b | 超长行在解析前被拒并断连；未授予的能力对应方法稳定返回 forbidden；模块报的 manifest 摘要与内核不符时握手失败；**版本协商矩阵**,四格各有写死的结果:模块版本在内核支持区间内 → 协商出 `min(模块上限, 内核上限)`;模块版本**高于**内核上限 → 内核回自己的上限,由模块决定降级还是断开(不是内核默默按高版本继续);模块版本**低于**内核下限 → 握手失败并返回内核支持的区间(不是崩,也不是继续);模块**不报版本** → 视为低于下限。协商结果能表达「本 daemon 不提供 `memory.scoped` 这个方法族」;错误形状与 JSON-RPC error code 一并定死；**params 解析失败固定返回 `-32602 Invalid params` 且不 dispatch handler**;**一条坏 params 只失败该行 RPC,连接继续处理下一行**(只有 framing 超限才断连);**重复的 JSON object key 被拒**(否则先解析成 Map 会丢掉「这个字段出现过几次」这一事实) | 实现者不需要猜 framing,也不需要猜版本不匹配时会怎样 |
| **ME-3d** | 记忆回调:**本轮只实现 `private/*`**(只认连接);`scoped/*` 保留方法名空间但不实现(§3 裁决 b) + 真配额 + 双向上限 + page size/cursor + 可取消 | ME-3c | **`private/*` 收到租约/scope 字段必须*报错*,不是忽略**(§3 留门第 1 条,本项最重要);**`scoped/*` 返回稳定的 method-not-found,不 fallback 到 `private/*`**;**`_meta` 里夹带 `org`/`space`/`lease` 不产生任何效果**（`_meta` 是唯一宽容的位置,因此是唯一可能的夹带路径）;**旧 manifest 的 `memory` 只被解析/映射成 `memory.private` entitlement**(门 5 本轮可验的那半;需要「新 daemon 支持 scoped + 有效租约」的那半随门 4 一起 deferred);后台任务能写 `private/*`；超配额写入返回明确错误；响应按 page size 分页（不是先构造再截断）；断连即取消 | 模块影响不了分区键 |
| **ME-3e** | 事件回调（只能发自己 `event_module`）+ 审批（**本轮不持租约**,绑定动作 + payload 摘要 + `X-A24-Request-Id` 相关性,§6;并按 §3 选定 (a) 加进程内 `ApprovalRequester` 或 (b) 拆 `ProviderCapability`） | ME-3c | 没在 manifest 里请求 `approval` 的模块拿不到审批（升级 daemon 不会凭空授予）；发别人模块的事件被拒；**为 A 请求审批、批准后改 payload 再执行被拒**；重放被拒；批准/拒绝的执行路径符合 §6 第 3 条（或文档如实降级为「建议」） | 「批准 A、执行 B」不可能 |
| **ME-3f** | **仓外** mock Provider 包 + 端到端 | ME-3a–e | **黑盒**：先构建 daemon；之后生成并安装一个**仓库之外**的 mock 包；不改源码、不重新构建，重启后完成挂载 → 路由代理 → 事件转发 → 记忆读写 → 审批 往返全绿 | **装第三方 OS 零改内核**（这一条只有黑盒测法算数） |
| **ME-3g** | F4e：启用路径做准入校验 | ME-3a | 准入被拒的模块 `enable` 返回错误且不落盘 | 不再有「启用了但永不生效」的条目 |

### 五条门的验收覆盖 —— 如实，不是「各有一条否定用例」

初稿在验收列里写过「四条 MUST 各有一条否定用例」。**核过之后那句是空话**，改成这张表：

| 门 | 本轮覆盖 | 说明 |
|---|---|---|
| 1. `private/*` 拒绝租约/scope 字段 | ✅ ME-3d | `deny_unknown_fields` 使其成为解析期失败;另加 `_meta` 夹带否定用例 |
| 2. `scoped/*` 不 fallback | ✅ ME-3d | 稳定 method-not-found |
| 3. `initialize` 带版本协商 | ✅ ME-3c（本轮补上） | 初稿只测了超长行/未授予能力/摘要不符,**没有任何版本负例**——已补版本矩阵 |
| 4. 租约按每请求建、用独立的秘密头 | ⚠️ **本轮无法验收，标 F8c/F9 deferred** | 本轮不签发租约,「过期/请求结束/跨连接被拒」只能靠测试伪造租约表——**那正是 §3 刚否掉的「测试证明生产不存在的性质」。不计入 ME-3d。** 本轮只验一件事:协议里 `X-A24-Request-Lease` 这个位置存在且与 `X-A24-Request-Id` 是两个头 |
| 5. `scoped` 是独立请求独立授予的能力 | 🟡 **本轮只验一半** | 可验:旧 manifest 的 `memory` 只映射成 `memory.private`。不可验:「新 daemon 支持 scoped + 有效租约 ⇒ 仍 forbidden」——两条都是 deferred 的生产路径,随门 4 一起标 F8c/F9 |
| 6. manifest/capability schema 的版本与兼容 | ✅ ME-3a/3c | 老 daemon + 新 manifest 在**解析期**就失败,早于握手 —— 所以要验的是 manifest 的最低版本字段与未知 capability 的稳定错误形状,不是握手协商 |

**总验收**：ME-3f 的黑盒往返通过，**且** ME-3d/3e 的否定用例全绿。只跑通「路由代理 + 事件转发」不算 ME-3 完成——身份绑定、审批绑定、配额是本设计提升为 MUST 的。

---

## 9. 做不到什么（逐条，不是遗漏）

- **不是沙箱，不防敌意本地二进制。** 见 §0 的完整清单：直接读写数据库、窃取同用户凭据、CPU/内存/FD/磁盘耗尽、ptrace 同用户进程——一条都不防。
- **不做模块签名与信任根。** 那是 ME-6（ADR-016 阶段 3，P4 门后）。在此之前，装一个进程外模块 = 信任它的作者，与 `curl | sh` 同级。**而且签名也不提供运行时隔离**，别把它当沙箱的替代。
- **不做跨空间回调。** `_a24/memory/scoped/*`、租约的签发与兑换、多用户并发与 shared space 的语义,整体推迟到 F8c/F9(§3 的裁决)。本轮范围是单用户 / home org / 模块私有空间。留门的**五条** MUST 见 §3(第 5 条:`scoped` 将来必须独立请求独立授予,否则老 `memory` grant 的含义会被扩大 —— 那不是加法)。
- **后台任务只能写自己的私有分区。** 定时/异步任务走连接授权，碰得到 `(启动 org, os:<module>)`，碰不到任何别的空间——那需要请求租约，而后台任务没有（§3）。
- **不做 `Models` / `Scheduler` / `Policy` 能力的回调。** 本轮授予集是 `{Memory, Events, Approval}`。这几个进程内都还没有稳定消费者，先有消费者再有提供者。
- **不做流式代理。** SSE / chunked / WebSocket 本轮拒绝，不含糊放行。
- **不做跨机。** Unix socket，本地进程。
- **不做模块间通信。** 模块之间不互相可见，一切经内核——直连会把所有权模型架空。
- **不做热更新。** 换版本 = disable + 重启 + enable。

---

## 10. 公开问题

1. **审批门的字段级契合度**：ADR-031 只核到语义层。要与 ACP 的 `session/request_permission` 对齐字段得逐字段比一遍——或裁定「只对齐语义，不追字段」。
2. **流式响应**：§9 本轮拒绝。等真有需要流式的领域 OS 再定，届时要一起想清楚超时、背压与取消。
3. **配额默认值**：§5 说必须有真配额，但**具体数值需要一张用量表**（F7 也卡在这）。ME-3d 先给保守默认 + 可配置，并把「值是猜的」写进注释。
4. ~~**本轮做不做 `scoped/*`**~~ —— **已裁决(2026-09-08,用户):取 (b),本轮只做 `private/*`**,留门**五条**见 §3。审批随之改为本轮不持租约(§6)。
5. **模块审批与内核 `Approval` 类型的对不上**（§6）：被代理的请求未必有 `run_id`（它来自外壳直调模块路由，不是一次 agent run），而内核现有的 `ApprovalRequest` / `Approval` 都把 `run_id` 当必填。给模块审批一个自己的类型/表，还是为它铸一个合成 run？**必须裁，不能含糊**——含糊的结果就是实现时随手塞一个假 `run_id`，而 `run_id` 今天是渠道桥把审批送回正确用户的路由键。
6. **写入幂等**：模块可提供幂等键，还是明确「至少一次」由模块自己去重（§5）。
7. **崩溃退避与熔断参数**：需要一个真实的第三方模块才知道合理值。先与 F2 一致，不另立。

---

> **给复审者的三句话**
>
> 1. **§0 是全文的前提**。初稿没有它，于是「不可信模块」「隔离」「不可伪造」这些词写得比机制强——而模块与内核同 UID、无沙箱。任何一处安全表述若脱离了「模块不绕过 broker」这个前缀，就是又犯了一次。
> 2. **§3 的租约**是初稿改出来的：初稿写「身份来自连接」，而连接只定得出 `module` 一维，`(org, space)` 定不出来。用连接去定 scope 是类型错误。若还有任何路径让模块影响分区键（过期租约、跨请求复用、跨连接、`_meta` 夹带），ADR-030 的所有权模型就被绕过了。反过来，**把租约要求施加到模块自己的私有分区上是过严**，且进程内并不这么要求——两种授权的分界见 §3 的表。
> 3. **§6 的审批**同样是改出来的：相关性 id 给的是相关性，不是因果与 payload 完整性;而相关性本身也必须由内核的 `RequestContext` 映射查证,不能采信模块自报。本仓在 `types.rs` / `resume.rs` 已经为「批准 A、执行 B」付过学费，ME-3 不能重新丢掉它。
