#!/bin/sh
# StarryOS init script.
#
# On a normal StarryOS root filesystem this just starts a login shell.
# On the OS competition test image (which has no `/bin/sh`, only `/musl` and
# `/glibc` test suites) it prepares a minimal runtime environment, runs the
# `*_testcode.sh` test points serially and finally powers the machine off.
#
# Test points that are known to be too slow or unsupported in the current
# implementation (lmbench) are skipped.  LTP is enabled and runs last, because
# it is 2800+ independent cases.  Everything is best-effort: a failing test
# point does not stop the remaining ones.

# 目前关闭了ltp
# 提交时注意该脚本打开了哪些测试点
# 执行某个特定测试点可以编辑下方的Test_Only

export HOME=/root
export PATH=/bin:/usr/bin:/sbin:/usr/sbin:/glibc:/musl
export TERM=linux

# The busybox binaries shipped in the competition image.
BB=""
for p in /musl/busybox /glibc/busybox /busybox; do
    if [ -x "$p" ]; then
        BB="$p"
        break
    fi
done

# Directories that contain test scripts.
TESTDIRS=""
for d in / /glibc /musl; do
    for f in "$d"/*_testcode.sh; do
        if [ -f "$f" ]; then
            TESTDIRS="$TESTDIRS $d"
            break
        fi
    done
done

if [ -z "$TESTDIRS" ]; then
    # No test image: behave like a normal init.
    echo -e "Welcome to \e[96m\e[1mStarry OS\e[0m!"
    cd /root 2>/dev/null || cd /
    exec /bin/sh --login
fi

# ---------------------------------------------------------------------------
# Competition mode
# ---------------------------------------------------------------------------

echo "#### StarryOS: OS competition test runner ####"
echo "#### StarryOS: interpreter $0, busybox ${BB:-none} ####"

prepare_environment() {
    if [ -z "$BB" ]; then
        return
    fi

    $BB mkdir -p /bin /lib /lib64

    # Scripts are executed through `/bin/sh`, and several test scripts carry a
    # `#!/bin/sh` line.  Provide a small static shell.
    for dst in /bin/sh /bin/busybox; do
        if [ ! -x "$dst" ]; then
            $BB cat "$BB" > "$dst"
            $BB chmod 755 "$dst"
        fi
    done

    # The LTP driver script (ltp_testcode.sh) shells out to `basename` to name
    # each case, and the competition image only ships a single busybox binary
    # with no /bin applets -- and that busybox may not even provide the
    # `basename` applet.  A tiny sh wrapper is cheaper and always works.
    if [ ! -x /bin/basename ]; then
        $BB cat > /bin/basename <<'BASENAME'
#!/bin/sh
p="${1%/}"
p="${p##*/}"
[ -n "$2" ] && p="${p%"$2"}"
echo "$p"
BASENAME
        $BB chmod 755 /bin/basename
    fi
    if [ ! -x /bin/dirname ]; then
        $BB cat > /bin/dirname <<'DIRNAME'
