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
use spin::mutex::SpinMutex;
use spin::rwlock::RwLock;
use smallvec::SmallVec;
use itertools::Itertools;

use wdk::{
    wdm::{
        EventType,
        KeEvent,
        NtTime,
        ke_delay_execution_thread,
    },
    dxgkrnl::{
        D3DDDIFORMAT,
        D3DKMT_HANDLE,
        DXGK_PTE,
    },
};

use virtio_drivers::device::gpu::commands::MemEntry;

use crate::adapter::*;
use crate::device::*;
use crate::uapi::*;
use crate::virgl::*;
use crate::queue::AlignedBox;
use crate::function;

const VIRTIO_GPU_ALLOCATION_TAG: u64 = u64::from_ne_bytes(*b"VGPUALLO");
const VIRTIO_GPU_DEVICE_SPECIFIC_ALLOCATION_TAG: u64 = u64::from_ne_bytes(*b"VGPUDEAL");
//const VIRTIO_GPU_RESOURCE_TAG: u64 = u64::from_ne_bytes(*b"VGPURESO");

pub struct AllocationDesc {
    pub width: u32,
    pub height: u32,
    pub format: D3DDDIFORMAT,
}

/*
trait AtomicRW: Clone + Default {
    type Item;

    fn write(&self, val: Self::Item);
    fn read(&self) -> Self::Item;
}

impl<T: AtomicRW> Clone for AtomicOption<T> {
    fn clone(&self) -> Self {
        if self.is_some() {
            AtomicOption::none()
        } else {
            AtomicOption::none()
        }
    }
}

impl<T: AtomicRW + fmt::Debug> fmt::Debug for AtomicOption<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

struct AtomicOption<T: AtomicRW> {
    value: T,
    some: AtomicBool,
}

impl<T: AtomicRW> AtomicOption<T> {
    fn some(value: T) -> Self {
        Self {
            value,
            some: AtomicBool::new(true),
        }
    }

    fn none() -> Self{
        Self {
            value: Default::default(),
            some: AtomicBool::new(false),
        }
    }

    fn is_some(&self) -> bool {
        self.some.load(Ordering::Acquire)
    }

    fn is_none(&self) -> bool {
        !self.is_some()
    }

    fn as_option(&self) -> Option<T::Item> {
        if self.is_some() {
            Some(self.value.read())
        } else {
            None
        }
    }

    fn as_ref(&self) -> Option<&T> {
        if self.is_some() {
            Some(&self.value)
        } else {
            None
        }
    }

    fn set(&self, value: T::Item) {
        self.some.store(true, Ordering::Release);
        self.value.write(value);
    }
}

struct AtomicBlobInfo {
    width: AtomicU32,
    height: AtomicU32,
    format: AtomicU32,
    bind: AtomicU32,
    modifier: AtomicU64,
    strides: [AtomicU32; 4],
    offsets: [AtomicU32; 4],
}

impl AtomicRW for AtomicBlobInfo {
    type Item = BlobInfo;

    fn write(&self, info: BlobInfo) {
        self.width.store(info.width, Ordering::Release);
        self.height.store(info.height, Ordering::Release);
        self.format.store(info.format, Ordering::Release);
        self.bind.store(info.bind, Ordering::Release);
        self.modifier.store(info.modifier, Ordering::Release);
        for i in 0..4 {
            self.strides[i].store(info.strides[i], Ordering::Release);
            self.offsets[i].store(info.offsets[i], Ordering::Release);
        }
    }

    fn read(&self) -> BlobInfo {
        BlobInfo {
            width: self.width.load(Ordering::Acquire),
            height: self.height.load(Ordering::Acquire),
            format: self.format.load(Ordering::Acquire),
            bind: self.bind.load(Ordering::Acquire),
            modifier: self.modifier.load(Ordering::Acquire),
            strides: [self.strides[0].load(Ordering::Acquire), self.strides[1].load(Ordering::Acquire), self.strides[2].load(Ordering::Acquire), self.strides[3].load(Ordering::Acquire)],
            offsets: [self.offsets[0].load(Ordering::Acquire), self.offsets[1].load(Ordering::Acquire), self.offsets[2].load(Ordering::Acquire), self.offsets[3].load(Ordering::Acquire)],
        }
    }
}

impl Default for AtomicBlobInfo {
    fn default() -> Self {
       Self {
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
            format: AtomicU32::new(0),
            bind: AtomicU32::new(0),
            modifier: AtomicU64::new(0),
            strides: [const { AtomicU32::new(0)}; 4],
            offsets: [const { AtomicU32::new(0)}; 4],
        }
    }
}

impl Clone for AtomicBlobInfo {
    fn clone(&self) -> Self {
        let new = AtomicBlobInfo::default();
        new.write(self.read());
        new
    }
}

impl fmt::Debug for AtomicBlobInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.read())
    }
}*/

