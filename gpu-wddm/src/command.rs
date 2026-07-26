use core::{
    ptr::NonNull,
    mem::{
        size_of,
        transmute,
    },
    num::NonZero,
    sync::atomic::Ordering,
};

use alloc::{
    vec::Vec,
    sync::Arc,
    boxed::Box,
};

use bitflags::bitflags;
use virtio_drivers::device::gpu::*;
use zerocopy::*;

use wdk::{
    wdm::{
        MdlRef,
        PAGE_SIZE,
        mm_get_physical_address,
    },
    dxgkrnl::{
        DXGK_ALLOCATIONLIST,
        RECT,
    },
};

use smallvec::SmallVec;

use crate::adapter::*;
use crate::uapi::*;
use crate::device::*;
use crate::allocation::*;
use crate::virgl::*;
use crate::queue::{GpuChannel, AlignedBox};
use crate::{
    function,
    slice_from_raw_parts,
    slice_from_raw_parts_mut,
};
const VIRTIO_GPU_COMMAND_DMA_PRIVATE_TAG: u64 = u64::from_ne_bytes(*b"VGPUDMAC");

bitflags! {
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    struct Flags: u8 {
        const RING_IDX     = 1u8 << 0;
        const SHADOW_VIRGL = 1u8 << 1;
        const NEEDS_PATCH  = 1u8 << 2;
        //const HAS_FENCE    = 1u8 << 3;
        //const BLOB_CREATED = 1u8 << 4;
    }
}

#[repr(C)]
#[derive(Tagged, Debug)]
#[tagged(VIRTIO_GPU_COMMAND_DMA_PRIVATE_TAG)]
pub struct CommandDmaPrivate {
    pub tag: u64,
    pub commands: SmallVec<[Command; 3]>,
    pub allocations: SmallVec<[Arc<DeviceSpecificAllocation>; 2]>,
}

// Max available space is 128 right now
const _: () = assert!(size_of::<CommandDmaPrivate>() == 112);

impl Default for CommandDmaPrivate {
    fn default() -> Self {
        // Invalid tag here is on purpose
        Self {
            tag: crate::VIRTIO_GPU_INVALID_TAG,
            commands: SmallVec::new(),
            allocations: SmallVec::new(),
        }
    }
}

impl CommandDmaPrivate {
    pub fn new() -> Self {
        Self {
            tag: VIRTIO_GPU_COMMAND_DMA_PRIVATE_TAG,
            commands: SmallVec::new(),
            allocations: SmallVec::new(),
        }
    }

    //pub const fn from_const(cmd: Command) -> Self {
    //    Self {
    //        tag: VIRTIO_GPU_COMMAND_DMA_PRIVATE_TAG,
    //        commands: SmallVec::from_const([cmd]),
    //        allocations: SmallVec::new_const(),
    //    }
    //}

    pub fn attach_allocation(&mut self, alloc: Arc<DeviceSpecificAllocation>) {
        self.allocations.push(alloc);
    }
}

#[derive(Debug, Clone)]
pub struct Command {
    pub id: CommandId,                                         // 2 bytes
    flags: Flags,                                              // 1 byte
    ring: u8,                                                  // 1 byte
    dma: Option<NonNull<[u8]>>,                                // 16 bytes
    /* can fit 4 more bytes here */
}

const _: () = assert!(size_of::<Command>() == 24);

impl Command {
    pub fn body_dma_len(body: &CommandBody) -> usize {
        let item_size = match body {
            //CommandBody::Nop => size_of::<commands::CmdSubmit3d>(),
            CommandBody::Nop => 0,
            CommandBody::Fence(_) => unreachable!("CommandId::Fence cannot be submitted to host"),
            CommandBody::Submit(_3d) => size_of::<commands::CmdSubmit3d>() + _3d.len(),
            CommandBody::TransferToHost(_) | CommandBody::TransferFromHost(_) => size_of::<commands::TransferHost3d>(),
            //CommandBody::AllocationList(_) => unreachable!("this command is handled in guest"),
            //CommandBody::MapBlob(_) => size_of::<commands::ResourceMapBlob>(),
            //CommandBody::UnmapBlob(_) => size_of::<commands::ResourceUnmapBlob>(),
        };
        let item_count = body.len();

        item_size * item_count
    }

