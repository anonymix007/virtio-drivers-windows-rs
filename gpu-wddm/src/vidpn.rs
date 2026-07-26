use core::{
    ptr::{
        null_mut,
    },
    num::NonZero,
    fmt,
    ops::{
        Deref,
        DerefMut,
    },
    sync::atomic::{
        AtomicU64,
        AtomicPtr,
        AtomicBool,
        Ordering,
    },
};
use alloc::{
    vec::Vec,
    boxed::Box,
    sync::{
        Arc,
        Weak,
    },
};

use wdk::{
    dxgkrnl::*,
    wdm::{
        KeTimer,
        NtTime,
    },
    *,
};

use crate::allocation::*;
use crate::adapter::*;
use crate::queue::*;
use crate::*;

use virtio_drivers::{
    BufferDirection,
    Dma,
    device::gpu::*,
};
use zerocopy::*;
use crossbeam::queue::*;
use spin::mutex::SpinMutex;

pub struct VidPnInterface(D3DKMDT_HVIDPN, *const DXGK_VIDPN_INTERFACE);

#[macro_export]
macro_rules! vidpn_call_status {
    ($op:tt $irql:ident | $self:ident . $func:ident ( $($args:expr),* )) => {{
        if let Some(func) = unsafe { (*$self.1).$func } {
            let result = wdm_call_status!($op $irql | func($self.0, $($args),*));
            result.map_err(|e| NtStatus(e))
        } else {
            error!(concat!("func ", stringify!($func), " is none"));
            Err(NtStatus(STATUS::INVALID_PARAMETER))
        }
    }};
}

impl VidPnInterface {
    pub fn new(handle: D3DKMDT_HVIDPN, interface: *const DXGK_VIDPN_INTERFACE) -> Self {
        trace!("{}: {:?}", function!(), handle);
        Self(handle, interface)
    }

    pub fn get_topology(&self) -> Result<VidPnTopologyInterface, NtStatus> {
        trace!("{}", function!());
        let mut handle: D3DKMDT_HVIDPNTOPOLOGY = null_mut();
        let mut interface: *const DXGK_VIDPNTOPOLOGY_INTERFACE = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnGetTopology(&mut handle as _, &mut interface as _))?;
        Ok(VidPnTopologyInterface::new(handle, interface))
    }

    pub fn acquire_source_mode_set(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID) -> Result<VidPnSourceModeSetInterface, NtStatus> {
        trace!("{}", function!());
        let mut handle: D3DKMDT_HVIDPNSOURCEMODESET = null_mut();
        let mut interface: *const DXGK_VIDPNSOURCEMODESET_INTERFACE = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAcquireSourceModeSet(source, &mut handle as _, &mut interface as _))?;
        Ok(VidPnSourceModeSetInterface::new(handle, interface))
    }

    pub fn acquire_source_mode_set_autorelease(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID) -> Result<VidPnSourceModeSetInterfaceGuard<'_>, NtStatus> {
        trace!("{}", function!());
        VidPnSourceModeSetInterfaceGuard::acquire(self, source)
    }

    pub fn release_source_mode_set(&self, set: VidPnSourceModeSetInterface) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnReleaseSourceModeSet(set.0))?;
        Ok(())
    }

    pub fn create_new_source_mode_set(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID) -> Result<VidPnSourceModeSetInterface, NtStatus> {
        trace!("{}", function!());
        let mut handle: D3DKMDT_HVIDPNSOURCEMODESET = null_mut();
        let mut interface: *const DXGK_VIDPNSOURCEMODESET_INTERFACE = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnCreateNewSourceModeSet(source, &mut handle as _, &mut interface as _))?;
        Ok(VidPnSourceModeSetInterface::new(handle, interface))
    }

    pub fn create_new_source_mode_set_autorelease(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID) -> Result<VidPnSourceModeSetInterfaceGuard<'_>, NtStatus> {
        trace!("{}", function!());
        VidPnSourceModeSetInterfaceGuard::create_new(self, source)
    }

    pub fn assign_source_mode_set(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID, handle: D3DKMDT_HVIDPNSOURCEMODESET) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAssignSourceModeSet(source, handle))?;
        Ok(())
    }

    pub fn assign_source_mode_set_autorelease(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID, mut source_mode_set_guard: VidPnSourceModeSetInterfaceGuard) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let source_mode_set = source_mode_set_guard.take().unwrap();
        match self.assign_source_mode_set(source, source_mode_set.0) {
            Ok(()) => {
                trace!("{}: {:?}", function!(), source_mode_set.0);
                Ok(())
            },
            Err(e) => {
                source_mode_set_guard.0.replace(source_mode_set);
                Err(e)
            },
        }
    }

    pub fn acquire_target_mode_set(&self, source: D3DDDI_VIDEO_PRESENT_TARGET_ID) -> Result<VidPnTargetModeSetInterface, NtStatus> {
        trace!("{}", function!());
        let mut handle: D3DKMDT_HVIDPNTARGETMODESET = null_mut();
        let mut interface: *const DXGK_VIDPNTARGETMODESET_INTERFACE = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAcquireTargetModeSet(source, &mut handle as _, &mut interface as _))?;
        Ok(VidPnTargetModeSetInterface::new(handle, interface))
    }

    pub fn acquire_target_mode_set_autorelease(&self, target: D3DDDI_VIDEO_PRESENT_TARGET_ID) -> Result<VidPnTargetModeSetInterfaceGuard<'_>, NtStatus> {
        trace!("{}", function!());
        VidPnTargetModeSetInterfaceGuard::acquire(self, target)
    }

    pub fn release_target_mode_set(&self, set: VidPnTargetModeSetInterface) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnReleaseTargetModeSet(set.0))?;
        Ok(())
    }

    pub fn create_new_target_mode_set(&self, target: D3DDDI_VIDEO_PRESENT_TARGET_ID) -> Result<VidPnTargetModeSetInterface, NtStatus> {
        trace!("{}", function!());
        let mut handle: D3DKMDT_HVIDPNTARGETMODESET = null_mut();
        let mut interface: *const DXGK_VIDPNTARGETMODESET_INTERFACE = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnCreateNewTargetModeSet(target, &mut handle as _, &mut interface as _))?;
        Ok(VidPnTargetModeSetInterface::new(handle, interface))
    }

    pub fn create_new_target_mode_set_autorelease(&self, target: D3DDDI_VIDEO_PRESENT_TARGET_ID) -> Result<VidPnTargetModeSetInterfaceGuard<'_>, NtStatus> {
        trace!("{}", function!());
        VidPnTargetModeSetInterfaceGuard::create_new(self, target)
    }

    pub fn assign_target_mode_set(&self, target: D3DDDI_VIDEO_PRESENT_TARGET_ID, handle: D3DKMDT_HVIDPNTARGETMODESET) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAssignTargetModeSet(target, handle))?;
        Ok(())
    }

    pub fn assign_target_mode_set_autorelease(&self, target: D3DDDI_VIDEO_PRESENT_TARGET_ID, mut target_mode_set_guard: VidPnTargetModeSetInterfaceGuard) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let target_mode_set = target_mode_set_guard.take().unwrap();
        match self.assign_target_mode_set(target, target_mode_set.0) {
            Ok(()) => {
                trace!("{}: {:?}", function!(), target_mode_set.0);
                Ok(())
            },
            Err(e) => {
                target_mode_set_guard.0.replace(target_mode_set);
                Err(e)
            },
        }
    }
}

pub struct VidPnTopologyInterface(D3DKMDT_HVIDPNTOPOLOGY, *const DXGK_VIDPNTOPOLOGY_INTERFACE);

impl VidPnTopologyInterface {
    pub fn new(handle: D3DKMDT_HVIDPNTOPOLOGY, interface: *const DXGK_VIDPNTOPOLOGY_INTERFACE) -> Self {
        trace!("{}: {:?}", function!(), handle);
        Self(handle, interface)
    }