#[derive(Debug)]
pub enum VirtioResource {
    _3D {
        target: VirglTarget,
        format: VirglFormat,
        bind: VirglBind,
        width: u32,
        height: u32,
        depth: u32,
        array_size: u32,
        last_level: u32,
        nr_samples: u32,
        flags: VirglFlags,
        map: RwLock<(u64, SmallVec<[MemEntry; 2]>)>,
        layout: RwLock<Option<VirglResourceLayout>>,
        size: u64,
    },
    Blob {
        id: u64,
        mem: BlobMem,
        flags: BlobFlag,
        info: RwLock<Option<BlobInfo>>,
        map: RwLock<Option<offset_allocator::Allocation>>,
        size: u64,
    },
}

// This is stupid, there is absolutely no reason why this can't be derived
impl Clone for VirtioResource {
    fn clone(&self) -> Self {
        match self {
            VirtioResource::_3D {
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
                map,
                layout,
                size,
            } => {
                let map_val = map.read().clone();
                let layout_val = layout.read().clone();

                VirtioResource::_3D {
                    target: *target,
                    format: *format,
                    bind: *bind,
                    width: *width,
                    height: *height,
                    depth: *depth,
                    array_size: *array_size,
                    last_level: *last_level,
                    nr_samples: *nr_samples,
                    flags: *flags,
                    map: RwLock::new(map_val),
                    layout: RwLock::new(layout_val),
                    size: *size,
                }
            },
            VirtioResource::Blob {
                id,
                mem,
                flags,
                info,
                map,
                size,
            } => {
                let info_val = *info.read();
                let map_val = *map.read();
                VirtioResource::Blob {
                    id: *id,
                    mem: *mem,
                    flags: *flags,
                    info: RwLock::new(info_val),
                    map: RwLock::new(map_val),
                    size: *size,
                }
            }
        }
    }
}

impl From<Allocate3d> for VirtioResource {
    fn from(info: Allocate3d) -> Self {
        Self::_3D {
            target: info.target.into(),
            format: info.format.into(),
            bind: VirglBind::from_bits_retain(info.bind),
            width: info.width,
            height: info.height,
            depth: info.depth,
            array_size: info.array_size,
            last_level: info.last_level,
            nr_samples: info.nr_samples,
            flags: VirglFlags::from_bits_retain(info.flags),
            map: RwLock::new((0, SmallVec::new())),
            layout: RwLock::new(None),
            size: info.size,
        }
    }
}

impl From<AllocateBlob> for VirtioResource {
    fn from(info: AllocateBlob) -> Self {
        Self::Blob {
            id: info.id,
            mem: info.mem,
            flags: info.flags,
            info: RwLock::new(None),
            map: RwLock::new(None),
            size: info.size,
        }
    }
}

pub const DRM_FORMAT_MOD_INVALID: u64 = 0x00FFFFFFFFFFFFFF;

impl Into<AllocationInfo> for VirtioResource {
    fn into(self) -> AllocationInfo {
        match self {
            VirtioResource::_3D {
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
                map,
                layout,
                size,
            } => {
                let layout = if let Some(layout) = *layout.read() {
                    layout
                } else {
                    VirglResourceLayout {
                        modifier: DRM_FORMAT_MOD_INVALID,
                        ..unsafe { core::mem::zeroed() }
                    }
                };
                AllocationInfo {
                    _3d: Allocate3dFull {
                       _3d: Allocate3d {
                           tag: ALLOCATE_3D_TAG,
                           target: target.into(),
                           format: format.into(),
                           bind: bind.bits(),
                           width: width,
                           height: height,
                           depth: depth,
                           array_size: array_size,
                           last_level: last_level,
                           nr_samples: nr_samples,
                           flags: flags.bits(),
                           size: size,
                       },
                       modifier: layout.modifier,
                       offsets: {
                           let mut offsets = [0; _];

                           for i in 0..layout.num_planes as usize {
                               offsets[i] = layout.planes[i].offset;
                           }

                           offsets
                       },
                       strides: {
                           let mut strides = [0; _];

                           for i in 0..layout.num_planes as usize {
                               strides[i] = layout.planes[i].stride;
                           }

                           strides
                       },
                       sizes: {
                           let mut sizes = [0; _];

                           for i in 0..layout.num_planes as usize {
                               sizes[i] = layout.planes[i].size;
                           }

                           sizes
                       },
                       num_planes: layout.num_planes,
                   },
                }
            },
            VirtioResource::Blob {
                id,
                mem,
                flags,
                info,
                map,
                size,
            } => AllocationInfo {
                blob: AllocateBlobFull {
                    blob: AllocateBlob {
                        tag: ALLOCATE_BLOB_TAG,
                        id,
                        mem,
                        flags,
                        size,
                    },
                    info: if let Some(info) = *info.read() {
                        info
                    } else {
                        unsafe { core::mem::zeroed() }
                    },
                    info_valid: info.read().is_some(),
                    created: true,
                }
            },
        }
    }
}