    pub fn item_dma_len(item: &CommandBodyItem) -> usize {
        match item {
            //CommandBodyItem::Nop => size_of::<commands::CmdSubmit3d>(),
            CommandBodyItem::Nop => 0,
            CommandBodyItem::Fence(_) => unreachable!("CommandId::Fence cannot be submitted to host"),
            CommandBodyItem::Submit(_3d) => size_of::<commands::CmdSubmit3d>() + _3d.len(),
            CommandBodyItem::TransferToHost(_) | CommandBodyItem::TransferFromHost(_) => size_of::<commands::TransferHost3d>(),
            //CommandBodyItem::MapBlob(_) => size_of::<commands::ResourceMapBlob>(),
            //CommandBodyItem::MapBlobAt(_, _) => size_of::<commands::ResourceMapBlob>(),
            //CommandBodyItem::UnmapBlob(_) => size_of::<commands::ResourceUnmapBlob>(),
        }
    }

    pub fn virgl_blit_dma_len(rects: &[RECT]) -> usize {
        size_of::<commands::CmdSubmit3d>() + size_of::<VirglResourceCopyRegion>() * rects.len()
    }

    pub fn virgl_blit(chan: &GpuChannel, context_id: Option<NonZero<u32>>, src: NonZero<u32>, dst: NonZero<u32>, delta: (i32, i32), rects: &[RECT], ring: Option<u8>, dma: &mut [u8]) -> Self {
        const HEADER_SIZE:    usize = size_of::<commands::CmdSubmit3d>();
        const BODY_ITEM_SIZE: usize = size_of::<VirglResourceCopyRegion>();

        let body_len = BODY_ITEM_SIZE * rects.len();

        let hdr = commands::CmdSubmit3d {
            header: chan.new_header(commands::Command::SUBMIT_3D, true, context_id, ring),
            size: body_len as _,
            _padding: 0,
        };

        let dma = &mut dma[..HEADER_SIZE+body_len];
        let (hdr_dma, body_dma) = dma.split_at_mut(HEADER_SIZE);

        hdr.write_to_prefix(hdr_dma).unwrap();

        for (i, rect) in rects.iter().enumerate() {
            let cmd = VirglResourceCopyRegion::new(
                dst.get(), 0, rect.top_left(),
                src.get(), 0, (*rect + delta).top_left(), rect.dimensions(),
            );

            cmd.write_to_prefix(&mut body_dma[i * BODY_ITEM_SIZE..]).unwrap();
        }

        let dma = Some(NonNull::from_mut(dma));
        Command::new(CommandId::Submit, ring, Flags::empty(), dma)
    }

    pub fn virgl_set_type_dma_len() -> usize {
        size_of::<commands::CmdSubmit3d>() + size_of::<VirglResourceSetType>()
    }

    pub fn virgl_set_type(chan: &GpuChannel, context_id: Option<NonZero<u32>>, id: NonZero<u32>, info: &BlobInfo, ring: Option<u8>, dma: &mut [u8]) -> Self {
        const HEADER_SIZE: usize = size_of::<commands::CmdSubmit3d>();
        const BODY_SIZE:   usize = size_of::<VirglResourceSetType>();

        let hdr = commands::CmdSubmit3d {
            header: chan.new_header(commands::Command::SUBMIT_3D, true, context_id, ring),
            size: BODY_SIZE as _,
            _padding: 0,
        };

        let dma = &mut dma[..HEADER_SIZE+BODY_SIZE];
        let (hdr_dma, body_dma) = dma.split_at_mut(HEADER_SIZE);

        hdr.write_to_prefix(hdr_dma).unwrap();

        let cmd = VirglResourceSetType::from_blob_info(id.get(), info);

        cmd.write_to_prefix(body_dma).unwrap();

        let dma = Some(NonNull::from_mut(dma));
        Command::new(CommandId::Submit, ring, Flags::empty(), dma)
    }


    pub fn virgl_get_resource_layout_dma_len() -> usize {
        size_of::<commands::CmdSubmit3d>() + size_of::<VirglGetResourceLayout>()
    }

    pub fn virgl_get_resource_layout(chan: &GpuChannel, context_id: NonZero<u32>, out: NonZero<u32>, target: NonZero<u32>, ring: Option<u8>, dma: &mut [u8]) -> Self {
        const HEADER_SIZE: usize = size_of::<commands::CmdSubmit3d>();
        const BODY_SIZE:   usize = size_of::<VirglGetResourceLayout>();

        let hdr = commands::CmdSubmit3d {
            header: chan.new_header(commands::Command::SUBMIT_3D, true, Some(context_id), ring),
            size: BODY_SIZE as _,
            _padding: 0,
        };

        let dma = &mut dma[..HEADER_SIZE+BODY_SIZE];
        let (hdr_dma, body_dma) = dma.split_at_mut(HEADER_SIZE);

        hdr.write_to_prefix(hdr_dma).unwrap();

        let cmd = VirglGetResourceLayout::new(out.get(), target.get());

        cmd.write_to_prefix(body_dma).unwrap();

        let dma = Some(NonNull::from_mut(dma));
        Command::new(CommandId::Submit, ring, Flags::empty(), dma)
    }

