//! Driver for VirtIO GPU devices.

mod edid;

pub use self::edid::Edid;

use crate::config::{ReadOnly, WriteOnly, read_config};
use crate::hal::{BufferDirection, Dma, Hal, PhysAddr};
use crate::queue::VirtQueue;
use crate::transport::{InterruptStatus, Transport};
use crate::{Error, Result, pages};
use alloc::{vec, vec::Vec};
use bitflags::bitflags;
use core::cmp::min;
use core::fmt;
use core::mem::size_of;
use log::info;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Number of descriptors per virtqueue (const generic bound on `VirtQueue`).
///
/// `submit_3d` drives a 3-buffer command — the `CtrlHeader` + the virgl command
/// stream + the response — and `VirtQueue::add` requires the total number of
/// in/out buffers to fit inside `SIZE` even when `RING_INDIRECT_DESC` is used
/// (they all go into one indirect list before occupying a single queue slot).
/// A depth of 16 is the smallest power of two that comfortably covers that and
/// any future multi-resource submit while keeping descriptor-table memory small.
const QUEUE_SIZE: u16 = 16;
const SUPPORTED_FEATURES: Features = Features::RING_EVENT_IDX
    .union(Features::RING_INDIRECT_DESC)
    .union(Features::VERSION_1)
    .union(Features::ACCESS_PLATFORM)
    .union(Features::EDID)
    .union(Features::VIRGL)
    .union(Features::CONTEXT_INIT)
    .union(Features::RESOURCE_BLOB);

pub const VIRTIO_GPU_SHM_ID_HOST_VISIBLE: u8 = 1;

/// A virtio based graphics adapter.
///
/// It can operate in 2D mode and in 3D (virgl) mode.
/// 3D mode will offload rendering ops to the host gpu and therefore requires
/// a gpu with 3D support on the host machine.
/// In 2D mode the virtio-gpu device provides support for ARGB Hardware cursors
/// and multiple scanouts (aka heads).
pub struct VirtIOGpu<H: Hal, T: Transport> {
    transport: T,
    rect: Option<Rect>,
    /// DMA area of frame buffer.
    frame_buffer_dma: Option<Dma<H>>,
    /// DMA area of cursor image buffer.
    cursor_buffer_dma: Option<Dma<H>>,
    /// Queue for sending control commands.
    control_queue: VirtQueue<H, { QUEUE_SIZE as usize }>,
    /// Queue for sending cursor commands.
    cursor_queue: VirtQueue<H, { QUEUE_SIZE as usize }>,
    /// Whether EDID feature was negotiated.
    has_edid: bool,
    /// Whether `VIRTIO_F_ACCESS_PLATFORM` was negotiated.
    access_platform: bool,
    /// Whether VIRGL (3D) feature was negotiated.
    has_virgl: bool,
    /// Whether `VIRTIO_GPU_F_RESOURCE_BLOB` was negotiated (blob resources,
    /// dma-buf sharing, host-visible memory).
    has_resource_blob: bool,
}

impl<H: Hal, T: Transport> VirtIOGpu<H, T> {
    /// Create a new VirtIO-Gpu driver.
    pub fn new(mut transport: T) -> Result<Self> {
        let negotiated_features = transport.begin_init(SUPPORTED_FEATURES);

        // read configuration space
        let events_read = read_config!(transport, Config, events_read)?;
        let num_scanouts = read_config!(transport, Config, num_scanouts)?;
        let num_capsets = read_config!(transport, Config, num_capsets)?;
        info!(
            "events_read: {:#x}, num_scanouts: {:#x}, num_capsets: {:#x}",
            events_read, num_scanouts, num_capsets
        );

        let access_platform = negotiated_features.contains(Features::ACCESS_PLATFORM);

        let control_queue = VirtQueue::new(
            &mut transport,
            QUEUE_TRANSMIT,
            negotiated_features.contains(Features::RING_INDIRECT_DESC),
            negotiated_features.contains(Features::RING_EVENT_IDX),
            access_platform,
        )?;
        let cursor_queue = VirtQueue::new(
            &mut transport,
            QUEUE_CURSOR,
            negotiated_features.contains(Features::RING_INDIRECT_DESC),
            negotiated_features.contains(Features::RING_EVENT_IDX),
            access_platform,
        )?;

        transport.finish_init();

        let has_edid = negotiated_features.contains(Features::EDID);
        let has_virgl = negotiated_features.contains(Features::VIRGL);
        let has_resource_blob = negotiated_features.contains(Features::RESOURCE_BLOB);

        info!(
            "GPU features: negotiated={:?}, has_virgl={}, has_edid={}, has_resource_blob={}",
            negotiated_features, has_virgl, has_edid, has_resource_blob
        );

        Ok(VirtIOGpu {
            transport,
            frame_buffer_dma: None,
            cursor_buffer_dma: None,
            rect: None,
            control_queue,
            cursor_queue,
            has_edid,
            access_platform,
            has_virgl,
            has_resource_blob,
        })
    }

    /// Acknowledge interrupt.
    pub fn ack_interrupt(&mut self) -> InterruptStatus {
        self.transport.ack_interrupt()
    }

    /// Get the resolution (width, height).
    pub fn resolution(&mut self) -> Result<(u32, u32)> {
        let display_info = self.get_display_info()?;
        Ok((
            display_info.pmodes[0].rect.width,
            display_info.pmodes[0].rect.height,
        ))
    }

    /// Get the EDID data for the specified scanout.
    ///
    /// Returns an [`Edid`] struct wrapping the EDID blob.
    /// Requires the EDID feature to have been negotiated.
    pub fn get_edid(&mut self, scanout: u32) -> Result<Edid> {
        if !self.has_edid {
            return Err(Error::Unsupported);
        }
        let rsp: RespEdid = self.request(CmdGetEdid {
            header: CtrlHeader::with_type(Command::GET_EDID),
            scanout,
            _padding: 0,
        })?;
        rsp.header.check_type(Command::OK_EDID)?;
        Ok(Edid {
            data: rsp.edid,
            size: rsp.size,
        })
    }

    /// Get the preferred resolution from the EDID data.
    ///
    /// Parses the first Detailed Timing Descriptor in the EDID to extract
    /// the preferred display resolution. Returns (width, height).
    pub fn edid_preferred_resolution(&mut self) -> Result<(u32, u32)> {
        let edid = self.get_edid(SCANOUT_ID)?;
        edid.preferred_resolution()
    }