    pub fn get_num_paths_from_source(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID) -> Result<u64, NtStatus> {
        trace!("{}", function!());
        let mut num_paths: u64 = 0;
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnGetNumPathsFromSource(source, &mut num_paths as _))?;
        Ok(num_paths)
    }

    pub fn enum_path_targets_from_source(&self, source_id: D3DDDI_VIDEO_PRESENT_SOURCE_ID, index: u64) -> Result<D3DDDI_VIDEO_PRESENT_TARGET_ID, NtStatus> {
        trace!("{}", function!());
        let mut target_id = !0;
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnEnumPathTargetsFromSource(source_id, index as _, &mut target_id as _))?;
        Ok(target_id)
    }

    pub fn get_num_paths(&self) -> Result<u64, NtStatus> {
        trace!("{}", function!());
        let mut num_paths = 0;
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnGetNumPaths(&mut num_paths as _))?;
        Ok(num_paths)
    }

    pub fn acquire_path_info(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID, target: D3DDDI_VIDEO_PRESENT_TARGET_ID) -> Result<Option<&'static D3DKMDT_VIDPN_PRESENT_PATH>, NtStatus> {
        trace!("{}", function!());
        let mut path_info: *mut D3DKMDT_VIDPN_PRESENT_PATH = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAcquirePathInfo(source, target, &mut path_info as *mut _ as *mut *const _))?;
        Ok(NonNull::new(path_info).and_then(|p| Some(unsafe { p.as_ref() })))
    }

    pub fn acquire_path_info_autorelease(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID, target: D3DDDI_VIDEO_PRESENT_TARGET_ID) -> Result<VidPnPathInfoGuard<'_>, NtStatus> {
        trace!("{}", function!());
        VidPnPathInfoGuard::acquire(self, source, target)
    }

    pub fn release_path_info(&self, path_info: &D3DKMDT_VIDPN_PRESENT_PATH) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnReleasePathInfo(path_info))?;
        Ok(())
    }

    pub fn acquire_first_path_info(&self) -> Result<&'static D3DKMDT_VIDPN_PRESENT_PATH, NtStatus> {
        trace!("{}", function!());
        let mut path_info = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAcquireFirstPathInfo(&mut path_info as *mut _ as *mut *const _))?;
        Ok(NonNull::new(path_info).and_then(|p| Some(unsafe { p.as_ref() })).unwrap())
    }

    pub fn acquire_first_path_info_autorelease(&self) -> Result<Option<VidPnPathInfoGuard<'_>>, NtStatus> {
        trace!("{}", function!());
        VidPnPathInfoGuard::acquire_first(self)
    }

    pub fn acquire_next_path_info(&self, previous: &D3DKMDT_VIDPN_PRESENT_PATH) -> Result<Option<&'static D3DKMDT_VIDPN_PRESENT_PATH>, NtStatus> {
        trace!("{}", function!());
        let mut path_info = null_mut();
        match vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAcquireNextPathInfo(previous, &mut path_info as *mut _ as *mut *const _)) {
            Ok(()) => Ok(NonNull::new(path_info).and_then(|p| Some(unsafe { p.as_ref() }))),
            Err(NtStatus(STATUS::GRAPHICS_NO_MORE_ELEMENTS_IN_DATASET)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn acquire_next_path_info_autorelease(&self, current: &D3DKMDT_VIDPN_PRESENT_PATH) -> Result<Option<VidPnPathInfoGuard<'_>>, NtStatus> {
        VidPnPathInfoGuard::acquire_next(self, current)
    }

    pub fn update_path_support_info(&self, path: &D3DKMDT_VIDPN_PRESENT_PATH) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnUpdatePathSupportInfo(path))?;
        Ok(())
    }
}

pub struct VidPnPathInfoIter<'a> {
    current: Option<&'static D3DKMDT_VIDPN_PRESENT_PATH>,
    topology: &'a VidPnTopologyInterface,
}

impl<'a> VidPnPathInfoIter<'a> {
    pub fn new(topology: &'a VidPnTopologyInterface) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let current = Some(topology.acquire_first_path_info()?);

        Ok(Self {
            current,
            topology,
        })
    }

    pub fn try_next(&mut self) -> Result<Option<VidPnPathInfoGuard<'_>>, NtStatus> {
        trace!("{}", function!());
        let Some(current) = self.current.take() else {
            return Ok(None);
        };

        match self.topology.acquire_next_path_info(current) {
            Ok(Some(next)) => {
                self.current = Some(next);
                //let _ = self.current.replace(next);
                Ok(Some(VidPnPathInfoGuard(Some(current), self.topology)))
            },
            Ok(None) => {
                Ok(Some(VidPnPathInfoGuard(Some(current), self.topology)))
            },
            Err(e) => {
                let _ = self.topology.release_path_info(current).inspect_err(|e|
                    error!("{}: failed to release path info: {:?}", function!(), e)
                );
                //let _ = self.current.replace(current);
                Err(e)
            }
        }

        //if let Some(next) = self.topology.acquire_next_path_info(current)? {
        //    let _ = self.current.replace(next);
        //}
        //
        //Ok(Some(VidPnPathInfoGuard(Some(current), self.topology)))
    }
}

impl<'a> Drop for VidPnPathInfoIter<'a> {
    fn drop(&mut self) {
        if let Some(v) = self.current.take() {
            trace!("{}", function!());
            let _ = self.topology.release_path_info(v).inspect_err(|e|
                error!("{}: failed to release path info: {:?}", function!(), e)
            );
        }
    }
}

pub struct VidPnPathInfoGuard<'a>(Option<&'static D3DKMDT_VIDPN_PRESENT_PATH>, &'a VidPnTopologyInterface);

impl<'a> VidPnPathInfoGuard<'a> {
    pub fn acquire(topology: &'a VidPnTopologyInterface, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID, target: D3DDDI_VIDEO_PRESENT_TARGET_ID) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let Some(path_info) = topology.acquire_path_info(source, target)? else {
            error!("{}: failed to acquire path info for source {}, target {}: no path available", function!(), source, target);
            return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDPN_TOPOLOGY));
        };
        trace!("{}: {:?}", function!(), path_info as *const _ as HANDLE);
        Ok(Self(Some(path_info), topology))
    }

    pub fn acquire_first(topology: &'a VidPnTopologyInterface) -> Result<Option<Self>, NtStatus> {
        trace!("{}", function!());
        let first = topology.acquire_first_path_info()?;
        trace!("{}: {:?}", function!(), first as *const _ as HANDLE);
        Ok(Some(Self(Some(first), topology)))
    }

    pub fn acquire_next(topology: &'a VidPnTopologyInterface, current: &D3DKMDT_VIDPN_PRESENT_PATH) -> Result<Option<Self>, NtStatus> {
        trace!("{}", function!());
        let next = topology.acquire_next_path_info(current)?;
        if let Some(next) = next {
            trace!("{}: {:?}", function!(), next as *const _ as HANDLE);
            Ok(Some(Self(Some(next), topology)))
        } else {
            Ok(None)
        }
    }

    pub fn take(&mut self) -> Option<&'static D3DKMDT_VIDPN_PRESENT_PATH> {
        self.0.take()
    }
}

impl<'a> Drop for VidPnPathInfoGuard<'a> {
    fn drop(&mut self) {
        if let Some(v) = self.take() {
            trace!("{}: {:?}", function!(), v as *const _ as HANDLE);
            let _ = self.1.release_path_info(v).inspect_err(|e|
                error!("{}: failed to release path info: {:?}", function!(), e)
            );
        }
    }
}