    pub fn from_item(chan: &GpuChannel, context_id: Option<NonZero<u32>>, item: &CommandBodyItem, ring: Option<u8>, dma: &mut [u8]) -> Self {
        let id = item.id();

        let (dma, flags) = match item {
            CommandBodyItem::Nop => {
                (None, Flags::empty())
                // /* DEBUG: submit nop as empty 3d command */
                // const HEADER_SIZE: usize = size_of::<commands::CmdSubmit3d>();
                //
                // let hdr = commands::CmdSubmit3d {
                //     header: adapter.queue_handler().unwrap().new_header(commands::Command::SUBMIT_3D, true, context_id, ring),
                //     size: 0,
                //     _padding: 0,
                // };
                //
                // let dma = &mut dma[..HEADER_SIZE];
                // hdr.write_to_prefix(dma).unwrap();
                //
                // (Some(NonNull::from_mut(dma)), Flags::empty())
            },
            CommandBodyItem::Submit(_3d) => {
                const HEADER_SIZE: usize = size_of::<commands::CmdSubmit3d>();

                let hdr = commands::CmdSubmit3d {
                    header: chan.new_header(commands::Command::SUBMIT_3D, true, context_id, ring),
                    size: _3d.len() as _,
                    _padding: 0,
                };

                let dma = &mut dma[..HEADER_SIZE+_3d.len()];
                let (hdr_dma, body_dma) = dma.split_at_mut(HEADER_SIZE);
                hdr.write_to_prefix(hdr_dma).unwrap();
                body_dma.copy_from_slice(_3d);

                (Some(NonNull::from_mut(dma)), Flags::empty())
            },
            CommandBodyItem::TransferToHost(transfer) => {
                const CMD_SIZE: usize = size_of::<commands::TransferHost3d>();

                let cmd = commands::TransferHost3d {
                    header: chan.new_header(commands::Command::TRANSFER_TO_HOST_3D, true, context_id, ring),
                    box_: transfer.r#box.into(),
                    offset: transfer.offset,
                    resource_id: transfer.res_id,
                    level: transfer.level,
                    stride: transfer.stride,
                    layer_stride: transfer.layer_stride,
                };

                let dma = &mut dma[..CMD_SIZE];
                cmd.write_to_prefix(dma).unwrap();

                (Some(NonNull::from_mut(dma)), Flags::empty())
            },
            CommandBodyItem::TransferFromHost(transfer) => {
                const CMD_SIZE: usize = size_of::<commands::TransferHost3d>();

                let cmd = commands::TransferHost3d {
                    header: chan.new_header(commands::Command::TRANSFER_FROM_HOST_3D, true, context_id, ring),
                    box_: transfer.r#box.into(),
                    offset: transfer.offset,
                    resource_id: transfer.res_id,
                    level: transfer.level,
                    stride: transfer.stride,
                    layer_stride: transfer.layer_stride,
                };

                let dma = &mut dma[..CMD_SIZE];
                cmd.write_to_prefix(dma).unwrap();

                (Some(NonNull::from_mut(dma)), Flags::empty())
            },
            CommandBodyItem::Fence(_) => {
                unreachable!("CommandId::Fence cannot be submitted to host");
            },
            //CommandBodyItem::MapBlob(map) => {
            //    const CMD_SIZE: usize = size_of::<commands::ResourceMapBlob>();
            //
            //    // This command needs to be patched to properly set up offset
            //    let cmd = commands::ResourceMapBlob {
            //        header: chan.new_header(commands::Command::RESOURCE_MAP_BLOB, true, context_id, ring),
            //        resource_id: map.res_id,
            //        _padding: 0,
            //        offset: 0,
            //    };
            //
            //    let dma = &mut dma[..CMD_SIZE];
            //    cmd.write_to_prefix(dma).unwrap();
            //
            //    (Some(NonNull::from_mut(dma)), Flags::NEEDS_PATCH)
            //},
            //CommandBodyItem::MapBlobAt(map, offset) => {
            //    const CMD_SIZE: usize = size_of::<commands::ResourceMapBlob>();
            //
            //    let cmd = commands::ResourceMapBlob {
            //        header: chan.new_header(commands::Command::RESOURCE_MAP_BLOB, true, context_id, ring),
            //        resource_id: map.res_id,
            //        _padding: 0,
            //        offset: *offset,
            //    };
            //
            //    let dma = &mut dma[..CMD_SIZE];
            //    cmd.write_to_prefix(dma).unwrap();
            //
            //    (Some(NonNull::from_mut(dma)), Flags::empty())
            //},
            //CommandBodyItem::UnmapBlob(map) => {
            //    const CMD_SIZE: usize = size_of::<commands::ResourceUnmapBlob>();
            //
            //    let cmd = commands::ResourceUnmapBlob {
            //        header: chan.new_header(commands::Command::RESOURCE_UNMAP_BLOB, true, context_id, ring),
            //        resource_id: map.res_id,
            //        _padding: 0,
            //    };
            //
            //    let dma = &mut dma[..CMD_SIZE];
            //    cmd.write_to_prefix(dma).unwrap();
            //
            //    (Some(NonNull::from_mut(dma)), Flags::empty())
            //},
        };

        Command::new(id, ring, flags, dma)
    }

