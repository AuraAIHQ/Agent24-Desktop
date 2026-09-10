#!/usr/bin/env bash
# ME-3 每一刀的状态 —— 由探针决定，不由人手写。
#
# 存在的理由：这份状态表上一版是手写的，在合并那一刻已经有三行是错的
# （PR 已合但表里写「复审中」、已交付的刀写「未开工」）。写的时候三行都对；
# 问题是没有任何东西让它们会过期。
#
# 判据（复审 @ #165 定的）：「这一行还准不准」必须是一条别人能跑的命令，
# 而不是一次阅读。
#
#   用法：bash docs/agent/me3-status.sh
#
# ── 探针的失效方向，以及为什么它比那张表更值得小心 ────────────────────
#
# 第一版探针有 7 行只查「文件在不在」、6 行用纯子串 grep。复审量出三格：
# 一个只有一行注释的 `supervise.rs` 报「已在 main」；把真的 `pub fn accept`
# 改名、文件头留一句 `// TODO: 这里以后会有 pub fn accept`，同样报「已在 main」。
#
# **而「建文件、写一句 TODO 点名将来那个函数」恰好是开工最自然的第一步。**
#
# 这个失效方向比旧表更糟：旧表错在「已交付却写未开工」→ 有人重做一件已完成的
# 事，浪费，但**去写的时候就会发现代码已经在那儿**；探针错在「未开工却报已
# 交付」→ 有人**跳过**这一刀，一直到 3f 的黑盒验收才暴露。
#
# 所以：**每一刀都必须有符号（没有只查文件在不在的行），符号必须行首锚定
# （注释里提到不算），并且下面的自证要覆盖这两种形态本身。**
set -u
cd "$(dirname "$0")/../.." || exit 1

# 行首锚定：`^[[:space:]]*<符号>` —— 一个定义在行首（可缩进），
# 一句 `// TODO: 将来会有 pub fn accept` 不在行首。
probe() { # probe <描述> <文件> <符号>
  local desc=$1 file=$2 sym=$3
  if [ ! -f "$file" ] || ! grep -qE "^[[:space:]]*${sym}\b" "$file" 2>/dev/null; then
    echo "  ○ 未开工   $desc"; return 1
  fi
  echo "  ● 已在 main $desc"; return 0
}

# 坐标：探针读的是**工作树**，不是 HEAD。第一版打印 `git rev-parse HEAD` 当
# 坐标，于是一个未提交的新文件会让某一行翻成 ●，而表头仍写着那个 commit ——
# **它报的坐标不是它量的坐标**，正是这整件事要修的那一类。
coord=$(git rev-parse --short HEAD 2>/dev/null || echo "?")
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
  coord="${coord} (+未提交改动：读的是工作树，不是这个 commit)"
fi
echo "ME-3 状态（探针读的是 ${coord}）"
echo

probe "3a   发现与安装"          rust/crates/agent24-os-packages/src/install.rs "pub fn install"
probe "3b-1 framing"             rust/crates/agent24-os-proto/src/frame.rs       "pub fn read_frame"
probe "3b-2a 版本协商"           rust/crates/agent24-os-proto/src/version.rs     "pub fn negotiate"
probe "3b-2b initialize 线格式"  rust/crates/agent24-os-proto/src/initialize.rs  "pub fn accept"
probe "3b-3 manifest spawn 字段" rust/crates/agent24-domain/src/lib.rs           "pub struct SpawnCommand"
probe "3b-3 解析+起进程"         rust/crates/agent24-os-proto/src/launch.rs      "pub fn spawn"
probe "3b-3 进程监督"            rust/crates/agent24-os-proto/src/supervise.rs   "pub struct Supervisor"
probe "3b-4 受约束代理"          rust/crates/agent24-os-proto/src/proxy.rs       "pub fn proxy_router"
probe "3b-5 两阶段热 disable"    rust/crates/agent24-os-proto/src/drain.rs       "pub enum DrainState"
probe "3c   回调通道其余部分"    rust/crates/agent24-os-proto/src/rpc.rs         "pub fn dispatch"
probe "3d   记忆回调"            rust/crates/agent24-os-proto/src/memory.rs      "pub fn handle_memory"
probe "3e   事件 + 审批"         rust/crates/agent24-os-proto/src/events.rs      "pub fn handle_event"
probe "3f   仓外包端到端"        rust/apps/agent24d/tests/me3f_blackbox.rs       "fn a_package_from_outside_the_repo"
probe "3g   启用路径准入"        rust/apps/agent24d/src/domain.rs                "fn admit_on_enable"
echo
echo "验收(3f)未通过之前,「支持第三方 OS」只是一句声称。"
echo "探针只回答「代码在不在」,回答不了「有没有生产调用方」—— 🟢 与 ✅ 的区别仍要人判断。"
echo
echo "--- 探针自证 ---"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
say() { [ "$1" = "$2" ] && echo "  ✓ $3" || echo "  ✗ 探针坏了:$3(得到 $2,应为 $1)"; }

probe "x" rust/crates/agent24-domain/src/lib.rs "pub struct DomainOsManifest" >/dev/null
say 0 $? "已知存在的符号被探到"

probe "x" rust/crates/agent24-domain/src/lib.rs "pub struct NoSuchSymbolEverXYZ" >/dev/null
say 1 $? "存在的文件里、不存在的符号 → 未开工"

# 复审量出的两种形态,各一格。没有这两格,上面那两格挡不住它们:
# 正对照查的是真符号,负对照查的是不存在的符号 —— 都没覆盖「文件在但是空壳」
# 和「符号只在注释里」。
printf '// TODO\n' > "$tmp/shell.rs"
probe "x" "$tmp/shell.rs" "pub fn something" >/dev/null
say 1 $? "空壳文件(只有一行注释) → 未开工"

printf '// TODO: 这里以后会有 pub fn something\n' > "$tmp/comment.rs"
probe "x" "$tmp/comment.rs" "pub fn something" >/dev/null
say 1 $? "符号只出现在注释里 → 未开工"

printf 'pub fn something() {}\n' > "$tmp/real.rs"
probe "x" "$tmp/real.rs" "pub fn something" >/dev/null
say 0 $? "同名符号真的定义了 → 已交付(证明上面两格不是靠「拒绝一切」通过的)"

printf '    pub fn indented() {}\n' > "$tmp/indent.rs"
probe "x" "$tmp/indent.rs" "pub fn indented" >/dev/null
say 0 $? "缩进的定义仍被探到(行首锚定不等于必须顶格)"