impl<'a> Deref for VidPnPathInfoGuard<'a> {
    type Target = D3DKMDT_VIDPN_PRESENT_PATH;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

pub struct VidPnSourceModeSetInterface(D3DKMDT_HVIDPNSOURCEMODESET, *const DXGK_VIDPNSOURCEMODESET_INTERFACE);

impl VidPnSourceModeSetInterface {
    pub fn new(handle: D3DKMDT_HVIDPNSOURCEMODESET, interface: *const DXGK_VIDPNSOURCEMODESET_INTERFACE) -> Self {
        trace!("{}: {:?}", function!(), handle);
        Self(handle, interface)
    }

    pub fn acquire_pinned_mode_info(&self) -> Result<Option<&'static D3DKMDT_VIDPN_SOURCE_MODE>, NtStatus> {
        trace!("{}", function!());
        let mut mode_info: *mut D3DKMDT_VIDPN_SOURCE_MODE = null_mut();
        match vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAcquirePinnedModeInfo(&mut mode_info as *mut _ as *mut *const _)) {
            Ok(()) => Ok(NonNull::new(mode_info).and_then(|m| Some(unsafe { m.as_ref() }))),
            Err(NtStatus(STATUS::GRAPHICS_MODE_NOT_PINNED)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn acquire_pinned_mode_info_autorelease(&self) -> Result<Option<VidPnSourceModeGuard<'_>>, NtStatus> {
        trace!("{}", function!());
        VidPnSourceModeGuard::acquire(self)
    }

    pub fn create_new_mode_info(&self) -> Result<&'static mut D3DKMDT_VIDPN_SOURCE_MODE, NtStatus> {
        trace!("{}", function!());
        let mut mode_info: *mut D3DKMDT_VIDPN_SOURCE_MODE = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnCreateNewModeInfo(&mut mode_info as *mut _ as *mut *mut _))?;

        Ok(NonNull::new(mode_info).and_then(|mut p| Some(unsafe { p.as_mut() })).unwrap())
    }

    pub fn create_new_mode_info_autorelease(&self) -> Result<VidPnSourceModeGuardMut<'_>, NtStatus> {
        trace!("{}", function!());
        VidPnSourceModeGuardMut::create_new(self)
    }

    pub fn release_mode_info(&self, mode_info: &D3DKMDT_VIDPN_SOURCE_MODE) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnReleaseModeInfo(mode_info as _))?;
        Ok(())
    }

    pub fn add_mode(&self, mode_info: &mut D3DKMDT_VIDPN_SOURCE_MODE) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAddMode(mode_info as _))?;
        Ok(())
    }

    pub fn add_mode_autorelease(&self, mut mode_info_guard: VidPnSourceModeGuardMut) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let mode_info = mode_info_guard.take().unwrap();
        match self.add_mode(mode_info) {
            Ok(()) => {
                trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
                Ok(())
            },
            Err(NtStatus(STATUS::GRAPHICS_MODE_ALREADY_IN_MODESET)) => {
                // Already in set, release and continue
                mode_info_guard.0.replace(mode_info);
                Ok(())
            },
            Err(e) => {
                error!("{}: failed to add mode: {:?}", function!(), e);
                mode_info_guard.0.replace(mode_info);
                Err(e)
            }
        }
    }
}

pub struct VidPnSourceModeSetInterfaceGuard<'a>(Option<VidPnSourceModeSetInterface>, &'a VidPnInterface);

impl<'a> VidPnSourceModeSetInterfaceGuard<'a> {
    pub fn acquire(vidpn: &'a VidPnInterface, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let source_mode_set = vidpn.acquire_source_mode_set(source)?;
        trace!("{}: {:?}", function!(), source_mode_set.0);
        Ok(Self(Some(source_mode_set), vidpn))
    }

    pub fn create_new(vidpn: &'a VidPnInterface, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let source_mode_set = vidpn.create_new_source_mode_set(source)?;
        trace!("{}: {:?}", function!(), source_mode_set.0);
        Ok(Self(Some(source_mode_set), vidpn))
    }

    pub fn take(&mut self) -> Option<VidPnSourceModeSetInterface> {
        self.0.take()
    }
}

impl<'a> Drop for VidPnSourceModeSetInterfaceGuard<'a> {
    fn drop(&mut self) {
        if let Some(v) = self.take() {
            trace!("{}: {:?}", function!(), v.0);
            let _ = self.1.release_source_mode_set(v).inspect_err(|e|
                error!("{}: failed to release source mode set: {:?}", function!(), e)
            );
        }
    }
}

impl<'a> Deref for VidPnSourceModeSetInterfaceGuard<'a> {
    type Target = VidPnSourceModeSetInterface;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

pub struct VidPnSourceModeGuard<'a>(Option<&'static D3DKMDT_VIDPN_SOURCE_MODE>, &'a VidPnSourceModeSetInterface);

impl<'a> VidPnSourceModeGuard<'a> {
    pub fn acquire(source_mode_set: &'a VidPnSourceModeSetInterface) -> Result<Option<Self>, NtStatus> {
        trace!("{}", function!());
        if let Some(mode_info) = source_mode_set.acquire_pinned_mode_info()? {
            trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
            Ok(Some(Self(Some(mode_info), source_mode_set)))
        } else {
            Ok(None)
        }
    }

    pub fn create_new(source_mode_set: &'a VidPnSourceModeSetInterface) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let mode_info = source_mode_set.create_new_mode_info()?;
        trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
        Ok(Self(Some(mode_info), source_mode_set))
    }

    pub fn take(&mut self) -> Option<&'static D3DKMDT_VIDPN_SOURCE_MODE> {
        self.0.take()
    }

}

impl<'a> Drop for VidPnSourceModeGuard<'a> {
    fn drop(&mut self) {
        if let Some(v) = self.take() {
            trace!("{}: {:?}", function!(), v as *const _ as HANDLE);
            let _ = self.1.release_mode_info(v).inspect_err(|e|
                error!("{}: failed to release source mode info: {:?}", function!(), e)
            );
        }
    }
}

impl<'a> Deref for VidPnSourceModeGuard<'a> {
    type Target = D3DKMDT_VIDPN_SOURCE_MODE;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

pub struct VidPnSourceModeGuardMut<'a>(Option<&'static mut D3DKMDT_VIDPN_SOURCE_MODE>, &'a VidPnSourceModeSetInterface);

impl<'a> VidPnSourceModeGuardMut<'a> {
    pub fn create_new(source_mode_set: &'a VidPnSourceModeSetInterface) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let mode_info = source_mode_set.create_new_mode_info()?;
        trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
        Ok(Self(Some(mode_info), source_mode_set))
    }

    pub fn take(&mut self) -> Option<&'static mut D3DKMDT_VIDPN_SOURCE_MODE> {
        self.0.take()
    }
}

impl<'a> Drop for VidPnSourceModeGuardMut<'a> {
    fn drop(&mut self) {
        if let Some(v) = self.take() {
            trace!("{}: {:?}", function!(), v as *const _ as HANDLE);
            let _ = self.1.release_mode_info(v).inspect_err(|e|
                error!("{}: failed to release source mode info: {:?}", function!(), e)
            );
        }
    }
}

impl<'a> Deref for VidPnSourceModeGuardMut<'a> {
    type Target = D3DKMDT_VIDPN_SOURCE_MODE;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl<'a> DerefMut for VidPnSourceModeGuardMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}

pub struct VidPnTargetModeSetInterface(D3DKMDT_HVIDPNTARGETMODESET, *const DXGK_VIDPNTARGETMODESET_INTERFACE);

impl VidPnTargetModeSetInterface {
    pub fn new(handle: D3DKMDT_HVIDPNTARGETMODESET, interface: *const DXGK_VIDPNTARGETMODESET_INTERFACE) -> Self {
        trace!("{}: {:?}", function!(), handle);
        Self(handle, interface)
    }

    pub fn get_num_modes(&self) -> Result<u64, NtStatus> {
        trace!("{}", function!());
        let mut num_modes: u64 = 0;
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnGetNumModes(&mut num_modes as _))?;
        Ok(num_modes)
    }