#!/bin/sh
p="${1%/}"
case "$p" in
    */*) echo "${p%/*}" ;;
    *)   echo "." ;;
esac
DIRNAME
        $BB chmod 755 /bin/dirname
    fi

    # LTP shell cases call cat/ls/grep/sed/... directly; the image ships only
    # the single busybox binary, so install its applets as /bin/<name> links.
    $BB --install -s /bin 2>/dev/null || :

    # Several LTP cases call getpwnam("nobody"); the competition image has no
    # /etc at all, so they die with TBROK before touching the kernel.
    $BB mkdir -p /etc
    if [ ! -f /etc/passwd ]; then
        $BB cat > /etc/passwd <<'PASSWD'
root:x:0:0:root:/root:/bin/sh
nobody:x:65534:65534:nobody:/nonexistent:/bin/sh
PASSWD
    fi
    if [ ! -f /etc/group ]; then
        $BB cat > /etc/group <<'GROUP'
root:x:0:
nobody:x:65534:
GROUP
    fi

    # glibc test binaries are dynamically linked.  Their interpreter is
    # `/lib/ld-linux-*.so.1` (riscv64) or `/lib64/ld-linux-loongarch-*.so.1`
    # (loongarch64), so copy it where the loader expects it and point the
    # library search path at the shipped glibc.
    if [ -d /glibc/lib ]; then
        for f in /glibc/lib/ld-linux-*.so.1; do
            [ -f "$f" ] || continue
            n=${f##*/}
            for d in /lib /lib64; do
                if [ ! -x "$d/$n" ]; then
                    $BB cp "$f" "$d/$n" 2>/dev/null || $BB cat "$f" > "$d/$n"
                    $BB chmod 755 "$d/$n"
                fi
            done
        done
    fi

    # The musl test binaries are dynamically linked against
    # `/lib/ld-musl-riscv64-sf.so.1` (riscv64) or
    # `/lib64/ld-musl-loongarch-lp64d.so.1` (loongarch64).  For musl, libc.so
    # itself is the dynamic loader, so install a copy under the expected name.
    if [ -f /musl/lib/libc.so ]; then
        case "$ARCH" in
            loongarch64) interp=/lib64/ld-musl-loongarch-lp64d.so.1 ;;
            *)           interp=/lib/ld-musl-riscv64-sf.so.1 ;;
        esac
        if [ ! -x "$interp" ]; then
            $BB cp /musl/lib/libc.so "$interp" 2>/dev/null || $BB cat /musl/lib/libc.so > "$interp"
            $BB chmod 755 "$interp"
        fi
    fi
}

# Run a test point.  The working directory must be the directory containing
# the script, because the scripts reference their tools with relative paths.
run_testcode() {
    dir="$1"
    script="$2"
    limit="$3"

    cd "$dir" 2>/dev/null || return
    case "$dir" in
        /musl) export LD_LIBRARY_PATH=/musl/lib ;;
        *)     export LD_LIBRARY_PATH=/glibc/lib ;;
    esac
    # LTP shell cases source their helpers as bare names (`tst_net.sh`,
    # `cgroup_lib.sh`, ...), so the directory holding those helpers must be on
    # PATH -- otherwise every .sh case dies with "not found".
    if [ -d "$dir/ltp/testcases/bin" ]; then
        PATH="$dir/ltp/testcases/bin:$dir/ltp/bin:$PATH"
    fi
    export PATH
    echo "#### StarryOS: running $dir/$script ####"
    if [ -n "$BB" ]; then
        $BB timeout -s KILL "$limit" /bin/sh "./$script"
    else
        /bin/sh "./$script"
    fi
    st=$?
    echo "#### StarryOS: finished $dir/$script (status $st) ####"
}

ARCH=$($BB uname -m 2>/dev/null)
echo "#### StarryOS: arch $ARCH ####"

# --- test-selection:begin ---
# 测试点选择：TEST_ONLY 白名单，TEST_SKIP 黑名单（skip 优先），留空 = 全部。
# token 用空格分隔，支持 shell 通配：
#   basic lua libctest ... 套件（也可写 basic_testcode.sh）
#   musl / glibc           libc 变体
#   basic-musl             套件 + 变体
# 例：TEST_ONLY="basic busybox" TEST_SKIP="iozone"
# TEST_ONLY="basic"
# 不评分的套件显式关闭：unixbench / lmbench 不在评分表里，ltp 已从在线评测移除。
# 它们本来也不在下面的 run_pass 清单里，这里是第二道保险。
TEST_SKIP="unixbench lmbench ltp"

# $1 是否命中 $2 里某个 token（token 可为通配）
match() {
    for _p in $2; do
        case "$1" in
            $_p) return 0 ;;
        esac
    done
    return 1
}