#[repr(C)]
#[derive(Tagged)]
#[tagged(VIRTIO_GPU_DEVICE_SPECIFIC_ALLOCATION_TAG)]
pub struct DeviceSpecificAllocation {
    pub tag: u64,
    pub device: Weak<Device>,
    pub alloc: Weak<Allocation>,
    //pub owned: Option<D3DKMT_HANDLE>,
    //pub alloc_kmt: D3DKMT_HANDLE,
    owned: Option<Arc<Allocation>>,
    virgl: AtomicBool,
}

impl DeviceSpecificAllocation {
    fn new(device: &Arc<Device>, alloc: &Arc<Allocation>, owned: bool) -> Result<Self, NtStatus> {
        if let Some(cmd) = alloc.cmd.lock().take() {
            device.context_submit_3d(cmd)?;
        }

        // There's no race here because this function call is protected with a mutex
        // So only the first ever attached device will be used to create the blob

        if (alloc.flags.fetch_or(ALLOCATION_FLAG_CREATED, Ordering::SeqCst) & ALLOCATION_FLAG_CREATED) == 0 {
            match alloc.resource {
                VirtioResource::_3D {..} => warn!("{}: 3d resources should have been created by now", function!()),
                VirtioResource::Blob {id, mem, flags, size, ..} => {
                    debug!("{}: creating blob resource id {} ({})", function!(), alloc.id, id);
                    device.context_create_blob(alloc.id, id, mem, flags, size)?;
                }
            }
        }

        let shadow_virgl = alloc.is_3d() && !matches!(device.capset(), Some(CapsetId::Virgl) | Some(CapsetId::Virgl2));

        if shadow_virgl {
            device.context_attach_virgl(alloc.id).inspect_err(|e| {
                error!("{}: failed to attach to shadow virgl: {:?}", function!(), alloc);
                error!("{}: device: {:?}", function!(), device);
            })?;
        }

        trace!("{}: attaching resource {} to context {} ", function!(), alloc.id, device.context().and_then(|id| Some(id.get())).unwrap_or(0));
        device.context_attach_resource(alloc.id)?;

        Ok(Self {
            tag: VIRTIO_GPU_DEVICE_SPECIFIC_ALLOCATION_TAG,
            device: Arc::downgrade(device),
            alloc: Arc::downgrade(alloc),
            owned: if owned {
                Some(alloc.clone())
            } else {
                None
            },
            virgl: AtomicBool::new(shadow_virgl),
        })
    }

    pub fn ensure_virgl_attached(&self) -> Result<(), NtStatus> {
        let alloc = self.alloc.upgrade().expect("checked by caller already");
        let device = self.device.upgrade().expect("device should still exist");

        if matches!(device.capset(), Some(CapsetId::Virgl) | Some(CapsetId::Virgl2)) {
            /* Already attached, nothing to do */
        } else if self.virgl.swap(true, Ordering::SeqCst) {
            /* Already attached, nothing to do */
        } else {
            trace!("{}: attaching {} to shadow virgl context", function!(), alloc.id);
            device.context_attach_virgl(alloc.id).inspect_err(|e| {
                error!("{}: failed to attach to shadow virgl: {:?}", function!(), alloc);
                error!("{}: device: {:?}", function!(), device);
            })?;
        }
        Ok(())
    }

    pub fn set_mapped(&self, mapped: bool) {
        self.alloc.upgrade().and_then(|a| Some(a.set_mapped(mapped))).unwrap();
    }

    pub fn is_mapped(&self) -> bool {
        self.alloc.upgrade().and_then(|a| Some(a.is_mapped())).unwrap()
    }
}

