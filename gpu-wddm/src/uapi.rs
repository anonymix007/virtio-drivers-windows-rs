#![allow(non_camel_case_types)]
#![allow(dead_code)]

use core::{
    ptr::NonNull,
    ffi::c_void,
    marker::{
        PhantomData,
        PhantomPinned,
    },
    cmp::min,
    fmt,
    str::from_utf8,
};

use alloc::{
    boxed::Box,
    sync::Arc,
};

use bitflags::bitflags;
use zerocopy::*;

pub use tagged::*;
use wdk::{dxgkrnl::D3DKMT_HANDLE, wdm::HANDLE};

use crate::{
    function,
    slice_from_raw_parts,
    slice_from_raw_parts_mut,
};

/*

pub const ADAPTER_INFO_TAG:           u64 = u64::from_ne_bytes(*b"VADAPINF");

pub const ESCAPE_CAPSET_TAG:          u64 = u64::from_ne_bytes(*b"VESCCAPS");
pub const ESCAPE_PCIE_TAG:            u64 = u64::from_ne_bytes(*b"VESCPPCI");
pub const ESCAPE_RESOURCE_INFO_TAG:   u64 = u64::from_ne_bytes(*b"VESCRINF");
pub const ESCAPE_RESOURCE_BUSY_TAG:   u64 = u64::from_ne_bytes(*b"VESCRBUS");
pub const ESCAPE_BLOB_INFO_SET_TAG:   u64 = u64::from_ne_bytes(*b"VESCBLOB");
pub const ESCAPE_BLOB_MAP_TAG:        u64 = u64::from_ne_bytes(*b"VESCMAPB");
pub const ESCAPE_CONTEXT_INIT_TAG:    u64 = u64::from_ne_bytes(*b"VESCCTXI");
pub const ESCAPE_BLIT_INIT_TAG:       u64 = u64::from_ne_bytes(*b"VESCBLIT");
pub const ESCAPE_EXEC_BUF_TAG:        u64 = u64::from_ne_bytes(*b"VESCEXEC");
pub const ESCAPE_RESOURCE_ATTACH_TAG: u64 = u64::from_ne_bytes(*b"VESCRATT");

pub const CREATE_RESOURCE_TAG:        u64 = u64::from_ne_bytes(*b"VALLRESR");
pub const ALLOCATE_3D_TAG:            u64 = u64::from_ne_bytes(*b"VALL3DGL");
pub const ALLOCATE_BLOB_TAG:          u64 = u64::from_ne_bytes(*b"VALLBLOB");

pub const SUBMIT_COMMAND_VIRTUAL_TAG: u64 = u64::from_ne_bytes(*b"VSUBCMDV");

fn main() {
    println!("pub const ADAPTER_INFO_TAG:           u64 = 0x{ADAPTER_INFO_TAG:X}u64;");
    println!("pub const ESCAPE_CAPSET_TAG:          u64 = 0x{ESCAPE_CAPSET_TAG:X}u64;");
    println!("pub const ESCAPE_PCIE_TAG:            u64 = 0x{ESCAPE_PCIE_TAG:X}u64;");
    println!("pub const ESCAPE_RESOURCE_INFO_TAG:   u64 = 0x{ESCAPE_RESOURCE_INFO_TAG:X}u64;");
    println!("pub const ESCAPE_RESOURCE_BUSY_TAG:   u64 = 0x{ESCAPE_RESOURCE_BUSY_TAG:X}u64;");
    println!("pub const ESCAPE_BLOB_INFO_SET_TAG:   u64 = 0x{ESCAPE_BLOB_INFO_SET_TAG:X}u64;");
    println!("pub const ESCAPE_CONTEXT_INIT_TAG:    u64 = 0x{ESCAPE_CONTEXT_INIT_TAG:X}u64;");
    println!("pub const ESCAPE_BLIT_INIT_TAG:       u64 = 0x{ESCAPE_BLIT_INIT_TAG:X}u64;");
    println!("pub const ESCAPE_BLOB_MAP_TAG:        u64 = 0x{ESCAPE_BLOB_MAP_TAG:X}u64;");
    println!("pub const ESCAPE_CONTEXT_INIT_TAG:    u64 = 0x{ESCAPE_CONTEXT_INIT_TAG:X}u64;");
    println!("pub const ESCAPE_BLIT_INIT_TAG:       u64 = 0x{ESCAPE_BLIT_INIT_TAG:X}u64;");
    println!("pub const ESCAPE_EXEC_BUF_TAG:        u64 = 0x{ESCAPE_EXEC_BUF_TAG:X}u64;");
    println!("pub const ESCAPE_RESOURCE_ATTACH_TAG: u64 = 0x{ESCAPE_RESOURCE_ATTACH_TAG:X}u64;");
    println!("pub const CREATE_RESOURCE_TAG:        u64 = 0x{CREATE_RESOURCE_TAG:X}u64;");
    println!("pub const ALLOCATE_3D_TAG:            u64 = 0x{ALLOCATE_3D_TAG:X}u64;");
    println!("pub const ALLOCATE_BLOB_TAG:          u64 = 0x{ALLOCATE_BLOB_TAG:X}u64;");
    println!("pub const SUBMIT_COMMAND_VIRTUAL_TAG: u64 = 0x{SUBMIT_COMMAND_VIRTUAL_TAG:X}u64;");
}

*/