# <dir> <script>：测试脚本要不要跑
script_wanted() {
    _v=${1##*/}
    _s=${2%_testcode.sh}
    [ "$_s" = "$2" ] && _s=${2%.sh}
    for _t in "$2" "$_s" "$_s-$_v" "$_v-$_s" "$_v"; do
        match "$_t" "$TEST_SKIP" && return 1
    done
    [ -z "$TEST_ONLY" ] && return 0
    for _t in "$2" "$_s" "$_s-$_v" "$_v-$_s" "$_v"; do
        match "$_t" "$TEST_ONLY" && return 0
    done
    return 1
}

# <dir>：LTP 这一轮要不要跑 (保留函数定义供调试用，但在线评测不再调用)
ltp_wanted() {
    _v=${1##*/}
    for _t in ltp "ltp-$_v" "$_v"; do
        match "$_t" "$TEST_SKIP" && return 1
    done
    [ -z "$TEST_ONLY" ] && return 0
    for _t in ltp "ltp-$_v" "$_v"; do
        match "$_t" "$TEST_ONLY" && return 0
    done
    case " $TEST_ONLY " in
        *" ltp/"*) return 0 ;;
    esac
    return 1
}

# <case>：单个 LTP 用例要不要跑
case_wanted() {
    match "$1" "$TEST_SKIP" && return 1
    match "ltp/$1" "$TEST_SKIP" && return 1
    _cases=""
    for _t in $TEST_ONLY; do
        case "$_t" in
            ltp/*) _cases="$_cases ${_t#ltp/}" ;;
        esac
    done
    [ -n "$_cases" ] || return 0
    match "$1" "$_cases"
}

echo "#### StarryOS: tests only=\"${TEST_ONLY:-all}\" skip=\"${TEST_SKIP:-none}\" ####"
# --- test-selection:end ---

prepare_environment

# Suites are ordered by "value per second, most fragile last".
# lmbench scores 0 on all environments and is very slow → omitted.
# LTP is no longer part of the online judge → not called.
#
#   1. fast, reliable              basic / busybox / lua / libcbench
#   2. expensive but high value    libctest (600 s), cyclictest
#   3. cheap network suites        iperf / netperf
#   4. slow benchmarks, last       iozone (600 s)
#
# `run_pass <per-script timeout> <scripts...>` keeps the passes uniform.
run_pass() {
    _limit=$1
    shift
    for d in $TESTDIRS; do
        for s in "$@"; do
            [ -f "$d/$s" ] && script_wanted "$d" "$s" && run_testcode "$d" "$s" "$_limit"
        done
    done
}

# Watchdog on both architectures.  The online judge enforces a hard 2-hour
# (7200 s) wall-clock limit across BOTH architectures run sequentially.
# Budget per architecture:
#   basic/busybox/lua/libcbench : ~4×240 s  = 960 s  (2 arches × 2 variants each)
#   libctest                    : ~2×600 s  = 1200 s
#   cyclictest                  : ~2×240 s  = 480 s
#   iperf/netperf               : ~2×240 s  = 480 s
#   iozone                      : ~2×600 s  = 1200 s  (cut from 900→600 to fit)
#   ----------                               --------
#   Total (2 arches)                         ~4320 s (~72 min) with margin
#
# Per-architecture watchdog: 3300 s for LA (slower), 2800 s for RV (faster).
# This gives a ~400 s safety margin vs the 7200 s hard limit.
WATCHDOG_PID=""
if [ -n "$BB" ]; then
    if [ "$ARCH" = "loongarch64" ]; then
        WATCHDOG_SECS=3300
    else
        WATCHDOG_SECS=2800
    fi
    (
        $BB sleep "$WATCHDOG_SECS"
        echo "#### StarryOS: watchdog timeout after ${WATCHDOG_SECS}s ####"
        $BB poweroff -f
    ) &
    WATCHDOG_PID=$!
fi

run_pass 240 basic_testcode.sh busybox_testcode.sh lua_testcode.sh libcbench_testcode.sh
run_pass 600 libctest_testcode.sh
run_pass 240 cyclictest_testcode.sh
run_pass 240 iperf_testcode.sh netperf_testcode.sh
# iozone timeout cut to 600 s (was 900 s) to reliably fit within the 2-hour
# judge limit when running 4 environments (2 arches × 2 libc variants).
# lmbench currently scores 0 everywhere and is very slow, so it is omitted.
run_pass 600 iozone_testcode.sh

# LTP 已从在线评测评分中移除，不再调用。
# run_ltp() 函数和 for d in $TESTDIRS loop 已删除。

echo "#### StarryOS: all test points finished ####"

if [ -n "$WATCHDOG_PID" ]; then
    $BB kill "$WATCHDOG_PID" 2>/dev/null
fi

$BB sync
# Exiting init lets the kernel unmount the root filesystem and power off.
exit 0