    pub fn create_new_mode_info(&self) -> Result<&'static mut D3DKMDT_VIDPN_TARGET_MODE, NtStatus> {
        trace!("{}", function!());
        let mut mode_info: *mut D3DKMDT_VIDPN_TARGET_MODE = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnCreateNewModeInfo(&mut mode_info as _))?;
        Ok(NonNull::new(mode_info).and_then(|mut p| Some(unsafe { p.as_mut() })).unwrap())
    }

    pub fn acquire_pinned_mode_info(&self) -> Result<Option<&'static D3DKMDT_VIDPN_TARGET_MODE>, NtStatus> {
        trace!("{}", function!());
        let mut mode_info: *mut D3DKMDT_VIDPN_TARGET_MODE = null_mut();
        match vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAcquirePinnedModeInfo(&mut mode_info as *mut _ as *mut *const _)) {
            Ok(()) => Ok(NonNull::new(mode_info).and_then(|m| Some(unsafe { m.as_ref() }))),
            Err(NtStatus(STATUS::GRAPHICS_MODE_NOT_PINNED)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn acquire_pinned_mode_info_autorelease(&self) -> Result<Option<VidPnTargetModeGuard<'_>>, NtStatus> {
        trace!("{}", function!());
        VidPnTargetModeGuard::acquire(self)
    }

    pub fn create_new_mode_info_autorelease(&self) -> Result<VidPnTargetModeGuardMut<'_>, NtStatus> {
        trace!("{}", function!());
        VidPnTargetModeGuardMut::create_new(self)
    }

    pub fn add_mode(&self, mode_info: &mut D3DKMDT_VIDPN_TARGET_MODE) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAddMode(mode_info as _))?;
        Ok(())
    }

    pub fn release_mode_info(&self, mode_info: &D3DKMDT_VIDPN_TARGET_MODE) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnReleaseModeInfo(mode_info as _))?;
        Ok(())
    }

    pub fn add_mode_autorelease(&self, mut mode_info_guard: VidPnTargetModeGuardMut) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let mode_info = mode_info_guard.take().unwrap();
        match self.add_mode(mode_info) {
            Ok(()) => {
                trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
                Ok(())
            },
            Err(NtStatus(STATUS::GRAPHICS_MODE_ALREADY_IN_MODESET)) => {
                // Already in set, release and continue
                mode_info_guard.0.replace(mode_info);
                Ok(())
            },
            Err(e) => {
                error!("{}: failed to add mode: {:?}", function!(), e);
                mode_info_guard.0.replace(mode_info);
                Err(e)
            }
        }
    }
}

pub struct VidPnTargetModeSetInterfaceGuard<'a>(Option<VidPnTargetModeSetInterface>, &'a VidPnInterface);

impl<'a> VidPnTargetModeSetInterfaceGuard<'a> {
    pub fn acquire(vidpn: &'a VidPnInterface, target: D3DDDI_VIDEO_PRESENT_TARGET_ID) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let target_mode_set = vidpn.acquire_target_mode_set(target)?;

        trace!("{}: {:?}", function!(), target_mode_set.0);
        Ok(Self(Some(target_mode_set), vidpn))
    }

    pub fn create_new(vidpn: &'a VidPnInterface, target: D3DDDI_VIDEO_PRESENT_TARGET_ID) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let target_mode_set = vidpn.create_new_target_mode_set(target)?;
        trace!("{}: {:?}", function!(), target_mode_set.0);
        Ok(Self(Some(target_mode_set), vidpn))
    }

    pub fn take(&mut self) -> Option<VidPnTargetModeSetInterface> {
        self.0.take()
    }
}

impl<'a> Drop for VidPnTargetModeSetInterfaceGuard<'a> {
    fn drop(&mut self) {
        if let Some(v) = self.take() {
            trace!("{}: {:?}", function!(), v.0);
            let _ = self.1.release_target_mode_set(v).inspect_err(|e|
                error!("{}: failed to release target mode set: {:?}", function!(), e)
            );
        }
    }
}

impl<'a> Deref for VidPnTargetModeSetInterfaceGuard<'a> {
    type Target = VidPnTargetModeSetInterface;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

pub struct VidPnTargetModeGuard<'a>(Option<&'static D3DKMDT_VIDPN_TARGET_MODE>, &'a VidPnTargetModeSetInterface);

impl<'a> VidPnTargetModeGuard<'a> {
    pub fn acquire(target_mode_set: &'a VidPnTargetModeSetInterface) -> Result<Option<Self>, NtStatus> {
        trace!("{}", function!());
        if let Some(mode_info) = target_mode_set.acquire_pinned_mode_info()? {
            trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
            Ok(Some(Self(Some(mode_info), target_mode_set)))
        } else {
            Ok(None)
        }
    }

    pub fn create_new(target_mode_set: &'a VidPnTargetModeSetInterface) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let mode_info = target_mode_set.create_new_mode_info()?;
        trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
        Ok(Self(Some(mode_info), target_mode_set))
    }

    pub fn take(&mut self) -> Option<&'static D3DKMDT_VIDPN_TARGET_MODE> {
        self.0.take()
    }

}

impl<'a> Drop for VidPnTargetModeGuard<'a> {
    fn drop(&mut self) {
        if let Some(v) = self.take() {
            trace!("{}: {:?}", function!(), v as *const _ as HANDLE);
            let _ = self.1.release_mode_info(v).inspect_err(|e|
                error!("{}: failed to release target mode info: {:?}", function!(), e)
            );
        }
    }
}

impl<'a> Deref for VidPnTargetModeGuard<'a> {
    type Target = D3DKMDT_VIDPN_TARGET_MODE;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

pub struct VidPnTargetModeGuardMut<'a>(Option<&'static mut D3DKMDT_VIDPN_TARGET_MODE>, &'a VidPnTargetModeSetInterface);

impl<'a> VidPnTargetModeGuardMut<'a> {
    pub fn create_new(target_mode_set: &'a VidPnTargetModeSetInterface) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let mode_info = target_mode_set.create_new_mode_info()?;
        trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
        Ok(Self(Some(mode_info), target_mode_set))
    }

    pub fn take(&mut self) -> Option<&'static mut D3DKMDT_VIDPN_TARGET_MODE> {
        self.0.take()
    }
}

impl<'a> Drop for VidPnTargetModeGuardMut<'a> {
    fn drop(&mut self) {
        if let Some(v) = self.take() {
            trace!("{}: {:?}", function!(), v as *const _ as HANDLE);
            let _ = self.1.release_mode_info(v).inspect_err(|e|
                error!("{}: failed to release target mode info: {:?}", function!(), e)
            );
        }
    }
}

impl<'a> Deref for VidPnTargetModeGuardMut<'a> {
    type Target = D3DKMDT_VIDPN_TARGET_MODE;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl<'a> DerefMut for VidPnTargetModeGuardMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}

pub struct MonitorSourceModeSetInterface(D3DKMDT_HMONITORSOURCEMODESET, *const DXGK_MONITORSOURCEMODESET_INTERFACE);

impl MonitorSourceModeSetInterface {
    pub fn new(handle: D3DKMDT_HMONITORSOURCEMODESET, interface: *const DXGK_MONITORSOURCEMODESET_INTERFACE) -> Self {
        trace!("{}: {:?}", function!(), handle);
        Self(handle, interface)
    }

    pub fn get_num_modes(&self) -> Result<u64, NtStatus> {
        trace!("{}", function!());
        let mut num_modes: u64 = 0;
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnGetNumModes(&mut num_modes as _))?;
        Ok(num_modes)
    }

