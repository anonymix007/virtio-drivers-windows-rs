/// Stolen from crosvm

use crate::{Error, Result};
use core::{
    cmp::min,
    fmt,
    marker::PhantomData,
    str::from_utf8,
};

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[repr(transparent)]
#[derive(Clone, Copy, Default, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq)]
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

    pub const UPDATE_CURSOR: Command = Command(0x300);
    pub const MOVE_CURSOR: Command = Command(0x301);

    pub const OK_NODATA: Command = Command(0x1100);
    pub const OK_DISPLAY_INFO: Command = Command(0x1101);
    pub const OK_CAPSET_INFO: Command = Command(0x1102);
    pub const OK_CAPSET: Command = Command(0x1103);
    pub const OK_EDID: Command = Command(0x1104);
    pub const OK_RESOURCE_UUID: Command = Command(0x1105);
    pub const OK_MAP_INFO: Command = Command(0x1106);

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
#[derive(Clone, Copy, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
pub struct CtrlHeader {
    pub hdr_type: Command,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
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

    /// Return error if the type is not same as expected.
    pub fn check_type(&self, expected: Command) -> Result {
        if self.hdr_type == expected {
            Ok(())
        } else {
            Err(Error::IoError)
        }
    }
}

/// data passed in the cursor vq

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct CursorPos {
    pub scanout_id: u32,
    pub x: u32,
    pub y: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_UPDATE_CURSOR, VIRTIO_GPU_CMD_MOVE_CURSOR
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct UpdateCursor {
    pub header: CtrlHeader,
    pub pos: CursorPos, /// update & move
    pub resource_id: u32,        /// update only
    pub hot_x: u32,              /// update only
    pub hot_y: u32,              /// update only
    pub _padding: u32,
}

/// data passed in the control vq, 2d related

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// VIRTIO_GPU_CMD_RESOURCE_UNREF
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceUnref {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: create a 2d resource with a format
#[derive(Copy, Clone, Debug, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceCreate2d {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub format: u32,
    pub width: u32,
    pub height: u32,
}

/// VIRTIO_GPU_CMD_SET_SCANOUT
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct SetScanout {
    pub header: CtrlHeader,
    pub rect: Rect,
    pub scanout_id: u32,
    pub resource_id: u32,
}

/// VIRTIO_GPU_CMD_RESOURCE_FLUSH
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceFlush {
    pub header: CtrlHeader,
    pub rect: Rect,
    pub resource_id: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: simple transfer to_host
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct TransferToHost2d {
    pub header: CtrlHeader,
    pub rect: Rect,
    pub offset: u64,
    pub resource_id: u32,
    pub _padding: u32,
}

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct MemEntry {
    pub addr: u64,
    pub length: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceAttachBacking {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub nr_entries: u32,
}

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceAttachBackingSingleEntry {
    pub header: ResourceAttachBacking,
    pub entry: MemEntry,
}

/// VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceDetachBacking {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
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
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct RespDisplayInfo {
    pub header: CtrlHeader,
    pub pmodes: [DisplayOne; VIRTIO_GPU_MAX_SCANOUTS],
}

/// data passed in the control vq, 3d related

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct Box {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub w: u32,
    pub h: u32,
    pub d: u32,
}

/// VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D, VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct TransferHost3d {
    pub header: CtrlHeader,
    pub box_: Box,
    pub offset: u64,
    pub resource_id: u32,
    pub level: u32,
    pub stride: u32,
    pub layer_stride: u32,
}

/// VIRTIO_GPU_CMD_RESOURCE_CREATE_3D
pub const VIRTIO_GPU_RESOURCE_FLAG_Y_0_TOP: u32 = 1 << 0;
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceCreate3d {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub target: u32,
    pub format: u32,
    pub bind: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    pub last_level: u32,
    pub nr_samples: u32,
    pub flags: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_CTX_CREATE
pub const VIRTIO_GPU_CONTEXT_INIT_CAPSET_ID_MASK: u32 = 0x000000ff;
#[derive(Clone, Copy, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct CtxCreate {
    pub header: CtrlHeader,
    pub nlen: u32,
    pub context_init: u32,
    pub debug_name: [u8; 64],
}

impl Default for CtxCreate {
    fn default() -> Self {
        Self {
            header: Default::default(),
            nlen: 0,
            context_init: 0,
            debug_name: [0u8; 64],
        }
    }
}

impl fmt::Debug for CtxCreate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let debug_name = from_utf8(&self.debug_name[..min(64, self.nlen as usize)])
            .unwrap_or("<invalid>");
        f.debug_struct("CtxCreate")
            .field("header", &self.header)
            .field("debug_name", &debug_name)
            .finish()
    }
}