    pub fn nop() -> Self {
        Command::new(CommandId::Nop, None, Flags::empty(), None)
    }

    pub const fn attach_backing_dma_len(n_pages: usize) -> usize {
        const _: () = assert!(size_of::<commands::ResourceAttachBacking>() == 32);
        const _: () = assert!(size_of::<commands::MemEntry>() == 16);

        size_of::<commands::ResourceAttachBacking>() + n_pages * size_of::<commands::MemEntry>()
    }

    pub fn attach_backing(chan: &GpuChannel, res_id: NonZero<u32>, mdl: MdlRef, offset: usize, n_pages: usize, dma: &mut [u8]) -> Self {
        const HEADER_SIZE: usize = size_of::<commands::ResourceAttachBacking>();

        let hdr = commands::ResourceAttachBacking {
            header: chan.new_header(commands::Command::RESOURCE_ATTACH_BACKING, true, None, None),
            resource_id: res_id.get(),
            nr_entries: n_pages as _,
        };

        let body_size = n_pages * size_of::<commands::MemEntry>();
        let dma = &mut dma[..HEADER_SIZE+body_size];
        let (hdr_dma, body_dma) = dma.split_at_mut(HEADER_SIZE);
        hdr.write_to_prefix(hdr_dma).unwrap();

        let entries = unsafe {
            let addr = transmute::<_, *mut commands::MemEntry>(body_dma.as_mut_ptr());
            slice_from_raw_parts_mut(addr, n_pages)
        };

        let phys_pages = &mdl.physical_pages()[offset..];
        for (i, phys_page) in phys_pages.iter().enumerate() {
            entries[i] = commands::MemEntry {
                addr: phys_page * (PAGE_SIZE as u64),
                length: PAGE_SIZE,
                _padding: 0,
            };
        };

        Command::new(CommandId::MapAperture, None, Flags::empty(), Some(NonNull::from_mut(dma)))
    }

    pub fn attach_backing_box(chan: &GpuChannel, res_id: NonZero<u32>, data: &AlignedBox<[u8]>, dma: &mut [u8]) -> Self {
        const HEADER_SIZE: usize = size_of::<commands::ResourceAttachBacking>();

        let n_pages = data.len().div_ceil(PAGE_SIZE as usize);

        let hdr = commands::ResourceAttachBacking {
            header: chan.new_header(commands::Command::RESOURCE_ATTACH_BACKING, true, None, None),
            resource_id: res_id.get(),
            nr_entries: n_pages as _,
        };

        let body_size = n_pages * size_of::<commands::MemEntry>();
        let dma = &mut dma[..HEADER_SIZE+body_size];
        let (hdr_dma, body_dma) = dma.split_at_mut(HEADER_SIZE);
        hdr.write_to_prefix(hdr_dma).unwrap();

        assert!(size_of::<commands::MemEntry>() * n_pages <= body_dma.len());

        let entries = unsafe {
            let addr = transmute::<_, *mut commands::MemEntry>(body_dma.as_mut_ptr());
            slice_from_raw_parts_mut(addr, n_pages)
        };

        for i in 0..n_pages {
            let vaddr = &data[i * (PAGE_SIZE as usize)..];
            let paddr = mm_get_physical_address(vaddr.as_ptr() as _);
            entries[i] = commands::MemEntry {
                addr: paddr,
                length: PAGE_SIZE,
                _padding: 0,
            };
        }

        Command::new(CommandId::MapAperture, None, Flags::empty(), Some(NonNull::from_mut(dma)))
    }