    pub fn create_new_mode_info(&self) -> Result<&'static mut D3DKMDT_MONITOR_SOURCE_MODE, NtStatus> {
        trace!("{}", function!());
        let mut mode_info: *mut D3DKMDT_MONITOR_SOURCE_MODE = null_mut();
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnCreateNewModeInfo(&mut mode_info as _))?;
        Ok(NonNull::new(mode_info).and_then(|mut p| Some(unsafe { p.as_mut() })).unwrap())
    }

    pub fn create_new_mode_info_autorelease(&self) -> Result<MonitorModeGuardMut<'_>, NtStatus> {
        trace!("{}", function!());
        MonitorModeGuardMut::create_new(self)
    }

    pub fn add_mode(&self, mode_info: &mut D3DKMDT_MONITOR_SOURCE_MODE) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnAddMode(mode_info as _))?;
        Ok(())
    }

    pub fn add_mode_autorelease(&self, mut mode_info_guard: MonitorModeGuardMut) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let mode_info = mode_info_guard.take().unwrap();
        match self.add_mode(mode_info) {
            Ok(()) => {
                trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
                Ok(())
            },
            Err(NtStatus(STATUS::GRAPHICS_MODE_ALREADY_IN_MODESET)) => {
                // Already in set, release and continue
                trace!("{}: duplicate mode", function!());
                mode_info_guard.0.replace(mode_info);
                Ok(())
            },
            Err(e) => {
                error!("{}: failed to add mode: {:?}", function!(), e);
                mode_info_guard.0.replace(mode_info);
                Err(e)
            }
        }
    }

    pub fn release_mode_info(&self, mode_info: &mut D3DKMDT_MONITOR_SOURCE_MODE) -> Result<(), NtStatus> {
        trace!("{}", function!());
        vidpn_call_status!(<= PASSIVE_LEVEL | self.pfnReleaseModeInfo(mode_info as _))?;
        Ok(())
    }
}

pub struct MonitorModeGuardMut<'a>(Option<&'static mut D3DKMDT_MONITOR_SOURCE_MODE>, &'a MonitorSourceModeSetInterface);

impl<'a> MonitorModeGuardMut<'a> {
    pub fn create_new(source_mode_set: &'a MonitorSourceModeSetInterface) -> Result<Self, NtStatus> {
        trace!("{}", function!());
        let mode_info = source_mode_set.create_new_mode_info()?;

        trace!("{}: {:?}", function!(), mode_info as *const _ as HANDLE);
        Ok(Self(Some(mode_info), source_mode_set))
    }

    pub fn take(&mut self) -> Option<&'static mut D3DKMDT_MONITOR_SOURCE_MODE> {
        self.0.take()
    }
}

impl<'a> Drop for MonitorModeGuardMut<'a> {
    fn drop(&mut self) {
        if let Some(v) = self.take() {
            trace!("{}: {:?}", function!(), v as *const _ as HANDLE);
            let _ = self.1.release_mode_info(v).inspect_err(|e|
                error!("{}: failed to release source mode info: {:?}", function!(), e)
            );
        }
    }
}

impl<'a> Deref for MonitorModeGuardMut<'a> {
    type Target = D3DKMDT_MONITOR_SOURCE_MODE;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl<'a> DerefMut for MonitorModeGuardMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}

pub fn check_vidpn_present_path(num_scanouts: u8, path: &D3DKMDT_VIDPN_PRESENT_PATH) -> Result<(), NtStatus> {
    trace!("{}", function!());
    if path.VidPnSourceId >= num_scanouts as _ {
        error!("{}: invalid source id: {:?} (max {})", function!(), path.VidPnSourceId, num_scanouts);
        return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_SOURCE));
    }

    if path.VidPnTargetId >= num_scanouts as _ {
        error!("{}: invalid target id: {:?} (max {})", function!(), path.VidPnTargetId, num_scanouts);
        return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_TARGET));
    }

    if path.GammaRamp.Type != D3DDDI_GAMMARAMP_TYPE::D3DDDI_GAMMARAMP_DEFAULT {
        error!("{}: invalid gamma ramp type: {:?}", function!(), path.GammaRamp.Type);
        return Err(NtStatus(STATUS::GRAPHICS_GAMMA_RAMP_NOT_SUPPORTED));
    }
    match path.ContentTransformation.Scaling {
        D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_IDENTITY |
        D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_CENTERED |
        D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_NOTSPECIFIED |
        D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_UNINITIALIZED => {},
        _ => {
            error!("{}: invalid scaling: {:?}", function!(), path.ContentTransformation.Scaling);
            return Err(NtStatus(STATUS::GRAPHICS_VIDPN_MODALITY_NOT_SUPPORTED))
        },
    }
    match path.ContentTransformation.Rotation {
        D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_IDENTITY |
        D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_ROTATE90 |
        D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_NOTSPECIFIED |
        D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_UNINITIALIZED => {},
        _ => {
            error!("{}: invalid rotation: {:?}", function!(), path.ContentTransformation.Scaling);
            return Err(NtStatus(STATUS::GRAPHICS_VIDPN_MODALITY_NOT_SUPPORTED))
        },
    }
    match path.VidPnTargetColorBasis {
        D3DKMDT_COLOR_BASIS::D3DKMDT_CB_SCRGB |
        D3DKMDT_COLOR_BASIS::D3DKMDT_CB_UNINITIALIZED => {},
        _ => {
            error!("{}: invalid color basis: {:?}", function!(), path.VidPnTargetColorBasis);
            return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_SOURCE_MODE))
        },
    }
    Ok(())
}

pub fn check_vidpn_source_mode(source_mode: &D3DKMDT_VIDPN_SOURCE_MODE) -> Result<(), NtStatus> {
    trace!("{}", function!());
    if source_mode.Type != D3DKMDT_VIDPN_SOURCE_MODE_TYPE::D3DKMDT_RMT_GRAPHICS {
        error!("{}: invalid source mode type: {:?}", function!(), source_mode.Type);
        return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_SOURCE_MODE));
    }

    let graphics = unsafe { &source_mode.Format.Graphics };

    match graphics.ColorBasis {
        D3DKMDT_COLOR_BASIS::D3DKMDT_CB_SCRGB |
        D3DKMDT_COLOR_BASIS::D3DKMDT_CB_UNINITIALIZED => {},
        _ => {
            error!("{}: invalid color basis: {:?}", function!(), graphics.ColorBasis);
            return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_SOURCE_MODE))
        },
    }
    if graphics.PixelValueAccessMode != D3DKMDT_PIXEL_VALUE_ACCESS_MODE::D3DKMDT_PVAM_DIRECT {
        error!("{}: invalid pixel access mode: {:?}", function!(), graphics.PixelValueAccessMode);
        return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_SOURCE_MODE));
    }

    if graphics.PixelFormat != D3DDDIFORMAT::D3DDDIFMT_A8R8G8B8 {
        error!("{}: invalid pixel format: {:?}", function!(), graphics.PixelFormat);
        return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_SOURCE_MODE));
    }

    Ok(())
}

pub const REFRESH_RATE_60HZ: (u32, u32) = (148500000, 2475000);

#[derive(Debug, Clone, Copy)]
pub struct MonitorMode {
    pub width: u32,
    pub height: u32,
    // TODO
    //pub refresh_rate: (u32, u32), /* (numerator, denomenator) */
}

impl MonitorMode {
    pub fn fill_video_signal_info(&self, info: &mut D3DKMDT_VIDEO_SIGNAL_INFO) {
        info.VideoStandard = D3DKMDT_VIDEO_SIGNAL_STANDARD::D3DKMDT_VSS_OTHER;
        info.TotalSize.cx = self.width;
        info.TotalSize.cy = self.height;
        info.ActiveSize = info.TotalSize;
        if true {
            info.VSyncFreq = (148500000, 2475000).into();
            info.HSyncFreq = (67500, 1).into();
            info.PixelRate = 148500000;
        } else {
            info.VSyncFreq = (!1, !1).into();
            info.HSyncFreq = (!1, !1).into();
            info.PixelRate = !1;
        }
        info.__bindgen_anon_1.ScanLineOrdering = D3DDDI_VIDEO_SIGNAL_SCANLINE_ORDERING::D3DDDI_VSSLO_PROGRESSIVE;
    }

