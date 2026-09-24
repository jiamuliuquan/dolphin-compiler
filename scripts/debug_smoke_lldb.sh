#!/usr/bin/env bash
# H20-05 调试器验收（规格 §9.2）：macOS lldb 上的等价实现。
# 用法、输出格式与退出码见 scripts/debug_smoke.sh（macOS 走本脚本）。
set -u

if [ "$#" -ne 3 ]; then
    echo "usage: debug_smoke_lldb.sh <exe> <break-file> <break-line>" >&2
    exit 2
fi

exe=$1
break_file=$2
break_line=$3

if ! command -v lldb >/dev/null 2>&1; then
    echo "debugger not available: lldb not found" >&2
    exit 2
fi

raw=$(mktemp)
normalized=$(mktemp)
trap 'rm -f "$raw" "$normalized"' EXIT

# 程序可能带非零退出码（DBG-04 的 Release 会跑完），结论以下方标记为准。
lldb -b \
    -o "breakpoint set --file ${break_file} --line ${break_line}" \
    -o "run" \
    -o "source list --line ${break_line}" \
    -o "next" \
    -o "source list" \
    -o "bt" \
    -- "$exe" >"$raw" 2>&1 || true

# 归一化：地址与进程号换成稳定占位符，去掉机器相关的启动噪声。
sed -E \
    -e 's/0x[0-9a-fA-F]+/0xADDR/g' \
    -e 's/process [0-9]+/process PID/g' \
    "$raw" \
    | grep -v -E '^(\(lldb\) )' \
    >"$normalized" || true
cat "$normalized"

if grep -q 'stop reason = breakpoint' "$raw"; then
    echo "debug smoke: breakpoint hit"
    exit 0
fi
if grep -q -e 'no locations' -e 'invalid target' "$raw"; then
    echo "debug smoke: no line table (breakpoint not hit)"
    exit 0
fi
echo "debug smoke: unexpected debugger output" >&2
exit 1
