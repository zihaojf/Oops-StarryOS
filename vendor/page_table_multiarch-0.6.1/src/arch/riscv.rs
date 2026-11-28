//! RISC-V specific page table structures.

use memory_addr::VirtAddr;
use page_table_entry::riscv::Rv64PTE;

use crate::{PageTable64, PageTable64Cursor, PagingMetaData};

/// A virtual address that can be used in RISC-V Sv39 and Sv48 page tables.
pub trait SvVirtAddr: memory_addr::MemoryAddr + Send + Sync {
    /// Flush the TLB.
    fn flush_tlb(vaddr: Option<Self>);
    /// Flush an update for this root; other translation schemes keep their own policy.
    fn flush_root(_root: memory_addr::PhysAddr, va: Option<Self>) {
        Self::flush_tlb(va);
    }
}

impl SvVirtAddr for VirtAddr {
    #[inline]
    fn flush_tlb(vaddr: Option<Self>) {
        if let Some(vaddr) = vaddr {
            unsafe { core::arch::asm!("sfence.vma {va}, zero", va = in(reg) vaddr.as_usize()); }
        } else {
            riscv::asm::sfence_vma_all();
        }
    }

    #[cfg(target_arch = "riscv64")]
    fn flush_root(root: memory_addr::PhysAddr, va: Option<Self>) {
        if crate::local_asid::enabled()
            && let Some(va) = va
            && va.as_usize() < (1usize << 38)
        {
            riscv::asm::sfence_vma(crate::local_asid::tag(root), va.as_usize());
        } else {
            Self::flush_tlb(va);
        }
    }
}

/// Metadata of RISC-V Sv39 page tables.
pub struct Sv39MetaData<VA: SvVirtAddr> {
    _virt_addr: core::marker::PhantomData<VA>,
}

/// Metadata of RISC-V Sv48 page tables.
pub struct Sv48MetaData<VA: SvVirtAddr> {
    _virt_addr: core::marker::PhantomData<VA>,
}

impl<VA: SvVirtAddr> PagingMetaData for Sv39MetaData<VA> {
    const LEVELS: usize = 3;
    const PA_MAX_BITS: usize = 56;
    const VA_MAX_BITS: usize = 39;

    type VirtAddr = VA;

    #[cfg(target_arch = "riscv64")]
    fn root_allocated(root: memory_addr::PhysAddr) {
        crate::local_asid::root_allocated(root);
    }
    fn flush_root(root: memory_addr::PhysAddr, va: Option<VA>) {
        VA::flush_root(root, va);
    }


    #[inline]
    fn flush_tlb(vaddr: Option<VA>) {
        <VA as SvVirtAddr>::flush_tlb(vaddr);
    }
}

impl<VA: SvVirtAddr> PagingMetaData for Sv48MetaData<VA> {
    const LEVELS: usize = 4;
    const PA_MAX_BITS: usize = 56;
    const VA_MAX_BITS: usize = 48;

    type VirtAddr = VA;

    #[inline]
    fn flush_tlb(vaddr: Option<VA>) {
        <VA as SvVirtAddr>::flush_tlb(vaddr);
    }
}

/// Sv39: Page-Based 39-bit (3 levels) Virtual-Memory System.
pub type Sv39PageTable<H> = PageTable64<Sv39MetaData<VirtAddr>, Rv64PTE, H>;
/// Sv39 page table cursor.
pub type Sv39PageTableCursor<'a, H> = PageTable64Cursor<'a, Sv39MetaData<VirtAddr>, Rv64PTE, H>;

/// Sv48: Page-Based 48-bit (4 levels) Virtual-Memory System.
pub type Sv48PageTable<H> = PageTable64<Sv48MetaData<VirtAddr>, Rv64PTE, H>;
/// Sv48 page table cursor.
pub type Sv48PageTableCursor<'a, H> = PageTable64Cursor<'a, Sv48MetaData<VirtAddr>, Rv64PTE, H>;