    pub fn fill_graphics_info(&self, info: &mut D3DKMDT_GRAPHICS_RENDERING_FORMAT) {
        info.PrimSurfSize.cx = self.width;
        info.PrimSurfSize.cy = self.height;
        info.VisibleRegionSize = info.PrimSurfSize;
        info.Stride = self.width * 4;
        info.PixelFormat = D3DDDIFORMAT::D3DDDIFMT_A8R8G8B8;
        info.ColorBasis = D3DKMDT_COLOR_BASIS::D3DKMDT_CB_SCRGB;
        info.PixelValueAccessMode = D3DKMDT_PIXEL_VALUE_ACCESS_MODE::D3DKMDT_PVAM_DIRECT;
    }
}

pub struct FlipTimerContext {
    pub rects: [commands::Rect; 16],
    pub addrs: [AtomicU64; 16],
    //pub addrs: [(AtomicU64, AtomicPtr<Allocation>); 16],
    pub flipq: [Option<Arc<ArrayQueue<(Weak<Allocation>, u64)>>>; 16],
    pub vsync_enabled: AtomicBool,
    pub chan: GpuChannel,
}

unsafe impl Send for FlipTimerContext {}

impl FlipTimerContext {
    fn flip(&self) {
        let mut scanouts = [None; 16];
        let mut scanout_sent = false;

        for (i, q) in self.flipq.iter().enumerate() {
            let Some(q) = q else {
                continue;
            };

            let Some((weak, addr)) = q.pop() else {
                let addr = self.addrs[i].load(Ordering::Acquire);
                //let addr = self.addrs[i].0.load(Ordering::Acquire);
                scanouts[i] = Some(PHYSICAL_ADDRESS { QuadPart: addr as i64 });
                /*
                let alloc = self.addrs[i].1.load(Ordering::SeqCst) as *const Allocation;
                if !alloc.is_null() {
                    let weak = unsafe { Weak::from_raw(alloc) };

                    let Some(alloc) = weak.upgrade() else {
                        error!("{}: allocation no longer exists: strong {}, weak {}", function!(), weak.strong_count(), weak.weak_count());
                        self.addrs[i].1.store(null_mut(), Ordering::SeqCst);
                        continue;
                    };

                    let _ = weak.into_raw();

                    let Some(res_id) = alloc.id() else {
                        error!("{}: allocation {:?} was not created yet", function!(), alloc);
                        continue;
                    };

                    //let _ = wdk::wdm::ke_delay_execution_thread(wdk::wdm::NtTime::relative_ms(10));
                    let _ = self.chan.resource_flush(res_id, self.rects[i]).inspect_err(|e|
                        error!("{}: failed to flush resource: {:?}", function!(), e)
                    );
                }
                */
                continue;
            };

            //self.addrs[i].0.store(addr, Ordering::Release);
            self.addrs[i].store(addr, Ordering::Release);
            scanouts[i] = Some(PHYSICAL_ADDRESS { QuadPart: addr as i64 });

            let Some(alloc) = weak.upgrade() else {
                error!("{}: allocation no longer exists: strong {}, weak {}", function!(), weak.strong_count(), weak.weak_count());
                continue;
            };

            let Some(res_id) = alloc.id() else {
                error!("{}: allocation {:?} was not created yet", function!(), alloc);
                continue;
            };

            trace!("{}: alloc id {:?}: strong: {}, weak: {}", function!(), alloc.id(), Arc::strong_count(&alloc), Arc::weak_count(&alloc));

            /*
            let prev = self.addrs[i].1.swap(weak.into_raw() as *mut _, Ordering::SeqCst);
            if !prev.is_null() {
                drop(unsafe { Weak::from_raw(prev) });
            }
            */

            trace!("{}: vsync {:x} for alloc {}", function!(), addr, res_id);
            //warn!("{}: vsync {:x} for alloc {:?}", function!(), addr, alloc);
            //alloc.debug_print_attached_pages(self.chan.dxgk_interface());

            //let _ = wdk::wdm::ke_delay_execution_thread(wdk::wdm::NtTime::relative_ms(10));

            match alloc.resource() {
                VirtioResource::_3D { .. } => {
                    let _ = self.chan.set_scanout(self.rects[i], i as u32, res_id).inspect_err(|e|
                        error!("{}: failed to set scanout: {:?}", function!(), e)
                    );
                },
                VirtioResource::Blob { info, .. } => {
                    let Some(info) = *info.read() else {
                        error!("{}: allocation {:?} does not have blob info yet", function!(), alloc);
                        continue;
                    };

                    let _ = self.chan.set_scanout_blob(self.rects[i], i as u32, res_id, info).inspect_err(|e|
                        error!("{}: failed to set scanout: {:?}", function!(), e)
                    );
                },
            }

            let _ = self.chan.resource_flush(res_id, self.rects[i]).inspect_err(|e|
                error!("{}: failed to flush resource: {:?}", function!(), e)
            );

            scanout_sent = true;
        }

        let vsync_enabled = self.vsync_enabled.load(Ordering::SeqCst);
        //info!("{}: vsync {:x} for alloc {}", function!(), Interrupt::VSync(scanouts));
        if vsync_enabled {
            let _ = self.chan.dxgk_interface().notify_interrupt_synchronized(Interrupt::VSync(scanouts)).inspect_err(|e|
                error!("{}: failed to notify about vsync: {:?}", function!(), e)
            );
        }

        if scanout_sent {
            self.chan.kick();
        }
    }
}

/*
impl Drop for FlipTimerContext {
    fn drop(&mut self) {
        for (addr, weak) in &mut self.addrs {
            let alloc = weak.swap(null_mut(), Ordering::SeqCst) as *const Allocation;
            if !alloc.is_null() {
                drop(unsafe { Weak::from_raw(alloc) });
            }
        }
    }
}
*/

pub struct FlipTimer {
    pub timer: KeTimer<FlipTimerContext>,
    pub inner: Box<FlipTimerContext>,
}

impl FlipTimer {
    pub fn try_new(data: FlipTimerContext) -> Result<Self, NtStatus> {
        let inner = Box::new(data);

        let timer = KeTimer::new_hires(|context: &FlipTimerContext| {
            //trace!("{}: flip", function!());
            context.flip();
        }, &*inner as *const _)?;

        Ok(Self {
            timer,
            inner,
        })
    }

    pub fn start(&self) -> Result<(), NtStatus> {
        Ok(self.timer.start_periodic(NtTime::fps(60))?)
    }

    pub fn stop(&self) -> Result<(), NtStatus> {
        self.timer.cancel();
        Ok(())
    }
}

impl Drop for FlipTimer {
    fn drop(&mut self) {
        info!("{}: stopping flip timer", function!());
    }
}

#[repr(packed(4))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Immutable, IntoBytes, KnownLayout, FromBytes)]
struct BGRA {
    b: u8,
    r: u8,
    g: u8,
    a: u8,
}

impl BGRA {
    fn read_from_prefix_into_u32(slice: &[u8]) -> u32 {
        BGRA::read_from_prefix(slice).unwrap().0.into()
    }
}

impl From<u32> for BGRA {
    fn from(pix: u32) -> Self {
        let b = ((pix >> (8 * 0)) & 0xFF) as u8;
        let g = ((pix >> (8 * 1)) & 0xFF) as u8;
        let r = ((pix >> (8 * 2)) & 0xFF) as u8;
        let a = ((pix >> (8 * 3)) & 0xFF) as u8;

        Self { b, g, r, a}
    }
}