    /// Get the list of supported resolutions from EDID data.
    ///
    /// Returns up to 8 resolutions from the Standard Timings block, sorted
    /// by total pixel count (largest first). Each entry is (width, height).
    pub fn edid_supported_resolutions(&mut self) -> Result<Vec<(u32, u32)>> {
        let edid = self.get_edid(SCANOUT_ID)?;
        Ok(edid.standard_timings())
    }

    /// Setup framebuffer at the display's default resolution.
    pub fn setup_framebuffer(&mut self) -> Result<&mut [u8]> {
        let display_info = self.get_display_info()?;
        info!("=> {:?}", display_info);
        self.change_resolution(
            display_info.pmodes[0].rect.width,
            display_info.pmodes[0].rect.height,
        )
    }

    /// Set or change the framebuffer resolution. If a framebuffer already exists, tears down the
    /// existing resource before creating the new one. Can be called before or after
    /// [`setup_framebuffer`](Self::setup_framebuffer) to set an explicit resolution.
    ///
    /// Returns a mutable slice to the new framebuffer memory.
    pub fn change_resolution(&mut self, width: u32, height: u32) -> Result<&mut [u8]> {
        let rect = Rect {
            x: 0,
            y: 0,
            width,
            height,
        };

        // Tear down existing framebuffer if one exists
        if self.frame_buffer_dma.is_some() {
            self.set_scanout(Rect::default(), SCANOUT_ID, 0)?;
            self.resource_detach_backing(RESOURCE_ID_FB)?;
            self.resource_unref(RESOURCE_ID_FB)?;
            self.frame_buffer_dma = None;
        }

        self.rect = Some(rect);
        self.resource_create_2d(RESOURCE_ID_FB, width, height)?;

        let size = width * height * 4;
        let frame_buffer_dma = Dma::new(
            pages(size as usize),
            BufferDirection::DriverToDevice,
            self.access_platform,
        )?;

        self.resource_attach_backing(RESOURCE_ID_FB, frame_buffer_dma.paddr(), size)?;
        self.set_scanout(rect, SCANOUT_ID, RESOURCE_ID_FB)?;

        // SAFETY: `Dma::new` guarantees that the pointer returned from
        // `raw_slice` is non-null, aligned, and the allocation is zeroed. We
        // store the `Dma` object in `self.frame_buffer_dma`, which prevents the
        // allocation from being freed while `self` exists. The returned ptr
        // borrows `self` mutably, which prevents other code from getting
        // another reference to `frame_buffer_dma` while the returned slice is
        // still in use.
        let buf = unsafe { frame_buffer_dma.raw_slice().as_mut() };
        self.frame_buffer_dma = Some(frame_buffer_dma);
        Ok(buf)
    }

    /// Flush framebuffer to screen.
    pub fn flush(&mut self) -> Result {
        let rect = self.rect.ok_or(Error::NotReady)?;
        // copy data from guest to host
        self.transfer_to_host_2d(rect, 0, RESOURCE_ID_FB)?;
        // flush data to screen
        self.resource_flush(rect, RESOURCE_ID_FB)?;
        Ok(())
    }

    /// Set the pointer shape and position.
    pub fn setup_cursor(
        &mut self,
        cursor_image: &[u8],
        pos_x: u32,
        pos_y: u32,
        hot_x: u32,
        hot_y: u32,
    ) -> Result {
        let size = CURSOR_RECT.width * CURSOR_RECT.height * 4;
        if cursor_image.len() != size as usize {
            return Err(Error::InvalidParam);
        }
        let cursor_buffer_dma = Dma::new(
            pages(size as usize),
            BufferDirection::DriverToDevice,
            self.access_platform,
        )?;

        // SAFETY: `Dma::new` guarantees that the pointer returned from
        // `raw_slice` is non-null, aligned, and the allocation is zeroed. The
        // returned reference is only used within this function while
        // `cursor_buffer_dma` is alive.
        let buf = unsafe { cursor_buffer_dma.raw_slice().as_mut() };
        buf.copy_from_slice(cursor_image);

        self.resource_create_2d(RESOURCE_ID_CURSOR, CURSOR_RECT.width, CURSOR_RECT.height)?;
        self.resource_attach_backing(RESOURCE_ID_CURSOR, cursor_buffer_dma.paddr(), size)?;
        self.transfer_to_host_2d(CURSOR_RECT, 0, RESOURCE_ID_CURSOR)?;
        self.update_cursor(
            RESOURCE_ID_CURSOR,
            SCANOUT_ID,
            pos_x,
            pos_y,
            hot_x,
            hot_y,
            false,
        )?;
        self.cursor_buffer_dma = Some(cursor_buffer_dma);
        Ok(())
    }

    /// Move the pointer without updating the shape.
    pub fn move_cursor(&mut self, pos_x: u32, pos_y: u32) -> Result {
        self.update_cursor(RESOURCE_ID_CURSOR, SCANOUT_ID, pos_x, pos_y, 0, 0, true)?;
        Ok(())
    }

    /// Send a request to the device and block for a response.
    ///
    /// The call is synchronous: `req` lives on the stack until the device has
    /// consumed it (the used ring entry is popped below), so `req.as_bytes()`
    /// can be handed to the queue directly without copying.
    fn request<Req: IntoBytes + Immutable, Rsp: FromBytes + IntoBytes>(
        &mut self,
        req: Req,
    ) -> Result<Rsp> {
        let mut response = Rsp::new_zeroed();
        self.control_queue.add_notify_wait_pop(
            &[req.as_bytes()],
            &mut [response.as_mut_bytes()],
            &mut self.transport,
        )?;
        Ok(response)
    }

    /// Like `request`, but in addition to the fixed-length response `Rsp` also accepts further
    /// response bytes in `extra_response`.
    ///
    /// Returns the number of bytes written to `extra_response` by the device.
    fn request_with_extra_response<Req: IntoBytes + Immutable, Rsp: FromBytes + IntoBytes>(
        &mut self,
        req: Req,
        extra_response: &mut [u8],
    ) -> Result<(Rsp, usize)> {
        let mut response = Rsp::new_zeroed();
        let used_len = self.control_queue.add_notify_wait_pop(
            &[req.as_bytes()],
            &mut [response.as_mut_bytes(), extra_response],
            &mut self.transport,
        )? as usize;
        Ok((
            response,
            min(
                used_len.saturating_sub(size_of::<Rsp>()),
                extra_response.len(),
            ),
        ))
    }

    /// Send a mouse cursor operation request to the device and block for a response.
    fn cursor_request<Req: IntoBytes + Immutable>(&mut self, req: Req) -> Result {
        self.cursor_queue
            .add_notify_wait_pop(&[req.as_bytes()], &mut [], &mut self.transport)?;
        Ok(())
    }