impl Drop for DeviceSpecificAllocation {
    fn drop(&mut self) {
        self.tag = 0;

        let Some(alloc) = self.alloc.upgrade() else {
            error!("{}: allocation no longer exists", function!());
            return;
        };

        let Some(device) = self.device.upgrade() else {
            error!("{}: device no longer exists", function!());
            return;
        };

        let context_id = device.context().and_then(|id| Some(id.get())).unwrap_or(0);
        let has_virgl = self.virgl.load(Ordering::SeqCst);

        trace!("{}: detaching resource {} from context {} (shadow virgl: {})", function!(), alloc.id, context_id, has_virgl);
        match device.context_detach_resource(alloc.id, has_virgl) {
            Ok(()) => {},
            Err(e) => {
                error!("{}: failed to detach resource {} from context {} (shadow virgl: {}): {:?}", function!(), alloc.id, context_id, has_virgl, e);
                error!("{}: alloc: {:?}", function!(), alloc);
                error!("{}: device: {:?}", function!(), device);
            },
        };

        if let Some(alloc) = self.owned.take() {
            trace!("{}: deallocating resource {})", function!(), alloc.id);
        }
    }
}

impl fmt::Debug for DeviceSpecificAllocation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("DeviceSpecificAllocation")
            .field("device", &self.device)
            .finish()
    }
}

const ALLOCATION_FLAG_CREATED: u32 = 1u32 << 0;
const ALLOCATION_FLAG_MAPPED:  u32 = 1u32 << 1;

#[repr(C)]
#[derive(Tagged, Debug)]
#[tagged(VIRTIO_GPU_ALLOCATION_TAG)]
pub struct Allocation {
    pub tag: u64,
    id: NonZero<u32>,
    //create_fence: Pin<Arc<KeEvent>>,
    flags: AtomicU32,
    resource: VirtioResource,
    busy: (Pin<Arc<KeEvent>>, AtomicUsize),
    sync: RwLock<Vec<NonNull<KeEvent>>>,
    device_specific: SpinMutex<BTreeMap<NonZero<u32>, Weak<DeviceSpecificAllocation>>>,
    cmd: SpinMutex<Option<AlignedBox<[u8]>>>, // This probably can just be an AtomicOptionBox
}

const _:() = assert!(size_of::<Allocation>() <= 512);

#[inline]
fn merge_entries(a: &MemEntry, b: &MemEntry) -> Option<u32> {
    if a.addr + a.length as u64 == b.addr {
        a.length.checked_add(b.length)
    } else {
        None
    }
}

impl Allocation {
    pub fn new<I: Into<VirtioResource>>(id: NonZero<u32>, cmd: Option<AlignedBox<[u8]>>, info: I) -> Result<Self, NtStatus> {
        let resource = info.into();

        // Blobs are created later
        let flags = if matches!(resource, VirtioResource::_3D { .. }) {
            ALLOCATION_FLAG_CREATED
        } else {
            0
        };

        Ok(Self {
            tag: VIRTIO_GPU_ALLOCATION_TAG,
            id,
            //fence,
            flags: AtomicU32::new(flags),
            resource,
            busy: (Arc::pin_init(KeEvent::new(EventType::Notification, false))?, AtomicUsize::new(0)),
            //sync: SpinMutex::new(Vec::new()),
            sync: RwLock::new(Vec::new()),
            //device_specific: SpinMutex::new(Vec::new()),
            device_specific: SpinMutex::new(BTreeMap::new()),
            cmd: SpinMutex::new(cmd),
        })
    }

    pub fn attached_devices_count(&self) -> usize {
        self.device_specific.lock().len()
    }

    pub fn is_mapped(&self) -> bool {
        (self.flags.load(Ordering::SeqCst) & ALLOCATION_FLAG_MAPPED) != 0
    }

    pub fn is_3d(&self) -> bool {
        matches!(self.resource, VirtioResource::_3D { .. })
    }

    pub fn is_blob(&self) -> bool {
        matches!(self.resource, VirtioResource::Blob { .. })
    }

    pub fn can_be_mapped(&self) -> bool {
        match &self.resource {
            VirtioResource::_3D { .. } => false,
            VirtioResource::Blob { flags, .. } => flags.contains(BlobFlag::MAPPABLE),
        }
    }


    fn set_flag_mapped(flags: &AtomicU32, mapped: bool) {
        if mapped {
            flags.fetch_or(ALLOCATION_FLAG_MAPPED, Ordering::SeqCst);
        } else {
            flags.fetch_and(!ALLOCATION_FLAG_MAPPED, Ordering::SeqCst);
        }
    }

    fn set_mapped(&self, mapped: bool) {
        Self::set_flag_mapped(&self.flags, mapped);
    }

    //pub fn create(&self) {
    //    self.created.store(true, Ordering::SeqCst);
    //}

    pub fn attach_sync_file(&self, event: NonNull<KeEvent>) {
        self.sync.write().push(event);
    }

    pub fn is_busy(&self) -> bool {
        self.busy.1.load(Ordering::SeqCst) > 0
    }