pub const PCI_VENDOR_ID:              u16 = 0x1AF4u16;
pub const PCI_DEVICE_ID:              u16 = 0x6969u16;

pub const ADAPTER_INFO_TAG:           u64 = 0x464E495041444156u64;

pub const ESCAPE_CAPSET_TAG:          u64 = 0x5350414343534556u64;
//pub const ESCAPE_PCIE_TAG:            u64 = 0x4943505043534556u64;
pub const ESCAPE_RESOURCE_INFO_TAG:   u64 = 0x464E495243534556u64;
pub const ESCAPE_RESOURCE_BUSY_TAG:   u64 = 0x5355425243534556u64;
pub const ESCAPE_BLOB_INFO_SET_TAG:   u64 = 0x424F4C4243534556u64;
pub const ESCAPE_BLOB_MAP_TAG:        u64 = 0x4250414D43534556u64;
pub const ESCAPE_CONTEXT_INIT_TAG:    u64 = 0x4958544343534556u64;
pub const ESCAPE_BLIT_INIT_TAG:       u64 = 0x54494C4243534556u64;
pub const ESCAPE_EXEC_BUF_TAG:        u64 = 0x4345584543534556u64;
//pub const ESCAPE_RESOURCE_ATTACH_TAG: u64 = 0x5454415243534556u64;

pub const CREATE_RESOURCE_TAG:        u64 = 0x525345524C4C4156u64;
pub const ALLOCATE_3D_TAG:            u64 = 0x4C4744334C4C4156u64;
pub const ALLOCATE_BLOB_TAG:          u64 = 0x424F4C424C4C4156u64;

pub const SUBMIT_COMMAND_VIRTUAL_TAG: u64 = 0x56444D4342555356u64;


/* This should not be manually implemented, use derive instead */
pub unsafe trait Tagged: Sized {
    const TAG: u64;
}

pub struct Tag(u64);

impl Tag {
    pub fn from_handle(handle: *const c_void) -> Self {
        let tag = unsafe {
            let ptr: *const u64 = core::mem::transmute(handle);
            ptr.read()
        };
        Self(tag)
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let bytes = self.0.as_bytes();

        write!(f, "\"")?;
        for b in bytes {
            write!(f, "{}", core::ascii::escape_default(*b))?;
        }
        write!(f, "\"")
    }
}

pub trait TaggedExt: Tagged + Sized {
    fn tag(handle: *const c_void) -> Tag {
        Tag::from_handle(handle)
    }

    fn valid_handle(handle: *const c_void) -> bool {
        let tag = <Self as TaggedExt>::tag(handle);
        tag.0 == Self::TAG
    }

    fn check_handle(handle: *const c_void) -> Option<*const Self> {
        if Self::valid_handle(handle) {
            Some(handle as *const _)
        } else {
            let expected = Tag(<Self as Tagged>::TAG);
            let actual = Self::tag(handle);

            error!("{}: invalid tag: expected {:x} ({:?}), but got {:x} ({:?})", function!(), expected.0, expected, actual.0, actual);
            None
        }
    }