    /// Send a request with additional data (as a second device-readable buffer)
    /// and block for a response. Used by SUBMIT_3D where the virgl command stream
    /// is sent as a separate scatter-gather buffer alongside the CtrlHeader.
    fn request_with_data<Req: IntoBytes + Immutable, Rsp: FromBytes + IntoBytes>(
        &mut self,
        req: Req,
        data: &[u8],
    ) -> Result<Rsp> {
        let mut response = Rsp::new_zeroed();
        if data.is_empty() {
            self.control_queue.add_notify_wait_pop(
                &[req.as_bytes()],
                &mut [response.as_mut_bytes()],
                &mut self.transport,
            )?;
        } else {
            self.control_queue.add_notify_wait_pop(
                &[req.as_bytes(), data],
                &mut [response.as_mut_bytes()],
                &mut self.transport,
            )?;
        }
        Ok(response)
    }

    fn get_display_info(&mut self) -> Result<RespDisplayInfo> {
        let info: RespDisplayInfo =
            self.request(CtrlHeader::with_type(Command::GET_DISPLAY_INFO))?;
        info.header.check_type(Command::OK_DISPLAY_INFO)?;
        Ok(info)
    }

    /// Create a 2D resource with the given dimensions.
    ///
    /// Format is always `B8G8R8A8UNORM`. Use [`VirtIOGpu::resource_attach_backing`]
    /// to give the resource guest-visible memory, and [`VirtIOGpu::set_scanout`] to
    /// bind it as the display output.
    pub fn resource_create_2d(&mut self, resource_id: u32, width: u32, height: u32) -> Result {
        let rsp: CtrlHeader = self.request(ResourceCreate2D {
            header: CtrlHeader::with_type(Command::RESOURCE_CREATE_2D),
            resource_id,
            format: Format::B8G8R8A8UNORM,
            width,
            height,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Bind a resource as the scanout (display output) for the given scanout ID.
    /// The `rect` specifies the display area.
    pub fn set_scanout(&mut self, rect: Rect, scanout_id: u32, resource_id: u32) -> Result {
        let rsp: CtrlHeader = self.request(SetScanout {
            header: CtrlHeader::with_type(Command::SET_SCANOUT),
            rect,
            scanout_id,
            resource_id,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Flush a resource's contents to the display. The `rect` specifies the
    /// area to refresh.
    pub fn resource_flush(&mut self, rect: Rect, resource_id: u32) -> Result {
        let rsp: CtrlHeader = self.request(ResourceFlush {
            header: CtrlHeader::with_type(Command::RESOURCE_FLUSH),
            rect,
            resource_id,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Transfer data from guest to host for a 2D resource within the given rectangle.
    pub fn transfer_to_host_2d(&mut self, rect: Rect, offset: u64, resource_id: u32) -> Result {
        let rsp: CtrlHeader = self.request(TransferToHost2D {
            header: CtrlHeader::with_type(Command::TRANSFER_TO_HOST_2D),
            rect,
            offset,
            resource_id,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Attach a single DMA-backed memory region to a resource.
    ///
    /// The host uses `paddr` to read/write guest memory for the resource.
    /// This must be called after [`VirtIOGpu::resource_create_2d`] and before any
    /// [`VirtIOGpu::transfer_to_host_2d`] or [`VirtIOGpu::set_scanout`].
    pub fn resource_attach_backing(&mut self, resource_id: u32, paddr: u64, length: u32) -> Result {
        let rsp: CtrlHeader = self.request(ResourceAttachBackingSingleEntry {
            header: ResourceAttachBacking {
                header: CtrlHeader::with_type(Command::RESOURCE_ATTACH_BACKING),
                resource_id,
                nr_entries: 1,
            },
            entry: MemEntry {
                addr: paddr,
                length,
                padding: 0,
            },
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Detach the backing memory from a resource.
    ///
    /// Call this before [`VirtIOGpu::resource_unref`] to release the host's mapping
    /// of the guest memory region.
    pub fn resource_detach_backing(&mut self, resource_id: u32) -> Result {
        let rsp: CtrlHeader = self.request(ResourceDetachBacking {
            header: CtrlHeader::with_type(Command::RESOURCE_DETACH_BACKING),
            resource_id,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Unreference a resource by its ID.
    ///
    /// This releases the host-side resource and is the only way to destroy
    /// a 3D resource (there is no RESOURCE_DESTROY_3D command — the protocol
    /// uses a single RESOURCE_UNREF for both 2D and 3D resources).
    pub fn resource_unref(&mut self, resource_id: u32) -> Result {
        let rsp: CtrlHeader = self.request(ResourceUnref {
            header: CtrlHeader::with_type(Command::RESOURCE_UNREF),
            resource_id,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    #[allow(clippy::too_many_arguments)]
    fn update_cursor(
        &mut self,
        resource_id: u32,
        scanout_id: u32,
        pos_x: u32,
        pos_y: u32,
        hot_x: u32,
        hot_y: u32,
        is_move: bool,
    ) -> Result {
        self.cursor_request(UpdateCursor {
            header: if is_move {
                CtrlHeader::with_type(Command::MOVE_CURSOR)
            } else {
                CtrlHeader::with_type(Command::UPDATE_CURSOR)
            },
            pos: CursorPos {
                scanout_id,
                x: pos_x,
                y: pos_y,
                _padding: 0,
            },
            resource_id,
            hot_x,
            hot_y,
            _padding: 0,
        })
    }

    // ------------------------------------------------------------------
    // Public 3D (virgl) API — only valid when `has_virgl` returns true.
    // These map to VirtIO-GPU spec §5.7.5, Table 5.7.5.2.
    // ------------------------------------------------------------------

    /// Whether the VIRGL (3D acceleration) feature was successfully negotiated.
    pub fn has_virgl(&self) -> bool {
        self.has_virgl
    }

    /// Whether `VIRTIO_GPU_F_RESOURCE_BLOB` was negotiated. Blob resources are
    /// required for host-visible memory and dma-buf sharing (PRIME).
    pub fn has_resource_blob(&self) -> bool {
        self.has_resource_blob
    }

    /// Returns `Err` if the VIRGL feature was not negotiated. All 3D commands
    /// are undefined without it and the host would reject them anyway.
    fn require_virgl(&self) -> Result {
        if self.has_virgl {
            Ok(())
        } else {
            Err(Error::IoError)
        }
    }

    /// Query capset information by index (0-based).
    ///
    /// The returned `RespCapsetInfo` contains the capset ID, max version, and max
    /// data size. Callers should then use `get_capset` to retrieve the actual
    /// capset data.
    pub fn get_capset_info(&mut self, capset_index: u32) -> Result<RespCapsetInfo> {
        self.require_virgl()?;
        let rsp: RespCapsetInfo = self.request(CmdGetCapsetInfo {
            header: CtrlHeader::with_type(Command::GET_CAPSET_INFO),
            capset_index,
            _padding: 0,
        })?;
        rsp.header.check_type(Command::OK_CAPSET_INFO)?;
        Ok(rsp)
    }

    /// Retrieve capset data for a given capset ID and version.
    ///
    /// `size` must be the capset's `capset_max_size` as returned by
    /// [`get_capset_info`](Self::get_capset_info), bounded by the receive buffer.
    /// Returns `Err` rather than panicking if `size` exceeds the buffer.
    pub fn get_capset(&mut self, capset_id: u32, version: u32, size: u32) -> Result<Vec<u8>> {
        self.require_virgl()?;
        let mut extra_response = vec![0; size as usize];
        // The response is a CtrlHeader (24 bytes) followed by the capset data,
        // all written back into queue_buf_recv. `size` is only an upper bound:
        // slice by the bytes the device actually wrote (`used_len`) so no
        // stale receive-buffer bytes leak into the returned blob.
        let (hdr, used_len): (CtrlHeader, usize) = self.request_with_extra_response(
            CmdGetCapset {
                header: CtrlHeader::with_type(Command::GET_CAPSET),
                capset_id,
                capset_version: version,
            },
            &mut extra_response,
        )?;
        hdr.check_type(Command::OK_CAPSET)?;
        extra_response.truncate(used_len);
        Ok(extra_response)
    }

    /// Create a 3D rendering context.
    ///
    /// `ctx_id` is assigned by the upper layer. `name` is a debug label for the
    /// host (truncated to 64 bytes).
    ///
    /// `context_init` carries the capset_id (e.g. 1 for VIRGL, 2 for VIRGL2).
    /// Linux: `vfpriv->context_init` → `cmd_p->context_init` in the wire command.
    pub fn ctx_create(&mut self, ctx_id: u32, name: &str, context_init: u32) -> Result {
        self.require_virgl()?;
        let mut cmd = CmdCtxCreate {
            header: CtrlHeader::with_type_and_ctx(Command::CTX_CREATE, ctx_id),
            nlen: 0,
            context_init,
            debug_name: [0u8; 64],
        };
        let bytes = name.as_bytes();
        let nlen = bytes.len().min(64);
        cmd.debug_name[..nlen].copy_from_slice(&bytes[..nlen]);
        cmd.nlen = nlen as u32;

        let rsp: CtrlHeader = self.request(cmd)?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Destroy a 3D rendering context.
    pub fn ctx_destroy(&mut self, ctx_id: u32) -> Result {
        self.require_virgl()?;
        let rsp: CtrlHeader =
            self.request(CtrlHeader::with_type_and_ctx(Command::CTX_DESTROY, ctx_id))?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Attach a resource to a 3D context.
    pub fn ctx_attach_resource(&mut self, ctx_id: u32, resource_id: u32) -> Result {
        self.require_virgl()?;
        let rsp: CtrlHeader = self.request(CmdCtxResource {
            header: CtrlHeader::with_type_and_ctx(Command::CTX_ATTACH_RESOURCE, ctx_id),
            resource_id,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Detach a resource from a 3D context.
    pub fn ctx_detach_resource(&mut self, ctx_id: u32, resource_id: u32) -> Result {
        self.require_virgl()?;
        let rsp: CtrlHeader = self.request(CmdCtxResource {
            header: CtrlHeader::with_type_and_ctx(Command::CTX_DETACH_RESOURCE, ctx_id),
            resource_id,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Create a 3D resource (texture, render target, buffer, etc.).
    ///
    /// `target`, `format`, `bind`, `flags`, etc. are pipe-level constants
    /// (e.g. PIPE_TEXTURE_2D, PIPE_FORMAT_B8G8R8A8_UNORM, PIPE_BIND_RENDER_TARGET).
    /// The virtio-drivers layer does not define these — the caller passes raw
    /// values from the Gallium/Mesa headers.
    #[allow(clippy::too_many_arguments)]
    pub fn resource_create_3d(
        &mut self,
        ctx_id: u32,
        resource_id: u32,
        target: u32,
        format: u32,
        bind: u32,
        width: u32,
        height: u32,
        depth: u32,
        array_size: u32,
        last_level: u32,
        nr_samples: u32,
        flags: u32,
    ) -> Result {
        self.require_virgl()?;
        let rsp: CtrlHeader = self.request(CmdResourceCreate3D {
            header: CtrlHeader::with_type_and_ctx(Command::RESOURCE_CREATE_3D, ctx_id),
            resource_id,
            target,
            format,
            bind,
            width,
            height,
            depth,
            array_size,
            last_level,
            nr_samples,
            flags,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Transfer data from guest to host for a 3D resource.
    #[allow(clippy::too_many_arguments)]
    pub fn transfer_to_host_3d(
        &mut self,
        ctx_id: u32,
        resource_id: u32,
        box_: GpuBox,
        offset: u64,
        level: u32,
        stride: u32,
        layer_stride: u32,
    ) -> Result {
        self.require_virgl()?;
        let rsp: CtrlHeader = self.request(CmdTransferHost3D {
            header: CtrlHeader::with_type_and_ctx(Command::TRANSFER_TO_HOST_3D, ctx_id),
            box_,
            offset,
            resource_id,
            level,
            stride,
            layer_stride,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Transfer data from host to guest for a 3D resource.
    #[allow(clippy::too_many_arguments)]
    pub fn transfer_from_host_3d(
        &mut self,
        ctx_id: u32,
        resource_id: u32,
        box_: GpuBox,
        offset: u64,
        level: u32,
        stride: u32,
        layer_stride: u32,
    ) -> Result {
        self.require_virgl()?;
        let rsp: CtrlHeader = self.request(CmdTransferHost3D {
            header: CtrlHeader::with_type_and_ctx(Command::TRANSFER_FROM_HOST_3D, ctx_id),
            box_,
            offset,
            resource_id,
            level,
            stride,
            layer_stride,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Submit a virgl command stream to a 3D context.
    ///
    /// `cmds` is the encoded virgl command stream produced by the Mesa virgl
    /// Gallium driver in userspace. It is sent as a separate scatter-gather
    /// buffer alongside the SUBMIT_3D header.
    ///
    /// `fence_id` is assigned by the upper layer. The host signals the fence
    /// once the command stream has been fully processed.
    pub fn submit_3d(&mut self, ctx_id: u32, fence_id: u64, cmds: &[u8]) -> Result {
        self.require_virgl()?;
        // The virgl command stream is a sequence of 32-bit dwords; the host
        // passes `size / 4` dwords to `virgl_renderer_submit_cmd`, so a length
        // that is not a multiple of 4 would be silently truncated.
        if !cmds.len().is_multiple_of(4) {
            return Err(Error::InvalidParam);
        }
        let size = u32::try_from(cmds.len()).map_err(|_| Error::IoError)?;
        let rsp: CtrlHeader = self.request_with_data(
            CmdSubmit3D {
                header: CtrlHeader::with_fence(Command::SUBMIT_3D, ctx_id, fence_id),
                size,
                _padding: 0,
            },
            cmds,
        )?;
        rsp.check_type(Command::OK_NODATA)
    }

    /// Create a blob resource (host-visible memory / dma-buf sharing).
    ///
    /// Requires `VIRTIO_GPU_F_RESOURCE_BLOB` (see [`VirtIOGpu::has_resource_blob`]).
    ///
    /// - `blob_mem`: `VIRTIO_GPU_BLOB_MEM_GUEST (0x1)`, `HOST3D (0x2)` or
    ///   `HOST3D_GUEST (0x3)`.
    /// - `blob_flags`: `VIRTIO_GPU_BLOB_FLAG_USE_MAPPABLE (0x1)` etc.
    /// - `size`: resource size in bytes.
    /// - `blob_id`: for HOST3D blobs, the id of a host-side resource already
    ///   created on this context via virgl (a bare `resource_create_blob`
    ///   cannot create it); for GUEST/HOST3D_GUEST blobs, 0 unless assigning one.
    /// - `mem_entries`: guest backing `(PhysAddr, len)` pairs, only for GUEST /
    ///   HOST3D_GUEST. **HOST3D must pass an empty slice** — QEMU ignores
    ///   backing entries for HOST3D blobs, and virglrenderer rejects blobs
    ///   with a nonzero `num_iovs`.
    ///
    /// # Safety
    ///
    /// For GUEST and HOST3D_GUEST blobs the device reads/writes the guest
    /// memory at the physical addresses in `mem_entries`. The caller must
    /// guarantee that every range is valid, device-accessible memory and
    /// stays allocated and free of concurrent access (no aliasing/UB) for
    /// as long as the blob resource exists — i.e. until the matching
    /// [`VirtIOGpu::resource_unref`]. The ranges must also cover `size` bytes
    /// in total: a blob larger than its backing lets the device access guest
    /// memory past the end of the provided ranges.
    ///
    /// Mirrors Linux `virtio_gpu_cmd_resource_create_blob` (`virtgpu_vq.c`).
    /// Responds with OK_NODATA.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn resource_create_blob(
        &mut self,
        ctx_id: u32,
        resource_id: u32,
        blob_mem: u32,
        blob_flags: u32,
        size: u64,
        blob_id: u64,
        mem_entries: &[(PhysAddr, usize)],
    ) -> Result {
        self.require_virgl()?;
        if !self.has_resource_blob {
            return Err(Error::Unsupported);
        }
        let nr_entries = u32::try_from(mem_entries.len()).map_err(|_| Error::IoError)?;
        let mut data = Vec::with_capacity(mem_entries.len() * core::mem::size_of::<MemEntry>());
        for (addr, len) in mem_entries {
            let length = u32::try_from(*len).map_err(|_| Error::InvalidParam)?;
            data.extend_from_slice(
                MemEntry {
                    addr: *addr,
                    length,
                    padding: 0,
                }
                .as_bytes(),
            );
        }
        let rsp: CtrlHeader = self.request_with_data(
            CmdResourceCreateBlob {
                header: CtrlHeader::with_type_and_ctx(Command::RESOURCE_CREATE_BLOB, ctx_id),
                resource_id,
                blob_mem,
                blob_flags,
                nr_entries,
                blob_id,
                size,
            },
            &data,
        )?;
        rsp.check_type(Command::OK_NODATA)
    }
}

impl<H: Hal, T: Transport> Drop for VirtIOGpu<H, T> {
    fn drop(&mut self) {
        // Clear any pointers pointing to DMA regions, so the device doesn't try to access them
        // after they have been freed.
        self.transport.queue_unset(QUEUE_TRANSMIT);
        self.transport.queue_unset(QUEUE_CURSOR);
    }
}

#[repr(C)]
#[derive(FromBytes, Immutable, IntoBytes)]
pub struct Config {
    /// Signals pending events to the driver。
    pub events_read: ReadOnly<u32>,

    /// Clears pending events in the device.
    pub events_clear: WriteOnly<u32>,

    /// Specifies the maximum number of scanouts supported by the device.
    ///
    /// Minimum value is 1, maximum value is 16.
    pub num_scanouts: ReadOnly<u32>,

    /// Specifies the number of capsets supported by the device.
    pub num_capsets: ReadOnly<u32>,

    /// Specifies the alignment requirement for blob resources.
    /// Valid when `VIRTIO_GPU_F_BLOB_ALIGNMENT` is negotiated.
    pub blob_alignment: ReadOnly<u32>,
}

/// Display configuration has changed.
const EVENT_DISPLAY: u32 = 1 << 0;

bitflags! {
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct Features: u64 {
        /// virgl 3D mode is supported.
        const VIRGL                 = 1 << 0;
        /// EDID is supported.
        const EDID                  = 1 << 1;
        /// Context init protocol (virgl2 capset, per-fd context). bit 4 per Linux UAPI!
        const CONTEXT_INIT          = 1 << 4;
        /// Blob resources (host-visible memory, dma-buf sharing) are supported.
        const RESOURCE_BLOB         = 1 << 3;
        /// Blob resource alignment requirements are exposed in the device configuration.
        const BLOB_ALIGNMENT        = 1 << 5;

        // device independent
        const NOTIFY_ON_EMPTY       = 1 << 24; // legacy
        const ANY_LAYOUT            = 1 << 27; // legacy
        const RING_INDIRECT_DESC    = 1 << 28;
        const RING_EVENT_IDX        = 1 << 29;
        const UNUSED                = 1 << 30; // legacy
        const VERSION_1             = 1 << 32; // detect legacy

        // since virtio v1.1
        const ACCESS_PLATFORM       = 1 << 33;
        const RING_PACKED           = 1 << 34;
        const IN_ORDER              = 1 << 35;
        const ORDER_PLATFORM        = 1 << 36;
        const SR_IOV                = 1 << 37;
        const NOTIFICATION_DATA     = 1 << 38;
    }
}

/// Specifies the type of the driver request or the device response
#[repr(transparent)]
#[derive(Clone, Copy, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq)]
pub struct Command(u32);

impl Command {
    pub const GET_DISPLAY_INFO: Command = Command(0x100);
    pub const RESOURCE_CREATE_2D: Command = Command(0x101);
    pub const RESOURCE_UNREF: Command = Command(0x102);
    pub const SET_SCANOUT: Command = Command(0x103);
    pub const RESOURCE_FLUSH: Command = Command(0x104);
    pub const TRANSFER_TO_HOST_2D: Command = Command(0x105);
    pub const RESOURCE_ATTACH_BACKING: Command = Command(0x106);
    pub const RESOURCE_DETACH_BACKING: Command = Command(0x107);
    pub const GET_CAPSET_INFO: Command = Command(0x108);
    pub const GET_CAPSET: Command = Command(0x109);
    pub const GET_EDID: Command = Command(0x10a);
    pub const RESOURCE_ASSIGN_UUID: Command = Command(0x10b);
    pub const RESOURCE_CREATE_BLOB: Command = Command(0x10c);
    pub const SET_SCANOUT_BLOB: Command = Command(0x10d);

    // 3D commands (VirtIO-GPU spec Section 5.7.5, Table 5.7.5.2)
    pub const CTX_CREATE: Command = Command(0x0200);
    pub const CTX_DESTROY: Command = Command(0x0201);
    pub const CTX_ATTACH_RESOURCE: Command = Command(0x0202);
    pub const CTX_DETACH_RESOURCE: Command = Command(0x0203);
    pub const RESOURCE_CREATE_3D: Command = Command(0x0204);
    pub const TRANSFER_TO_HOST_3D: Command = Command(0x0205);
    pub const TRANSFER_FROM_HOST_3D: Command = Command(0x0206);
    pub const SUBMIT_3D: Command = Command(0x0207);
    pub const RESOURCE_MAP_BLOB: Command = Command(0x0208);
    pub const RESOURCE_UNMAP_BLOB: Command = Command(0x0209);

    // Cursor commands
    pub const UPDATE_CURSOR: Command = Command(0x300);
    pub const MOVE_CURSOR: Command = Command(0x301);

    // Success responses
    pub const OK_NODATA: Command = Command(0x1100);
    pub const OK_DISPLAY_INFO: Command = Command(0x1101);
    pub const OK_CAPSET_INFO: Command = Command(0x1102);
    pub const OK_CAPSET: Command = Command(0x1103);
    pub const OK_EDID: Command = Command(0x1104);
    pub const OK_RESOURCE_UUID: Command = Command(0x1105);
    pub const OK_MAP_INFO: Command = Command(0x1106);

    // Error responses
    pub const ERR_UNSPEC: Command = Command(0x1200);
    pub const ERR_OUT_OF_MEMORY: Command = Command(0x1201);
    pub const ERR_INVALID_SCANOUT_ID: Command = Command(0x1202);
    pub const ERR_INVALID_RESOURCE_ID: Command = Command(0x1203);
    pub const ERR_INVALID_CONTEXT_ID: Command = Command(0x1204);
    pub const ERR_INVALID_PARAMETER: Command = Command(0x1205);

    pub fn is_response(&self) -> bool {
        self.0 >= Self::OK_NODATA.0
    }

    pub fn is_error(&self) -> bool {
        self.0 >= Self::ERR_UNSPEC.0
    }

    pub fn is_cursor(&self) -> bool {
        self.0 == Self::UPDATE_CURSOR.0 || self.0 == Self::MOVE_CURSOR.0
    }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Command::GET_DISPLAY_INFO => f.write_str("GET_DISPLAY_INFO"),
            Command::RESOURCE_CREATE_2D => f.write_str("RESOURCE_CREATE_2D"),
            Command::RESOURCE_UNREF => f.write_str("RESOURCE_UNREF"),
            Command::SET_SCANOUT => f.write_str("SET_SCANOUT"),
            Command::RESOURCE_FLUSH => f.write_str("RESOURCE_FLUSH"),
            Command::TRANSFER_TO_HOST_2D => f.write_str("TRANSFER_TO_HOST_2D"),
            Command::RESOURCE_ATTACH_BACKING => f.write_str("RESOURCE_ATTACH_BACKING"),
            Command::RESOURCE_DETACH_BACKING => f.write_str("RESOURCE_DETACH_BACKING"),
            Command::GET_CAPSET_INFO => f.write_str("GET_CAPSET_INFO"),
            Command::GET_CAPSET => f.write_str("GET_CAPSET"),
            Command::GET_EDID => f.write_str("GET_EDID"),
            Command::RESOURCE_ASSIGN_UUID => f.write_str("RESOURCE_ASSIGN_UUID"),
            Command::RESOURCE_CREATE_BLOB => f.write_str("RESOURCE_CREATE_BLOB"),
            Command::SET_SCANOUT_BLOB => f.write_str("SET_SCANOUT_BLOB"),
            Command::CTX_CREATE => f.write_str("CTX_CREATE"),
            Command::CTX_DESTROY => f.write_str("CTX_DESTROY"),
            Command::CTX_ATTACH_RESOURCE => f.write_str("CTX_ATTACH_RESOURCE"),
            Command::CTX_DETACH_RESOURCE => f.write_str("CTX_DETACH_RESOURCE"),
            Command::RESOURCE_CREATE_3D => f.write_str("RESOURCE_CREATE_3D"),
            Command::TRANSFER_TO_HOST_3D => f.write_str("TRANSFER_TO_HOST_3D"),
            Command::TRANSFER_FROM_HOST_3D => f.write_str("TRANSFER_FROM_HOST_3D"),
            Command::SUBMIT_3D => f.write_str("SUBMIT_3D"),
            Command::RESOURCE_MAP_BLOB => f.write_str("RESOURCE_MAP_BLOB"),
            Command::RESOURCE_UNMAP_BLOB => f.write_str("RESOURCE_UNMAP_BLOB"),
            Command::UPDATE_CURSOR => f.write_str("UPDATE_CURSOR"),
            Command::MOVE_CURSOR => f.write_str("MOVE_CURSOR"),
            Command::OK_NODATA => f.write_str("OK_NODATA"),
            Command::OK_DISPLAY_INFO => f.write_str("OK_DISPLAY_INFO"),
            Command::OK_CAPSET_INFO => f.write_str("OK_CAPSET_INFO"),
            Command::OK_CAPSET => f.write_str("OK_CAPSET"),
            Command::OK_EDID => f.write_str("OK_EDID"),
            Command::OK_RESOURCE_UUID => f.write_str("OK_RESOURCE_UUID"),
            Command::OK_MAP_INFO => f.write_str("OK_MAP_INFO"),
            Command::ERR_UNSPEC => f.write_str("ERR_UNSPEC"),
            Command::ERR_OUT_OF_MEMORY => f.write_str("ERR_OUT_OF_MEMORY"),
            Command::ERR_INVALID_SCANOUT_ID => f.write_str("ERR_INVALID_SCANOUT_ID"),
            Command::ERR_INVALID_RESOURCE_ID => f.write_str("ERR_INVALID_RESOURCE_ID"),
            Command::ERR_INVALID_CONTEXT_ID => f.write_str("ERR_INVALID_CONTEXT_ID"),
            Command::ERR_INVALID_PARAMETER => f.write_str("ERR_INVALID_PARAMETER"),
            _ => write!(f, "Command({:#x})", self.0),
        }
    }
}

pub const GPU_FLAG_FENCE: u32 = 1 << 0;
pub const GPU_FLAG_RING_INDEX: u32 = 1 << 1;

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, Immutable, IntoBytes, KnownLayout)]
pub struct CtrlHeader {
    pub hdr_type: Command,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    /// Ring index for fence. Only used when VIRTIO_GPU_FLAG_INFO_RING_IDX is set
    /// in `flags` (requires VIRTIO_GPU_F_CONTEXT_INIT, which is a virgl2 feature).
    pub ring_idx: u8,
    pub _padding: [u8; 3],
}

impl CtrlHeader {
    pub fn with_type(hdr_type: Command) -> CtrlHeader {
        CtrlHeader {
            hdr_type,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            _padding: [0; 3],
        }
    }

    /// Create a CtrlHeader with the given command type and context ID.
    /// Used for 3D commands that target a specific rendering context.
    pub fn with_type_and_ctx(hdr_type: Command, ctx_id: u32) -> CtrlHeader {
        CtrlHeader {
            hdr_type,
            flags: 0,
            fence_id: 0,
            ctx_id,
            ring_idx: 0,
            _padding: [0; 3],
        }
    }

    /// Create a CtrlHeader with the given command type, context ID, and fence.
    /// Used for SUBMIT_3D to signal fence completion after the command stream
    /// has been processed.
    pub fn with_fence(hdr_type: Command, ctx_id: u32, fence_id: u64) -> CtrlHeader {
        CtrlHeader {
            hdr_type,
            flags: GPU_FLAG_FENCE,
            fence_id,
            ctx_id,
            ring_idx: 0,
            _padding: [0; 3],
        }
    }

    /// Return error if the type is not same as expected.
    pub fn check_type(&self, expected: Command) -> Result {
        if self.hdr_type == expected {
            Ok(())
        } else {
            Err(Error::IoError)
        }
    }
}

/// Rectangle region used by 2D operations (scanout, flush, transfer).
#[repr(C)]
#[derive(Debug, Copy, Clone, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
pub struct Rect {
    /// X offset.
    pub x: u32,
    /// Y offset.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct DisplayOne {
    pub rect: Rect,
    pub enabled: u32,
    pub flags: u32,
}

/// VIRTIO_GPU_RESP_OK_DISPLAY_INFO
pub const VIRTIO_GPU_MAX_SCANOUTS: usize = 16;
#[derive(Copy, Clone, Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct RespDisplayInfo {
    pub header: CtrlHeader,
    pub pmodes: [DisplayOne; VIRTIO_GPU_MAX_SCANOUTS],
}

#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct CmdGetEdid {
    pub header: CtrlHeader,
    pub scanout: u32,
    pub _padding: u32,
}

#[repr(C)]
#[derive(Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
pub struct RespEdid {
    pub header: CtrlHeader,
    pub size: u32,
    pub _padding: u32,
    pub edid: [u8; 1024],
}

#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct ResourceCreate2D {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub format: Format,
    pub width: u32,
    pub height: u32,
}

#[repr(u32)]
#[derive(Copy, Clone, Debug, Immutable, IntoBytes, KnownLayout)]
pub enum Format {
    B8G8R8A8UNORM = 1,
    B8G8R8X8UNORM = 2,
    A8R8G8B8UNORM = 3,
    X8R8G8B8UNORM = 4,
    R8G8B8A8UNORM = 67,
    X8B8G8R8UNORM = 68,
    A8B8G8R8UNORM = 121,
    R8G8B8X8UNORM = 134,
}

impl Format {
    pub fn stride(&self) -> usize {
        match self {
            Format::B8G8R8A8UNORM => 4,
            Format::B8G8R8X8UNORM => 4,
            Format::A8R8G8B8UNORM => 4,
            Format::X8R8G8B8UNORM => 4,
            Format::R8G8B8A8UNORM => 4,
            Format::X8B8G8R8UNORM => 4,
            Format::A8B8G8R8UNORM => 4,
            Format::R8G8B8X8UNORM => 4,
        }
    }
}

#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, FromBytes, KnownLayout)]
pub struct ResourceAttachBacking {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub nr_entries: u32,
}

#[repr(C)]
#[derive(Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
pub struct ResourceAttachBackingSingleEntry {
    pub header: ResourceAttachBacking,
    pub entry: MemEntry,
}

#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct ResourceDetachBacking {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
}

#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, FromBytes, KnownLayout)]
pub struct ResourceUnref {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
}

#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct SetScanout {
    pub header: CtrlHeader,
    pub rect: Rect,
    pub scanout_id: u32,
    pub resource_id: u32,
}

#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct TransferToHost2D {
    pub header: CtrlHeader,
    pub rect: Rect,
    pub offset: u64,
    pub resource_id: u32,
    pub _padding: u32,
}

#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct ResourceFlush {
    pub header: CtrlHeader,
    pub rect: Rect,
    pub resource_id: u32,
    pub _padding: u32,
}

// -----------------------------------------------------------------------
// 3D command/response structures (VirtIO-GPU spec §5.7.5, Table 5.7.5.2)
// -----------------------------------------------------------------------

/// 3D box used by TRANSFER_TO_HOST_3D and TRANSFER_FROM_HOST_3D.
/// Describes a sub-rectangle within a 3D texture.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Immutable, IntoBytes, KnownLayout)]
pub struct GpuBox {
    /// X offset within the resource.
    pub x: u32,
    /// Y offset within the resource.
    pub y: u32,
    /// Z offset within the resource.
    pub z: u32,
    /// Width of the sub-region.
    pub w: u32,
    /// Height of the sub-region.
    pub h: u32,
    /// Depth of the sub-region.
    pub d: u32,
}

/// VIRTIO_GPU_CMD_GET_CAPSET_INFO
#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct CmdGetCapsetInfo {
    pub header: CtrlHeader,
    pub capset_index: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_RESP_OK_CAPSET_INFO
#[repr(C)]
#[derive(Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
pub struct RespCapsetInfo {
    pub header: CtrlHeader,
    /// Capset ID (1 = VIRGL, 2 = VIRGL2).
    pub capset_id: u32,
    /// Maximum version of this capset supported by the device.
    pub capset_max_version: u32,
    /// Maximum size in bytes of one capset data blob.
    pub capset_max_size: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_GET_CAPSET
#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct CmdGetCapset {
    pub header: CtrlHeader,
    pub capset_id: u32,
    pub capset_version: u32,
}

/// VIRTIO_GPU_CMD_CTX_CREATE
#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct CmdCtxCreate {
    pub header: CtrlHeader,
    /// Length of the debug name (0..64).
    pub nlen: u32,
    /// Context init flags (used by virgl2 / VIRTIO_GPU_F_CONTEXT_INIT, 0 for virgl1).
    pub context_init: u32,
    /// Null-terminated debug name, 64 bytes.
    pub debug_name: [u8; 64],
}

/// VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE
#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct CmdCtxResource {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_RESOURCE_CREATE_3D
#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct CmdResourceCreate3D {
    pub header: CtrlHeader,
    pub resource_id: u32,
    /// Pipe texture target (PIPE_TEXTURE_2D, PIPE_TEXTURE_3D, PIPE_BUFFER, …).
    pub target: u32,
    /// Pipe format (PIPE_FORMAT_*).
    pub format: u32,
    /// Pipe bind flags (PIPE_BIND_*).
    pub bind: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    /// Number of mipmap levels.
    pub last_level: u32,
    /// Number of MSAA samples.
    pub nr_samples: u32,
    /// Resource flags (VIRTIO_GPU_RESOURCE_FLAG_* from the Linux UAPI).
    pub flags: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D, VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D
#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct CmdTransferHost3D {
    pub header: CtrlHeader,
    pub box_: GpuBox,
    /// Byte offset within the resource.
    pub offset: u64,
    pub resource_id: u32,
    /// Mipmap level.
    pub level: u32,
    /// Row stride in bytes.
    pub stride: u32,
    /// Layer stride in bytes (for array textures and 3D textures).
    pub layer_stride: u32,
}

/// VIRTIO_GPU_CMD_SUBMIT_3D
#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, FromBytes, KnownLayout)]
pub struct CmdSubmit3D {
    pub header: CtrlHeader,
    /// Size of the virgl command stream that follows in a separate buffer.
    pub size: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB
///
/// Field order matches `struct virtio_gpu_resource_create_blob` in the UAPI
/// header exactly: `resource_id, blob_mem, blob_flags, nr_entries, blob_id, size`.
/// `nr_entries > 0` mem entries follow as a separate buffer (see
/// [`VirtIOGpu::resource_create_blob`]).
#[repr(C)]
#[derive(Debug, Immutable, IntoBytes, KnownLayout)]
pub struct CmdResourceCreateBlob {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub blob_mem: u32,
    pub blob_flags: u32,
    pub nr_entries: u32,
    pub blob_id: u64,
    pub size: u64,
}

/// One entry of a blob resource backing (guest memory for GUEST / HOST3D_GUEST).
/// Matches `struct virtio_gpu_mem_entry { __le64 addr; __le32 length; __le32 padding; }`.
#[repr(C)]
#[derive(Debug, Copy, Clone, Immutable, IntoBytes, FromBytes, KnownLayout)]
pub struct MemEntry {
    pub addr: u64,
    pub length: u32,
    pub padding: u32,
}

/// VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB
#[derive(Copy, Clone, Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct CmdResourceMapBlob {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
    pub offset: u64,
}

/// VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB
#[derive(Copy, Clone, Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct CmdResourceUnmapBlob {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
}

pub const VIRTIO_GPU_MAP_CACHE_NONE: u32 = 0x00;
pub const VIRTIO_GPU_MAP_CACHE_CACHED: u32 = 0x01;
pub const VIRTIO_GPU_MAP_CACHE_UNCACHED: u32 = 0x02;
pub const VIRTIO_GPU_MAP_CACHE_WC: u32 = 0x03;

#[derive(Copy, Clone, Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct RespMapInfo {
    pub header: CtrlHeader,
    pub map_info: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_RESOURCE_ASSIGN_UUID
#[derive(Copy, Clone, Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct CmdResourceAssignUuid {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
}

#[derive(Copy, Clone, Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct RespResourceUuid {
    pub header: CtrlHeader,
    pub uuid: [u8; 16],
}

/// VIRTIO_GPU_CMD_SET_SCANOUT_BLOB
#[derive(Copy, Clone, Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct SetScanoutBlob {
    pub header: CtrlHeader,
    pub rect: Rect,
    pub scanout_id: u32,
    pub resource_id: u32,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub _padding: u32,
    pub strides: [u32; 4],
    pub offsets: [u32; 4],
}

// -----------------------------------------------------------------------
// End of 3D structures
// -----------------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, Debug, Immutable, IntoBytes, KnownLayout)]
pub struct CursorPos {
    pub scanout_id: u32,
    pub x: u32,
    pub y: u32,
    pub _padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Immutable, IntoBytes, KnownLayout)]
pub struct UpdateCursor {
    pub header: CtrlHeader,
    pub pos: CursorPos,
    pub resource_id: u32,
    pub hot_x: u32,
    pub hot_y: u32,
    pub _padding: u32,
}

/// Control queue
pub const QUEUE_TRANSMIT: u16 = 0;
/// Cursor queue
pub const QUEUE_CURSOR: u16 = 1;

const SCANOUT_ID: u32 = 0;
const RESOURCE_ID_FB: u32 = 0xbabe;
const RESOURCE_ID_CURSOR: u32 = 0xdade;

const CURSOR_RECT: Rect = Rect {
    x: 0,
    y: 0,
    width: 64,
    height: 64,
};
