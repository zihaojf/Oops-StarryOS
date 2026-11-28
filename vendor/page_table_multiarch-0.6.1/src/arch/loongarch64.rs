//! LoongArch64 specific page table structures.

use core::arch::asm;

use crate::local_asid;
use memory_addr::{PhysAddr, VirtAddr};
use page_table_entry::loongarch64::LA64PTE;

use crate::{PageTable64, PageTable64Cursor, PagingMetaData};

/// Metadata of LoongArch64 page tables.
#[derive(Copy, Clone, Debug)]
pub struct LA64MetaData;

impl LA64MetaData {
    /// PWCL(Page Walk Controller for Lower Half Address Space) CSR flags
    ///
    /// <https://loongson.github.io/LoongArch-Documentation/LoongArch-Vol1-EN.html#page-walk-controller-for-lower-half-address-space>
    ///
    /// | BitRange | Name      | Value |
    /// | ----     | ----      | ----  |
    /// | 4:0      | PTBase    |    12 |
    /// | 9:5      | PTWidth   |     9 |
    /// | 14:10    | Dir1Base  |    21 |
    /// | 19:15    | Dir1Width |     9 |
    /// | 24:20    | Dir2Base  |    30 |
    /// | 29:25    | Dir2Width |     9 |
    /// | 31:30    | PTEWidth  |     0 |
    pub const PWCL_VALUE: u32 = 12 | (9 << 5) | (21 << 10) | (9 << 15) | (30 << 20) | (9 << 25);

    /// PWCH(Page Walk Controller for Higher Half Address Space) CSR flags
    ///
    /// <https://loongson.github.io/LoongArch-Documentation/LoongArch-Vol1-EN.html#page-walk-controller-for-higher-half-address-space>
    ///
    /// | BitRange | Name                            | Value |
    /// | ----     | ----                            | ----  |
    /// | 5:0      | Dir3Base                        |    39 |
    /// | 11:6     | Dir3Width                       |     9 |
    /// | 17:12    | Dir4Base                        |     0 |
    /// | 23:18    | Dir4Width                       |     0 |
    /// | 24       | 0                               |     0 |
    /// | 24       | HPTW_En(CPUCFG.2.HPTW(bit24)=1) |     0 |
    /// | 31:25    | 0                               |     0 |
    pub const PWCH_VALUE: u32 = 39 | (9 << 6);
}

impl PagingMetaData for LA64MetaData {
    const LEVELS: usize = 4;
    const PA_MAX_BITS: usize = 48;
    const VA_MAX_BITS: usize = 48;

    type VirtAddr = VirtAddr;

    fn root_allocated(root: PhysAddr) {
        local_asid::root_allocated(root);
    }

    fn flush_root(root: PhysAddr, vaddr: Option<VirtAddr>) {
        if !local_asid::enabled() {
            return Self::flush_tlb(vaddr);
        }
        // Rootless/kernel changes and shared kernel mappings need a full flush.
        // A low user VA must instead invalidate the tag of its *own* table,
        // including when COW or unmap modifies an inactive address space.
        match vaddr {
            Some(va) if va.as_usize() < (1usize << 47) && root.as_usize() != 0 => {
                let tag = local_asid::tag(root);
                unsafe {
                    asm!("dbar 0; invtlb 0x06, {tag}, {va}", tag=in(reg) tag, va=in(reg) va.as_usize());
                }
            }
            _ => Self::flush_tlb(None),
        }
    }

    #[inline]
    fn flush_tlb(vaddr: Option<VirtAddr>) {
        unsafe {
            if let Some(vaddr) = vaddr.filter(|_| !local_asid::enabled()) {
                // <https://loongson.github.io/LoongArch-Documentation/LoongArch-Vol1-EN.html#_dbar>
                //
                // Only after all previous load/store access operations are completely
                // executed, the DBAR 0 instruction can be executed; and only after the
                // execution of DBAR 0 is completed, all subsequent load/store access
                // operations can be executed.
                //
                // <https://loongson.github.io/LoongArch-Documentation/LoongArch-Vol1-EN.html#_invtlb>
                //
                // formats: invtlb op, asid, addr
                //
                // op 0x5: Clear all page table entries with G=0 and ASID equal to the
                // register specified ASID, and VA equal to the register specified VA.
                //
                // When the operation indicated by op does not require an ASID, the
                // general register rj should be set to r0.
                asm!("dbar 0; invtlb 0x05, $r0, {reg}", reg = in(reg) vaddr.as_usize());
            } else {
                // op 0x0: Clear all page table entries
                asm!("dbar 0; invtlb 0x00, $r0, $r0");
            }
        }
    }
}

/// loongarch64 page table
///
/// <https://loongson.github.io/LoongArch-Documentation/LoongArch-Vol1-EN.html#section-multi-level-page-table-structure-supported-by-page-walking>
///
/// 4 levels:
///
/// using page table dir3, dir2, dir1 and pt, ignore dir4
pub type LA64PageTable<H> = PageTable64<LA64MetaData, LA64PTE, H>;
/// loongarch64 page table cursor.
pub type LA64PageTableCursor<'a, H> = PageTable64Cursor<'a, LA64MetaData, LA64PTE, H>;