    pub fn wait(&self) {
        while self.is_busy() {
            let r = self.busy.0.wait_usermode(NtTime::INFINITE);
            if !self.is_busy() {
                break;
            }
            let r = ke_delay_execution_thread(NtTime::relative_ms(1));
        }
    }

    pub fn mark_busy(&self) {
        let busy_count = self.busy.1.load(Ordering::SeqCst);

        self.busy.1.fetch_add(1, Ordering::SeqCst);
        self.busy.0.clear();

        for sync in self.sync.read().deref() {
            unsafe {
                sync.as_ref().clear();
            }
        }
    }

    pub fn mark_free(&self) {
        let busy_count = self.busy.1.load(Ordering::SeqCst);

        if self.busy.1.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.busy.0.set();
            let mut sync_files = self.sync.write();
            for sync in sync_files.deref() {
                unsafe {
                    sync.as_ref().set();
                    KeEvent::destroy(*sync);
                }
            }
            sync_files.clear();
        }
    }

    pub fn resource(&self) -> &VirtioResource {
        &self.resource
    }

    pub fn id(&self) -> Option<NonZero<u32>> {
        Some(self.id)
    }

    pub fn mapped_range(&self) -> Option<(u64, u64)> {
        match &self.resource {
            VirtioResource::_3D {..} => {
                None
            },
            VirtioResource::Blob { size, map, ..} => {
                if let Some(map) = *map.read() {
                    Some((map.offset as u64 * wdk::wdm::PAGE_SIZE as u64, *size))
                } else {
                    None
                }
            },
        }
    }

    pub fn map_blob(&self, device: &Device) -> Result<(u64, u64, u32), NtStatus> {
        let is_mapped = self.is_mapped();

        match &self.resource {
            VirtioResource::_3D {..} => {
                error!("{}: cannot map 3d resource: {:?}", function!(), self);
                Err(NtStatus(STATUS::INVALID_PARAMETER))
            },
            VirtioResource::Blob { size, flags, map, ..} => {
                //if !self.can_be_mapped
                if flags.contains(BlobFlag::MAPPABLE) {
                    if is_mapped {
                        error!("{}: cannot map blob multiple times: {:?}", function!(), self);
                        Err(NtStatus(STATUS::ALREADY_COMMITTED))
                    } else {
                        let (offset_alloc, bar_offset, map_info) = device.context_map_blob(self.id, *size)?;
                        // Fuck borrow checker, this is stupid
                        Self::set_flag_mapped(&self.flags, true);
                        map.write().replace(offset_alloc);
                        Ok((bar_offset, *size, map_info))
                    }
                } else {
                    error!("{}: cannot map unmappable blob: {:?}", function!(), self);
                    Err(NtStatus(STATUS::INVALID_PARAMETER))
                }
            },
        }
    }

    pub fn unmap_blob(&self, device: &Device) -> Result<(), NtStatus> {
        if self.is_mapped() {
            self.set_mapped(false);
            let offset_alloc = match &self.resource {
                VirtioResource::_3D {..} => {
                    unreachable!();
                },
                VirtioResource::Blob {map, ..} => {
                    map.write().take().unwrap()
                },
            };
            device.context_unmap_blob(self.id, offset_alloc)
        } else {
            warn!("{}: cannot unmap not mapped resource: {:?}", function!(), self);
            Err(NtStatus(STATUS::INVALID_PARAMETER))
        }
    }

    pub fn set_blob_info(&self, blob_info: BlobInfo) -> Option<()> {
        match &self.resource {
            VirtioResource::_3D {..} => {
                None
            },
            VirtioResource::Blob {info, ..} => {
                info.write().replace(blob_info);
                Some(())
            },
        }
    }

    pub fn attach_to_device(self: Arc<Self>, device: Arc<Device>, owned: bool) -> Result<Arc<DeviceSpecificAllocation>, NtStatus> {
        let mut attached = self.device_specific.lock();
        let context_id = device.context().ok_or(STATUS::REINITIALIZATION_NEEDED).inspect_err(|e|
            error!("{}: context on device {:?} was not initialized yet", function!(), device)
        )?;

        if let Some(device_specific) = attached.get(&context_id).and_then(|ds| ds.upgrade()) {
            if owned {
                warn!("{}: double-attaching owning device: {:?} / {:?}", function!(), self, device);
            }

            return Ok(device_specific);
        }

        //if matches!(device.capset(), Some(CapsetId::Venus)) && self.is_3d() {
        //    warn!("{}: alloc: ({}, {:?}), dev: {:?}", function!(), self.id, self.resource, device);
        //}

        //warn!("Attaching {} to {:?}", self.id, device);

        let new = Arc::try_new(DeviceSpecificAllocation::new(&device, &self, owned)?)?;
        attached.insert(context_id, Arc::downgrade(&new));

        Ok(new)
    }

    pub fn detach_from_device(&self, device_specific: Arc<DeviceSpecificAllocation>) -> Result<(), NtStatus> {
        let device = device_specific.device.upgrade().ok_or(STATUS::INVALID_HANDLE).inspect_err(|e|
            error!("{}: device no longer exists", function!())
        )?;

        let context_id = device.context().ok_or(STATUS::REINITIALIZATION_NEEDED).inspect_err(|e|
            error!("{}: context on device {:?} was not initialized yet", function!(), device_specific.device)
        )?;
        drop(device_specific);

        //warn!("Detaching {} from {:?}", self.id, device);

        //if matches!(device.capset(), Some(CapsetId::Venus)) && self.is_3d() {
        //    warn!("{}: alloc: ({}, {:?}), dev: {:?}", function!(), self.id, self.resource, device);
        //}

        let mut attached = self.device_specific.lock();
        let device_specific = attached.get(&context_id).ok_or(STATUS::INVALID_PARAMETER)?;

        if device_specific.strong_count() == 0 {
            //warn!("Removing context_id {}", context_id);
            attached.remove(&context_id);
        }

        Ok(())
    }

    pub fn description(&self) -> Result<AllocationDesc, NtStatus> {
        match &self.resource {
            VirtioResource::_3D {width, height, format, ..} => {
                let width = *width;
                let height = *height;
                let format = (*format).into();

                Ok(AllocationDesc {
                    width,
                    height,
                    format,
                })
            },
            VirtioResource::Blob {info, ..} => {
                let Some(info) = *info.read() else {
                    error!("{}: blob info is missing", function!());
                    return Err(NtStatus(STATUS::UNSUCCESSFUL));
                };

                let width = info.width;
                let height = info.height;
                let format = VirglFormat::from(info.format).into();

                Ok(AllocationDesc {
                    width,
                    height,
                    format,
                })
            },
        }
    }

    pub fn needs_transfer(&self) -> bool {
        match self.resource {
            VirtioResource::_3D { flags, .. } => flags.contains(VirglFlags::MAP_COHERENT),
            VirtioResource::Blob { mem, .. } => mem.contains(BlobMem::GUEST),
        }
    }

    pub fn can_attach(&self) -> bool {
        match self.resource {
            VirtioResource::_3D { .. } => true,
            VirtioResource::Blob { mem, .. } => mem.contains(BlobMem::GUEST),
        }
    }

    pub fn num_attached_pages(&self) -> usize {
        match &self.resource {
            VirtioResource::_3D { map, .. } => map.read().1.len(),
            VirtioResource::Blob { .. } => 0,
        }
    }

    pub fn size(&self) -> usize {
        match &self.resource {
            VirtioResource::_3D { size, .. } => *size as _,
            VirtioResource::Blob { size, .. } => *size as _,
        }
    }

    pub fn total_attached_bytes(&self) -> usize {
        match &self.resource {
            VirtioResource::_3D { map, .. } => map.read().1.iter().fold(0usize, |len, entry| len + entry.length as usize),
            VirtioResource::Blob { .. } => 0,
        }
    }

    pub fn fill_attached_pages(&self, entries: &mut [MemEntry]) -> bool {
        match &self.resource {
            VirtioResource::_3D { map, .. } => {
                entries.copy_from_slice(map.read().1.as_ref());
                true
            },
            VirtioResource::Blob { .. } => false,
        }
    }

    /*
    pub fn debug_print_attached_pages(&self, interface: &DxgkInterface) {
        match &self.resource {
            VirtioResource::_3D { map, .. } => {
                let map = map.read();

                let Some(first) = map.first() else {
                    info!("{}: no attached pages for {:?}", function!(), self);
                    return;
                };

                if let Some(ptr) = wdk::wdm::mm_map_io_space(first.addr, first.length as _, wdk::wdm::MEMORY_CACHING_TYPE::MmCached) {
                    let slice = NonNull::slice_from_raw_parts(ptr, first.length as _);
                    warn!("alloc {}: first {:?}", self.id, unsafe { slice.as_ref() } );
                    wdk::wdm::mm_unmap_io_space(ptr, first.length as _);
                } else {
                    error!("{}: failed to map {:?} for {:?}", function!(), first, self);
                }

                /*
                match microseh::try_seh(|| -> Result<(), NtStatus> {
                    //let mdl = wdk::wdm::MdlOwned::from_physical_range2(first.addr, first.length as _)?;
                    //let Some(ptr) = wdk::wdm::mm_map_locked_pages_specify_cache(&mdl, false, wdk::wdm::MEMORY_CACHING_TYPE::MmCached, None) else {
                    //    warn!("{}: failed to map alloc pages {:?} for {:?}", function!(), first, self);
                    //    return Ok(());
                    //};
                    //let slice = NonNull::slice_from_raw_parts(ptr, first.length as _);
                    //warn!("alloc {}: first {:?}", self.id, unsafe { slice.as_ref() } );
                    //wdk::wdm::mm_unmap_locked_pages(&mdl, ptr);

                    Ok(())
                }) {
                    Ok(Ok(())) => {},
                    Ok(Err(e)) => {
                        error!("{}: Error {:?} while trying to map {:?} for {:?}", function!(), e, first, self);
                        return;
                    },
                    Err(e) => {
                        error!("{}: Exception {:?} while trying to map {:?} for {:?}", function!(), e, first, self);
                        return;
                    },
                };
                */

                let Some(last) = map.last() else {
                    info!("{}: no attached pages for {:?}", function!(), self);
                    return;
                };

                if first.addr == last.addr {
                    return;
                }

                if let Some(ptr) = wdk::wdm::mm_map_io_space(last.addr, last.length as _, wdk::wdm::MEMORY_CACHING_TYPE::MmCached) {
                    let slice = NonNull::slice_from_raw_parts(ptr, last.length as _);
                    warn!("alloc {}: last {:?}", self.id, unsafe { slice.as_ref() } );
                    wdk::wdm::mm_unmap_io_space(ptr, last.length as _);
                } else {
                    error!("{}: failed to map {:?} for {:?}", function!(), last, self);
                }

                /*
                match microseh::try_seh(|| -> Result<(), NtStatus> {
                    let mdl = wdk::wdm::MdlOwned::from_physical_range2(last.addr, last.length as _)?;
                    let Some(ptr) = wdk::wdm::mm_map_locked_pages_specify_cache(&mdl, false, wdk::wdm::MEMORY_CACHING_TYPE::MmCached, None) else {
                        warn!("{}: failed to map alloc pages {:?} for {:?}", function!(), last, self);
                        return Ok(());
                    };
                    let slice = NonNull::slice_from_raw_parts(ptr, last.length as _);
                    warn!("alloc {}: last {:?}", self.id, unsafe { slice.as_ref() } );
                    wdk::wdm::mm_unmap_locked_pages(&mdl, ptr);

                    Ok(())
                }) {
                    Ok(Ok(())) => {},
                    Ok(Err(e)) => {
                        error!("{}: Error {:?} while trying to map {:?} for {:?}", function!(), e, last, self);
                        return;
                    },
                    Err(e) => {
                        error!("{}: Exception {:?} while trying to map {:?} for {:?}", function!(), e, last, self);
                        return;
                    },
                };
                */
            },
            VirtioResource::Blob { .. } => {},
        }
    }
    */

    pub fn attach_pages(&self, offset: u64, pages: &[DXGK_PTE]) -> bool {
        match &self.resource {
            VirtioResource::_3D { map, size, .. } => {
                let mut map = map.write();
                let mut attached_size = map.1.iter().fold(0u64, |len, entry| len + entry.length as u64);
                if attached_size >= *size {
                    // We got a duplicate, probably because there wasn't enough DMA buffer space to encode the command last time
                    // TODO: this should be detected more reliably by checking the virtual address
                    warn!("{}: already fully attached, but tried to attach again: {:?}", function!(), pages);
                    return true;
                }

                if attached_size > 0 && offset == 0 {
                    warn!("{}: already fully attached, but tried to attach again from the same offset: {:?}", function!(), pages);
                }

                for entry in pages {
                    if !entry.Flags().Valid() {
                        error!("{}: detaching is not yet supported: {:?}", function!(), pages);
                        return false;
                    }
                }

                let entries = pages.iter().map(|entry| {
                    let length = entry.Len() as u32;
                    assert!(length > 0, "unexpected page size: {}", length);
                    assert!(entry.Flags().Segment() == 0, "only system memory backing for 3d resources is supported, not {}", entry.Flags().Segment());
                    let addr = entry.PageAddress() * wdk::wdm::PAGE_SIZE as u64;

                    attached_size += length as u64;

                    MemEntry {
                        addr,
                        length,
                        _padding: 0,
                    }
                }).coalesce(|previous, current| {
                    if let Some(new_length) = merge_entries(&previous, &current) {
                        // Physically contiguous pages, reuse last entry
                        Ok(MemEntry {
                            length: new_length,
                            ..previous
                        })
                    } else {
                        Err((previous, current))
                    }
                });

                if offset < map.0 {
                    warn!("{}: out of order attach: attached pages start from offset {}, new offset {}", function!(), offset, map.0);
                    map.1.insert_many(0, entries);
                } else {
                    for entry in entries {
                        if let Some(previous) = map.1.last_mut() && let Some(new_length) = merge_entries(previous, &entry) {
                            // Physically contiguous pages, reuse last entry
                            previous.length = new_length;
                        } else {
                            map.1.push(entry);
                        }
                    }
                }

                /*

                if offset < map.0 {
                    error!("{}: out of order attach: attached pages start from offset {}, new offset {}", function!(), offset, map.0);
                    return false;
                } else {
                    for page in pages {
                        let length = page.Len() as u32;
                        assert!(length > 0, "unexpected page size: {}", length);
                        assert!(page.Flags().Segment() == 0, "only system memory backing for 3d resources is supported, not {}", page.Flags().Segment());
                        let addr = page.PageAddress() * wdk::wdm::PAGE_SIZE as u64;

                        attached_size += length as u64;

                        let entry = MemEntry {
                            addr,
                            length,
                            _padding: 0,
                        };

                        if let Some(previous) = map.1.last_mut() && let Some(new_length) = merge_entries(previous, &entry) {
                            // Physically contiguous pages, reuse last entry
                            previous.length = new_length;
                        } else {
                            map.1.push(entry);
                        }
                    }
                }
                */

                debug!("{}: ready: {}, pages: {:?}", function!(), attached_size >= *size, pages);
                debug!("{}: ({} / {}): {:?}", function!(), map.0, offset, map.1);
                map.0 = offset;

                attached_size >= *size
            },
            VirtioResource::Blob { .. } => false,
        }
    }

    pub fn query_layout(self: &Arc<Allocation>, device: Arc<Device>) -> Result<Option<VirglResourceLayout>, NtStatus> {
        if !self.is_3d() {
            error!("{}: cannot query layout for blob resource: {:?}", function!(), self);
            return Err(NtStatus(STATUS::INVALID_PARAMETER));
        }
        match &self.resource {
            VirtioResource::_3D { layout, .. } => {
                if let Some(layout) = *layout.read() {
                    return Ok(Some(layout))
                }
            }
            _ => unreachable!(),
        }

        match device.query_layout(&self) {
            Ok(layout_) => {
                match &self.resource {
                    VirtioResource::_3D { layout, .. } => {
                        layout.write().replace(layout_);
                    }
                    _ => unreachable!(),
                }

                Ok(Some(layout_))
            },
            Err(NtStatus(STATUS::REINITIALIZATION_NEEDED)) => Ok(None),
            Err(e) => {
                error!("{}: failed to query layout: {:?}", function!(), e);
                Err(e)
            }
        }
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        self.tag = 0;
    }
}