/// VIRTIO_GPU_CMD_CTX_DESTROY
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct CtxDestroy {
    pub header: CtrlHeader,
}

/// VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct CtxResource {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_SUBMIT_3D
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct CmdSubmit3d {
    pub header: CtrlHeader,
    pub size: u32,
    pub _padding: u32,
}

pub const VIRTIO_GPU_CAPSET_VIRGL: u32 = 1;
pub const VIRTIO_GPU_CAPSET_VIRGL2: u32 = 2;
pub const VIRTIO_GPU_CAPSET_GFXSTREAM: u32 = 3;
pub const VIRTIO_GPU_CAPSET_VENUS: u32 = 4;
pub const VIRTIO_GPU_CAPSET_CROSS_DOMAIN: u32 = 5;
pub const VIRTIO_GPU_CAPSET_DRM: u32 = 6;

/// VIRTIO_GPU_CMD_GET_CAPSET_INFO
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct GetCapsetInfo {
    pub header: CtrlHeader,
    pub capset_index: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_RESP_OK_CAPSET_INFO
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct RespCapsetInfo {
    pub header: CtrlHeader,
    pub capset_id: u32,
    pub capset_max_version: u32,
    pub capset_max_size: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_CMD_GET_CAPSET
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct GetCapset {
    pub header: CtrlHeader,
    pub capset_id: u32,
    pub capset_version: u32,
}

/// VIRTIO_GPU_RESP_OK_CAPSET
#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct RespCapset {
    pub header: CtrlHeader,
    pub capset_data: PhantomData<[u8]>,
}

/// VIRTIO_GPU_CMD_GET_EDID
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct GetEdid {
    pub header: CtrlHeader,
    pub scanout: u32,
    pub _padding: u32,
}

/// VIRTIO_GPU_RESP_OK_EDID
#[derive(Copy, Clone, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct RespGetEdid {
    pub header: CtrlHeader,
    pub size: u32,
    pub _padding: u32,
    pub edid: [u8; 1024],
}

pub const VIRTIO_GPU_EVENT_DISPLAY: u32 = 1 << 0;

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceCreateBlob {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub blob_mem: u32,
    pub blob_flags: u32,
    pub nr_entries: u32,
    pub blob_id: u64,
    pub size: u64,
}

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceMapBlob {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
    pub offset: u64,
}

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceUnmapBlob {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
}


pub const VIRTIO_GPU_MAP_CACHE_NONE: u32     = 0x00;
pub const VIRTIO_GPU_MAP_CACHE_CACHED: u32   = 0x01;
pub const VIRTIO_GPU_MAP_CACHE_UNCACHED: u32 = 0x02;
pub const VIRTIO_GPU_MAP_CACHE_WC: u32       = 0x03;

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct RespMapInfo {
    pub header: CtrlHeader,
    pub map_info: u32,
    pub _padding: u32,
}

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct ResourceAssignUuid {
    pub header: CtrlHeader,
    pub resource_id: u32,
    pub _padding: u32,
}

#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
#[repr(C)]
pub struct RespResourceUuid {
    pub header: CtrlHeader,
    pub uuid: [u8; 16],
}

/// VIRTIO_GPU_CMD_SET_SCANOUT_BLOB
#[derive(Copy, Clone, Debug, Default, FromBytes, Immutable, IntoBytes, KnownLayout)]
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