    fn check_handle_mut(handle: *mut c_void) -> Option<*mut Self> {
        if Self::valid_handle(handle) {
            Some(handle as *mut _)
        } else {
            let expected = Tag(<Self as Tagged>::TAG);
            let actual = Self::tag(handle);

            error!("{}: invalid tag: expected {:x} ({:?}), but got {:x} ({:?})", function!(), expected.0, expected, actual.0, actual);
            None
        }
    }

    fn from_arc_handle_clone(handle: *mut c_void) -> Option<Arc<Self>> {
        let val = Self::from_arc_handle_owned(handle)?;

        unsafe {
            Arc::increment_strong_count(handle as *mut Self);
        }

        Some(val)
    }

    fn from_arc_handle_owned(handle: *mut c_void) -> Option<Arc<Self>> {
        if handle.is_null() {
            //error!("{}: handle is null", function!());
            return None;
        }

        let ptr = <Self as TaggedExt>::check_handle(handle)?;
        Some(unsafe { Arc::from_raw(ptr) })
    }

    fn into_arc_handle(value: Arc<Self>) -> *mut c_void {
        Arc::into_raw(value) as _
    }

    /* Lifetime is handled manually, so this has to be static */
    fn from_handle_mut(handle: *mut c_void) -> Option<&'static mut Self> {
        if handle.is_null() {
            //error!("{}: handle is null", function!());
            return None;
        }

        let ptr = <Self as TaggedExt>::check_handle_mut(handle)?;
        Some(unsafe { &mut *ptr })
    }

    fn from_handle(handle: *const c_void) -> Option<&'static Self> {
        if handle.is_null() {
            //error!("{}: handle is null", function!());
            return None;
        }

        let ptr = <Self as TaggedExt>::check_handle(handle)?;
        Some(unsafe { &*ptr })
    }

    fn from_handle_silent_mut(handle: *mut c_void) -> Option<&'static mut Self> {
        if handle.is_null() {
            return None;
        }

        if <Self as TaggedExt>::valid_handle(handle) {
            Some(unsafe { &mut *(handle as *mut _)})
        } else {
            None
        }
    }

    fn from_handle_silent(handle: *const c_void) -> Option<&'static Self> {
        if handle.is_null() {
            return None;
        }

        if <Self as TaggedExt>::valid_handle(handle) {
            Some(unsafe { &*(handle as *const _)})
        } else {
            None
        }
    }

    fn as_handle(value: &mut Self) -> *mut c_void {
        value as *mut Self as _
    }

    fn into_handle(value: Box<Self>) -> *mut c_void {
        Box::into_raw(value) as _
    }
}

impl<T: Tagged + Sized> TaggedExt for T { }

/*#[repr(C, packed)]
#[derive(Copy, Clone)]
union Tagged {
    pub tag: u64,
    pub adapter_info: AdapterInfo,
    pub create_allocation: CreateAllocation,
    pub create_resource: CreateResource,
    pub escape: Escape,
}*/

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CapsetId {
    Virgl       = 1,
    Virgl2      = 2,
    Gfxstream   = 3,
    Venus       = 4,
    CrossDomain = 5,
    Drm         = 6,
}

impl TryFrom<u32> for CapsetId {
    type Error = crate::adapter::NtStatus;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Virgl),
            2 => Ok(Self::Virgl2),
            3 => Ok(Self::Gfxstream),
            4 => Ok(Self::Venus),
            5 => Ok(Self::CrossDomain),
            6 => Ok(Self::Drm),
            _ => Err(crate::adapter::NtStatus(winresult::STATUS::INVALID_PARAMETER)),
        }
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct CapsetMask: u64 {
        const VIRGL        = 1u64 << 1;
        const VIRGL2       = 1u64 << 2;
        const GFXSTREAM    = 1u64 << 3;
        const VENUS        = 1u64 << 4;
        const CROSS_DOMAIN = 1u64 << 5;
        const DRM          = 1u64 << 6;
    }
}

