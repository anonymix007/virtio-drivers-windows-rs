//! Driver for VirtIO GPU devices.

pub mod commands;
mod edid;

pub use self::edid::Edid;

use self::commands::*;

use crate::config::{ReadOnly, WriteOnly, read_config};
use crate::hal::{BufferDirection, Dma, Hal};
use crate::queue::VirtQueue;
use crate::transport::{InterruptStatus, Transport};
use crate::{Error, PAGE_SIZE, Result, pages};
use alloc::boxed::Box;
use alloc::vec::Vec;
use bitflags::bitflags;
use log::info;
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes};

const QUEUE_SIZE: u16 = 2;
const SUPPORTED_FEATURES: Features = Features::RING_EVENT_IDX
    .union(Features::RING_INDIRECT_DESC)
    .union(Features::VERSION_1)
    .union(Features::ACCESS_PLATFORM)
    .union(Features::EDID);

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
    /// Send buffer for queue.
    queue_buf_send: Box<[u8]>,
    /// Recv buffer for queue.
    queue_buf_recv: Box<[u8]>,
    /// Whether EDID feature was negotiated.
    has_edid: bool,
    /// Whether `VIRTIO_F_ACCESS_PLATFORM` was negotiated.
    access_platform: bool,
}

impl<H: Hal, T: Transport> VirtIOGpu<H, T> {
    /// Create a new VirtIO-Gpu driver.
    pub fn new(mut transport: T) -> Result<Self> {
        let negotiated_features = transport.begin_init(SUPPORTED_FEATURES);

        // read configuration space
        let events_read = read_config!(transport, Config, events_read)?;
        let num_scanouts = read_config!(transport, Config, num_scanouts)?;
        info!(
            "events_read: {:#x}, num_scanouts: {:#x}",
            events_read, num_scanouts
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

        let queue_buf_send = FromZeros::new_box_zeroed_with_elems(PAGE_SIZE).unwrap();
        let queue_buf_recv = FromZeros::new_box_zeroed_with_elems(PAGE_SIZE).unwrap();

        transport.finish_init();

        let has_edid = negotiated_features.contains(Features::EDID);

        Ok(VirtIOGpu {
            transport,
            frame_buffer_dma: None,
            cursor_buffer_dma: None,
            rect: None,
            control_queue,
            cursor_queue,
            queue_buf_send,
            queue_buf_recv,
            has_edid,
            access_platform,
        })
    }

    /// Acknowledge interrupt.
    pub fn ack_interrupt(&mut self) -> InterruptStatus {
        self.transport.ack_interrupt()
    }

    /// Get the resolution (width, height).
    pub fn resolution(&mut self) -> Result<(u32, u32)> {
        let display_info = self.get_display_info()?;
        Ok((display_info.pmodes[SCANOUT_ID as usize].rect.width, display_info.pmodes[SCANOUT_ID as usize].rect.height))
    }

    /// Get the EDID data for the specified scanout.
    ///
    /// Returns an [`Edid`] struct wrapping the EDID blob.
    /// Requires the EDID feature to have been negotiated.
    pub fn get_edid(&mut self, scanout: u32) -> Result<Edid> {
        if !self.has_edid {
            return Err(Error::Unsupported);
        }
        let rsp: RespGetEdid = self.request(GetEdid {
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
        self.change_resolution(display_info.pmodes[SCANOUT_ID as usize].rect.width, display_info.pmodes[SCANOUT_ID as usize].rect.height)
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

        self.resource_attach_backing(RESOURCE_ID_FB, frame_buffer_dma.paddr() as u64, size)?;
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
        self.resource_attach_backing(RESOURCE_ID_CURSOR, cursor_buffer_dma.paddr() as u64, size)?;
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
    fn request<Req: IntoBytes + Immutable, Rsp: FromBytes>(&mut self, req: Req) -> Result<Rsp> {
        req.write_to_prefix(&mut self.queue_buf_send).unwrap();
        self.control_queue.add_notify_wait_pop(
            &[&self.queue_buf_send],
            &mut [&mut self.queue_buf_recv],
            &mut self.transport,
        )?;
        Ok(Rsp::read_from_prefix(&self.queue_buf_recv).unwrap().0)
    }

    /// Send a mouse cursor operation request to the device and block for a response.
    fn cursor_request<Req: IntoBytes + Immutable>(&mut self, req: Req) -> Result {
        req.write_to_prefix(&mut self.queue_buf_send).unwrap();
        self.cursor_queue.add_notify_wait_pop(
            &[&self.queue_buf_send],
            &mut [],
            &mut self.transport,
        )?;
        Ok(())
    }

    fn get_display_info(&mut self) -> Result<RespDisplayInfo> {
        let info: RespDisplayInfo =
            self.request(CtrlHeader::with_type(Command::GET_DISPLAY_INFO))?;
        info.header.check_type(Command::OK_DISPLAY_INFO)?;
        Ok(info)
    }

    fn resource_create_2d(&mut self, resource_id: u32, width: u32, height: u32) -> Result {
        let rsp: CtrlHeader = self.request(ResourceCreate2d {
            header: CtrlHeader::with_type(Command::RESOURCE_CREATE_2D),
            resource_id,
            format: Format::B8G8R8A8UNORM as u32,
            width,
            height,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn set_scanout(&mut self, rect: Rect, scanout_id: u32, resource_id: u32) -> Result {
        let rsp: CtrlHeader = self.request(SetScanout {
            header: CtrlHeader::with_type(Command::SET_SCANOUT),
            rect,
            scanout_id,
            resource_id,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn resource_flush(&mut self, rect: Rect, resource_id: u32) -> Result {
        let rsp: CtrlHeader = self.request(ResourceFlush {
            header: CtrlHeader::with_type(Command::RESOURCE_FLUSH),
            rect,
            resource_id,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn transfer_to_host_2d(&mut self, rect: Rect, offset: u64, resource_id: u32) -> Result {
        let rsp: CtrlHeader = self.request(TransferToHost2d {
            header: CtrlHeader::with_type(Command::TRANSFER_TO_HOST_2D),
            rect,
            offset,
            resource_id,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn resource_attach_backing(&mut self, resource_id: u32, paddr: u64, length: u32) -> Result {
        let rsp: CtrlHeader = self.request(ResourceAttachBackingSingleEntry {
            header: ResourceAttachBacking {
                header: CtrlHeader::with_type(Command::RESOURCE_ATTACH_BACKING),
                resource_id,
                nr_entries: 1,
            },
            entry: MemEntry {
                addr: paddr,
                length,
                _padding: 0,
            },
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn resource_detach_backing(&mut self, resource_id: u32) -> Result {
        let rsp: CtrlHeader = self.request(ResourceDetachBacking {
            header: CtrlHeader::with_type(Command::RESOURCE_DETACH_BACKING),
            resource_id,
            _padding: 0,
        })?;
        rsp.check_type(Command::OK_NODATA)
    }

    fn resource_unref(&mut self, resource_id: u32) -> Result {
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
pub struct Config {
    /// Signals pending events to the driver。
    pub events_read: ReadOnly<u32>,

    /// Clears pending events in the device.
    pub events_clear: WriteOnly<u32>,

    /// Specifies the maximum number of scanouts supported by the device.
    ///
    /// Minimum value is 1, maximum value is 16.
    pub num_scanouts: ReadOnly<u32>,

    /// Specifies the maximum number of capability sets supported by the device.
    ///
    /// Minimum value is 0
    pub num_capsets: ReadOnly<u32>,
}

/// Display configuration has changed.
const EVENT_DISPLAY: u32 = 1 << 0;

bitflags! {
    /// Device specific features for virtio based graphics adapter.
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct Features: u64 {
        /// virgl 3D mode is supported.
        const VIRGL                 = 1 << 0;
        /// EDID is supported.
        const EDID                  = 1 << 1;
        /// Assigning UUID to resources is supported.
        const RESOURCE_UUID         = 1 << 2;
        /// Blob resources are supported.
        const RESOURCE_BLOB         = 1 << 3;
        /// Initializing contexts is supported.
        const CONTEXT_INIT          = 1 << 4;

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

pub const QUEUE_TRANSMIT: u16 = 0;
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
