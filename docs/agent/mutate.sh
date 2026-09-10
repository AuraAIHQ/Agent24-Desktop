#!/usr/bin/env bash
# 变异测试脚手架 —— 它自己先要被验过。
#
# 用法：
#   source docs/agent/mutate.sh
#   mut_baseline <crate> [测试过滤]        # 先立基线,拿到它才允许开始
#   mut <文件> <锚点> <替换> <标签>
#
# ── 这个脚本存在的理由,以及它拦的四类假结论 ──────────────────────
#
# 变异测试的产出是「测试有没有变红」。**每一种「没变红」都可能不是「判据不承重」**,
# 而是这四件事之一 —— 每一件都在实际的复审里发生过:
#
#   1. 锚点不存在 → 什么都没注入,而 `0 failed` 读起来和「没覆盖」一模一样。
#   2. 锚点不唯一 → 注入到了另一处(注释里那处),同样是「变异了但全绿」。
#   3. **纯插入**(替换里仍完整包含锚点)→ 原代码还在,行为没变。
#      **不改变行为的变异必然全绿,而那个绿和「没覆盖」分不开。**
#   4. **编译失败** → 拿到的是「编译不过」不是「测试红」。
#      **编译错误不是测试结果。**
#
# 还拦一种非假结论但同样致命的:**挂住**。没有超时的话,「挂住」和「还在跑」
# 是同一种表现 —— 一片安静。
#
# ── 一个试过并否定的做法,记在这里免得下一个人再试 ────────────────
#
# 曾想用「编译产物哈希是否变化」来自动识别第 3 类。**实测不成立**:加一个
# 没人用的 `const` 同样改变二进制哈希(基线 1324…、空操作 1bb3…、真变异 47c0…
# 三者互不相同)。哈希能证明「有变化」,证明不了「行为有变化」。改用第 3 条的
# 词法规则 —— 它拦不住所有空操作,但拦得住实际发生过的那一类,而且它不撒谎。
set -u

_MUT_CRATE=""; _MUT_FILTER=""; _MUT_BASE=""

# macOS 没有 timeout(1),自己写一个 —— 挂住必须报成「挂住」,不能报成安静。
_mut_tmo() {
  local secs=$1; shift
  "$@" & local p=$!
  ( sleep "$secs"; kill -9 "$p" 2>/dev/null ) & local k=$!
  wait "$p"; local rc=$?
  kill "$k" 2>/dev/null; wait "$k" 2>/dev/null
  [ $rc -ge 128 ] && return 124 || return $rc
}

_mut_run() { # → 打印 "PASS n" / "FAIL n" / "COMPILE" / "TIMEOUT"
  local out; out=$(mktemp)
  if _mut_tmo "${MUT_TIMEOUT:-150}" cargo test -p "$_MUT_CRATE" --lib $_MUT_FILTER >"$out" 2>&1; then
    :
  elif [ $? -eq 124 ]; then echo "TIMEOUT"; rm -f "$out"; return; fi
  # 编译失败与测试红必须分开:前者拿到的不是测试结果。
  if grep -qE "^error(\[E[0-9]+\])?: " "$out" && ! grep -qE "^test result:" "$out"; then
    echo "COMPILE"; rm -f "$out"; return
  fi
  local line; line=$(grep -E "^test result:" "$out" | head -1)
  rm -f "$out"
  case "$line" in
    *"result: ok"*) echo "PASS ${line}" ;;
    *) echo "FAIL ${line}" ;;
  esac
}

mut_baseline() { # mut_baseline <crate> [过滤]
  _MUT_CRATE=$1; _MUT_FILTER=${2:-}
  ( cd "$(git rev-parse --show-toplevel)/rust" 2>/dev/null || cd rust ) || return 1
  pushd "$(git rev-parse --show-toplevel)/rust" >/dev/null || return 1
  _MUT_BASE=$(_mut_run); popd >/dev/null
  case "$_MUT_BASE" in
    PASS*) printf "  %-44s %s\n" "基线(必须全绿,否则后面读数无意义)" "$_MUT_BASE" ;;
    *) printf "  ⛔ 基线不是全绿(%s) —— 停止,先修基线\n" "$_MUT_BASE"; return 1 ;;
  esac
}

mut() { # mut <文件> <锚点> <替换> <标签>
  local file=$1 anchor=$2 repl=$3 label=$4
  [ -n "$_MUT_BASE" ] || { echo "  ⛔ 未立基线,拒绝开始"; return 1; }
  local root; root=$(git rev-parse --show-toplevel)
  local bak; bak=$(mktemp)
  cp "$root/$file" "$bak"

  ANCHOR="$anchor" REPL="$repl" python3 - "$root/$file" <<'PY'
import io,os,sys
p=sys.argv[1]; a=os.environ["ANCHOR"]; b=os.environ["REPL"]
s=io.open(p,encoding='utf-8').read()
if a not in s:            print("锚点不存在"); sys.exit(1)
if s.count(a)>1:          print(f"锚点出现 {s.count(a)} 次,不唯一"); sys.exit(1)
if a==b:                  print("替换与锚点相同"); sys.exit(1)
if a in b:                print("纯插入:替换里仍完整包含锚点,原代码还在 → 行为未必变"); sys.exit(1)
io.open(p,'w',encoding='utf-8').write(s.replace(a,b,1))
PY
  local rc=$?
  if [ $rc -ne 0 ]; then
    printf "  %-44s ⛔ 注入自证失败,不读测试结果\n" "$label"
    cp "$bak" "$root/$file"; rm -f "$bak"; return 0
  fi

  local r; pushd "$root/rust" >/dev/null; r=$(_mut_run); popd >/dev/null
  cp "$bak" "$root/$file"; rm -f "$bak"
  case "$r" in
    FAIL*)    printf "  %-44s 🔴 %s\n" "$label" "${r#FAIL }" ;;
    PASS*)    printf "  %-44s 🟢 存活(判据不承重) %s\n" "$label" "${r#PASS }" ;;
    COMPILE)  printf "  %-44s ⛔ 编译失败 —— 不是测试红,本格作废\n" "$label" ;;
    TIMEOUT)  printf "  %-44s ⏱ 挂住 —— 不是测试结果,去看它挂在哪\n" "$label" ;;
  esac
}