impl From<CapsetId> for CapsetMask {
    fn from(id: CapsetId) -> Self {
        match id {
            CapsetId::Virgl => CapsetMask::VIRGL,
            CapsetId::Virgl2 => CapsetMask::VIRGL2,
            CapsetId::Gfxstream => CapsetMask::GFXSTREAM,
            CapsetId::Venus => CapsetMask::VENUS,
            CapsetId::CrossDomain => CapsetMask::CROSS_DOMAIN,
            CapsetId::Drm => CapsetMask::DRM,
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(ADAPTER_INFO_TAG)]
pub struct AdapterInfo {
    pub tag: u64,

    pub luid: u64,
    pub capset_mask: CapsetMask,
    pub supports_3d: bool,
    pub has_shmem: bool,
}

/*
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(ESCAPE_PCIE_TAG)]
pub struct PCIInfo {
    pub tag: u64,

    pub domain: u32,
    pub bus: u32,
    pub dev: u32,
    pub func: u32,
}*/

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(ESCAPE_CAPSET_TAG)]
pub struct Capset {
    pub tag: u64,

    pub capset_id: CapsetId,
    pub version: u32,

    pub capset: [u8; 0],
}

impl Capset {
    pub fn capset_slice(&mut self, priv_size: usize) -> &mut [u8] {
        slice_from_raw_parts_mut(self.capset.as_mut_ptr(), priv_size - core::mem::size_of::<Capset>())
    }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct CapsetRaw {
    pub tag: u64,

    pub capset_id: u32,
    pub version: u32,
}

const _: () = assert!(core::mem::size_of::<CapsetRaw>() == core::mem::size_of::<Capset>());

impl CapsetRaw {
    pub fn try_as_capset(&mut self) -> Result<&mut Capset, &'static str> {
        CapsetId::try_from(self.capset_id).ok().ok_or("invalid capset id")?;
        Ok(unsafe {
            core::mem::transmute(self)
        })
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Tagged)]
#[tagged(ESCAPE_CONTEXT_INIT_TAG)]
pub struct ContextInit {
    pub tag: u64,

    pub capset_id: CapsetId,
    pub num_rings: u32,
    pub debug_name: [u8; 64],
}

impl ContextInit {
    pub fn debug_name(&self) -> &str {
        let len = self.debug_name.iter().position(|&b| b == 0).unwrap_or(64);
        let debug_name = from_utf8(&self.debug_name[..len]).unwrap_or("<invalid>");

        &debug_name
    }
}

impl fmt::Debug for ContextInit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let debug_name = self.debug_name();
        let capset_id = self.capset_id;
        let num_rings = self.num_rings;

