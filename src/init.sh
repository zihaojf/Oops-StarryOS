#!/bin/sh
# StarryOS init script.
#
# On a normal StarryOS root filesystem this just starts a login shell.
# On the OS competition test image (which has no `/bin/sh`, only `/musl` and
# `/glibc` test suites) it prepares a minimal runtime environment, runs the
# `*_testcode.sh` test points serially and finally powers the machine off.
#
# Test points that are known to be too slow or unsupported in the current
# implementation (lmbench, ltp) are skipped.  Everything is best-effort: a
# failing test point does not stop the remaining ones.

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

prepare_environment

# Fast functional test points.  These are run for every libc variant first so
# that a slow benchmark in a later pass cannot prevent them from running.
FAST_ORDER="basic_testcode.sh lua_testcode.sh libcbench_testcode.sh libctest_testcode.sh busybox_testcode.sh cyclictest_testcode.sh"
# Benchmarks that complete quickly enough.  Network tests are run before the
# file-system benchmark: on LoongArch64 the iozone test currently hangs, so it
# is skipped there (parallel timeouts do not work due to a timer issue).
if [ "$ARCH" = "loongarch64" ]; then
    MEDIUM_ORDER="iperf_testcode.sh netperf_testcode.sh"
    LONG_ORDER=""
else
    MEDIUM_ORDER="iperf_testcode.sh netperf_testcode.sh iozone_testcode.sh"
    # The long UnixBench run goes last; it is optional.
    LONG_ORDER="unixbench_testcode.sh"
fi

# Shut down eventually even if a test point hangs in an unkillable state.
# (Sleep/timers are unreliable on the current LoongArch64 port, so the watchdog
# is only installed on RISC-V.)
WATCHDOG_PID=""
if [ -n "$BB" ] && [ "$ARCH" != "loongarch64" ]; then
    ( $BB sleep 1800; echo "#### StarryOS: watchdog timeout ####"; $BB poweroff ) &
    WATCHDOG_PID=$!
fi

for d in $TESTDIRS; do
    for s in $FAST_ORDER; do
        [ -f "$d/$s" ] && run_testcode "$d" "$s" 240
    done
done

for d in $TESTDIRS; do
    for s in $MEDIUM_ORDER; do
        [ -f "$d/$s" ] && run_testcode "$d" "$s" 240
    done
done

for d in $TESTDIRS; do
    for s in $LONG_ORDER; do
        [ -f "$d/$s" ] && run_testcode "$d" "$s" 600
    done
done

echo "#### StarryOS: all test points finished ####"

if [ -n "$WATCHDOG_PID" ]; then
    $BB kill "$WATCHDOG_PID" 2>/dev/null
fi

$BB sync
# Exiting init lets the kernel unmount the root filesystem and power off.
exit 0
