use core::{
    array::from_fn,
    num::NonZero,
    slice::{
        from_ref,
    },
    ops::Deref,
    ptr::NonNull,
    iter::zip,
    mem::transmute,
    fmt,
};

use alloc::{
    boxed::Box,
    vec::Vec,
    sync::{
        Arc,
        Weak,
    },
};

use virtio_drivers::device::gpu::*;
use winresult::STATUS;
use zerocopy::*;
use microseh;
use offset_allocator;
use spin::mutex::SpinMutex;
use spin::rwlock::RwLock;

use wdk::{
    dxgkrnl::*,
    *,
};
use crate::{
    function,
    slice_from_raw_parts,
    slice_from_raw_parts_mut,
};
use crate::uapi::*;
use crate::adapter::*;
use crate::queue::*;
use crate::command::*;
use crate::allocation::*;
use crate::virgl::*;

const VIRTIO_GPU_DEVICE_TAG: u64 = u64::from_ne_bytes(*b"VGPUDEVI");
const VIRTIO_GPU_CONTEXT_TAG: u64 = u64::from_ne_bytes(*b"VGPUDCTX");

pub fn allocation_from_res_id(allocations: &[DXGK_ALLOCATIONLIST], res_id: NonZero<u32>) -> Option<&DXGK_ALLOCATIONLIST> {
    for alloc in allocations {
        let Some(device_specific): Option<&mut DeviceSpecificAllocation> = TaggedExt::from_handle_mut(alloc.hDeviceSpecificAllocation) else {
            error!("{}: invalid DeviceSpecificAllocation: {:?}", function!(), alloc.hDeviceSpecificAllocation);
            continue;
        };
        if device_specific.alloc.upgrade().and_then(|alloc| alloc.id()) == Some(res_id) {
            return Some(alloc);
        }
    }
    None
}

#[derive(Debug)]
struct Context3D {
    id: NonZero<u32>,
    capset: CapsetId,
}

impl Context3D {
    fn try_new(chan: GpuChannel, capset: CapsetId, name: &str) -> Result<Self, NtStatus> {
        let id = chan.context_create(capset, name)?;

        //warn!("Created context: {:?}, {} => {}", capset, name, id);

        Ok(Self {
            id,
            capset,
        })
    }
}

struct QueryBuffer {
    id: NonZero<u32>,
    buf: AlignedBox<[u8]>,
}

impl QueryBuffer {
    pub fn new(device: &Device) -> Result<Self, NtStatus> {
        let alloc_3d = Allocate3d {
            tag: ALLOCATE_3D_TAG,
            target: 0,
            format: VIRGL_FORMAT_R8_UNORM,
            bind: VirglBind::CUSTOM.bits(),
            width: 4096,
            height: 1,
            depth: 1,
            array_size: 1,
            last_level: 0,
            nr_samples: 0,
            flags: 0,
            size: PAGE_SIZE as _,
        };

        let id = device.chan.resource_create_3d(&alloc_3d)?;
        let buf = Box::<[u8], _>::try_new_zeroed_slice_in(alloc_3d.size as usize, AlignedAlloc)?;
        let buf = unsafe { buf.assume_init() };

        let ctx_id = device.context_virgl().ok_or(STATUS::REINITIALIZATION_NEEDED)?.0;

        device.chan.context_attach_resource(ctx_id, id)?;

        let mut cmd_data = [0u8; size_of::<commands::ResourceAttachBacking>() + size_of::<commands::MemEntry>()];
        assert_eq!(Command::attach_backing_dma_len(1), size_of_val(&cmd_data));

        let cmd = Command::attach_backing_box(&device.chan, id, &buf, &mut cmd_data);
        device.chan.submit_command_sync(&cmd)?;

        // TODO: this should be stored as owned DeviceSpecific allocation

        Ok(Self {
            id,
            buf,
        })
    }
}

#[repr(C)]
#[derive(Tagged)]
#[tagged(VIRTIO_GPU_DEVICE_TAG)]
pub struct Device {
    pub tag: u64,
    chan: GpuChannel,
    main_context: RwLock<Option<Context3D>>,
    virgl_blit_context: RwLock<Option<Context3D>>,
    query_buffer: SpinMutex<Option<QueryBuffer>>,
}

#[repr(C)]
#[derive(Tagged, Debug)]
#[tagged(VIRTIO_GPU_CONTEXT_TAG)]
pub struct DeviceContext {
    pub tag: u64,
    pub device: Arc<Device>,
    pub engine: Engine,
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Device")
            .field("main_context", &self.main_context)
            .field("virgl_blit_context", &self.virgl_blit_context)
            .finish()
    }
}

impl Device {
    pub fn new(parent: &Adapter) -> Result<Arc<Self>, NtStatus> {
        Ok(Arc::try_new(Self {
            tag: VIRTIO_GPU_DEVICE_TAG,
            chan: parent.queue_channel().ok_or(STATUS::REINITIALIZATION_NEEDED)?,
            main_context: RwLock::new(None),
            virgl_blit_context: RwLock::new(None),
            query_buffer: SpinMutex::new(None),
        })?)
    }

