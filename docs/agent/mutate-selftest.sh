#!/usr/bin/env bash
# `mutate.sh` 自己的自证 —— 一个拦不住假结论的脚手架，会安静地把假结论当成读数。
#
#   bash docs/agent/mutate-selftest.sh
#
# 四类假结论各一格，外加一格对照（真变异必须红）。**对照是必要的**：没有它，
# 一个「拒绝一切」的脚手架也能通过上面四格。
set -u
cd "$(dirname "$0")/../.." || exit 1
source docs/agent/mutate.sh

F=rust/crates/agent24-os-proto/src/supervise.rs
mut_baseline agent24-os-proto supervise || exit 1

mut "$F" 'NO-SUCH-ANCHOR' 'x' "① 锚点不存在 → 应作废"
mut "$F" '    }' '    };' "② 锚点不唯一 → 应作废"
mut "$F" 'pub const REAP_TIMEOUT' 'const UNUSED_XYZ: u32 = 7;
pub const REAP_TIMEOUT' "③ 纯插入(空操作) → 应作废"
mut "$F" '        if self.consecutive >= BREAKER_THRESHOLD {' '        if self.consecutive >= BREAKER_THRESHOLD (' "④ 编译失败 → 应作废"
mut "$F" '        if self.consecutive >= BREAKER_THRESHOLD {' '        if false {' "⑤ 对照:真变异 → 必须红"

echo
echo "读法:①②③④ 必须是 ⛔,⑤ 必须是 🔴。任何一格不符,脚手架本身坏了,"
echo "     这一轮所有变异读数作废 —— 而不是去调这个自证。"
