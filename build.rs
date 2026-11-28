use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=scripts/lmbench_hello.c");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    for libc in ["glibc", "musl"] {
        let output = out.join(format!("lmbench-hello-{libc}"));
        let compiler = match (arch.as_str(), libc) {
            ("riscv64", "glibc") => "riscv64-linux-gnu-gcc",
            ("riscv64", "musl") => "riscv64-linux-musl-gcc",
            ("loongarch64", "glibc") => "loongarch64-linux-gnu-gcc",
            ("loongarch64", "musl") => "loongarch64-linux-musl-gcc",
            _ => {
                fs::write(output, []).unwrap();
                continue;
            }
        };
        let status = Command::new(compiler)
            .args([
                "-O2",
                "-static",
                "-s",
                "-ffunction-sections",
                "-fdata-sections",
                "-Wl,--gc-sections",
                "scripts/lmbench_hello.c",
                "-o",
            ])
            .arg(&output)
            .status()
            .unwrap_or_else(|err| panic!("cannot build lmbench hello with {compiler}: {err}"));
        assert!(status.success(), "{compiler} failed to build lmbench hello");
    }
}