    pub fn dxgk_context(self: &Arc<Self>, engine: Engine) -> Result<Arc<DeviceContext>, NtStatus> {
        Ok(Arc::try_new(DeviceContext {
            tag: VIRTIO_GPU_CONTEXT_TAG,
            device: self.clone(),
            engine,
        })?)
    }

    pub fn dxgk_interface(&self) -> &DxgkInterface {
        self.chan.dxgk_interface()
    }

    pub fn init_context(&self, supported_capsets: CapsetMask, params: &ContextInit) -> Result<(), NtStatus> {
        if !supported_capsets.contains(params.capset_id.into()) {
            error!("unsupported capset: {:?}", { params.capset_id });
            return Err(NtStatus(STATUS::NOT_SUPPORTED));
        }

        if self.main_context.read().is_some() {
            error!("{}: device already has a virtio context initialized: {:?}", function!(), self);
        }

        let context = Context3D::try_new(self.chan.clone(), params.capset_id, params.debug_name())?;
        self.main_context.write().replace(context);

        if matches!(params.capset_id, CapsetId::Venus) {
            let capset_id = if supported_capsets.contains(CapsetMask::VIRGL2) {
                CapsetId::Virgl2
            } else {
                CapsetId::Virgl
            };

            self.virgl_blit_context.write().replace(Context3D::try_new(self.chan.clone(), capset_id, "venus-shadow-virgl-win32")?);
        }

        debug!("{}: debug name {}, main {:?}, blit {:?}", function!(), params.debug_name(), self.main_context, self.virgl_blit_context);

        Ok(())
    }

    pub fn context(&self) -> Option<NonZero<u32>> {
        Some(self.context_internal(false)?.0)
    }

    pub fn capset(&self) -> Option<CapsetId> {
        Some(self.context_internal(false)?.1)
    }

    fn context_virgl(&self) -> Option<(NonZero<u32>, CapsetId)> {
        let main_context = self.main_context.read();
        let Some(main_context) = main_context.deref() else {
            return None;
        };

        if matches!(main_context.capset, CapsetId::Virgl | CapsetId::Virgl2) {
            Some((main_context.id, main_context.capset))
        } else {
            let virgl_blit_context = self.virgl_blit_context.read();
            let Some(virgl_blit_context) = virgl_blit_context.deref() else {
                return None;
            };

            Some((virgl_blit_context.id, virgl_blit_context.capset))
        }
    }

    fn context_internal(&self, shadow_virgl: bool) -> Option<(NonZero<u32>, CapsetId)> {
        if shadow_virgl {
            let virgl_blit_context = self.virgl_blit_context.read();
            let Some(virgl_blit_context) = virgl_blit_context.deref() else {
                return None;
            };
            Some((virgl_blit_context.id, virgl_blit_context.capset))
        } else {
            let main_context = self.main_context.read();
            let Some(main_context) = main_context.deref() else {
                return None;
            };
            Some((main_context.id, main_context.capset))
        }
    }

    pub fn query_layout(self: Arc<Device>, alloc: &Arc<Allocation>) -> Result<VirglResourceLayout, NtStatus> {
        let ctx_id = self.context_internal(true).ok_or(STATUS::REINITIALIZATION_NEEDED)?.0;
        let device_specific = Allocation::attach_to_device(alloc.clone(), self.clone(), false)?;
        device_specific.ensure_virgl_attached()?;

        let target = alloc.id().unwrap();

        let mut query = self.query_buffer.lock();
        let query = if let Some(query) = query.as_ref() {
            query
        } else {
            query.replace(QueryBuffer::new(&self)?);
            query.as_ref().unwrap()
        };

        let mut cmd_data = [0u8; size_of::<commands::CmdSubmit3d>() + size_of::<VirglGetResourceLayout>()];
        assert_eq!(Command::virgl_get_resource_layout_dma_len(), size_of_val(&cmd_data));

        let cmd = Command::virgl_get_resource_layout(&self.chan, ctx_id, query.id, target, None, &mut cmd_data);
        self.chan.submit_command_sync(&cmd)?;

        let layout = VirglResourceLayout::read_from_prefix(query.buf.as_ref()).unwrap().0;

        debug!("{}: resource layout: {:?}", function!(), layout);
        Ok(layout)
    }