    pub fn attach_backing_virtual_dma_len(alloc: &Allocation) -> usize {
        let n_pages = alloc.num_attached_pages();
        Self::attach_backing_dma_len(n_pages)
    }

    pub fn attach_backing_virtual(chan: &GpuChannel, alloc: &Allocation, dma: &mut [u8]) -> Option<Self> {
        const HEADER_SIZE: usize = size_of::<commands::ResourceAttachBacking>();

        let n_pages = alloc.num_attached_pages();
        if n_pages == 0 {
            error!("{}: no attached pages for alloc {:?}", function!(), alloc);
            return None;
        }

        let hdr = commands::ResourceAttachBacking {
            header: chan.new_header(commands::Command::RESOURCE_ATTACH_BACKING, true, None, None),
            resource_id: alloc.id().unwrap().get(),
            nr_entries: n_pages as _,
        };

        let body_size = n_pages * size_of::<commands::MemEntry>();
        let dma = &mut dma[..HEADER_SIZE+body_size];
        let (hdr_dma, body_dma) = dma.split_at_mut(HEADER_SIZE);
        hdr.write_to_prefix(hdr_dma).unwrap();

        let entries = unsafe {
            let addr = transmute::<_, *mut commands::MemEntry>(body_dma.as_mut_ptr());
            slice_from_raw_parts_mut(addr, n_pages)
        };

        if !alloc.fill_attached_pages(entries) {
            error!("{}: failed to fill attached pages for alloc {:?}", function!(), alloc);
            return None;
        }

        Some(Command::new(CommandId::MapAperture, None, Flags::empty(), Some(NonNull::from_mut(dma))))
    }

    pub fn detach_backing_dma_len() -> usize {
        size_of::<commands::ResourceDetachBacking>()
    }

    pub fn detach_backing(chan: &GpuChannel, res_id: NonZero<u32>, dma: &mut [u8]) -> Self {
        const CMD_SIZE: usize = size_of::<commands::ResourceUnmapBlob>();

        let cmd = commands::ResourceDetachBacking {
            header: chan.new_header(commands::Command::RESOURCE_DETACH_BACKING, true, None, None),
            resource_id: res_id.get(),
            _padding: 0,
        };

        let dma = &mut dma[..CMD_SIZE];
        cmd.write_to_prefix(dma).unwrap();

        Command::new(CommandId::UnmapAperture, None, Flags::empty(), Some(NonNull::from_mut(dma)))
    }

    fn new(id: CommandId, ring: Option<u8>, flags: Flags, dma: Option<NonNull<[u8]>>) -> Self {
        let (ring, flags) = ring.map(|ring| (ring, Flags::RING_IDX | flags)).unwrap_or((0, flags));

        Self {
            //tag: VIRTIO_GPU_COMMAND_TAG,
            id,
            flags,
            ring,
            //dxgk_fence: 0,
            dma,
            //allocations: SmallVec::new(),
        }
    }

    pub fn patch(&mut self, allocations: &[DXGK_ALLOCATIONLIST]) {
        match self.id {
            //CommandId::MapBlob => {
            //    let dma = unsafe { self.dma.as_mut().unwrap().as_mut() };
            //    let cmd = commands::ResourceMapBlob::mut_from_prefix(dma).unwrap().0;
            //    let res_id = NonZero::new(cmd.resource_id).unwrap();
            //    let alloc = allocation_from_res_id(allocations, res_id).unwrap();
            //
            //    assert_eq!(alloc.SegmentId(), SEGMENT_ID_BLOB_MAPPABLE);
            //    let addr = unsafe { alloc.__bindgen_anon_2.PhysicalAddress.QuadPart } as u64;
            //    assert!(addr >= BLOB_MAP_SEGMENT_GPU_PADDR);
            //    let offset = addr - BLOB_MAP_SEGMENT_GPU_PADDR;
            //
            //    warn!("{}: patching offset for res_id {} to {}", function!(), res_id, offset);
            //    cmd.offset = offset;
            //
            //    self.flags -= Flags::NEEDS_PATCH;
            //},
            _ => { /* no need to patch */ },
        };
    }

    pub fn len(&self) -> usize {
        match self.dma {
            Some(slice) => slice.len(),
            None => 0,
        }
    }

    pub fn dma(&self) -> Option<NonNull<[u8]>> {
        if self.flags.contains(Flags::NEEDS_PATCH) {
            warn!("{}: Command needs patching but patching was never performed: {:?}", function!(), self);
            None
        } else {
            self.dma
        }
    }
}

//impl Drop for Command {
//    fn drop(&mut self) {
//        self.tag = 0;
//    }
//}