/*
#[repr(C)]
#[derive(Tagged, Debug)]
#[tagged(VIRTIO_GPU_RESOURCE_TAG)]
pub struct Resource {
    pub tag: u64,
    alloc: Weak<Allocation>,
    owned_device_specific: SpinMutex<SmallVec<[Arc<DeviceSpecificAllocation>; 2]>>
}

impl Resource {
    pub fn new(alloc: &Arc<Allocation>) -> Result<Arc<Self>, NtStatus> {
        Ok(Arc::try_new(Self {
            tag: VIRTIO_GPU_RESOURCE_TAG,
            alloc: Arc::downgrade(alloc),
            owned_device_specific: SpinMutex::new(SmallVec::new()),
        })?)
    }

    pub fn attach_to_device(self: Arc<Self>, device: Arc<Device>) -> Result<(), NtStatus> {
        let Some(alloc) = self.alloc.upgrade() else {
            error!("{}: dead allocation for resource: {:?}", function!(), self);
            return Err(NtStatus(STATUS::INVALID_PARAMETER));
        };

        warn!("{}: alloc: ({}, {:?}), dev: {:?}", function!(), alloc.id().unwrap(), alloc.resource(), device);

        let mut owned_device_specific = self.owned_device_specific.lock();

        if let Some(device_specific) = owned_device_specific.iter().find(|ds| ds.device.upgrade().and_then(|dev| Some(Arc::ptr_eq(&dev, &device))).unwrap_or(false)) {
            assert!(Arc::ptr_eq(&device_specific.alloc.upgrade().unwrap(), &alloc));
            /* Already attached, no need to do anything */
        } else {
            /* Attaching once more */
            let device_specific = alloc.attach_to_device(device, false)?;
            owned_device_specific.push(device_specific);
        }

        Ok(())
    }
}*/