    pub fn present(&self, present: &mut DXGKARG_PRESENT) -> Result<(), NtStatus> {
        let allocations = unsafe {
            slice_from_raw_parts(present.__bindgen_anon_1.pAllocationList, (DXGK_PRESENT_MAX_INDEX + 1) as usize)
        };

        let Some(src): Option<Arc<DeviceSpecificAllocation>> = TaggedExt::from_arc_handle_clone(allocations[DXGK_PRESENT_SOURCE_INDEX as usize].hDeviceSpecificAllocation) else {
            warn!("{}: no source allocation, flags: {:?}", function!(), present.Flags);
            return Ok(());
        };

        let Some(dst): Option<Arc<DeviceSpecificAllocation>> = TaggedExt::from_arc_handle_clone(allocations[DXGK_PRESENT_DESTINATION_INDEX as usize].hDeviceSpecificAllocation) else {
            if present.Flags.Flip() || present.Flags.FlipWithNoWait() {
                debug!("{}: no destination allocation, flags: {:?}", function!(), present.Flags);
            } else {
                warn!("{}: no destination allocation, flags: {:?}", function!(), present.Flags);
            }
            return Ok(());
        };

        if !present.Flags.Blt() {
            warn!("{}: no blit, flags: {:?}", function!(), present.Flags);
            return Ok(());
        }

        if present.pDmaBuffer.is_null() {
            warn!("{}: no dma buffer, flags: {:?}", function!(), present.Flags);
            return Ok(());
        }

        let Some(src_alloc) = src.alloc.upgrade() else {
            error!("{}: src allocation no longer exists", function!());
            return Err(NtStatus(STATUS::INVALID_HANDLE));
        };

        let Some(dst_alloc) = dst.alloc.upgrade() else {
            error!("{}: dst allocation no longer exists", function!());
            return Err(NtStatus(STATUS::INVALID_HANDLE));
        };

        debug!("{}: flags: {:?}", function!(), present.Flags);
        debug!("{}: src: {:?}", function!(), src_alloc);
        debug!("{}: dst: {:?}", function!(), dst_alloc);

        let patchloc_out = slice_from_raw_parts_mut(present.pPatchLocationListOut, present.PatchLocationListOutSize as _);

        if patchloc_out.len() < 2 {
            warn!("{}: need at least 2 patch location slots", function!());
            return Err(NtStatus(STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER));
        }

        patchloc_out[0].AllocationIndex = DXGK_PRESENT_DESTINATION_INDEX;
        patchloc_out[0].AllocationOffset = 0;
        patchloc_out[0].DriverId = 1;
        patchloc_out[0].set_SlotId(1);
        patchloc_out[0].PatchOffset = 0;
        patchloc_out[0].SplitOffset = 0;

        patchloc_out[1].AllocationIndex = DXGK_PRESENT_SOURCE_INDEX;
        patchloc_out[1].AllocationOffset = 0;
        patchloc_out[1].DriverId = 2;
        patchloc_out[1].set_SlotId(2);
        patchloc_out[1].PatchOffset = 0;
        patchloc_out[1].SplitOffset = 0;

        present.PatchLocationListOutSize = 2;

        present.pPatchLocationListOut = unsafe {
            present.pPatchLocationListOut.add(present.PatchLocationListOutSize as _)
        };

        let mut dmabuf_offset = 0;
        let dmabuf = slice_from_raw_parts_mut(present.pDmaBuffer as *mut u8, present.DmaSize as _);
        let mut dma_priv = CommandDmaPrivate::new();

        let dst_subrects = if present.SubRectCnt >= 1 {
            slice_from_raw_parts(present.pDstSubRects, present.SubRectCnt as _)
        } else {
            from_ref(&present.DstRect)
        };

        let mut cover_rect = dst_subrects[0];
        for r in dst_subrects {
            cover_rect |= *r;
        }

        let dx = present.SrcRect.left - present.DstRect.left;
        let dy = present.SrcRect.top  - present.DstRect.top;

        let Some((context_id, _)) = self.context_virgl() else {
            warn!("{}: virtio context was not created yet: {:?}", function!(), self);
            return Err(NtStatus(STATUS::REINITIALIZATION_NEEDED));
        };

        src.ensure_virgl_attached().inspect_err(|e|
            error!("{}: failed to attach to virgl: {:?}", function!(), src_alloc)
        )?;
        dst.ensure_virgl_attached().inspect_err(|e|
            error!("{}: failed to attach to virgl: {:?}", function!(), dst_alloc)
        )?;

        if src_alloc.is_blob() {
            let needed = Command::virgl_set_type_dma_len();
            let actual = (&dmabuf[dmabuf_offset..]).len();
            if needed > actual {
                error!("{}: VirglResourceSetType needs {} bytes, but only {} bytes are available", function!(), needed, actual);
                present.pDmaBuffer = unsafe { present.pDmaBuffer.byte_add(dmabuf_offset + needed) };
                return Err(NtStatus(STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER))
            }

            let id = src_alloc.id().unwrap();
            if let VirtioResource::Blob { info, .. } = src_alloc.resource() && let Some(info) = *info.read() {
                let cmd = Command::virgl_set_type(&self.chan, Some(context_id), id, &info, None, &mut dmabuf[dmabuf_offset..]);
                dmabuf_offset += cmd.len();
                dma_priv.commands.push(cmd);
            } else {
                error!("{}: no blob info set for {:?}", function!(), src_alloc);
            }
        }
        if dst_alloc.is_blob() {
            let needed = Command::virgl_set_type_dma_len();
            let actual = (&dmabuf[dmabuf_offset..]).len();
            if needed > actual {
                error!("{}: VirglResourceSetType needs {} bytes, but only {} bytes are available", function!(), needed, actual);
                present.pDmaBuffer = unsafe { present.pDmaBuffer.byte_add(dmabuf_offset + needed) };
                return Err(NtStatus(STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER))
            }

            let id = dst_alloc.id().unwrap();
            if let VirtioResource::Blob { info, .. } = src_alloc.resource() && let Some(info) = *info.read() {
                let cmd = Command::virgl_set_type(&self.chan, Some(context_id), id, &info, None, &mut dmabuf[dmabuf_offset..]);
                dmabuf_offset += cmd.len();
                dma_priv.commands.push(cmd);
            } else {
                error!("{}: no blob info set for {:?}", function!(), dst_alloc);
            }
        }

        if src_alloc.needs_transfer() {
            let top_left = (cover_rect + (dx, dy)).top_left();
            let dim = cover_rect.dimensions();

            let transfer = CommandTransfer {
                res_id: src_alloc.id().unwrap().get(),
                stride: 0,
                offset: 0,
                level: 0,
                layer_stride: 0,
                r#box: Box3D {
                    x: top_left.0,
                    y: top_left.1,
                    z: top_left.2,
                    width:  dim.0,
                    height: dim.1,
                    depth:  dim.2,
                },
            };

            let item = CommandBodyItem::TransferToHost(&transfer);
            let needed = Command::item_dma_len(&item);
            let actual = (&dmabuf[dmabuf_offset..]).len();
            if needed > actual {
                error!("{}: TransferToHost needs {} bytes, but only {} bytes are available", function!(), needed, actual);
                present.pDmaBuffer = unsafe { present.pDmaBuffer.byte_add(dmabuf_offset + needed) };
                return Err(NtStatus(STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER))
            }

            let cmd = Command::from_item(&self.chan, Some(context_id), &item, None, &mut dmabuf[dmabuf_offset..]);
            dmabuf_offset += cmd.len();
            dma_priv.commands.push(cmd);
        }