        f.debug_struct("ContextInit")
            .field("capset_id", &capset_id)
            .field("num_rings", &num_rings)
            .field("debug_name", &debug_name)
            .finish()
    }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct BlobInfo {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub bind: u32,
    pub modifier: u64,
    pub strides: [u32; 4],
    pub offsets: [u32; 4],
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct AllocateBlobFull {
    pub blob: AllocateBlob,
    pub info: BlobInfo,
    pub info_valid: bool,
    pub created: bool,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct Allocate3dFull {
    pub _3d: Allocate3d,
    pub modifier: u64,
    pub offsets: [u64; 4],
    pub strides: [u32; 4],
    pub sizes: [u32; 4],
    pub num_planes: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub union AllocationInfo {
    pub tag: u64,
    pub _3d: Allocate3dFull,
    pub blob: AllocateBlobFull,
}

impl AllocationInfo {
    pub fn tag(&self) -> u64 {
        unsafe { self.tag }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Tagged)]
#[tagged(ESCAPE_RESOURCE_INFO_TAG)]
pub struct ResourceInfo {
    pub tag: u64,

    pub handle: D3DKMT_HANDLE,
    pub id: u32,
    pub info: AllocationInfo,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union HANDLE64 {
    pub handle: HANDLE,
    pub wow64: u64,
}

impl fmt::Debug for HANDLE64 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", unsafe { self.handle })
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union Pointer64<T: Copy> {
    pub ptr: Option<NonNull<T>>,
    pub wow64: u64,
}

impl<T: Copy> fmt::Debug for Pointer64<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", unsafe { self.ptr })
    }
}

/*
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Slice64<T: Copy> {
    pub ptr: Pointer64<T>,
    pub len: u64,
}
*/

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(ESCAPE_RESOURCE_BUSY_TAG)]
pub struct ResourceBusy {
    pub tag: u64,

    pub event: HANDLE64,

    pub handle: D3DKMT_HANDLE,
    pub wait: bool,
    pub is_busy: bool,
    /* pub create_event: bool */
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(ESCAPE_BLOB_INFO_SET_TAG)]
pub struct BlobInfoSet {
    pub tag: u64,

    pub handle: D3DKMT_HANDLE,
    pub _padding: u32,
    pub blob_info: BlobInfo,
}

bitflags! {
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct BlobMapFlags: u32 {
        const UNMAP  = 1u32 << 0;
        // TODO: const PLACED = 1u32 << 1;
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Tagged)]
#[tagged(ESCAPE_BLOB_MAP_TAG)]
pub struct BlobMap {
    pub tag: u64,

    pub handle: D3DKMT_HANDLE,
    pub flags: BlobMapFlags,
    pub ptr: Pointer64<u8>,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(ESCAPE_EXEC_BUF_TAG)]
pub struct ExecBuffer {
    pub tag: u64,

    pub fence_id: u64,
    pub cmd: [u8; 0],
}

impl ExecBuffer {
    pub fn command_slice(&self, priv_size: usize) -> &[u8] {
        slice_from_raw_parts(self.cmd.as_ptr(), priv_size - core::mem::size_of::<ExecBuffer>())
    }
}

/*
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(ESCAPE_RESOURCE_ATTACH_TAG)]
pub struct ResourceAttachContext {
    pub tag: u64,

    pub handle: D3DKMT_HANDLE,
}*/

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub union Escape {
    pub tag: u64,

    //pub pci_info: PCIInfo, // TODO: use D3DKMT_ADAPTERADDRESS
    pub caps_req: Capset,
    pub ctx_init: ContextInit,
    pub res_info: ResourceInfo,
    pub res_busy: ResourceBusy,
    pub blob_set: BlobInfoSet,
    pub blob_map: BlobMap,
    pub exec_buf: ExecBuffer,
    //pub res_atta: ResourceAttachContext,
}

impl Escape {
    pub fn tag(&self) -> u64 {
        unsafe { self.tag }
    }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(CREATE_RESOURCE_TAG)]
pub struct CreateResource {
    pub tag: u64,

    // TODO: inline slice instead
    pub cmd: [u8; 0],
    //pub submit: Slice64<u8>,
}

impl CreateResource {
    pub fn command_slice(&self, priv_size: usize) -> &[u8] {
        slice_from_raw_parts(self.cmd.as_ptr(), priv_size - core::mem::size_of::<CreateResource>())
    }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(ALLOCATE_3D_TAG)]
pub struct Allocate3d {
    pub tag: u64,

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
    pub size: u64,
}

//#[repr(u32)]
//#[derive(Debug, Copy, Clone)]
//pub enum BlobMem {
//    Guest = 0x1,
//    Host3d = 0x2,
//    Host3dGuest = 0x3,
//}

bitflags! {
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct BlobMem: u32 {
        const GUEST        = 1u32 << 0;
        const HOST3D       = 1u32 << 1;
        const HOST3D_GUEST = 1u32 << 0 | 1u32 << 1;

        //const _ = !0;
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct BlobFlag: u32 {
        const NONE         = 0;
        const MAPPABLE     = 1u32 << 0;
        const SHAREABLE    = 1u32 << 1;
        const CROSS_DEVICE = 1u32 << 2;

        //const _ = !0;
    }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(ALLOCATE_BLOB_TAG)]
pub struct AllocateBlob {
    pub tag: u64,

    pub id: u64,
    pub mem: BlobMem,
    pub flags: BlobFlag,

    pub size: u64,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub union CreateAllocation {
    pub tag: u64,
    pub _3d: Allocate3d,
    pub blob: AllocateBlob,
}

impl CreateAllocation {
    pub fn tag(&self) -> u64 {
        unsafe { self.tag }
    }
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, TryFromBytes, Immutable, IntoBytes, KnownLayout)]
pub enum CommandId {
    Nop = 0x0, /* Empty body */
    Submit = 0x1, /* Opaque */
    TransferToHost = 0x2, /* CommandTransfer */
    TransferFromHost = 0x3, /* CommandTransfer */
    Fence = 0x4, /* u64 */
    // AllocationList = 0x5, /* CommandAllocation */
    // MapBlob = 0x6, /* CommandMap */
    // UnmapBlob = 0x7, /* CommandMap */
    // BlitUM = 0x8, /* CommandBlitUM */

    // These commands cannot be submitted from userspace
    MapAperture = 0xF000,
    UnmapAperture = 0xF001,
}

impl TryFrom<u16> for CommandId {
    type Error = &'static str;

    fn try_from(v: u16) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(CommandId::Nop),
            1 => Ok(CommandId::Submit),
            2 => Ok(CommandId::TransferToHost),
            3 => Ok(CommandId::TransferFromHost),
            4 => Ok(CommandId::Fence),
            // 4 => Ok(CommandId::AllocationList),
            // 5 => Ok(CommandId::MapBlob),
            // 6 => Ok(CommandId::UnmapBlob),
            // 7 => Ok(CommandId::BlitUM),
            _ => Err("invalid id"),
        }
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
    pub struct CommandFlag: u8 {
        const RING_IDX     = 1u8 << 0;
        const SHADOW_VIRGL = 1u8 << 1;

        //const _ = !0;
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct CommandHeader {
    pub id: CommandId,
    pub flags: CommandFlag,
    pub ring: u8,
    pub size: u32,
    pub body: [Commands; 0],
    pub phantom: PhantomData<(*const Commands, PhantomPinned)>,
}

impl fmt::Debug for CommandHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let id = self.id;
        let flags = self.flags;
        let ring = self.ring;
        let size = self.size;

        f.debug_struct("CommandHeader")
            .field("id", &id)
            .field("flags", &flags)
            .field("ring", &ring)
            .field("size", &size)
            .finish()
    }
}

#[derive(Debug)]
pub enum CommandBody<'a> {
    Nop,
    Submit(&'a [u8]),
    Fence(u64),
    TransferToHost(&'a [CommandTransfer]),
    TransferFromHost(&'a [CommandTransfer]),
    //AllocationList(&'a [CommandAllocation]),
    //MapBlob(&'a [CommandMapBlob]),
    //UnmapBlob(&'a [CommandMapBlob]),
}

impl<'a> CommandBody<'a> {
    pub fn len(&self) -> usize {
        match self {
            CommandBody::Nop => 0,
            CommandBody::Submit(_) => 1,
            CommandBody::Fence(_) => 1,
            CommandBody::TransferToHost(slice) | CommandBody::TransferFromHost(slice) => slice.len(),
            //CommandBody::AllocationList(slice) => slice.len(),
            //CommandBody::MapBlob(slice) | CommandBody::UnmapBlob(slice) => slice.len(),
        }
    }

    pub fn id(&self) -> CommandId {
        match self {
            CommandBody::Nop => CommandId::Nop,
            CommandBody::Submit(_) => CommandId::Submit,
            CommandBody::Fence(_) => CommandId::Fence,
            CommandBody::TransferToHost(_) => CommandId::TransferToHost,
            CommandBody::TransferFromHost(_) => CommandId::TransferFromHost,
            //CommandBody::AllocationList(_) => CommandId::AllocationList,
            //CommandBody::MapBlob(_) => CommandId::MapBlob,
            //CommandBody::UnmapBlob(_) => CommandId::UnmapBlob,
        }
    }
}

#[derive(Debug)]
pub enum CommandBodyItem<'a> {
    Nop,
    Submit(&'a [u8]),
    Fence(u64),
    TransferToHost(&'a CommandTransfer),
    TransferFromHost(&'a CommandTransfer),
    //AllocationList(&'a CommandAllocation),
    //MapBlob(&'a CommandMapBlob),
    //MapBlobAt(&'a CommandMapBlob, u64),
    //UnmapBlob(&'a CommandMapBlob),
}

impl<'a> CommandBodyItem<'a> {
    pub fn id(&self) -> CommandId {
        match self {
            CommandBodyItem::Nop => CommandId::Nop,
            CommandBodyItem::Submit(_) => CommandId::Submit,
            CommandBodyItem::Fence(_) => CommandId::Fence,
            CommandBodyItem::TransferToHost(_) => CommandId::TransferToHost,
            CommandBodyItem::TransferFromHost(_) => CommandId::TransferFromHost,
            //CommandBodyItem::AllocationList(_) => CommandId::AllocationList,
            //CommandBodyItem::MapBlob(_) => CommandId::MapBlob,
            //CommandBodyItem::MapBlobAt(_, _) => CommandId::MapBlob,
            //CommandBodyItem::UnmapBlob(_) => CommandId::UnmapBlob,
        }
    }
}

use core::slice::Iter;

enum CommandBodyIterState<'a> {
    Nop,
    Submit(Option<&'a [u8]>),
    Fence(Option<u64>),
    TransferToHost(Iter<'a, CommandTransfer>),
    TransferFromHost(Iter<'a, CommandTransfer>),
    //AllocationList(Iter<'a, CommandAllocation>),
    //MapBlob(Iter<'a, CommandMapBlob>),
    //UnmapBlob(Iter<'a, CommandMapBlob>),
}

pub struct CommandBodyIterator<'a> {
    state: CommandBodyIterState<'a>,
}

impl<'a> CommandBodyIterator<'a> {
    fn new(body: &CommandBody<'a>) -> Self {
        let state = match body {
            CommandBody::Nop => CommandBodyIterState::Nop,
            CommandBody::Submit(data) => CommandBodyIterState::Submit(Some(data)),
            CommandBody::Fence(fence) => CommandBodyIterState::Fence(Some(*fence)),
            CommandBody::TransferToHost(slice) => CommandBodyIterState::TransferToHost(slice.iter()),
            CommandBody::TransferFromHost(slice) => CommandBodyIterState::TransferFromHost(slice.iter()),
            //CommandBody::AllocationList(slice) => CommandBodyIterState::AllocationList(slice.iter()),
            //CommandBody::MapBlob(slice) => CommandBodyIterState::MapBlob(slice.iter()),
            //CommandBody::UnmapBlob(slice) => CommandBodyIterState::UnmapBlob(slice.iter()),
        };
        CommandBodyIterator { state }
    }
}

impl<'a> Iterator for CommandBodyIterator<'a> {
    type Item = CommandBodyItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            CommandBodyIterState::Nop => None,
            CommandBodyIterState::Submit(data) => data.take().map(CommandBodyItem::Submit),
            CommandBodyIterState::Fence(fence) => fence.take().map(CommandBodyItem::Fence),
            CommandBodyIterState::TransferToHost(iter) => iter.next().map(CommandBodyItem::TransferToHost),
            CommandBodyIterState::TransferFromHost(iter) => iter.next().map(CommandBodyItem::TransferFromHost),
            //CommandBodyIterState::AllocationList(iter) => iter.next().map(CommandBodyItem::AllocationList),
            //CommandBodyIterState::MapBlob(iter) => iter.next().map(CommandBodyItem::MapBlob),
            //CommandBodyIterState::UnmapBlob(iter) => iter.next().map(CommandBodyItem::UnmapBlob),
        }
    }
}

impl<'a> IntoIterator for &'a CommandBody<'a> {
    type Item = CommandBodyItem<'a>;
    type IntoIter = CommandBodyIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        CommandBodyIterator::new(self)
    }
}

impl CommandHeader {
    fn try_commands_from_body<T>(&self) -> Result<&'_ [T], &'static str> {
        let ptr = self.body.as_ptr() as *const T;
        if !ptr.is_aligned() {
            return Err("body is not aligned to element size");
        }

        let size = self.size as usize;
        if !size.is_multiple_of(core::mem::size_of::<T>()){
            return Err("body size is not multiple of command size");
        }

        let count = size / core::mem::size_of::<T>();
        Ok(slice_from_raw_parts(ptr, count))
    }

    pub fn command_slice(&self) -> Result<CommandBody<'_>, &'static str> {
        Ok(match self.id {
            CommandId::Nop => CommandBody::Nop,
            CommandId::Submit => CommandBody::Submit(self.try_commands_from_body()?),
            CommandId::Fence => {
                let slice = self.try_commands_from_body::<u64>()?;
                if slice.len() != 1 {
                    return Err("CommandId::Fence only supports one fence currently");
                }

                CommandBody::Fence(slice[0])
            },
            CommandId::TransferToHost => CommandBody::TransferToHost(self.try_commands_from_body()?),
            CommandId::TransferFromHost => CommandBody::TransferFromHost(self.try_commands_from_body()?),
            //CommandId::AllocationList => CommandBody::AllocationList(self.try_commands_from_body()?),
            //CommandId::MapBlob => CommandBody::MapBlob(self.try_commands_from_body()?),
            //CommandId::UnmapBlob => CommandBody::UnmapBlob(self.try_commands_from_body()?),
            _ => Err("invalid userspace command id")?,
        })
    }

    pub fn ring(&self) -> Option<u8> {
        if self.flags.contains(CommandFlag::RING_IDX) {
            Some(self.ring)
        } else {
            None
        }
    }

    pub fn shadow_virgl(&self) -> bool {
        self.flags.contains(CommandFlag::SHADOW_VIRGL)
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug, FromBytes, Immutable, KnownLayout)]
pub struct CommandHeaderRaw {
    pub id: u16,
    pub flags: u8,
    pub ring: u8,
    pub size: u32,
    //pub body: [Commands; 0],
    //pub phantom: PhantomData<(*const Commands, PhantomPinned)>,
}

const _: () = assert!(core::mem::size_of::<CommandHeaderRaw>() == core::mem::size_of::<CommandHeader>());

impl CommandHeaderRaw {
    pub fn try_as_hdr(&self) -> Result<&CommandHeader, &'static str> {
        CommandId::try_from(self.id)?;
        CommandFlag::from_bits(self.flags).ok_or("invalid flags")?;
        Ok(unsafe {
            core::mem::transmute(self)
        })
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub union Commands {
    pub submit: [u8; 0],
    pub transfer: [CommandTransfer; 0],
    //pub map: [CommandMapBlob; 0],
    //pub alloc: [CommandAllocation; 0],
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct Box3D {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

impl Into<virtio_drivers::device::gpu::commands::Box> for Box3D {
    fn into(self) -> virtio_drivers::device::gpu::commands::Box {
        virtio_drivers::device::gpu::commands::Box {
            x: self.x,
            y: self.y,
            z: self.z,
            w: self.width,
            h: self.height,
            d: self.depth,
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct CommandTransfer {
    pub res_id: u32,
    pub stride: u32,
    pub offset: u64,
    pub level: u32,
    pub layer_stride: u32,
    pub r#box: Box3D,
}

/*
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct CommandMapBlob {
    pub res_id: u32,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct CommandAllocation {
    pub handle: D3DKMT_HANDLE,
}
*/

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, Tagged)]
#[tagged(SUBMIT_COMMAND_VIRTUAL_TAG)]
pub struct SubmitCommand {
    pub tag: u64,
    pub cmd: [u8; 0],
}

impl SubmitCommand {
    pub fn command_slice(&self, priv_size: usize) -> &[u8] {
        slice_from_raw_parts(self.cmd.as_ptr(), priv_size - core::mem::size_of::<SubmitCommand>())
    }
}

pub const MAX_SUBMIT_COMMAND_VIRTUAL_SIZE: u32 = 8192;

const _: () = assert!(core::mem::size_of::<AdapterInfo>() == 26);
//const _: () = assert!(core::mem::size_of::<PCIInfo>() == 24);
const _: () = assert!(core::mem::size_of::<Capset>() == 16);
const _: () = assert!(core::mem::size_of::<ContextInit>() == 80);
const _: () = assert!(core::mem::size_of::<ResourceInfo>() == 148);
const _: () = assert!(core::mem::size_of::<ResourceBusy>() == 22);
const _: () = assert!(core::mem::size_of::<BlobInfoSet>() == 72);
const _: () = assert!(core::mem::size_of::<Escape>() == 148);
const _: () = assert!(core::mem::size_of::<AllocateBlob>() == 32);
const _: () = assert!(core::mem::size_of::<Allocate3d>() == 56);
const _: () = assert!(core::mem::size_of::<CreateAllocation>() == 56);
const _: () = assert!(core::mem::size_of::<CreateResource>() == 8);
const _: () = assert!(core::mem::size_of::<CommandHeader>() == 8);
//const _: () = assert!(core::mem::size_of::<CommandMapBlob>() == 4);
//const _: () = assert!(core::mem::size_of::<CommandAllocation>() == 4);
const _: () = assert!(core::mem::size_of::<CommandTransfer>() == 48);
const _: () = assert!(core::mem::size_of::<SubmitCommand>() == 8);