impl Into<u32> for BGRA {
    fn into(self) -> u32 {
        ((self.a as u32) << (8 * 3)) |
        ((self.r as u32) << (8 * 2)) |
        ((self.g as u32) << (8 * 1)) |
        ((self.b as u32) << (8 * 0))
    }
}

pub struct Cursor {
    pixels: Framebuffer,
    hotspot: (u32, u32),
    position: (u32, u32),
    hidden: bool,
}

impl Cursor {
    const BACKGROUND:  u32 = 0x00000000;
    const FOREGROUND:  u32 = 0x00FFFFFF;
    const TRANSPARENT: u32 = 0x00000000;
    const OPAQUE:      u32 = 0xFF000000;
    const INVERTED:    u32 = 0x80000000;

    pub fn try_new(chan: &GpuChannel, width: u32, height: u32, x_hot: u32, y_hot: u32, x: u32, y: u32) -> Result<Self, NtStatus> {
        // A8R8G8B8UNORM
        let pixels = Framebuffer::try_new(chan, width, height, commands::Format::B8G8R8A8UNORM)?;
        let hotspot = (x_hot, y_hot);
        let position = (x, y);
        let hidden = false;

        Ok(Self {
            pixels,
            hotspot,
            position,
            hidden,
        })
    }

    pub fn set_position(&mut self, x: u32, y: u32) {
        self.position = (x, y);
    }

    pub fn set_hotspot(&mut self, x_hot: u32, y_hot: u32) {
        self.hotspot = (x_hot, y_hot);
    }

    pub fn write_bgra(&mut self, src: &[u8], src_stride: usize) {
        let dst_stride = (self.pixels.width as usize) * self.pixels.format.stride();
        let rows = src.len() / src_stride;
        let pixels = self.pixels.pixels.as_mut();

        for row in 0..rows {
            pixels[row * dst_stride..row * dst_stride + src_stride].copy_from_slice(&src[row * src_stride..(row + 1) * src_stride]);
        }
    }

    pub fn write_mono(&mut self, mask: &[u8], mask_stride: usize) {
        let dst_w = self.pixels.width as usize;
        let w = mask_stride * 8;
        let h = mask.len() / (mask_stride * 2);
        let pixels = self.pixels.pixels.as_mut();
        pixels.fill(0);

        let and_mask = &mask[..mask.len() / 2];
        let xor_mask = &mask[mask.len() / 2..];

        //info!("{}: and: {:?}", function!(), and_mask);
        //info!("{}: xor: {:?}", function!(), xor_mask);
        //info!("{}: dst_w {}, w {}, h {}", function!(), dst_w, w, h);

        let mut has_inverted = false;

        for y in 0..h {
            let mut bit = 0x80u8;
            for x in 0..w {
                let byte_idx = y * mask_stride + x / 8;
                let and = (and_mask[byte_idx] & bit) != 0;
                let xor = (xor_mask[byte_idx] & bit) != 0;

                let pix = if and {
                    if xor {
                        has_inverted = true;
                        Self::INVERTED
                    } else {
                        Self::TRANSPARENT
                    }
                } else {
                    if xor {
                        Self::OPAQUE | Self::FOREGROUND
                    } else {
                        Self::OPAQUE | Self::BACKGROUND
                    }
                };
                BGRA::from(pix).write_to_prefix(&mut pixels[(y * dst_w + x) * 4..]).unwrap();

                bit >>= 1;
                if bit == 0 {
                    bit = 0x80;
                }
            }
        }

        if has_inverted {
            for y in 0..h {
                for x in 0..w {
                    let idx = y * dst_w + x;
                    if BGRA::read_from_prefix_into_u32(&pixels[idx * 4..]) != Self::TRANSPARENT {
                        continue;
                    }

                    let has_inverted_neighbor =
                        (x     > 0 && BGRA::read_from_prefix_into_u32(&pixels[(idx - 1    ) * 4..]) == Self::INVERTED) ||
                        (x + 1 < w && BGRA::read_from_prefix_into_u32(&pixels[(idx + 1    ) * 4..]) == Self::INVERTED) ||
                        (y > 0     && BGRA::read_from_prefix_into_u32(&pixels[(idx - dst_w) * 4..]) == Self::INVERTED) ||
                        (y + 1 < h && BGRA::read_from_prefix_into_u32(&pixels[(idx + dst_w) * 4..]) == Self::INVERTED);

                    if has_inverted_neighbor {
                        BGRA::from(Self::OPAQUE | Self::BACKGROUND).write_to_prefix(&mut pixels[idx * 4..]).unwrap();
                    }
                }
            }

            for y in 0..h {
                for x in 0..w {
                    let idx = y * dst_w + x;
                    if BGRA::read_from_prefix_into_u32(&pixels[idx * 4..]) == Self::INVERTED {
                        BGRA::from(Self::OPAQUE | Self::FOREGROUND).write_to_prefix(&mut pixels[idx * 4..]).unwrap();
                    }
                }
            }
        }

        //for y in 0..h {
        //    let row = &pixels[y * dst_w * 4..(y * dst_w + w) * 4];
        //    info!("{}: {:?}", y, row);
        //}
    }

    // TODO: this seems to work for grab/grabbing cursors, but is it correct for others?
    pub fn write_masked_color(&mut self, masked_color: &[u8], mask_stride: usize) {
        let dst_w = self.pixels.width as usize;
        let w = mask_stride / 4;
        let h = masked_color.len() / mask_stride;
        let pixels = self.pixels.pixels.as_mut();
        pixels.fill(0);

        let mut has_inverted = false;

        for y in 0..h {
            for x in 0..w {
                let b = masked_color[y * mask_stride + x * 4 + 0];
                let g = masked_color[y * mask_stride + x * 4 + 1];
                let r = masked_color[y * mask_stride + x * 4 + 2];
                let mask = masked_color[y * mask_stride + x * 4 + 3];

                let pix = if mask == 0 {
                    let bgr = ((r as u32) << (8 * 2)) | ((g as u32) << (8 * 1)) | ((b as u32) << (8 * 0));
                    Self::OPAQUE | bgr
                } else {
                    has_inverted = true;
                    Self::INVERTED
                };

                BGRA::from(pix).write_to_prefix(&mut pixels[(y * dst_w + x) * 4..]).unwrap();
            }
        }

        if has_inverted {
            for y in 0..h {
                for x in 0..w {
                    let idx = y * dst_w + x;
                    if BGRA::read_from_prefix_into_u32(&pixels[idx * 4..]) != Self::TRANSPARENT {
                        continue;
                    }
                    let idx = y * dst_w + x;
                    if BGRA::read_from_prefix_into_u32(&pixels[idx * 4..]) != Self::TRANSPARENT {
                        continue;
                    }

                    let has_inverted_neighbor =
                        (x     > 0 && BGRA::read_from_prefix_into_u32(&pixels[(idx - 1    ) * 4..]) == Self::INVERTED) ||
                        (x + 1 < w && BGRA::read_from_prefix_into_u32(&pixels[(idx + 1    ) * 4..]) == Self::INVERTED) ||
                        (y > 0     && BGRA::read_from_prefix_into_u32(&pixels[(idx - dst_w) * 4..]) == Self::INVERTED) ||
                        (y + 1 < h && BGRA::read_from_prefix_into_u32(&pixels[(idx + dst_w) * 4..]) == Self::INVERTED);

                    if has_inverted_neighbor {
                        BGRA::from(Self::OPAQUE | Self::BACKGROUND).write_to_prefix(&mut pixels[idx * 4..]).unwrap();
                    }
                }
            }

            for y in 0..h {
                for x in 0..w {
                    let idx = y * dst_w + x;
                    if BGRA::read_from_prefix_into_u32(&pixels[idx * 4..]) == Self::INVERTED {
                        BGRA::from(Self::TRANSPARENT).write_to_prefix(&mut pixels[idx * 4..]).unwrap();
                    }
                }
            }
        }
    }