        {
            let needed = Command::virgl_blit_dma_len(dst_subrects);
            let actual = (&dmabuf[dmabuf_offset..]).len();
            if needed > actual {
                error!("{}: {} * VirglResourceCopyRegion needs {} bytes, but only {} bytes are available", function!(), dst_subrects.len(), needed, actual);
                present.pDmaBuffer = unsafe { present.pDmaBuffer.byte_add(dmabuf_offset + needed) };
                return Err(NtStatus(STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER))
            }

            let cmd = Command::virgl_blit(&self.chan, Some(context_id), src_alloc.id().unwrap(), dst_alloc.id().unwrap(), (dx, dy), dst_subrects, None, &mut dmabuf[dmabuf_offset..]);
            dmabuf_offset += cmd.len();
            dma_priv.commands.push(cmd);
        }

        if dst_alloc.needs_transfer() {
            let top_left = cover_rect.top_left();
            let dim = cover_rect.dimensions();

            let transfer = CommandTransfer {
                res_id: dst_alloc.id().unwrap().get(),
                stride: 0,
                offset: 0,
                level: 0,
                layer_stride: 0,
                r#box: Box3D {
                    x: top_left.0,
                    y: top_left.1,
                    z: top_left.2,
                    width:  dim.0,
                    height: dim.1,
                    depth:  dim.2,
                },
            };

            let item = CommandBodyItem::TransferFromHost(&transfer);
            let needed = Command::item_dma_len(&item);
            let actual = (&dmabuf[dmabuf_offset..]).len();
            if needed > actual {
                error!("{}: TransferFromHost needs {} bytes, but only {} bytes are available", function!(), needed, actual);
                present.pDmaBuffer = unsafe { present.pDmaBuffer.byte_add(dmabuf_offset + needed) };
                return Err(NtStatus(STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER))
            }

