//! Rewrite the physical addresses and entry point of a kernel ELF image so that
//! QEMU's `-kernel` loader can start it directly.
//!
//! The kernel is linked at its high-half virtual address (`kernel-base-vaddr`),
//! but the boot code expects to be loaded at the low physical address
//! (`kernel-base-paddr`).  `llvm-objcopy -O binary` achieves this implicitly for
//! raw images; this tool keeps the result a valid ELF file instead.
//!
//! Usage: mkphyself <input> <output> <virt-to-phys-offset> <physical-entry>

use std::{env, fs, process::exit};

fn read_u16(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(data[off..off + 2].try_into().unwrap())
}

fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(data[off..off + 4].try_into().unwrap())
}

fn read_u64(data: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(data[off..off + 8].try_into().unwrap())
}

fn write_u64(data: &mut [u8], off: usize, value: u64) {
    data[off..off + 8].copy_from_slice(&value.to_le_bytes());
}

fn parse_num(text: &str) -> u64 {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u64::from_str_radix(&hex.replace('_', ""), 16).unwrap_or_else(|_| {
            eprintln!("invalid hex number: {text}");
            exit(1);
        })
    } else {
        text.replace('_', "").parse().unwrap_or_else(|_| {
            eprintln!("invalid number: {text}");
            exit(1);
        })
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 5 {
        eprintln!("usage: {} <input> <output> <virt-to-phys-offset> <physical-entry>", args[0]);
        exit(1);
    }

    let input = &args[1];
    let output = &args[2];
    let offset = parse_num(&args[3]);
    let entry = parse_num(&args[4]);

    let mut data = fs::read(input).unwrap_or_else(|e| {
        eprintln!("failed to read {input}: {e}");
        exit(1);
    });

    if data.len() < 64 || &data[0..4] != b"\x7fELF" || data[4] != 2 || data[5] != 1 {
        eprintln!("{input}: not a little-endian ELF64 file");
        exit(1);
    }

    let e_phoff = read_u64(&data, 0x20) as usize;
    let e_phentsize = read_u16(&data, 0x36) as usize;
    let e_phnum = read_u16(&data, 0x38) as usize;

    let mut changed = 0;
    for i in 0..e_phnum {
        let ph = e_phoff + i * e_phentsize;
        if ph + 56 > data.len() {
            eprintln!("{input}: malformed program header table");
            exit(1);
        }
        // p_type == PT_LOAD
        if read_u32(&data, ph) == 1 {
            let paddr = read_u64(&data, ph + 24);
            let vaddr = read_u64(&data, ph + 16);
            if paddr < offset {
                eprintln!("{input}: PT_LOAD paddr {paddr:#x} is below the offset {offset:#x}");
                exit(1);
            }
            write_u64(&mut data, ph + 24, paddr - offset);
            // The `.percpu` output section intentionally has virtual address 0
            // while its load address is at the end of the kernel image; keep it
            // that way.  Everything else is moved to its physical address so
            // that QEMU's `-kernel` loader works no matter whether it places
            // segments by virtual or by physical address.
            if vaddr != 0 {
                if vaddr < offset {
                    eprintln!("{input}: PT_LOAD vaddr {vaddr:#x} is below the offset {offset:#x}");
                    exit(1);
                }
                write_u64(&mut data, ph + 16, vaddr - offset);
            }
            changed += 1;
        }
    }

    if changed == 0 {
        eprintln!("{input}: no PT_LOAD segments found");
        exit(1);
    }

    // e_entry
    write_u64(&mut data, 0x18, entry);

    fs::write(output, &data).unwrap_or_else(|e| {
        eprintln!("failed to write {output}: {e}");
        exit(1);
    });
}
