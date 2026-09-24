#!/usr/bin/env bash
# H20-05 调试器验收（规格 §9.2）：用 gdb 批处理验证 LLVM Debug 产物的断点、
# 行表与调用栈。Cranelift/PDB 不在本阶段范围（DBG-04 记录边界）。
#
# 用法：scripts/debug_smoke.sh <exe> <break-file> <break-line>
#
# 输出（stdout；地址与进程号已归一化）：
#   - 调试器的断点设置/命中、断点后的 `info line`、单步（`next`）后的位置、
#     `info line` 与 `bt` 文本；
#   - 固定结论行：`debug smoke: breakpoint hit`
#     或 `debug smoke: no line table (breakpoint not hit)`。
#
# 退出码：0 = 调试器已运行并给出结论（含“无行表/未命中”）；
#         2 = 用法错误或未找到 gdb；1 = 其他错误。
set -u

if [ "$#" -ne 3 ]; then
    echo "usage: debug_smoke.sh <exe> <break-file> <break-line>" >&2
    exit 2
fi

exe=$1
break_file=$2
break_line=$3

if ! command -v gdb >/dev/null 2>&1; then
    echo "debugger not available: gdb not found" >&2
    exit 2
fi

raw=$(mktemp)
normalized=$(mktemp)
trap 'rm -f "$raw" "$normalized"' EXIT

# 程序可能带非零退出码（DBG-04 的 Release 会跑完），结论以下方标记为准。
gdb -q -batch \
    -ex "set pagination off" \
    -ex "set confirm off" \
    -ex "set debuginfod enabled off" \
    -ex "break ${break_file}:${break_line}" \
    -ex "run" \
    -ex "info line" \
    -ex "next" \
    -ex "info line" \
    -ex "bt" \
    --args "$exe" >"$raw" 2>&1 || true

# 归一化：地址与进程号换成稳定占位符，去掉机器相关的启动噪声。
sed -E \
    -e 's/0x[0-9a-fA-F]+/0xADDR/g' \
    -e 's/process [0-9]+/process PID/g' \
    "$raw" \
    | grep -v -E '^(\[Thread debugging|Using host libthread_db|warning: .debug_names|This GDB supports auto-downloading|Enable debuginfod|Debuginfod has been disabled|To make this setting permanent)' \
    >"$normalized" || true
cat "$normalized"

if grep -qE '^Breakpoint [0-9]+, ' "$raw"; then
    echo "debug smoke: breakpoint hit"
    exit 0
fi
if grep -q -e 'No line number information available' -e 'No symbol table is loaded' "$raw"; then
    echo "debug smoke: no line table (breakpoint not hit)"
    exit 0
fi
echo "debug smoke: unexpected debugger output" >&2
exit 1