            let cmd = Command::from_item(&self.chan, Some(context_id), &item, None, &mut dmabuf[dmabuf_offset..]);
            dmabuf_offset += cmd.len();
            dma_priv.commands.push(cmd);
        }

        present.pDmaBuffer = unsafe { present.pDmaBuffer.byte_add(dmabuf_offset) };

        if (present.DmaBufferPrivateDataSize as usize) < size_of::<CommandDmaPrivate>() {
            error!("{}: no dma private data (not enough space: {} < size_of::<CommandDmaPrivate>())", function!(), present.DmaBufferPrivateDataSize);
            return Err(NtStatus(STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER));
        }

        if present.pDmaBufferPrivateData.is_null() {
            error!("{}: no dma private data", function!());
            return Err(NtStatus(STATUS::INVALID_PARAMETER));
        }

        dst_alloc.mark_busy();
        src_alloc.mark_busy();

        dma_priv.attach_allocation(src);
        dma_priv.attach_allocation(dst);

        unsafe {
            let dma_priv_ptr = transmute::<_, *mut CommandDmaPrivate>(present.pDmaBufferPrivateData);
            dma_priv_ptr.write(dma_priv);
            present.pDmaBufferPrivateData = present.pDmaBufferPrivateData.byte_add(size_of::<CommandDmaPrivate>());
            present.DmaBufferPrivateDataSize -= size_of::<CommandDmaPrivate>() as u32;
        };

        Ok(())
    }

    pub fn render(&self, render: &mut DXGKARG_RENDER) -> Result<(), NtStatus> {
        trace!("{}: commands length: {}", function!(), render.CommandLength);

        let dma_priv = match microseh::try_seh(|| -> Result<CommandDmaPrivate, NtStatus> {
            let patchloc_in = slice_from_raw_parts(render.pPatchLocationListIn, render.PatchLocationListInSize as _);

            render.PatchLocationListOutSize = render.PatchLocationListInSize;
            let patchloc_out = slice_from_raw_parts_mut(render.pPatchLocationListOut, render.PatchLocationListOutSize as _);

            for (i, (pin, pout)) in zip(patchloc_in.iter(), patchloc_out.iter_mut()).enumerate() {
                pout.AllocationIndex = pin.AllocationIndex;
                pout.AllocationOffset = 0;
                pout.PatchOffset = 0;
                pout.set_SlotId(i);

                //warn!("[{}] {:?} -> {:?}", i, pin, pout);
            }

            //warn!("patch in: {:?}", patchloc_in);
            //warn!("patch out: {:?}", patchloc_out);

            render.pPatchLocationListOut = unsafe {
                render.pPatchLocationListOut.add(render.PatchLocationListOutSize as _)
            };

            let allocations = slice_from_raw_parts(render.pAllocationList, render.AllocationListSize as _);
            debug!("alloc list len: {}", allocations.len());

            let mut alloc_good = true;
            for alloc_info in allocations {
                let Some(device_specific): Option<&DeviceSpecificAllocation> = TaggedExt::from_handle(alloc_info.hDeviceSpecificAllocation) else {
                    error!("{}: invalid DeviceSpecificAllocation: {:?}", function!(), alloc_info.hDeviceSpecificAllocation);
                    alloc_good = false;
                    continue;
                };
                let Some(alloc) = device_specific.alloc.upgrade() else {
                    error!("{}: invalid DeviceSpecificAllocation: {:?} (allocation no longer exists)", function!(), alloc_info.hDeviceSpecificAllocation);
                    alloc_good = false;
                    continue;
                };

                debug!("{}: dev: {:X}, alloc {:?}", function!(), device_specific as *const _ as usize, alloc);
                debug!("{}: alloc_info: physical addr: {:X}", function!(), unsafe { alloc_info.__bindgen_anon_2.PhysicalAddress.QuadPart as u64 });
            }

            if !alloc_good {
                return Err(NtStatus(STATUS::INVALID_HANDLE));
            }

            let command = slice_from_raw_parts(render.pCommand as *const u8, render.CommandLength as _);
            let dmabuf = slice_from_raw_parts_mut(render.pDmaBuffer as *mut u8, render.DmaSize as _);
            let mut dma_priv = CommandDmaPrivate::new();
            let mut nop = false;

            let mut command_offset = 0;
            let mut dmabuf_offset = 0;
            while command_offset < command.len() {
                let (hdr, _) = CommandHeaderRaw::ref_from_prefix(&command[command_offset..]).map_err(|e| {
                    error!("{}: failed to read raw command header: {:?}", function!(), e);
                    STATUS::INVALID_PARAMETER
                })?;
                command_offset += size_of::<CommandHeaderRaw>();

                let hdr = hdr.try_as_hdr().map_err(|e| {
                    error!("{}: command validation failed: {}", function!(), e);
                    STATUS::INVALID_PARAMETER
                })?;

                let body = hdr.command_slice().map_err(|e| {
                    error!("{}: command validation failed: {}", function!(), e);
                    STATUS::INVALID_PARAMETER
                })?;

                command_offset += hdr.size as usize;

                debug!("{}: header: {:?}", function!(), hdr);
                trace!("{}: body: {:?}", function!(), body);

                match body {
                    CommandBody::Nop => {
                        nop = true
                    },
                    CommandBody::Fence(_) => {
                        error!("{}: command validation failed: CommandId::Fence is not allowed for physical contexts", function!());
                        return Err(NtStatus(STATUS::INVALID_HANDLE));
                    },
                    CommandBody::Submit(data) => {
                        let is_virgl = matches!(self.context_internal(false).unwrap().1, CapsetId::Virgl | CapsetId::Virgl2);

                        if false && is_virgl {
                            let mut offset = 0;
                            let typed_data = slice_from_raw_parts(data.as_ptr() as *const u32, data.len() / size_of::<u32>());

                            while offset < typed_data.len() {
                                let hdr = typed_data[offset];
                                let cmd = VirglCommand::from((hdr & 0xff) as u8);
                                let obj = VirglObject::from(((hdr >> 8) & 0xff) as u8);
                                let len = ((hdr >> 16) & 0xffff) as usize;
                                debug!("{}: decoded: cmd: {:?}, obj: {:?}, len: {}", function!(), cmd, obj, len);
                                offset += 1 + len;
                            }
                        }
                    },
                    //CommandBody::MapBlob(blobs) => {
                    //    for blob in blobs {
                    //        let blob_res_id = blob.res_id;
                    //        let res_id = NonZero::new(blob_res_id).ok_or(STATUS::INVALID_PARAMETER).inspect_err(|e| {
                    //            error!("{}: command validation failed: invalid resource id: {}", function!(), blob_res_id);
                    //        })?;
                    //        let alloc_info = allocation_from_res_id(allocations, res_id).ok_or(STATUS::INVALID_PARAMETER).inspect_err(|e| {
                    //            error!("{}: command validation failed: no resource {} in allocation list", function!(), res_id);
                    //        })?;
                    //
                    //        let device_specific: &DeviceSpecificAllocation = TaggedExt::from_handle(alloc_info.hDeviceSpecificAllocation).unwrap();
                    //        let alloc = device_specific.alloc.upgrade().ok_or(STATUS::INVALID_PARAMETER).inspect_err(|e| {
                    //            error!("{}: command validation failed: resource {} no longer exists", function!(), res_id);
                    //        })?;
                    //
                    //        (alloc.can_be_mapped()).then(|| ()).ok_or(STATUS::INVALID_PARAMETER).inspect_err(|e| {
                    //            error!("{}: command validation failed: cannot map 3d resources: {:?}", function!(), alloc);
                    //        })?;
                    //
                    //        (!alloc.is_mapped()).then(|| ()).ok_or(STATUS::INVALID_PARAMETER).inspect_err(|e| {
                    //            error!("{}: command validation failed: cannot map the same resource twice: {}", function!(), res_id);
                    //        })?;
                    //
                    //        device_specific.set_mapped(true);
                    //    }
                    //},
                    _ => {
                        debug!("{}: content: {:?}", function!(), body);
                    },
                };

                let context = Some(self.context_internal(hdr.flags.contains(CommandFlag::SHADOW_VIRGL)).ok_or(STATUS::REINITIALIZATION_NEEDED)?.0);
                let ring = hdr.ring();
                // TODO: if there is more than one item in the body, we probably want to actually set the ring

                let needed = Command::body_dma_len(&body);
                let actual = (&dmabuf[dmabuf_offset..]).len();

                if needed > actual {
                    error!("{}: command {:?}/{:?} needs {} bytes, but only {} bytes are available", function!(), hdr, body, needed, actual);
                    render.pDmaBuffer = unsafe { render.pDmaBuffer.byte_add(dmabuf_offset + needed) };
                    return Err(NtStatus(STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER))
                }

                let mut dma_written = 0;

                for cmd_item in &body {
                    if cmd_item.id() == CommandId::Nop {
                        continue;
                    }

                    //let cmd_item = match cmd_item {
                    //    CommandBodyItem::MapBlob(map) => {
                    //        let res_id = NonZero::new(map.res_id).unwrap();
                    //        let alloc_info = allocation_from_res_id(allocations, res_id).unwrap();
                    //        match alloc_info.SegmentId() {
                    //            0 => {
                    //                /* needs patching later */
                    //                cmd_item
                    //            },
                    //            SEGMENT_ID_BLOB_MAPPABLE => {
                    //                let addr = unsafe { alloc_info.__bindgen_anon_2.PhysicalAddress.QuadPart as u64 };
                    //                assert!(addr >= BLOB_MAP_SEGMENT_GPU_PADDR);
                    //                let offset = addr - BLOB_MAP_SEGMENT_GPU_PADDR;
                    //                debug!("{}: Mapping blob at offset {:X}", function!(), offset);
                    //                CommandBodyItem::MapBlobAt(map, offset)
                    //            },
                    //            _ => unreachable!("invalid segment for blob allocation"),
                    //        }
                    //    },
                    //    _ => cmd_item,
                    //};

                    let cmd = Command::from_item(&self.chan, context, &cmd_item, ring, &mut dmabuf[dmabuf_offset..]);

                    dmabuf_offset += cmd.len();
                    dma_written += cmd.len();
                    dma_priv.commands.push(cmd);
                }

                if !nop {
                    assert_eq!(dma_written, needed);
                }
            }

            if nop && dma_priv.commands.len() == 0 {
                // /* Add NOP */
                // let item = CommandBodyItem::Nop;
                // let context = self.context();
                // let cmd = Command::from_item(self.parent, context, &item, None, &mut dmabuf[dmabuf_offset..]);
                // dmabuf_offset += cmd.len();
                // commands.push(cmd);
                dma_priv.commands.push(Command::nop());
            }

            //if dmabuf_offset == 0 {
            //    dmabuf_offset = 8;
            //}

            let ptr_before = render.pDmaBuffer;
            render.pDmaBuffer = unsafe { render.pDmaBuffer.byte_add(dmabuf_offset) };
            let ptr_after = render.pDmaBuffer;
            debug!("{}: total bytes written into dma buf: {} (dma addr {:?} -> {:?})", function!(), dmabuf_offset, ptr_before, ptr_after);

            for alloc in allocations {
                let device_specific: Arc<DeviceSpecificAllocation> = TaggedExt::from_arc_handle_clone(alloc.hDeviceSpecificAllocation).unwrap();
                trace!("{}: marking {:?} busy", function!(), device_specific.alloc);

                if let Some(alloc) = device_specific.alloc.upgrade() {
                    alloc.mark_busy();
                } else {
                    error!("{}: allocation {:?} no longer exists", function!(), alloc.hDeviceSpecificAllocation);
                }

                dma_priv.attach_allocation(device_specific);
            }

            // UPD: should not be anymore
            //if dma_priv.commands.len() > 1 {
            //    warn!("{}: Sending more than one command header is probably broken!!!", function!());
            //}

            //info!("{}: commands: {:?}", function!(), dma_priv.commands);
            //info!("{}: allocations: {:?}", function!(), dma_priv.allocations);

            trace!("{}: converted commands into dma buffer length {}", function!(), dmabuf_offset);

            // DEBUG!!!!!
            //if true {
            //    warn!("{}: OVERRIDING COMMANDS TO NOP: {:?}", function!(), dma_priv.commands);
            //    dma_priv.commands.clear();
            //    dma_priv.commands.push(Command::nop());
            //    render.pDmaBuffer = ptr_before;
            //}

            Ok(dma_priv)
        }) {
            Ok(Ok(dma_priv)) => {
                dma_priv
            },
            Ok(Err(e)) => {
                error!("{}: invalid buffer: {:?}", function!(), e);
                return Err(NtStatus(STATUS::INVALID_PARAMETER));
            },
            Err(e) => {
                error!("{}: failed to validate buffer: {:?}", function!(), e);
                return Err(NtStatus(STATUS::INVALID_PARAMETER));
            },
        };

        debug!("{}: converted commands into dma: {:?}", function!(), dma_priv);

        if (render.DmaBufferPrivateDataSize as usize) < size_of::<CommandDmaPrivate>() {
            error!("{}: no dma private data (not enough space: {} < size_of::<CommandDmaPrivate>())", function!(), render.DmaBufferPrivateDataSize);
            return Err(NtStatus(STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER));
        }

        if !render.pDmaBufferPrivateData.is_null() {
            unsafe {
                let dma_priv_ptr = transmute::<_, *mut CommandDmaPrivate>(render.pDmaBufferPrivateData);
                dma_priv_ptr.write(dma_priv);
                render.pDmaBufferPrivateData = render.pDmaBufferPrivateData.byte_add(size_of::<CommandDmaPrivate>());
            };
        } else {
            error!("{}: no dma private data", function!());
            return Err(NtStatus(STATUS::INVALID_PARAMETER));
        }

        Ok(())
    }

    pub fn allocate(self: Arc<Self>, create_allocation: &mut DXGKARG_CREATEALLOCATION, allocations: &mut [DXGK_OPENALLOCATIONINFO]) -> Result<(), NtStatus> {
        Adapter::allocate_full(&self.chan, create_allocation)?;

        let alloc_infos = slice_from_raw_parts_mut(create_allocation.pAllocationInfo, create_allocation.NumAllocations as _);
        assert_eq!(alloc_infos.len(), allocations.len());

        for (create_info, alloc_info) in zip(alloc_infos.iter(), allocations.iter_mut()) {
            let Some(alloc) = <Allocation as TaggedExt>::from_arc_handle_owned(create_info.hAllocation) else {
                error!("{}: invalid handle provided: {:?}", function!(), create_info.hAllocation);
                return Err(NtStatus(STATUS::INVALID_HANDLE));
            };

            //self.chan.dxgk_interface().add_kmt(alloc_info.hAllocation, Arc::downgrade(&alloc))?;

            debug!("{}: allocation handle in: {:X?}", function!(), alloc_info.hAllocation);

            let blob = alloc.is_blob();
            if blob {
                warn!("{}: allocation handle in: {:X?}, alloc: {:?}", function!(), alloc_info.hAllocation, alloc);
            }

            let device_specific = alloc.attach_to_device(self.clone(), true)?;

            if blob {
                warn!("{}: dev alloc: {:X}", function!(), Arc::as_ptr(&device_specific) as usize);
            }

            alloc_info.hDeviceSpecificAllocation = TaggedExt::into_arc_handle(device_specific);
        }

        Ok(())
    }

    pub fn open_allocation(self: Arc<Self>, flags: DXGK_OPENALLOCATIONFLAGS, allocations: &mut [DXGK_OPENALLOCATIONINFO]) -> Result<(), NtStatus> {
        for alloc_info in allocations {
            let Some(alloc) = self.chan.dxgk_interface().allocation_from_handle(alloc_info.hAllocation) else {
                error!("{}: invalid handle provided: {:X} (flags {:?})", function!(), alloc_info.hAllocation, flags);
                return Err(NtStatus(STATUS::INVALID_HANDLE));
            };

            //if matches!(self.capset(), Some(CapsetId::Venus)) && alloc.is_3d() {
            //    warn!("{}: alloc: ({}, {:?}), dev: {:?}", function!(), alloc.id().unwrap(), alloc.resource(), self);
            //}

            debug!("Opening allocation: {:?}", alloc);
            let device_specific = alloc.attach_to_device(self.clone(), false)?;
            alloc_info.hDeviceSpecificAllocation = TaggedExt::into_arc_handle(device_specific);
        }

        Ok(())
    }

    pub fn close_allocation(&self, device_allocations: &[HANDLE]) -> Result<(), NtStatus> {
        for handle in device_allocations {
            let Some(device_specific): Option<Arc<DeviceSpecificAllocation>> = TaggedExt::from_arc_handle_owned(*handle) else {
                continue;
            };
            if let Some(alloc) = device_specific.alloc.upgrade() {
                alloc.detach_from_device(device_specific)?;
                if let Some(alloc) = Arc::into_inner(alloc) {
                    let n = alloc.attached_devices_count();
                    if n > 0 {
                        warn!("{}: dropping last allocation instance (attached devices: {}) {:?}", function!(), n, alloc);
                    } else {
                        trace!("{}: dropping last allocation instance (attached devices: {}) {:?}", function!(), n, alloc);
                    }

                    Adapter::destroy_allocation_inner(&self.chan, &alloc);
                }
            } else {
                error!("{}: failed to close allocation which no longer exists", function!());
            }
        }

        Ok(())
    }

    fn context_attach_resource_internal(&self, res_id: NonZero<u32>, shadow_virgl: bool) -> Result<(), NtStatus> {
        let context_id = self.context_internal(shadow_virgl).ok_or(STATUS::REINITIALIZATION_NEEDED)?.0;
        self.chan.context_attach_resource(context_id, res_id)
    }

    fn context_detach_resource_internal(&self, res_id: NonZero<u32>, shadow_virgl: bool) -> Result<(), NtStatus> {
        let context_id = self.context_internal(shadow_virgl).ok_or(STATUS::REINITIALIZATION_NEEDED)?.0;
        self.chan.context_detach_resource(context_id, res_id)
    }

    pub fn context_attach_resource(&self, res_id: NonZero<u32>) -> Result<(), NtStatus> {
        self.context_attach_resource_internal(res_id, false)
    }

    pub fn context_attach_virgl(&self, res_id: NonZero<u32>) -> Result<(), NtStatus> {
        self.context_attach_resource_internal(res_id, true)
    }

    pub fn context_detach_resource(&self, res_id: NonZero<u32>, has_virgl: bool) -> Result<(), NtStatus> {
        if has_virgl {
            self.context_detach_resource_internal(res_id, true)?;
        }
        self.context_detach_resource_internal(res_id, false)
    }

    pub fn context_submit_3d(&self, mut data: AlignedBox<[u8]>) -> Result<(), NtStatus> {
        let ctx_id = self.context_internal(false).ok_or(STATUS::REINITIALIZATION_NEEDED)?.0;

        let hdr = commands::CmdSubmit3d {
            header: self.chan.new_header(commands::Command::SUBMIT_3D, false, Some(ctx_id), None),
            size: (data.len() - size_of::<commands::CmdSubmit3d>()) as _,
            _padding: 0,
        };

        hdr.write_to_prefix(&mut data)?;

        trace!("{}: resource alloc command submit: {:?}", function!(), data);

        self.chan.submit_async(data)
    }

    pub fn context_create_blob(&self, res_id: NonZero<u32>, blob_id: u64, mem: BlobMem, flags: BlobFlag, size: u64) -> Result<(), NtStatus> {
        let ctx_id = self.context_internal(false).ok_or(STATUS::REINITIALIZATION_NEEDED)?.0;
        self.chan.resource_create_blob(ctx_id, res_id, blob_id, mem, flags, size)
    }

    pub fn context_map_blob(&self, res_id: NonZero<u32>, size: u64) -> Result<(offset_allocator::Allocation, u64, u32), NtStatus> {
        let ctx_id = self.context_internal(false).ok_or(STATUS::REINITIALIZATION_NEEDED)?.0;
        self.chan.resource_map_blob(ctx_id, res_id, size)
    }

    pub fn context_unmap_blob(&self, res_id: NonZero<u32>, offset: offset_allocator::Allocation) -> Result<(), NtStatus> {
        self.chan.resource_unmap_blob(res_id, offset)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.tag = 0;

        if let Some(query) = self.query_buffer.get_mut() {
            let _ = self.chan.resource_detach_backing(query.id).inspect_err(|e|
               error!("{}: failed to detach backing from resource: {:?}", function!(), e)
            );

            if let Some(blit) = self.virgl_blit_context.get_mut() {
                let _ = self.chan.context_detach_resource(blit.id, query.id).inspect_err(|e|
                   error!("{}: failed to detach resource from context: {:?}", function!(), e)
                );
            }

            let _ = self.chan.resource_unref(query.id).inspect_err(|e|
               error!("{}: failed to unref resource: {:?}", function!(), e)
            );
        }

        if let Some(main) = self.main_context.get_mut() {
            debug!("Destroying context {:?}", main);

            let _ = self.chan.context_destroy(main.id).inspect_err(|e|
               error!("{}: failed to destroy context: {:?}", function!(), e)
            );
        }

        if let Some(blit) = self.virgl_blit_context.get_mut() {
            debug!("Destroying context {:?}", blit);

            let _ = self.chan.context_destroy(blit.id).inspect_err(|e|
               error!("{}: failed to destroy context: {:?}", function!(), e)
            );
        }
    }
}