    pub fn transfer_pixels(&self, chan: &GpuChannel) -> Result<(), NtStatus> {
        self.pixels.transfer_to_host(chan)
    }

    pub fn r#move(&self, chan: &GpuChannel, scanout: u32) -> Result<(), NtStatus> {
        let (x, y) = self.position;
        chan.move_cursor(scanout, x, y)
    }

    pub fn update(&self, chan: &GpuChannel, scanout: u32) -> Result<(), NtStatus> {
        let (x, y) = self.position;
        let (x_hot, y_hot) = self.hotspot;
        let res_id = self.pixels.id;
        chan.update_cursor(scanout, res_id, x_hot, y_hot, x, y)
    }
}

pub struct VidPnOutput {
    pub rect: commands::Rect,
    pub edid: Box<Edid>,
    pub current_mode: Option<usize>,
    pub modes: Vec<MonitorMode>,
    pub flipq: Arc<ArrayQueue<(Weak<Allocation>, u64)>>,
    pub cursor: SpinMutex<Option<Cursor>>,
    // pub framebuffer: Option<(NonZero<u32>, Box<[u8]>)>, // res_id, buf
}

impl VidPnOutput {
    pub fn new(info: commands::DisplayOne, edid: Box<Edid>) -> Self {
        let preferred = if info.rect.width < 640 || info.rect.height < 480 {
            edid.preferred_resolution().unwrap()
        } else {
            (info.rect.width, info.rect.height)
        };

        let modes = [MonitorMode { width: preferred.0, height: preferred.1 }]
            .into_iter()
            .chain(edid
                .standard_timings()
                .iter()
                .filter_map(|resolution|
                    if *resolution != preferred {
                        Some(MonitorMode { width: resolution.0, height: resolution.1 })
                    } else {
                        None
                    }
                )
            )
            .collect();

        let flipq = Arc::new(ArrayQueue::new(128));

        let current_mode = if info.enabled == 0 {
            None
        } else {
            Some(0)
        };

        let cursor = SpinMutex::new(None);

        Self {
            rect: info.rect,
            current_mode,
            edid,
            modes,
            flipq,
            cursor,
        }
    }

    pub fn hide_cursor(&self, scanout: u32, chan: &GpuChannel, empty: &Cursor) -> Result<(), NtStatus> {
        let mut cursor = self.cursor.lock();

        let ((x, y), (x_hot, y_hot)) = if let Some(cursor) = cursor.as_mut() {
            cursor.hidden = true;
            (cursor.position, cursor.hotspot)
        } else {
            (empty.position, empty.hotspot)
        };

        let res_id = empty.pixels.id;
        chan.update_cursor(scanout, res_id, x_hot, y_hot, x, y)
    }

    pub fn move_cursor(&self, scanout: u32, chan: &GpuChannel, x: i32, y: i32) -> Result<(), NtStatus> {
        let mut cursor = self.cursor.lock();

        let (w, h) = if let Some(current_mode) = self.current_mode {
            (self.modes[current_mode].width, self.modes[current_mode].height)
        } else {
            (self.modes[0].width, self.modes[0].height)
        };
        let x = x.clamp(0, w as i32) as u32;
        let y = y.clamp(0, h as i32) as u32;

        if let Some(cursor) = cursor.as_mut() {
            cursor.set_position(x, y);

            if cursor.hidden {
                cursor.hidden = false;
                cursor.update(chan, scanout)?;
            } else {
                cursor.r#move(chan, scanout)?;
            }
        } else {
            *cursor = Some(Cursor::try_new(chan, 64, 64, 0, 0, x, y)?);
        }

        Ok(())
    }

    pub fn update_cursor_bgra(&self, scanout: u32, chan: &GpuChannel, y_hot: u32, x_hot: u32, pixels: &[u8], stride: usize) -> Result<(), NtStatus> {
        let mut cursor = self.cursor.lock();

        if let Some(cursor) = cursor.as_mut() {
            cursor.set_hotspot(x_hot, y_hot);
        } else {
            *cursor = Some(Cursor::try_new(chan, 64, 64, y_hot, x_hot, 0, 0)?);
        }
        cursor.as_mut().unwrap().write_bgra(pixels, stride);
        cursor.as_ref().unwrap().transfer_pixels(chan)?;
        cursor.as_ref().unwrap().update(chan, scanout)
    }

    pub fn update_cursor_mono(&self, scanout: u32, chan: &GpuChannel, y_hot: u32, x_hot: u32, mask: &[u8], stride: usize) -> Result<(), NtStatus> {
        let mut cursor = self.cursor.lock();

        if let Some(cursor) = cursor.as_mut() {
            cursor.set_hotspot(x_hot, y_hot);
        } else {
            *cursor = Some(Cursor::try_new(chan, 64, 64, y_hot, x_hot, 0, 0)?);
        }
        cursor.as_mut().unwrap().write_mono(mask, stride);
        cursor.as_ref().unwrap().transfer_pixels(chan)?;
        cursor.as_ref().unwrap().update(chan, scanout)
    }

    pub fn update_cursor_masked_color(&self, scanout: u32, chan: &GpuChannel, y_hot: u32, x_hot: u32, masked_color: &[u8], stride: usize) -> Result<(), NtStatus> {
        let mut cursor = self.cursor.lock();

        if let Some(cursor) = cursor.as_mut() {
            cursor.set_hotspot(x_hot, y_hot);
        } else {
            *cursor = Some(Cursor::try_new(chan, 64, 64, y_hot, x_hot, 0, 0)?);
        }
        cursor.as_mut().unwrap().write_masked_color(masked_color, stride);
        cursor.as_ref().unwrap().transfer_pixels(chan)?;
        cursor.as_ref().unwrap().update(chan, scanout)
    }
}

impl fmt::Debug for VidPnOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("VidPnOutput")
            .field("rect", &self.rect)
            .field("modes", &self.modes)
            .field("current_mode", &self.current_mode.and_then(|m| Some(self.modes[m])))
            .field("flipq", &self.flipq)
            .finish()
    }
}

pub struct Framebuffer {
    pub width: u32,
    pub height: u32,
    pub format: commands::Format,
    pub pixels: AlignedBox<[u8]>,
    pub id: NonZero<u32>,
}

impl Framebuffer {
    pub fn try_new(chan: &GpuChannel, width: u32, height: u32, format: commands::Format) -> Result<Self, NtStatus> {
        let id = chan.resource_create_2d(width, height, format)?;

        let size = (width as usize) * (height as usize) * format.stride();

        let pixels = {
            let data = Box::<[u8], _>::try_new_zeroed_slice_in(size, AlignedAlloc)?;
            unsafe { data.assume_init() }
        };

        {
            let attach_pages = Command::attach_backing_dma_len(size.div_ceil(PAGE_SIZE as usize)).div_ceil(PAGE_SIZE as usize);
            let dma = Dma::<DxgkInterface>::new(attach_pages, BufferDirection::DriverToDevice, false)?;
            let dmabuf = unsafe { dma.raw_slice().as_mut() };
            let cmd = Command::attach_backing_box(chan, id, &pixels, dmabuf);
            chan.submit_command_sync(&cmd)?;
        }

        let framebuffer = Self {
            width,
            height,
            format,
            pixels,
            id,
        };

        framebuffer.transfer_to_host(chan)?;

        Ok(framebuffer)
    }

    // TODO: partial transfers
    pub fn transfer_to_host(&self, chan: &GpuChannel) -> Result<(), NtStatus> {
        let rect = commands::Rect {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        };

        chan.resource_transfer_to_host_2d(rect, 0, self.id)
    }

    pub fn destroy(self, chan: &GpuChannel) -> Result<(), NtStatus> {
        chan.resource_detach_backing(self.id)?;
        chan.resource_unref(self.id)?;
        Ok(())
    }
}
