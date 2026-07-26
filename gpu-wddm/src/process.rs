use core::{
    mem::MaybeUninit,
    num::NonZero,
    ops::{
        DerefMut,
        Deref,
    },
    pin::Pin,
    fmt,
    sync::atomic::{
        AtomicUsize,
        AtomicU8,
        AtomicU32,
        AtomicU64,
        AtomicBool,
        Ordering,
    },
    ptr::NonNull,
};

use alloc::{
    sync::{
        Arc,
        Weak,
    },
    vec::Vec,
    collections::BTreeMap,
};

use pin_init::*;
use winresult::STATUS;

use spin::rwlock::RwLock;

use crate::adapter::*;
use crate::device::*;
use crate::uapi::*;
use crate::function;

use modular_bitfield::prelude::*;

use wdk::dxgkrnl::{
    HANDLE,
    DXGK_PAGE_TABLE_LEVEL_DESC,
    DXGK_SEGMENTFLAGS,
    DXGK_SEGMENTDESCRIPTOR3,
    DXGK_SEGMENTDESCRIPTOR4,
};

const VIRTIO_GPU_PROCESS_TAG: u64 = u64::from_ne_bytes(*b"VGPUPROC");

#[derive(Specifier, Clone, Copy)]
#[bits = 2]
pub enum MemorySegment {
    ImplicitSystemMemory = 0,
    /* 3d resources / guest blobs */
    Aperture3D = 1,
    /* mappable blob resources (unused) */
    BlobMappable = 2,
    /* not mappable blob resources*/
    BlobHost3D = 3,
}

impl MemorySegment {
    pub const COUNT: u32 = 3;
    pub const SEGMENTS: [MemorySegment; Self::COUNT as usize] = [
        MemorySegment::Aperture3D,
        MemorySegment::BlobMappable,
        MemorySegment::BlobHost3D,
    ];

    #[inline]
    pub const fn mask(&self) -> u32 {
        1 << (self.index() as u32)
    }

    #[inline]
    pub const fn index(&self) -> u32 {
        match self {
            MemorySegment::ImplicitSystemMemory => unreachable!(),
            _ => *self as u32 - 1,
        }
    }

    #[inline]
    pub const fn phys(&self) -> u64 {
        match self {
            MemorySegment::ImplicitSystemMemory => unreachable!(),
            MemorySegment::Aperture3D   => 0x00C0000000,
            MemorySegment::BlobHost3D   => 0x0700000000,
            MemorySegment::BlobMappable => 0x2000000000,
        }
    }

    #[inline]
    pub const fn size(&self, shmem_len: u64) -> u64 {
        match self {
            MemorySegment::ImplicitSystemMemory => unreachable!(),
            MemorySegment::Aperture3D   => 1024 * 1024 * 1024,
            MemorySegment::BlobHost3D   => 16 * 1024 * 1024 * 1024,
            MemorySegment::BlobMappable => shmem_len,
        }
    }

    #[inline]
    pub fn flags(&self) -> DXGK_SEGMENTFLAGS {
        let mut flags: DXGK_SEGMENTFLAGS = unsafe { core::mem::zeroed() };

        match self {
            MemorySegment::ImplicitSystemMemory => unreachable!(),
            MemorySegment::Aperture3D => {
                flags.set_Aperture(true);
                flags.set_CacheCoherent(true);
                flags.set_DirectFlip(true);
            },
            MemorySegment::BlobHost3D => {
                flags.set_CacheCoherent(true);
                flags.set_CpuVisible(true);
                flags.set_DirectFlip(true);
            },
            MemorySegment::BlobMappable => {
                flags.set_CacheCoherent(true);
                flags.set_DirectFlip(true);
                // FIXME: this isn't really a CpuVisible segment, but allocating from it fails otherwise.
                // Apparently, VirtualBox authors encountered the same problem:
                // https://github.com/VirtualBox/virtualbox/blob/499d5a317f23448903c7662a38baf957de37ddf4/src/VBox/Additions/win/Graphics/Video/mp/wddm/VBoxMPWddm.cpp#L2339
                flags.set_CpuVisible(true);
            },
        };

        flags
    }

    pub fn fill_desc3(&self, shmem_start: u64, shmem_len: u64, desc: &mut DXGK_SEGMENTDESCRIPTOR3) {
        desc.BaseAddress.QuadPart = self.phys() as _;
        desc.Size = self.size(shmem_len) as _;
        desc.CommitLimit = self.size(shmem_len) as _;
        desc.Flags = self.flags();

        if matches!(self, MemorySegment::BlobMappable) {
            desc.CpuTranslatedAddress.QuadPart = shmem_start as _;
        }
    }

    pub fn fill_desc4(&self, shmem_start: u64, shmem_len: u64, desc: &mut DXGK_SEGMENTDESCRIPTOR4) {
        desc.BaseAddress.QuadPart = self.phys() as _;
        desc.Size = self.size(shmem_len) as _;
        desc.CommitLimit = self.size(shmem_len) as _;
        desc.Flags = self.flags();

        if matches!(self, MemorySegment::BlobMappable) {
            desc.__bindgen_anon_1.CpuTranslatedAddress.QuadPart = shmem_start as _;
        }
    }
}

#[bitfield(bytes = 8)]
pub struct PageTableEntry {
    pub valid: bool,
    #[bits = 2]
    pub segment: MemorySegment,
    pub address: B61,
}

pub const PAGE_TABLE_DESC: [DXGK_PAGE_TABLE_LEVEL_DESC; 3] = [
    DXGK_PAGE_TABLE_LEVEL_DESC {
        PageTableIndexBitCount: 12,
        PageTableSegmentId: 0,
        PagingProcessPageTableSegmentId: 0,
        PageTableSizeInBytes: 4096 * size_of::<PageTableEntry>() as u32,
        PageTableAlignmentInBytes: 4096,
    },
    DXGK_PAGE_TABLE_LEVEL_DESC {
        PageTableIndexBitCount: 12,
        PageTableSegmentId: 0,
        PagingProcessPageTableSegmentId: 0,
        PageTableSizeInBytes: 4096 * size_of::<PageTableEntry>() as u32,
        PageTableAlignmentInBytes: 4096,
    },
    DXGK_PAGE_TABLE_LEVEL_DESC {
        PageTableIndexBitCount: 12,
        PageTableSegmentId: 0,
        PagingProcessPageTableSegmentId: 0,
        PageTableSizeInBytes: 4096 * size_of::<PageTableEntry>() as u32,
        PageTableAlignmentInBytes: 4096,
    },
];

const _: () = assert!(core::mem::size_of::<PageTableEntry>() == 8);

#[repr(C)]
#[derive(Tagged)]
#[tagged(VIRTIO_GPU_PROCESS_TAG)]
pub struct Process {
    pub tag: u64,
    dxg_process: HANDLE,
    // alloc_va_map: RwLock<HashMap<D3DGPU_VIRTUAL_ADDRESS, Weak<Allocation>>>
    // TODO: page table
    // TODO: process name
}

impl Process {
    pub fn new(dxg_process: HANDLE) -> Result<Arc<Self>, NtStatus> {
        Ok(Arc::try_new(Self {
            tag: VIRTIO_GPU_PROCESS_TAG,
            dxg_process,
        })?)
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.tag = 0;
    }
}
