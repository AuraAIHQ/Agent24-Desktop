#!/usr/bin/env bash
# ME-3 每一刀的状态 —— 由探针决定，不由人手写。
#
# 存在的理由：这份状态表上一版是手写的，在合并那一刻已经有三行是错的
# （PR 已合但表里写「复审中」、已交付的刀写「未开工」）。写的时候三行都对；
# 问题是没有任何东西让它们会过期，于是读者会照着一份陈旧的表去挑活，
# 重做一件已经交付的事 —— 正是那份文档存在所要避免的。
#
# 判据（复审 @ #165 定的）：改完之后，「这一行还准不准」必须是一条别人能跑的
# 命令，而不是一次阅读。所以这里每一刀都配一个**代码探针**（某个符号/文件在不在
# main 上），而不是一个状态字段。
#
#   用法：bash docs/agent/me3-status.sh
#
# 探针查的是代码，不是 PR 状态 —— PR 会关会改，代码在不在是事实。
set -u
cd "$(dirname "$0")/../.." || exit 1

# 正对照：一个必定存在的探针和一个必定不存在的探针。没有这一格，
# 「全部未开工」与「探针全坏了」是同一个读数。
probe() { # probe <描述> <文件> <符号|-)
  local desc=$1 file=$2 sym=${3:--}
  if [ ! -f "$file" ]; then echo "  ○ 未开工   $desc"; return 1; fi
  if [ "$sym" != "-" ] && ! grep -q -- "$sym" "$file"; then
    echo "  ○ 未开工   $desc"; return 1
  fi
  echo "  ● 已在 main $desc"; return 0
}

echo "ME-3 状态（探针读的是 $(git rev-parse --short HEAD) 这棵树）"
echo
probe "3a   发现与安装"          rust/crates/agent24-os-packages/src/install.rs "pub fn install"
probe "3b-1 framing"             rust/crates/agent24-os-proto/src/frame.rs       "pub fn read_frame"
probe "3b-2a 版本协商"           rust/crates/agent24-os-proto/src/version.rs     "pub fn negotiate"
probe "3b-2b initialize 线格式"  rust/crates/agent24-os-proto/src/initialize.rs  "pub fn accept"
probe "3b-3 manifest spawn 字段" rust/crates/agent24-domain/src/lib.rs           "pub struct SpawnCommand"
probe "3b-3 spawn + 进程监督"    rust/crates/agent24-os-proto/src/supervise.rs   "-"
probe "3b-4 受约束代理"          rust/crates/agent24-os-proto/src/proxy.rs       "-"
probe "3b-5 两阶段热 disable"    rust/crates/agent24-os-proto/src/drain.rs       "-"
probe "3c   回调通道其余部分"    rust/crates/agent24-os-proto/src/rpc.rs         "-"
probe "3d   记忆回调"            rust/crates/agent24-os-proto/src/memory.rs      "-"
probe "3e   事件 + 审批"         rust/crates/agent24-os-proto/src/events.rs      "-"
probe "3f   仓外包端到端"        rust/tests/me3f_blackbox.rs                     "-"
probe "3g   启用路径准入"        rust/apps/agent24d/src/domain.rs                "fn admit_on_enable"
echo
echo "验收(3f)未通过之前,「支持第三方 OS」只是一句声称。"
echo
echo "--- 探针自证(没有这一格,「全部未开工」与「探针全坏了」是同一个读数) ---"
probe "正对照:必定存在"  rust/crates/agent24-domain/src/lib.rs "pub struct DomainOsManifest" >/dev/null \
  && echo "  ✓ 已知存在的符号被探到" || echo "  ✗ 探针坏了:已知存在的符号没探到"
probe "负对照:必定不存在" rust/crates/agent24-domain/src/lib.rs "NoSuchSymbolEverXYZ" >/dev/null \
  && echo "  ✗ 探针坏了:不存在的符号被探到" || echo "  ✓ 不存在的符号未被探到"
