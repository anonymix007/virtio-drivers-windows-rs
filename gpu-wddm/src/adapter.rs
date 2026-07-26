#![allow(non_upper_case_globals)]

use core::{
    ptr::{
        null_mut,
        drop_in_place,
        NonNull,
        addr_eq,
    },
    pin::Pin,
    marker::PhantomPinned,
    sync::{
        atomic::{
            AtomicU32,
            AtomicU64,
            AtomicPtr,
            AtomicBool,
            Ordering,
        },
    },
    mem::{
        transmute,
        zeroed,
        size_of,
        MaybeUninit,
        ManuallyDrop,
    },
    num::NonZero,
    convert::Infallible,
    cell::UnsafeCell,
};

use alloc::{
    boxed::Box,
    sync::{
        Arc,
        Weak,
    },
    vec::Vec,
    collections::TryReserveError,
};

use winresult::{STATUS};
use ida_rs::*;
use bitflags::*;
use pin_init::*;
use zerocopy::*;
use smallvec::*;
use spin::RwLock;
use hashbrown::*;

use tagged::*;
use wdk::{
    dxgkrnl::*,
    wdm::{
        KeEvent,
        MdlOwned,
        mm_map_locked_pages_specify_cache,
        mm_unmap_locked_pages,
    },
    *,
};
use virtio_drivers::{
    config::*,
    device::{
        gpu::*,
    },
    transport::{
        pci::{
            PciTransport,
            VirtioCapabilityInfo,
            bus::{
                BarInfo,
                ConfigurationAccess,
                DeviceFunction,
                PciRoot,
            },
        },
        DeviceType,
        Transport,
    },
    queue::*,
    BufferDirection,
    Hal,
    PhysAddr,
};

use crate::uapi::*;
use crate::queue::*;
use crate::init_option::*;
use crate::device::*;
use crate::command::*;
use crate::allocation::*;
use crate::vidpn::*;
use crate::process::*;
use crate::virgl::VirglResourceLayout;
use crate::{
    slice_from_raw_parts,
    slice_from_raw_parts_mut,
};

const PAGE_SIZE: usize = wdk::wdm::PAGE_SIZE as usize;

#[macro_export]
macro_rules! function {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            core::any::type_name::<T>()
        }
        let name = type_name_of(f);
        let prefix = concat!(env!("CARGO_PKG_NAME"), "::");

        if name.starts_with('<') {
            &name[..name.len() - "::f".len()]
        } else {
            &name[prefix.len()..name.len() - "::f".len()]
        }
    }};
}

#[repr(transparent)]
#[derive(PartialEq, Eq, Debug)]
pub struct NtStatus(pub winresult::NtStatus);

impl NtStatus {
    pub fn is_success(&self) -> bool {
        self.0.is_success()
    }

    pub fn to_u32(self) -> u32 {
        self.0.to_u32()
    }
}

impl From<Infallible> for NtStatus {
    fn from(e: Infallible) -> Self {
        match e {}
    }
}

impl From<hashbrown::TryReserveError> for NtStatus {
    fn from(_: hashbrown::TryReserveError) -> Self {
        Self(winresult::STATUS::NO_MEMORY)
    }
}

impl From<zerocopy::AllocError> for NtStatus {
    fn from(_: zerocopy::AllocError) -> Self {
        Self(winresult::STATUS::NO_MEMORY)
    }
}

impl From<core::alloc::AllocError> for NtStatus {
    fn from(_: core::alloc::AllocError) -> Self {
        Self(winresult::STATUS::NO_MEMORY)
    }
}

impl From<TryReserveError> for NtStatus {
    fn from(_: TryReserveError) -> Self {
        Self(winresult::STATUS::NO_MEMORY)
    }
}

impl<Src, Dst: Sized> From<zerocopy::error::SizeError<Src, Dst>> for NtStatus {
    fn from(_: zerocopy::error::SizeError<Src, Dst>) -> Self {
        Self(winresult::STATUS::BUFFER_TOO_SMALL)
    }
}

impl From<u32> for NtStatus {
    fn from(code: u32) -> Self {
        Self(winresult::NtStatus::from(code))
    }
}

impl From<winresult::NtStatus> for NtStatus {
    fn from(status: winresult::NtStatus) -> Self {
        Self(status)
    }
}

impl From<virtio_drivers::Error> for NtStatus {
    fn from(err: virtio_drivers::Error) -> Self {
        Self(match err {
            virtio_drivers::Error::QueueFull => STATUS::BUFFER_TOO_SMALL,
            virtio_drivers::Error::NotReady => STATUS::DEVICE_NOT_READY,
            virtio_drivers::Error::WrongToken => STATUS::INVALID_HANDLE,
            virtio_drivers::Error::AlreadyUsed => STATUS::ALREADY_COMMITTED,
            virtio_drivers::Error::InvalidParam => STATUS::INVALID_PARAMETER,
            virtio_drivers::Error::DmaError => STATUS::NO_MEMORY,
            virtio_drivers::Error::IoError => STATUS::IO_DEVICE_ERROR,
            virtio_drivers::Error::Unsupported => STATUS::INVALID_DEVICE_REQUEST,
            virtio_drivers::Error::ConfigSpaceTooSmall => STATUS::BUFFER_TOO_SMALL,
            virtio_drivers::Error::ConfigSpaceMissing => STATUS::DEVICE_CONFIGURATION_ERROR,
            virtio_drivers::Error::SocketDeviceError(_) => STATUS::UNEXPECTED_NETWORK_ERROR,
        })
    }
}

impl From<microseh::Exception> for NtStatus {
    fn from(e: microseh::Exception) -> Self {
        NtStatus(match e.code() {
            microseh::ExceptionCode::Invalid => STATUS::UNSUCCESSFUL,
            microseh::ExceptionCode::AccessViolation => STATUS::ACCESS_VIOLATION,
            microseh::ExceptionCode::ArrayBoundsExceeded => STATUS::ARRAY_BOUNDS_EXCEEDED,
            microseh::ExceptionCode::Breakpoint => STATUS::BREAKPOINT,
            microseh::ExceptionCode::DataTypeMisalignment => STATUS::DATATYPE_MISALIGNMENT,
            microseh::ExceptionCode::FltDenormalOperand => STATUS::FLOAT_DENORMAL_OPERAND,
            microseh::ExceptionCode::FltDivideByZero => STATUS::FLOAT_DIVIDE_BY_ZERO,
            microseh::ExceptionCode::FltInexactResult => STATUS::FLOAT_INEXACT_RESULT,
            microseh::ExceptionCode::FltInvalidOperation => STATUS::FLOAT_INVALID_OPERATION,
            microseh::ExceptionCode::FltOverflow => STATUS::FLOAT_OVERFLOW,
            microseh::ExceptionCode::FltStackCheck => STATUS::FLOAT_STACK_CHECK,
            microseh::ExceptionCode::FltUnderflow => STATUS::FLOAT_UNDERFLOW,
            microseh::ExceptionCode::GuardPage => STATUS::GUARD_PAGE_VIOLATION,
            microseh::ExceptionCode::IllegalInstruction => STATUS::ILLEGAL_INSTRUCTION,
            microseh::ExceptionCode::InPageError => STATUS::IN_PAGE_ERROR,
            microseh::ExceptionCode::IntDivideByZero => STATUS::INTEGER_DIVIDE_BY_ZERO,
            microseh::ExceptionCode::IntOverflow => STATUS::INTEGER_OVERFLOW,
            microseh::ExceptionCode::InvalidDisposition => STATUS::INVALID_DISPOSITION,
            microseh::ExceptionCode::InvalidHandle => STATUS::INVALID_HANDLE,
            microseh::ExceptionCode::NonContinuableException => STATUS::NONCONTINUABLE_EXCEPTION,
            microseh::ExceptionCode::PrivilegedInstruction => STATUS::PRIVILEGED_INSTRUCTION,
            microseh::ExceptionCode::SingleStep => STATUS::SINGLE_STEP,
            microseh::ExceptionCode::StackOverflow => STATUS::STACK_OVERFLOW,
            microseh::ExceptionCode::UnwindConsolidate => STATUS::UNWIND_CONSOLIDATE,
        })
    }
}


#[macro_export]
macro_rules! check_buffer_size {
    ($ptr:expr, $size:expr, $ty:ty) => {{
        let size = $size as usize;
        let required = core::mem::size_of::<$ty>();

        if size < required {
            error!("{}: output buffer too small for {}: {} < {}", function!(), core::any::type_name::<$ty>(), size, required);
            Err(NtStatus(STATUS::BUFFER_TOO_SMALL))
        } else if $ptr.is_null() {
            error!("{}: buffer for {} is null", function!(), core::any::type_name::<$ty>());
            Err(NtStatus(STATUS::INVALID_PARAMETER))
        } else {
            Ok(unsafe { &mut *($ptr as *mut $ty) })
        }
    }};
}

#[macro_export]
macro_rules! map_virtio_error {
    ($expr:expr) => {
        $expr.map_err(|e| {
            error!("failed to call {}: {:?}", stringify!($expr), e);
            NtStatus::from(e)
        })
    };
}

#[macro_export]
macro_rules! map_virtio_pci_error {
    ($expr:expr) => {
        $expr.map_err(|e| {
            error!("failed to call {}: {:?}", stringify!($expr), e);
            NtStatus(STATUS::DEVICE_CONFIGURATION_ERROR)
        })
    };
}

#[macro_export]
macro_rules! dxgk_call_unchecked {
    ($self:ident . $func:ident ( $($args:expr),* )) => {{
        //trace!("{}: calling {}", function!(), stringify!($func));
        if let Some(func) = unsafe { (*$self.interface.get()).$func } {
            unsafe {
                func((*$self.interface.get()).DeviceHandle, $($args),*)
            }
        } else {
            error!(concat!("func ", stringify!($func), " is none"));
            Default::default()
        }
    }};
}

#[macro_export]
macro_rules! dxgk_call {
    ($op:tt $irql:ident | $self:ident . $func:ident ( $($args:expr),* )) => {{
        assert_irql!($op $irql);
        dxgk_call_unchecked!($self.$func($($args),*))
    }};
}

#[macro_export]
macro_rules! dxgk_call_status {
    ($op:tt $irql:ident | $self:ident . $func:ident ( $($args:expr),* )) => {{
        //trace!("{}: calling {}", function!(), stringify!($func));
        if let Some(func) = unsafe { (*$self.interface.get()).$func } {
            let result = wdm_call_status!($op $irql | func((*$self.interface.get()).DeviceHandle, $($args),*));
            result.map_err(|e| NtStatus(e))
        } else {
            error!(concat!("func ", stringify!($func), " is none"));
            Err(NtStatus(STATUS::INVALID_PARAMETER))
        }
    }};
}

#[macro_export]
macro_rules! dxgk_call_no_handle_unchecked {
    ($self:ident . $func:ident ( $($args:expr),* )) => {{
        if let Some(func) = unsafe { (*$self.interface.get()).$func } {
            unsafe {
                func($($args),*)
            }
        } else {
            error!(concat!("func ", stringify!($func), " is none"));
            Default::default()
        }
    }};
}

#[macro_export]
macro_rules! dxgk_call_no_handle {
    ($op:tt $irql:ident | $self:ident . $func:ident ( $($args:expr),* )) => {{
        assert_irql!($op $irql);
        dxgk_call_no_handle_unchecked!($self.$func($($args),*))
    }};
}

#[macro_export]
macro_rules! dxgk_call_status_no_handle {
    ($op:tt $irql:ident | $self:ident . $func:ident ( $($args:expr),* )) => {{
        if let Some(func) = unsafe { (*$self.interface.get()).$func } {
            let result = wdm_call_status!($op $irql | func($($args),*));
            result.map_err(|e| NtStatus(e))
        } else {
            error!(concat!("func ", stringify!($func), " is none"));
            Err(NtStatus(STATUS::INVALID_PARAMETER))
        }
    }};
}

macro_rules! check_state {
    ($self:ident) => {{
        if let Some(state) = $self.state.as_ref() {
            Ok(state)
        } else {
            error!("{}: adapter was not yet started", function!());
            Err(NtStatus(STATUS::UNSUCCESSFUL))
        }
    }}
}
macro_rules! check_state_mut {
    ($self:ident) => {{
        if let Some(state) = $self.state.as_mut() {
            Ok(state)
        } else {
            error!("{}: adapter was not yet started", function!());
            Err(NtStatus(STATUS::UNSUCCESSFUL))
        }
    }}
}

const SUPPORTED_FEATURES: Features = Features::RING_EVENT_IDX
    .union(Features::RING_INDIRECT_DESC)
    .union(Features::VERSION_1)
    .union(Features::ACCESS_PLATFORM)
    .union(Features::EDID)
    .union(Features::VIRGL)
    .union(Features::RESOURCE_BLOB)
    .union(Features::CONTEXT_INIT);

//#[derive(Clone)]
pub struct DxgkInterface {
    interface: UnsafeCell<DXGKRNL_INTERFACE>,
    device_function: DeviceFunction,
    pci_bars: [Option<(BarInfo, Option<NonNull<u8>>)>; 6],
    unsafe_copy: bool,
}

impl DxgkInterface {
    pub fn get(&self) -> *mut DXGKRNL_INTERFACE {
        self.interface.get()
    }

    fn get_device_info(&self) -> Result<DXGK_DEVICE_INFO, NtStatus> {
        let mut device_info: DXGK_DEVICE_INFO = unsafe { zeroed() };
        dxgk_call_status!(<= PASSIVE_LEVEL | self.DxgkCbGetDeviceInformation(&mut device_info))?;
        Ok(device_info)
    }

    fn read_device_space(&self, whichspace: u32, offset: u32, data: &mut [u8]) -> Result<u32, NtStatus> {
        let mut bytes_read: u32 = 0;
        dxgk_call_status!(<= PASSIVE_LEVEL | self.DxgkCbReadDeviceSpace(whichspace, data.as_mut_ptr() as _, offset, data.len() as _, &mut bytes_read))?;
        Ok(bytes_read)
    }

    fn write_device_space(&self, whichspace: u32, offset: u32, data: &mut [u8]) -> Result<u32, NtStatus> {
        let mut bytes_written: u32 = 0;
        dxgk_call_status!(<= PASSIVE_LEVEL | self.DxgkCbWriteDeviceSpace(whichspace, data.as_mut_ptr() as _, offset, data.len() as _, &mut bytes_written))?;
        Ok(bytes_written)
    }

    fn read_pci_common_header(&self) -> Result<PCI_COMMON_HEADER, NtStatus> {
        let mut pci_common: PCI_COMMON_HEADER = unsafe { zeroed() };
        let data = slice_from_raw_parts_mut(&mut pci_common as *mut PCI_COMMON_HEADER as *mut u8, size_of::<PCI_COMMON_HEADER>());

        let _ = self.read_device_space(DXGK_WHICHSPACE_CONFIG, 0, data)?;

        Ok(pci_common)
    }

    pub fn map_memory(&self, paddr: u64, size: u32, io: bool, user: bool, caching: MEMORY_CACHING_TYPE) -> Result<NonNull<u8>, NtStatus> {
        let mut vaddr = null_mut();
        dxgk_call_status!(<= PASSIVE_LEVEL | self.DxgkCbMapMemory(LARGE_INTEGER { QuadPart: paddr as _ }, size, io as _, user as _, caching, &mut vaddr as *mut _ as _))?;
        let Some(vaddr) = NonNull::new(vaddr) else {
            error!("failed to map memory for unknown reason");
            return Err(NtStatus(STATUS::UNSUCCESSFUL));
        };
        Ok(vaddr)
    }

    pub fn unmap_memory(&self, vaddr: NonNull<u8>) -> Result<(), NtStatus> {
        dxgk_call_status!(<= PASSIVE_LEVEL | self.DxgkCbUnmapMemory(vaddr.as_ptr() as _))?;
        Ok(())
    }

    fn queue_dpc(&self) -> bool {
        dxgk_call!(<= PROFILE_LEVEL | self.DxgkCbQueueDpc()) != 0
    }

    fn find_bar(&self, addr: u64) -> Option<(u8, u64)> {
        for b in 0..self.pci_bars.len() {
            let Some((BarInfo::Memory { address, size, .. }, _)) = self.pci_bars[b] else {
                continue;
            };
            if (address..address + size).contains(&addr) {
                let offset = addr - address;
                return Some((b as _, offset));
            }
        }
        None
    }

    pub fn get_physical_bar_address(&self, bar: u8) -> Option<PhysAddr> {
        let bar = bar as usize;
        if bar < self.pci_bars.len() {
            if let Some((BarInfo::Memory { address, .. }, _)) = self.pci_bars[bar] {
                return Some(address);
            } else if let Some((BarInfo::IO { .. }, _)) = self.pci_bars[bar] {
                error!("TODO: physical address for I/O bars");
                return None;
            }
        }

        error!("invalid bar {}", bar);
        None
    }

    fn get_mapped_bar_address(&self, bar: u8) -> Option<(NonNull<u8>, usize)> {
        let bar = bar as usize;
        if bar < self.pci_bars.len() {
            if let Some((BarInfo::Memory { address, size, .. }, addr)) = self.pci_bars[bar] {
                return if let Some(addr) = addr {
                    Some((addr, size as _))
                } else {
                    let size: u32 = size.try_into().unwrap();
                    let vaddr = self.map_memory(address, size, false, false, MEMORY_CACHING_TYPE::MmNonCached).map_err(|e| {
                        error!("failed to map memory: {:?}", e);
                        e
                    }).ok()?;
                    self.pci_bars[bar].unwrap().1 = Some(vaddr);

                    Some((vaddr, size as _))
                };
            } else if let Some((BarInfo::IO { .. }, _)) = self.pci_bars[bar] {
                error!("TODO: map I/O bars");
                return None;
            }
        }

        error!("invalid bar {}", bar);
        None
    }

    fn notify_dpc(&self) {
        dxgk_call!(== DISPATCH_LEVEL | self.DxgkCbNotifyDpc());
    }

    fn notify_interrupt(&self, mut data: DXGKARGCB_NOTIFY_INTERRUPT_DATA) {
        dxgk_call!(> DISPATCH_LEVEL | self.DxgkCbNotifyInterrupt(&mut data as _));
    }

    fn synchronize_execution(&self, callback: extern "C" fn(PVOID) -> BOOLEAN, context: PVOID, message: u32) -> Result<bool, NtStatus> {
        let mut result: BOOLEAN = false as _;
        dxgk_call_status!(<= PROFILE_LEVEL | self.DxgkCbSynchronizeExecution(Some(callback), context, message, &mut result as _))?;
        Ok(result != 0)
    }

    pub fn notify_interrupt_synchronized(&self, interrupt: Interrupt) -> Result<bool, NtStatus> {
        //trace!("{}: ", function!());

        struct NotifyContext<'a> {
            interface: &'a DxgkInterface,
            interrupt: Interrupt,
        }

        extern "C" fn synchronize_routine(context: PVOID) -> BOOLEAN {
            let context = unsafe {
                transmute::<_, &NotifyContext>(context)
            };

            /*
            unsafe {
                use core::arch::asm;
                asm!("wbinvd", options(nostack, nomem));
            }
            */

            match context.interrupt {
                Interrupt::DmaCompleted(engine, fence) => {
                    //trace!("{}: fence {}", function!(), fence);

                    let mut interrupt: DXGKARGCB_NOTIFY_INTERRUPT_DATA = unsafe { zeroed() };
                    interrupt.InterruptType = DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_DMA_COMPLETED;

                    let dma_completed = unsafe { interrupt.__bindgen_anon_1.DmaCompleted.as_mut() };
                    dma_completed.SubmissionFenceId = fence;
                    dma_completed.NodeOrdinal = engine.node_ordinal();
                    dma_completed.EngineOrdinal = 0;

                    context.interface.notify_interrupt(interrupt);

                    context.interface.queue_dpc();
                    true as _
                },
                Interrupt::DmaPreempted(engine, preemption_fence, last_completed_fence) => {
                    //debug!("{}: preemption fence {}, last completed {}", function!(), preemption_fence, last_completed_fence);

                    let mut interrupt: DXGKARGCB_NOTIFY_INTERRUPT_DATA = unsafe { zeroed() };
                    interrupt.InterruptType = DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_DMA_PREEMPTED;

                    let dma_preempted = unsafe { interrupt.__bindgen_anon_1.DmaPreempted.as_mut() };
                    dma_preempted.PreemptionFenceId = preemption_fence;
                    dma_preempted.LastCompletedFenceId = last_completed_fence;
                    dma_preempted.NodeOrdinal = engine.node_ordinal();
                    dma_preempted.EngineOrdinal = 0;

                    context.interface.notify_interrupt(interrupt);

                    context.interface.queue_dpc();
                    true as _
                },
                Interrupt::DmaFaulted(engine, fence) => {
                    error!("{}: engine {:?}, faulted fence {}", function!(), engine, fence);

                    let mut interrupt: DXGKARGCB_NOTIFY_INTERRUPT_DATA = unsafe { zeroed() };
                    interrupt.InterruptType = DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_DMA_PAGE_FAULTED;

                    let dma_faulted = unsafe { interrupt.__bindgen_anon_1.DmaPageFaulted.as_mut() };
                    dma_faulted.FaultedFenceId = fence;
                    dma_faulted.FaultedPrimitiveAPISequenceNumber = u64::MAX;
                    dma_faulted.FaultedPipelineStage = DXGK_RENDER_PIPELINE_STAGE::DXGK_RENDER_PIPELINE_STAGE_UNKNOWN;
                    dma_faulted.FaultedBindTableEntry = DXGK_BIND_TABLE_ENTRY_UNKNOWN;
                    dma_faulted.PageFaultFlags = DXGK_PAGE_FAULT_FLAGS::DXGK_PAGE_FAULT_ENGINE_RESET_REQUIRED;
                    //dma_faulted.PageFaultFlags = DXGK_PAGE_FAULT_FLAGS::DXGK_PAGE_FAULT_ADAPTER_RESET_REQUIRED;
                    dma_faulted.NodeOrdinal = engine.node_ordinal();
                    dma_faulted.EngineOrdinal = 0;
                    //dma_faulted.FaultErrorCode.set_IsDeviceSpecificCode(true); // timeout
                    dma_faulted.FaultErrorCode.set_IsDeviceSpecificCode(false);
                    dma_faulted.FaultErrorCode.set_GeneralErrorCode(DXGK_GENERAL_ERROR_CODE::DXGK_GENERAL_ERROR_INVALID_INSTRUCTION);

                    context.interface.notify_interrupt(interrupt);

                    context.interface.queue_dpc();
                    true as _
                },
                Interrupt::VSync(addrs) => {
                    for (i, addr) in addrs.iter().enumerate() {
                        let Some(addr) = addr else {
                            continue;
                        };

                        //trace!("{}: vsync {} for addr {:x}", function!(), i, unsafe { addr.QuadPart as u64 });

                        let mut interrupt: DXGKARGCB_NOTIFY_INTERRUPT_DATA = unsafe { zeroed() };
                        interrupt.InterruptType = DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_CRTC_VSYNC;

                        let vsync = unsafe { interrupt.__bindgen_anon_1.CrtcVsync.as_mut() };
                        vsync.VidPnTargetId = i as u32;
                        vsync.PhysicalAddress = *addr;

                        context.interface.notify_interrupt(interrupt);
                    }

                    context.interface.queue_dpc();

                    true as _
                }
            }
        }

        /*
        if matches!(interrupt, Interrupt::DmaCompleted(..)) {
            info!("{}: {:?}", function!(), interrupt);
        }

        if matches!(interrupt, Interrupt::VSync(..)) {
            info!("{}: {:?}", function!(), interrupt);
        }
        */

        let mut context = NotifyContext {
            interface: self,
            interrupt,
        };
        self.synchronize_execution(synchronize_routine, &mut context as *mut _ as _, 0)
    }

    pub fn device_allocation_from_handle(&self, handle: D3DKMT_HANDLE) -> Option<Arc<DeviceSpecificAllocation>> {
        let mut handle_data = DXGKARGCB_GETHANDLEDATA {
            hObject: handle,
            Type: DXGK_HANDLE_TYPE::DXGK_HANDLE_ALLOCATION,
            Flags: unsafe { zeroed() },
        };
        handle_data.Flags.set_DeviceSpecific(true);

        let alloc = dxgk_call_no_handle!(< DISPATCH_LEVEL | self.DxgkCbGetHandleData(&handle_data as _));

        if alloc.is_null() {
            let mut release: DXGKARG_RELEASE_HANDLE = null_mut();
            let alloc = dxgk_call_no_handle!(<= APC_LEVEL | self.DxgkCbAcquireHandleData(&handle_data as _, &mut release as _));
            if alloc.is_null() {
                error!("{}: DxgkCb{{Get,Acquire}}HandleData returned NULL for device-specific alloc handle {:X?}. Is it NOT device-specific?", function!(), handle);
                None
            } else {
                if !release.is_null() {
                    let release_data = DXGKARGCB_RELEASEHANDLEDATA {
                        ReleaseHandle: release,
                        Type: DXGK_HANDLE_TYPE::DXGK_HANDLE_ALLOCATION,
                    };
                    dxgk_call_no_handle!(<= APC_LEVEL | self.DxgkCbReleaseHandleData(release_data));
                }

                TaggedExt::from_arc_handle_clone(alloc)
            }
        } else {
            TaggedExt::from_arc_handle_clone(alloc)
        }
    }

    pub fn allocation_from_handle(&self, handle: D3DKMT_HANDLE) -> Option<Arc<Allocation>> {
        let handle_data = DXGKARGCB_GETHANDLEDATA {
            hObject: handle,
            Type: DXGK_HANDLE_TYPE::DXGK_HANDLE_ALLOCATION,
            Flags: unsafe { zeroed() },
        };

        let alloc = dxgk_call_no_handle!(< DISPATCH_LEVEL | self.DxgkCbGetHandleData(&handle_data as _));

        if alloc.is_null() {
            let mut release: DXGKARG_RELEASE_HANDLE = null_mut();
            let alloc = dxgk_call_no_handle!(<= APC_LEVEL | self.DxgkCbAcquireHandleData(&handle_data as _, &mut release as _));
            if alloc.is_null() {
                error!("{}: DxgkCb{{Get,Acquire}}HandleData returned NULL for alloc handle {:X?}. Is it device-specific alloc?", function!(), handle);
                None
            } else {
                if !release.is_null() {
                    let release_data = DXGKARGCB_RELEASEHANDLEDATA {
                        ReleaseHandle: release,
                        Type: DXGK_HANDLE_TYPE::DXGK_HANDLE_ALLOCATION,
                    };
                    dxgk_call_no_handle!(<= APC_LEVEL | self.DxgkCbReleaseHandleData(release_data));
                }

                TaggedExt::from_arc_handle_clone(alloc)
            }
        } else {
            TaggedExt::from_arc_handle_clone(alloc)
        }
    }

    /*
    pub fn resource_from_handle(&self, handle: D3DKMT_HANDLE) -> Option<Arc<Resource>> {
        let handle_data = DXGKARGCB_GETHANDLEDATA {
            hObject: handle,
            Type: DXGK_HANDLE_TYPE::DXGK_HANDLE_RESOURCE,
            Flags: unsafe { zeroed() },
        };

        let alloc = dxgk_call_no_handle!(< DISPATCH_LEVEL | self.DxgkCbGetHandleData(&handle_data as _));

        if alloc.is_null() {
            let mut release: DXGKARG_RELEASE_HANDLE = null_mut();
            let alloc = dxgk_call_no_handle!(<= APC_LEVEL | self.DxgkCbAcquireHandleData(&handle_data as _, &mut release as _));
            if alloc.is_null() {
                error!("{}: DxgkCb{{Get,Acquire}}HandleData returned NULL for resource handle {:X?}", function!(), handle);
                None
            } else {
                if !release.is_null() {
                    let release_data = DXGKARGCB_RELEASEHANDLEDATA {
                        ReleaseHandle: release,
                        Type: DXGK_HANDLE_TYPE::DXGK_HANDLE_RESOURCE,
                    };
                    dxgk_call_no_handle!(<= APC_LEVEL | self.DxgkCbReleaseHandleData(release_data));
                }

                TaggedExt::from_arc_handle_clone(alloc)
            }
        } else {
            TaggedExt::from_arc_handle_clone(alloc)
        }
    }
    */

    pub fn query_vidpn_interface(&self, handle: D3DKMDT_HVIDPN) -> Result<VidPnInterface, NtStatus> {
        trace!("{}", function!());
        let mut interface: *const DXGK_VIDPN_INTERFACE = null_mut();
        dxgk_call_status_no_handle!(<= APC_LEVEL | self.DxgkCbQueryVidPnInterface(handle, DXGK_VIDPN_INTERFACE_VERSION::DXGK_VIDPN_INTERFACE_VERSION_V1, &mut interface as *mut _))?;
        Ok(VidPnInterface::new(handle, interface))
    }

    pub fn acquire_post_display_ownership(&self) -> Result<DXGK_DISPLAY_INFORMATION, NtStatus> {
        trace!("{}", function!());
        let mut display_info = unsafe { zeroed() };
        dxgk_call_status!(<= APC_LEVEL | self.DxgkCbAcquirePostDisplayOwnership(&mut display_info as *mut _))?;
        Ok(display_info)
    }
}

impl Drop for DxgkInterface {
    fn drop(&mut self) {
        trace!("{}", function!());
        if self.unsafe_copy {
            warn!("{}: dropping unsafe copy of dxgk interface", function!());
            return;
        }
        for bar in self.pci_bars {
            if let Some((BarInfo::Memory { .. }, addr)) = bar {
                if let Some(addr) = addr {
                    let _ = self.unmap_memory(addr).map_err(|e| {
                        error!("failed to unmap memory: {:?}", e);
                        e
                    });
                }
            } else if let Some((BarInfo::IO { .. }, addr)) = bar {
                if let Some(_) = addr {
                    error!("TODO: unmap I/O bars");
                }
            }
        }
    }
}

unsafe impl Hal for DxgkInterface {
    fn dma_alloc(pages: usize, _direction: BufferDirection, _access_platform: bool) -> (PhysAddr, NonNull<u8>) {
        //trace!("alloc DMA: pages={}", pages);

        let highest_acceptable = LARGE_INTEGER { QuadPart: 0xFFFFFFFFFF };
        let size = PAGE_SIZE * pages;

        assert_irql!(<= DISPATCH_LEVEL);
        let Some(vaddr) = NonNull::new(unsafe { MmAllocateContiguousMemory(size as _, highest_acceptable) as *mut u8 }) else {
            error!("cannot allocate contiguous memory");
            return (0, NonNull::dangling());
        };

        unsafe { vaddr.write_bytes(0, size); }

        let paddr = unsafe { MmGetPhysicalAddress(vaddr.as_ptr() as _).QuadPart as u64 };
        //debug!("phys {:x}, virt {:x}", paddr, vaddr.addr());

        (paddr, vaddr)
    }

    unsafe fn dma_dealloc(paddr: PhysAddr, vaddr: NonNull<u8>, pages: usize, _access_platform: bool) -> i32 {
        //trace!("dealloc DMA: paddr={:#x}, pages={}", paddr, pages);
        assert_irql!(<= DISPATCH_LEVEL);
        unsafe { MmFreeContiguousMemory(vaddr.as_ptr() as _); }
        0
    }

    unsafe fn mmio_phys_to_virt(&self, paddr: PhysAddr, size: usize) -> NonNull<u8> {
        let phys = PHYSICAL_ADDRESS { QuadPart: paddr as _ };

        if let Some((bar, offset)) = self.find_bar(paddr) {
            let Some((vaddr, bar_size)) = self.get_mapped_bar_address(bar) else {
                error!("failed to get mapped bar address");
                return unsafe { NonNull::new_unchecked(1 as *mut u8) };
            };
            if offset as usize + size > bar_size {
                error!("offset {} with size {} is outside of bar {} size {}", offset, size, bar, bar_size);
                return unsafe { NonNull::new_unchecked(1 as *mut u8) };
            }

            debug!("addr {:x} is in bar {} at offset {}", paddr, bar, offset);

            unsafe { vaddr.offset(offset as _) }
        } else if let Some(vaddr) = NonNull::new(unsafe { MmGetVirtualForPhysical(phys) } as _) {
            debug!("phys addr {:x} is at virt: {:?}", paddr, vaddr);
            vaddr
        } else {
            error!("failed get virtual address for physical {:x?}", paddr);
            unsafe { NonNull::new_unchecked(1 as *mut u8) }
        }

        /*if let Some(vaddr) = NonNull::new(unsafe { MmGetVirtualForPhysical(phys) } as _) {
            debug!("phys addr {:x} is at virt: {:?}", paddr, vaddr);
            vaddr
        } else {
            let Some((bar, offset)) = self.find_bar(paddr) else {
                panic!("cannot convert arbitrary (non-bar) physical memory to virtual: {:x?}", paddr);
                //return unsafe { NonNull::new_unchecked(1 as *mut u8) };
            };
            let Some((vaddr, bar_size)) = self.get_mapped_bar_address(bar) else {
                panic!("failed to get mapped bar address");
                //return unsafe { NonNull::new_unchecked(1 as *mut u8) };
            };
            if offset as usize + size > bar_size {
                panic!("offset {} with size {} is outside of bar {} size {}", offset, size, bar, bar_size);
                //return unsafe { NonNull::new_unchecked(1 as *mut u8) };
            }

            debug!("addr {:x} is in bar {} at offset {}", paddr, bar, offset);

            return unsafe { vaddr.offset(offset as _) };
        }*/
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection, _access_platform: bool) -> PhysAddr {
        // Nothing to do, as the host already has access to all memory, so just get the physical address
        unsafe { MmGetPhysicalAddress(buffer.as_ptr() as _).QuadPart as u64 }
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection, _access_platform: bool) {
        //trace!("{}", function!());
        // Nothing to do, as the host already has access to all memory and we didn't copy the buffer
        // anywhere else.
    }
}

impl ConfigurationAccess for DxgkInterface {
    fn read_word(&self, device_function: DeviceFunction, register_offset: u8) -> u32 {
        if self.device_function != device_function {
            error!("attempted to access invalid device: {}", device_function);
            return 0xffffffff;
        }

        let mut word: u32 = 0;
        let data = slice_from_raw_parts_mut(&mut word as *mut u32 as _, size_of::<u32>());
        let _ = self.read_device_space(DXGK_WHICHSPACE_CONFIG, register_offset as u32, data).unwrap();

        word
    }

    fn write_word(&mut self, device_function: DeviceFunction, register_offset: u8, data: u32) {
        if self.device_function != device_function {
            error!("attempted to access invalid device: {}", device_function);
            return;
        }
        let mut word: u32 = data;
        let data = slice_from_raw_parts_mut(&mut word as *mut u32 as _, size_of::<u32>());
        let _ = self.write_device_space(DXGK_WHICHSPACE_CONFIG, register_offset as u32, data).unwrap();
    }

    unsafe fn unsafe_clone(&self) -> Self {
        Self {
            interface: UnsafeCell::new(unsafe { (*self.interface.get()).clone() }),
            device_function: self.device_function.clone(),
            pci_bars: self.pci_bars.clone(),
            unsafe_copy: true,
        }
    }
}

struct AdapterState {
    negotiated_features: Features,
    luid: u64,
    num_scanouts: u8,
    is_vga: bool,
    is_msi: bool,
    supported_capsets: CapsetMask,
    capset_info: [(CapsetInfo, RwLock<Option<Box<[u8]>>>); 64],

    max_simultaneously_running_commands: [AtomicU32; Engine::TOTAL_COUNT as usize],

    queue_handler: QueueHandler,

    system_display_info: Option<DXGK_DISPLAY_INFORMATION>,
    empty_cursor: Option<Cursor>,
    outputs: [Option<VidPnOutput>; 16],
    source_target_map: [SmallVec<[D3DDDI_VIDEO_PRESENT_TARGET_ID; 16]>; 16]
}

const VIRTIO_GPU_ADAPTER_TAG: u64 = u64::from_ne_bytes(*b"VGPUADAP");

#[repr(C)]
#[derive(Tagged)]
#[tagged(VIRTIO_GPU_ADAPTER_TAG)]
pub struct Adapter {
    pub tag: u64,

    device: NonNull<UnsafeCell<DEVICE_OBJECT>>,
    state: InitOption<AdapterState>,
    flip_timer: Option<FlipTimer>,
}

pub const PAGING_DMA_BUFFER_SIZE: u32 = 128 * 1024;

/*
pub const SEGMENT_ID_3D:            u8 = 1;
pub const SEGMENT_ID_BLOB_MAPPABLE: u8 = 2;
pub const SEGMENT_ID_BLOB_HOST3D:   u8 = 3;

/* 3d resources / guest blobs */
pub const APERTURE_SEGMENT_GPU_PADDR:   u64 = 0x00C0000000;
/* mappable blob resources */
pub const BLOB_MAP_SEGMENT_GPU_PADDR:   u64 = 0x0700000000;
/* not mappable blob resources*/
pub const BLOB_SHARE_SEGMENT_GPU_PADDR: u64 = 0x2000000000;
*/

#[derive(Debug, Clone, Copy)]
pub enum Engine {
    Graphics,
    PhysicalOther,
    Copy,
    Other(u8),
    // TODO: do we need video engine?
}

impl Engine {
    const GRAPHICS_ENGINE:    u32 = 0;
    const PHYS_OTHER_ENGINE:  u32 = 1;
    const COPY_ENGINE:        u32 = 2;
    const OTHER_ENGINE_START: u32 = 3;
    const OTHER_ENGINE_COUNT: u32 = 61;
    const OTHER_ENGINE_END:   u32 = Engine::OTHER_ENGINE_START + Engine::OTHER_ENGINE_COUNT - 1;

    pub const TOTAL_COUNT: u32 = Engine::OTHER_ENGINE_START + Engine::OTHER_ENGINE_COUNT;

    pub const fn try_from_node_ordinal(index: u32) -> Option<Self> {
        match index {
            Engine::GRAPHICS_ENGINE => Some(Engine::Graphics),
            Engine::COPY_ENGINE => Some(Engine::Copy),
            Engine::PHYS_OTHER_ENGINE => Some(Engine::PhysicalOther),
            Engine::OTHER_ENGINE_START..=Engine::OTHER_ENGINE_END => Some(Engine::Other((index - Engine::OTHER_ENGINE_START) as _)),
            _ => None,
        }
    }

    pub const fn node_ordinal(&self) -> u32 {
        match self {
            Engine::Graphics => Engine::GRAPHICS_ENGINE,
            Engine::Copy => Engine::COPY_ENGINE,
            Engine::PhysicalOther => Engine::PHYS_OTHER_ENGINE,
            Engine::Other(i) => {
                let i = *i as u32;
                assert!(i < Engine::OTHER_ENGINE_COUNT);
                Engine::OTHER_ENGINE_START + i
            },
        }
    }

    pub fn fill_metadata(&self, metadata: &mut DXGK_NODEMETADATA) {
        match self {
            Engine::Graphics => {
                debug!("{}: {:?} -> DXGK_ENGINE_TYPE_3D", function!(), self);
                metadata.EngineType = DXGK_ENGINE_TYPE::DXGK_ENGINE_TYPE_3D;
            },
            Engine::Copy => {
                debug!("{}: {:?} -> DXGK_ENGINE_TYPE_COPY", function!(), self);
                metadata.EngineType = DXGK_ENGINE_TYPE::DXGK_ENGINE_TYPE_COPY;
                metadata.GpuMmuSupported = true as _;
            },
            Engine::PhysicalOther => {
                debug!("{}: {:?} -> DXGK_ENGINE_TYPE_OTHER (phys)", function!(), self);
                metadata.EngineType = DXGK_ENGINE_TYPE::DXGK_ENGINE_TYPE_OTHER;
            },
            Engine::Other(_) => {
                debug!("{}: {:?} -> DXGK_ENGINE_TYPE_OTHER", function!(), self);
                metadata.EngineType = DXGK_ENGINE_TYPE::DXGK_ENGINE_TYPE_OTHER;
                metadata.GpuMmuSupported = true as _;
                //metadata.Flags.set_ContextSchedulingSupported(true);
            },
        }
    }
    /*
    pub const fn dxgk_type(&self) -> DXGK_ENGINE_TYPE {
        match self {
            Engine::Graphics => DXGK_ENGINE_TYPE::DXGK_ENGINE_TYPE_3D,
            Engine::Other(_) => DXGK_ENGINE_TYPE::DXGK_ENGINE_TYPE_OTHER,
        }
    }
    */
}

#[derive(Debug)]
pub enum Interrupt {
    DmaCompleted(Engine, u32),
    DmaPreempted(Engine, u32, u32),
    DmaFaulted(Engine, u32),
    VSync([Option<PHYSICAL_ADDRESS>; 16]),
}

impl Adapter {
    fn init(device: NonNull<UnsafeCell<DEVICE_OBJECT>>) -> impl Init<Self, NtStatus> {
        init!(Self {
            tag: VIRTIO_GPU_ADAPTER_TAG,
            device: device,
            state <- InitOption::none(),
            flip_timer: None,
        }? NtStatus)
    }

    pub fn new(device: NonNull<UnsafeCell<DEVICE_OBJECT>>) -> Result<Box<Self>, NtStatus> {
        trace!("{}", function!());
        Box::try_init(Self::init(device)).map_err(|e| NtStatus(STATUS::NO_MEMORY))
    }

    fn get_pci_bus_info(&self) -> Result<DeviceFunction, NtStatus> {
        trace!("{}", function!());
        let mut len: u32 = 0;

        let mut bus: u32 = 0;
        let mut addr: u32 = 0;

        wdm_call_status!(<= PASSIVE_LEVEL | IoGetDeviceProperty(self.device.as_ref().get(), DEVICE_REGISTRY_PROPERTY::DevicePropertyBusNumber, size_of::<u32>() as _, &mut bus as *mut _ as _, &mut len as _))?;
        wdm_call_status!(<= PASSIVE_LEVEL | IoGetDeviceProperty(self.device.as_ref().get(), DEVICE_REGISTRY_PROPERTY::DevicePropertyAddress, size_of::<u32>() as _, &mut addr as *mut _ as _, &mut len as _))?;

        let dev = (addr >> 16) & 0xFFFF;
        let func = addr & 0xFFFF;

        let device_function = DeviceFunction { bus: bus as _, device: dev as _, function: func as _ };

        Ok(device_function)
    }

    fn process_resource_descriptors(&self, full_descriptors: &[CM_FULL_RESOURCE_DESCRIPTOR], pci_common: &PCI_COMMON_HEADER) -> Result<u16, NtStatus> {
        trace!("{}", function!());
        let mut interrupt_flags = None;

        for full_descriptor in full_descriptors {
            let partial_descriptors = slice_from_raw_parts(
                full_descriptor.PartialResourceList.PartialDescriptors.as_ptr(),
                full_descriptor.PartialResourceList.Count as _
            );

            for desc in partial_descriptors {
                match desc.Type as u32 {
                    CmResourceTypePort => {
                        //info!("CmResourceTypePort");
                    },
                    CmResourceTypeInterrupt => {
                        interrupt_flags = Some(desc.Flags);
                        //info!("CmResourceTypeInterrupt: flags = {:x} (msi = {})", desc.Flags, (desc.Flags as u32 & CM_RESOURCE_INTERRUPT_MESSAGE) != 0);
                    },
                    CmResourceTypeMemory => {
                        /*
                        let (start, length) = unsafe { (desc.u.Port.Start.QuadPart as u64, desc.u.Port.Length as usize) };
                        let Some(bar) = lookup_bar_index(pci_common, start) else {
                            error!("failed to find bar index for {:x} (len = {})", start, length);
                            continue;
                        };
                        info!("CmResourceTypeMemory: IO memory at {:x} (len = {}), bar = {}", start, length, bar);
                        */
                    },
                    CmResourceTypeDma => {
                        //info!("CmResourceTypeDma");
                    },
                    CmResourceTypeDeviceSpecific => {
                        //info!("CmResourceTypeDeviceSpecific");
                    },
                    CmResourceTypeBusNumber => {
                        //info!("CmResourceTypeBusNumber");
                    },
                    CmResourceTypeMemoryLarge => {
                        /*
                        let (start, length) = if (desc.Flags as u32 & CM_RESOURCE_MEMORY_LARGE_40) != 0 {
                            unsafe {
                                (desc.u.Memory40.Start.QuadPart as u64, (desc.u.Memory40.Length40 as usize) << 8)
                            }
                        } else if (desc.Flags as u32 & CM_RESOURCE_MEMORY_LARGE_48) != 0 {
                            unsafe {
                                (desc.u.Memory48.Start.QuadPart as u64, (desc.u.Memory48.Length48 as usize)  << 16)
                            }
                        } else if (desc.Flags as u32 & CM_RESOURCE_MEMORY_LARGE_64) != 0 {
                            unsafe {
                                (desc.u.Memory64.Start.QuadPart as u64, (desc.u.Memory64.Length64 as usize) << 32)
                            }
                        } else {
                            error!("unknown large memory type: {:x}", desc.Flags);
                            continue;
                        };

                        let Some(bar) = lookup_bar_index(pci_common, start) else {
                            error!("failed to find bar index for {:x} (len = {})", start, length);
                            continue;
                        };
                        info!("CmResourceTypeMemoryLarge: IO64 memory at {:x} (len = {}), bar = {}", start, length, bar);
                        */
                    },
                    CmResourceTypeDevicePrivate => {
                        let data = unsafe { desc.u.DevicePrivate.Data };
                        //info!("CmResourceTypeDevicePrivate: {:?}", data);
                    }
                    _ => {
                        error!("unknown partial descriptor: {:x}", desc.Type);
                    }
                }
            }
        }

        return Ok(interrupt_flags.unwrap());
    }

    pub fn start(&mut self, start_info: &DXGK_START_INFO, interface: DXGKRNL_INTERFACE) -> Result<u8, NtStatus> {
        trace!("{}", function!());

        // TODO: write registry info

        //info!("Num dma queue entries = {}, luid = ({}, {})", start_info.RequiredDmaQueueEntry, start_info.AdapterLuid.LowPart, start_info.AdapterLuid.HighPart);

        let device_function = self.get_pci_bus_info()?;

        let interface = DxgkInterface {
            interface: UnsafeCell::new(interface),
            device_function,
            pci_bars: [None; 6],
            unsafe_copy: false,
        };

        let device_info = interface.get_device_info()?;
        let pci_common = interface.read_pci_common_header()?;
        let luid = unsafe { transmute(start_info.AdapterLuid) };

        let mut pci_root = PciRoot::new(interface);

        let pci_bars = map_virtio_pci_error!(pci_root.bars(device_function))?.map(|b| b.map(|b| (b, None)));

        for i in 0..pci_bars.len() {
            if let Some((bar, _)) = &pci_bars[i] {
                debug!("found bar {}: {}", i, bar);
            }
        }

        pci_root.configuration_access.pci_bars = pci_bars;

        let full_descriptors = {
            let ptr = unsafe { (*device_info.TranslatedResourceList).List.as_ptr() };
            let size = unsafe { (*device_info.TranslatedResourceList).Count as usize };
            slice_from_raw_parts(ptr, size)
        };

        let interrupt_flags = self.process_resource_descriptors(full_descriptors, &pci_common)?;

        info!("vendor id = 0x{:04X}, device id = 0x{:04X}", pci_common.VendorID, pci_common.DeviceID);

        let mut pci_transport = map_virtio_pci_error!(PciTransport::new(&mut pci_root, device_function))?;

        let Some(shmem) = pci_transport.shmem() else {
            error!("no shared memory support");
            return Err(NtStatus(STATUS::UNSUCCESSFUL));
        };

        //info!("device type: {:?}", pci_transport.device_type());
        //info!("shared memory: {:?}", shmem);
        //info!("max control queue size: {:?}", pci_transport.max_queue_size(0));
        //info!("max cursor queue size: {:?}", pci_transport.max_queue_size(1));
        //
        //info!("size_of::<VirtQueue<DxgkInterface, 256>>(): {}", size_of::<VirtQueue<DxgkInterface, 256>>());

        let negotiated_features = pci_transport.begin_init(SUPPORTED_FEATURES);

        let events_read = map_virtio_error!(read_config!(pci_transport, Config, events_read))?;
        let num_scanouts = map_virtio_error!(read_config!(pci_transport, Config, num_scanouts))? as u8;
        let num_capsets = map_virtio_error!(read_config!(pci_transport, Config, num_capsets))?;
        //info!("events_read: {}, num_scanouts: {}, num_capsets: {}", events_read, num_scanouts, num_capsets);

        //info!("negotiated features: {:?}", negotiated_features);
        //
        //info!("left stack size: {}", io_get_remaining_stack_size());

        let is_vga = pci_common.SubClass as u32 == PCI_SUBCLASS_VID_VGA_CTLR;
        let is_msi = (interrupt_flags as u32 & CM_RESOURCE_INTERRUPT_MESSAGE) != 0;

        pci_transport.finish_init();

        self.state.write_init(init!(AdapterState {
            luid,
            num_scanouts,
            is_vga,
            is_msi,
            negotiated_features,
            supported_capsets: CapsetMask::default(),
            capset_info <- init_array_from_fn(|_| (CapsetInfo::default(), RwLock::new(None))),
            queue_handler <- QueueHandler::new(pci_transport, pci_root.configuration_access, *negotiated_features, shmem),
            max_simultaneously_running_commands <- init_array_from_fn(|_| AtomicU32::new(0)),
            system_display_info: None,
            empty_cursor: None,
            outputs <- init_array_from_fn(|_| None),
            source_target_map <- init_array_from_fn(|_| SmallVec::new()),
        }? NtStatus))?;

        let state = self.state.as_mut().unwrap();
        state.queue_handler.start_handler_thread()?;
        let chan = state.queue_handler.channel();

        let (supported_capsets, capset_info) = chan.get_capset_infos(num_capsets)?;

        state.supported_capsets = supported_capsets;
        state.capset_info = capset_info.map(|i| (i, RwLock::new(None)));

        let display_modes = chan.get_display_info()?;

        let mut rects = [commands::Rect { width: 0, height: 0, x: 0, y: 0}; 16];
        let mut flipq = [const { None }; 16];
        let addrs = [const { AtomicU64::new(0) }; 16];
        let vsync_enabled = AtomicBool::new(false);
        //let addrs = [const { (AtomicU64::new(0), AtomicPtr::new(null_mut())) }; 16];

        for scanout in 0..(num_scanouts as usize) {
            let edid = chan.get_edid(scanout as _)?;
            let _ = map_virtio_error!(edid.preferred_resolution())?;

            let info = VidPnOutput::new(display_modes[scanout], edid);

            debug!("scanout {}: {:?}", scanout, info);
            for (i, mode) in info.modes.iter().enumerate() {
                debug!("mode {}: {:?}", i, mode);
            }
            rects[scanout] = info.rect;
            flipq[scanout] = Some(info.flipq.clone());
            state.outputs[scanout] = Some(info);
        }

        state.empty_cursor = Some(Cursor::try_new(&chan, 64, 64, 0, 0, 0, 0)?);

        // TODO: allocate Framebuffer for this
        //if state.system_display_info.is_none() {
        //    state.system_display_info = Some(DXGK_DISPLAY_INFORMATION {
        //        Width: rects[0].width as _,
        //        Height: rects[0].height as _,
        //        Pitch: (rects[0].width * 4) as _,
        //        ColorFormat: D3DDDIFORMAT::D3DDDIFMT_X8R8G8B8,
        //        PhysicAddress: 0,
        //        TargetId: 0,
        //        AcpiId: 0,
        //    });
        //}

        // DEBUG => false
        //if false {
        if true {

        let flip_timer = FlipTimer::try_new(FlipTimerContext {
            rects,
            addrs,
            flipq,
            vsync_enabled,
            chan,
        }).inspect_err(|e|
            error!("{}: failed to create flip timer: {:?}", function!(), e)
        )?;

        flip_timer.start().inspect_err(|e|
            error!("{}: failed to start flip timer: {:?}", function!(), e)
        )?;

        self.flip_timer = Some(flip_timer);

        if is_vga {
            state.system_display_info = Some(state.queue_handler.dxgk_interface().acquire_post_display_ownership()?);
        }

        //Err(NtStatus(STATUS::UNSUCCESSFUL))

        Ok(num_scanouts)

        } else {
        Ok(0)
        }
    }

    pub fn query_info(&self, query_info: &DXGKARG_QUERYADAPTERINFO) -> Result<(), NtStatus> {
        trace!("{}: {:?}", function!(), query_info.Type);
        let state = check_state!(self)?;

        match query_info.Type {
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_UMDRIVERPRIVATE => {
                let umd_priv: &mut AdapterInfo = check_buffer_size!(query_info.pOutputData, query_info.OutputDataSize, AdapterInfo)?;
                //trace!("{}: private buffer: {:?}", function!(), buf);
                //let umd_priv: &mut AdapterInfo = TaggedExt::from_handle(query_info.pOutputData).ok_or(STATUS::INVALID_PARAMETER)?;
                umd_priv.tag = AdapterInfo::TAG;
                umd_priv.luid = state.luid;
                umd_priv.capset_mask = state.supported_capsets;
                umd_priv.supports_3d = state.negotiated_features.contains(Features::VIRGL) &&
                                       state.negotiated_features.contains(Features::RESOURCE_BLOB) &&
                                       state.negotiated_features.contains(Features::CONTEXT_INIT);
                umd_priv.has_shmem = true;

                Ok(())
            },
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_DRIVERCAPS => {
                let driver_caps: &mut DXGK_DRIVERCAPS = check_buffer_size!(query_info.pOutputData, query_info.OutputDataSize, DXGK_DRIVERCAPS)?;

                unsafe { *driver_caps = zeroed(); }

                // TODO: choose v2/v1.3 dynamically based on the OS version
                // Or maybe add a feature flag to build either WDDM1 or WDDM2 driver
                driver_caps.WDDMVersion = DXGK_WDDMVERSION::DXGKDDI_WDDMv2;
                //driver_caps.WDDMVersion = DXGK_WDDMVERSION::DXGKDDI_WDDMv2_3;
                //driver_caps.WDDMVersion = DXGK_WDDMVERSION::DXGKDDI_WDDMv1_3;
                driver_caps.HighestAcceptableAddress.QuadPart = -1i64;

                //driver_caps.PreemptionCaps.GraphicsPreemptionGranularity = D3DKMDT_GRAPHICS_PREEMPTION_GRANULARITY::D3DKMDT_GRAPHICS_PREEMPTION_NONE;
                //driver_caps.PreemptionCaps.ComputePreemptionGranularity = D3DKMDT_COMPUTE_PREEMPTION_GRANULARITY::D3DKMDT_COMPUTE_PREEMPTION_NONE;

                driver_caps.PreemptionCaps.GraphicsPreemptionGranularity = D3DKMDT_GRAPHICS_PREEMPTION_GRANULARITY::D3DKMDT_GRAPHICS_PREEMPTION_DMA_BUFFER_BOUNDARY;
                driver_caps.PreemptionCaps.ComputePreemptionGranularity = D3DKMDT_COMPUTE_PREEMPTION_GRANULARITY::D3DKMDT_COMPUTE_PREEMPTION_DMA_BUFFER_BOUNDARY;

                /* I don't think we can realistically support this:
                   > No generation of a DMA buffer to pass in a call to its DxgkDdiPresent function (that is, NULL is passed in the pDmaBuffer member of the DXGKARG_PRESENT structure).
                   But maybe this does not actually require performing blit? Need to test this later, should just work if so */
                driver_caps.FlipCaps.set_FlipOnVSyncMmIo(true);
                driver_caps.FlipCaps.set_FlipOnVSyncWithNoWait(true);
                /* Should be doable, but why bother */
                driver_caps.FlipCaps.set_FlipInterval(true);
                driver_caps.FlipCaps.set_FlipImmediateMmIo(true);
                /* Should work, might need to special case present for this */
                driver_caps.FlipCaps.set_FlipIndependent(true);

                driver_caps.MaxQueuedFlipOnVSync = 1;
                driver_caps.MemoryManagementCaps.set_SectionBackedPrimary(true);

                // GPU MMU emulation is currently broken
                driver_caps.MemoryManagementCaps.set_VirtualAddressingSupported(true);
                driver_caps.MemoryManagementCaps.set_GpuMmuSupported(true);

                // QEMU VirtIO-GPU cannot do IOMMU
                //driver_caps.MemoryManagementCaps.set_IoMmuSupported(true);

                driver_caps.MemoryManagementCaps.PagingNode = Engine::COPY_ENGINE;

                // TODO: I don't think we can realistically support this
                // It's the same story as with preemption all over again
                driver_caps.SupportPerEngineTDR = false as _;
                driver_caps.SupportDirectFlip = true as _;
                driver_caps.SchedulingCaps.set_MultiEngineAware(true);
                driver_caps.SchedulingCaps.set_PreemptionAware(true);
                // TODO: 15 seems to be the max as this is u4
                //driver_caps.SchedulingCaps.set_HwQueuePacketCap(0);

                driver_caps.GpuEngineTopology.NbAsymetricProcessingNodes = Engine::TOTAL_COUNT;
                driver_caps.SupportSmoothRotation = false as _;
                driver_caps.SupportNonVGA = state.is_vga as _;

                driver_caps.MaxPointerWidth = 64;
                driver_caps.MaxPointerHeight = 64;
                driver_caps.PointerCaps.set_Monochrome(true);
                driver_caps.PointerCaps.set_Color(true);
                driver_caps.PointerCaps.set_MaskedColor(true);

                Ok(())
            },
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_QUERYSEGMENT3 => {
                let segment_info = check_buffer_size!(query_info.pOutputData, query_info.OutputDataSize, DXGK_QUERYSEGMENTOUT3)?;

                segment_info.NbSegment = MemorySegment::COUNT;

                if !segment_info.pSegmentDescriptor.is_null() {
                    segment_info.PagingBufferPrivateDataSize = 8192;
                    //segment_info.PagingBufferSegmentId = 1;
                    segment_info.PagingBufferSegmentId = 0;
                    segment_info.PagingBufferSize = PAGING_DMA_BUFFER_SIZE;

                    let descriptors: &mut [DXGK_SEGMENTDESCRIPTOR3] = slice_from_raw_parts_mut(segment_info.pSegmentDescriptor, segment_info.NbSegment as _);

                    unsafe {
                        descriptors.fill(zeroed());
                    };

                    let (shmem_start, shmem_len) = state.queue_handler.get_shmem_slice();

                    for segment in MemorySegment::SEGMENTS {
                        segment.fill_desc3(shmem_start, shmem_len, &mut descriptors[segment.index() as usize]);
                    }

                    /*
                    let desc_3d = &mut descriptors[SEGMENT_ID_3D as usize - 1];
                    desc_3d.BaseAddress.QuadPart = APERTURE_SEGMENT_GPU_PADDR as _;
                    desc_3d.Size = 1024 * 1024 * 1024;
                    desc_3d.CommitLimit = 1024 * 1024 * 1024;
                    desc_3d.Flags.set_Aperture(true);
                    desc_3d.Flags.set_CacheCoherent(true);
                    desc_3d.Flags.set_DirectFlip(true);

                    let (shmem_start, shmem_len) = state.queue_handler.get_shmem_slice();

                    let desc_blob_mappable = &mut descriptors[SEGMENT_ID_BLOB_MAPPABLE as usize - 1];
                    desc_blob_mappable.BaseAddress.QuadPart = BLOB_MAP_SEGMENT_GPU_PADDR as _;
                    desc_blob_mappable.Size = shmem_len;
                    desc_blob_mappable.CommitLimit = shmem_len;
                    desc_blob_mappable.CpuTranslatedAddress.QuadPart = shmem_start as _;
                    desc_blob_mappable.Flags.set_CacheCoherent(true);
                    desc_blob_mappable.Flags.set_CpuVisible(true);
                    desc_blob_mappable.Flags.set_DirectFlip(true);

                    let desc_blob_host3d = &mut descriptors[SEGMENT_ID_BLOB_HOST3D as usize - 1];
                    desc_blob_host3d.BaseAddress.QuadPart = BLOB_SHARE_SEGMENT_GPU_PADDR as _;
                    desc_blob_host3d.Size = 16 * 1024 * 1024 * 1024;
                    desc_blob_host3d.CommitLimit = 16 * 1024 * 1024 * 1024;
                    desc_blob_host3d.Flags.set_CacheCoherent(true);
                    desc_blob_host3d.Flags.set_DirectFlip(true);
                    // FIXME: this isn't really a CpuVisible segment, but allocating from it fails otherwise.
                    // Apparently, VirtualBox authors encountered the same problem:
                    // https://github.com/VirtualBox/virtualbox/blob/499d5a317f23448903c7662a38baf957de37ddf4/src/VBox/Additions/win/Graphics/Video/mp/wddm/VBoxMPWddm.cpp#L2339
                    desc_blob_host3d.Flags.set_CpuVisible(true);
                    */

                }

                Ok(())
            },
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_QUERYSEGMENT4 => {
                let segment_info = check_buffer_size!(query_info.pOutputData, query_info.OutputDataSize, DXGK_QUERYSEGMENTOUT4)?;
                trace!("{}: {:?}: stride = {}", function!(), query_info.Type, segment_info.SegmentDescriptorStride);

                if segment_info.pSegmentDescriptor.is_null() {
                    segment_info.NbSegment = 3;
                    segment_info.SegmentDescriptorStride = size_of::<DXGK_SEGMENTDESCRIPTOR4>() as _;
                } else {
                    assert!(segment_info.NbSegment == 3);
                    assert!(size_of::<DXGK_SEGMENTDESCRIPTOR4>() <= segment_info.SegmentDescriptorStride as usize);

                    segment_info.PagingBufferPrivateDataSize = 8192;
                    //segment_info.PagingBufferSegmentId = 1;
                    segment_info.PagingBufferSegmentId = 0;
                    segment_info.PagingBufferSize = PAGING_DMA_BUFFER_SIZE;

                    //let desc_3d            = unsafe { &mut *(segment_info.pSegmentDescriptor.offset((segment_info.SegmentDescriptorStride * (SEGMENT_ID_3D as u64 - 1)) as isize) as *mut DXGK_SEGMENTDESCRIPTOR4) };
                    //let desc_blob_mappable = unsafe { &mut *(segment_info.pSegmentDescriptor.offset((segment_info.SegmentDescriptorStride * (SEGMENT_ID_BLOB_MAPPABLE as u64 - 1)) as isize) as *mut DXGK_SEGMENTDESCRIPTOR4) };
                    //let desc_blob_host3d   = unsafe { &mut *(segment_info.pSegmentDescriptor.offset((segment_info.SegmentDescriptorStride * (SEGMENT_ID_BLOB_HOST3D as u64 - 1)) as isize) as *mut DXGK_SEGMENTDESCRIPTOR4) };

                    let (shmem_start, shmem_len) = state.queue_handler.get_shmem_slice();

                    for segment in MemorySegment::SEGMENTS {
                        let desc = unsafe { &mut *(segment_info.pSegmentDescriptor.offset((segment_info.SegmentDescriptorStride * segment.index() as u64) as isize) as *mut DXGK_SEGMENTDESCRIPTOR4) };
                        segment.fill_desc4(shmem_start, shmem_len, desc);
                    }
                }

                Ok(())
            },
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_DISPLAY_DRIVERCAPS_EXTENSION => {
                let display_caps_ext = check_buffer_size!(query_info.pOutputData, query_info.OutputDataSize, DXGK_DISPLAY_DRIVERCAPS_EXTENSION)?;
                *display_caps_ext = unsafe { zeroed() };
                Ok(())
            }
            //DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_PHYSICALADAPTERCAPS => {
            //    // FIXME: size seems to be invalid for whatever reason
            //    const _: () = assert!(size_of::<DXGK_PHYSICALADAPTERCAPS>() == 24);
            //    let adapter_caps = check_buffer_size!(query_info.pOutputData, query_info.OutputDataSize, DXGK_PHYSICALADAPTERCAPS)?;
            //    *adapter_caps = unsafe { zeroed() };
            //    adapter_caps.PagingNodeIndex = Engine::COPY_ENGINE as _;
            //    Ok(())
            //},
            //DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_64BITONLYCAPS => {
            //    Ok(())
            //},
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_HISTORYBUFFERPRECISION => {
                let history_buffers = slice_from_raw_parts_mut(
                    query_info.pOutputData as *mut DXGKARG_HISTORYBUFFERPRECISION,
                    query_info.OutputDataSize as usize / size_of::<DXGKARG_HISTORYBUFFERPRECISION>()
                );

                for history_buffer in history_buffers {
                      history_buffer.PrecisionBits = 64;
                }
                Ok(())
            },

            //DXGKQAITYPE_NODEPERFDATA
            DXGK_QUERYADAPTERINFOTYPE(24) => {
                Ok(())
            },

            //DXGKQAITYPE_ADAPTERPERFDATA
            DXGK_QUERYADAPTERINFOTYPE(25) => {
                Ok(())
            },
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_GPUMMUCAPS => {
                trace!("{}: {:?}", function!(), query_info.Type);
                let gpu_mmu_caps = check_buffer_size!(query_info.pOutputData, query_info.OutputDataSize, DXGK_GPUMMUCAPS)?;

                *gpu_mmu_caps = unsafe { zeroed() };
                gpu_mmu_caps.set_ExplicitPageTableInvalidation(true);
                // TODO: gpu_mmu_caps.set_CacheCoherentMemorySupported(true);
                gpu_mmu_caps.PageTableUpdateMode = DXGK_PAGETABLEUPDATEMODE::DXGK_PAGETABLEUPDATE_CPU_VIRTUAL;
                //gpu_mmu_caps.PageTableUpdateMode = DXGK_PAGETABLEUPDATEMODE::DXGK_PAGETABLEUPDATE_GPU_PHYSICAL;
                gpu_mmu_caps.VirtualAddressBitCount = 48;

                gpu_mmu_caps.PageTableLevelCount = 3;

                Ok(())
            },

            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_PAGETABLELEVELDESC => {
                let page_table_level = unsafe { *(query_info.pInputData as *const UINT) };
                trace!("{}: {:?} / level {}", function!(), query_info.Type, page_table_level);
                let page_table_desc = check_buffer_size!(query_info.pOutputData, query_info.OutputDataSize, DXGK_PAGE_TABLE_LEVEL_DESC)?;

                if page_table_level < 3 {
                    *page_table_desc = PAGE_TABLE_DESC[page_table_level as usize];
                    Ok(())
                } else {
                    error!("{}: invalid page level: {}", function!(), page_table_level);
                    Err(NtStatus(STATUS::INVALID_PARAMETER))
                }
            },

            _ => {
                //if query_info.Type.0 != 24 && query_info.Type.0 != 25 {
                    trace!("unknown adapter info type: {:?}", query_info.Type);
                //}
                Err(NtStatus(STATUS::NOT_SUPPORTED))
            },
        }
    }

    pub fn escape(&self, escape: &DXGKARG_ESCAPE) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let state = check_state!(self)?;

        let escape_tag = check_buffer_size!(escape.pPrivateDriverData, escape.PrivateDriverDataSize, u64)?;

        match *escape_tag {
            ESCAPE_CAPSET_TAG => {
                let capset = check_buffer_size!(escape.pPrivateDriverData, escape.PrivateDriverDataSize, Capset)?;
                trace!("{}: capset: {:?}", function!(), capset);

                let id = capset.capset_id;
                let index = id as usize;

                if index >= state.capset_info.len() {
                    error!("{}: unsupported capset: {:?}", function!(), id);
                    return Err(NtStatus(STATUS::INVALID_PARAMETER_1));
                }

                let (info, data) = &state.capset_info[index];

                if data.read().is_none() {
                    data.write().replace(state.queue_handler.chan.get_capset(id, info)?);
                    debug!("{}: capset data: {:?}", function!(), data.read().as_ref().unwrap());
                }

                let user_slice = capset.capset_slice(escape.PrivateDriverDataSize as _);
                let capset_data = data.read();
                let capset_data = capset_data.as_ref().unwrap();
                let copy_size = core::cmp::min(user_slice.len(), capset_data.len());

                //[..user_slice.len()];

                debug!("{}: copying local capset len {} to user len {}", function!(), capset_data.len(), user_slice.len());

                (&mut user_slice[..copy_size]).copy_from_slice(&capset_data[..copy_size]);

                Ok(())
            },

            ESCAPE_RESOURCE_INFO_TAG => {
                let res_info = check_buffer_size!(escape.pPrivateDriverData, escape.PrivateDriverDataSize, ResourceInfo)?;
                let alloc = state.queue_handler.dxgk_interface().allocation_from_handle(res_info.handle).ok_or(STATUS::INVALID_HANDLE)?;
                trace!("{}: res info for allocation {:?}", function!(), alloc);

                let id = alloc.id().ok_or(STATUS::UNSUCCESSFUL)?.get();
                let resource = alloc.resource().clone();

                // TODO: virgl needs to query resources BEFORE context init
                let layout = if
                    alloc.is_3d() &&
                    unsafe { res_info.info._3d.modifier == DRM_FORMAT_MOD_INVALID } &&
                    let Some(device) = <Device as TaggedExt>::from_arc_handle_clone(escape.hDevice)
                {
                    alloc.query_layout(device)?
                } else {
                    None
                }.unwrap_or(VirglResourceLayout {
                    modifier: DRM_FORMAT_MOD_INVALID,
                    ..unsafe { zeroed() }
                });

                res_info.id = id;
                res_info.info = resource.into();

                match res_info.info.tag() {
                    ALLOCATE_3D_TAG => {
                        let info_3d = unsafe { &mut res_info.info._3d };

                        info_3d.modifier = layout.modifier;
                        info_3d.num_planes = layout.num_planes;

                        for (i, plane) in layout.planes[0..layout.num_planes as usize].iter().enumerate() {
                            info_3d.offsets[i] = plane.offset;
                            info_3d.strides[i] = plane.stride;
                            info_3d.sizes[i] = plane.size;
                        }

                        //warn!("{}: alloc: ({}, {:?})", function!(), alloc.id().unwrap(), layout);

                        debug!("{}: returning {:?}", function!(), info_3d);
                    },
                    ALLOCATE_BLOB_TAG => {
                        debug!("{}: returning {:?}", function!(), unsafe { &res_info.info.blob });
                    },
                    _ => unreachable!(),
                };

                Ok(())
            },

            ESCAPE_RESOURCE_BUSY_TAG => {
                let res_busy = check_buffer_size!(escape.pPrivateDriverData, escape.PrivateDriverDataSize, ResourceBusy)?;
                let alloc = state.queue_handler.dxgk_interface().allocation_from_handle(res_busy.handle).ok_or(STATUS::INVALID_HANDLE)?;
                trace!("{}: <enter> res busy (wait {}) for allocation {:?}: {}", function!(), res_busy.wait, alloc, alloc.is_busy());
                let handle = unsafe { res_busy.event.handle };
                if !handle.is_null() {
                    let sync = KeEvent::from_usermode_handle(handle)?;
                    alloc.attach_sync_file(sync);

                    if alloc.is_busy() {
                        unsafe { sync.as_ref().clear(); }
                    } else {
                        unsafe { sync.as_ref().set(); }
                    }
                }

                if res_busy.wait {
                    trace!("{}: START WAIT for id {} {:?}", function!(), alloc.id().unwrap(), alloc);
                    alloc.wait();
                    trace!("{}: END WAIT for id {} {:?}", function!(), alloc.id().unwrap(), alloc);
                }

                res_busy.is_busy = alloc.is_busy();

                trace!("{}: <exit> res busy (wait {}) for allocation {:?}: {}", function!(), res_busy.wait, alloc, alloc.is_busy());

                Ok(())
            },

            /*
            ESCAPE_RESOURCE_ATTACH_TAG => {
                let res_atta = check_buffer_size!(escape.pPrivateDriverData, escape.PrivateDriverDataSize, ResourceAttachContext)?;

                let Some(device) = <Device as TaggedExt>::from_arc_handle_clone(escape.hDevice) else {
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                };

                if let Some(alloc) = state.queue_handler.dxgk_interface().allocation_from_handle(res_atta.handle) {
                    warn!("{}: resource_attach: alloc: ({}, {:?}), device {:?}", function!(), alloc.id().unwrap(), if alloc.is_3d() {"3D"} else {"BLOB"}, device);
                    return Ok(())
                }

                let res = state.queue_handler.dxgk_interface().resource_from_handle(res_atta.handle).ok_or(STATUS::INVALID_HANDLE)?;

                res.attach_to_device(device)
            },
            */

            ESCAPE_BLOB_INFO_SET_TAG => {
                let blob_set = check_buffer_size!(escape.pPrivateDriverData, escape.PrivateDriverDataSize, BlobInfoSet)?;
                trace!("{}: blob set info: {:?}", function!(), blob_set);
                let alloc = state.queue_handler.dxgk_interface().allocation_from_handle(blob_set.handle).ok_or(STATUS::INVALID_HANDLE)?;
                alloc.set_blob_info(blob_set.blob_info).ok_or(STATUS::INVALID_HANDLE).inspect_err(|_|
                    error!("{}: cannot set blob info for 3d resource: {:?}", function!(), alloc)
                )?;

                Ok(())
            },

            ESCAPE_BLOB_MAP_TAG => {
                trace!("{}: map blob", function!());
                let blob_map = check_buffer_size!(escape.pPrivateDriverData, escape.PrivateDriverDataSize, BlobMap)?;

                //let chan = state.handler.chan

                let Some(device) = <Device as TaggedExt>::from_arc_handle_clone(escape.hDevice) else {
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                };

                let alloc = state.queue_handler.dxgk_interface().allocation_from_handle(blob_map.handle).ok_or(STATUS::INVALID_HANDLE)?;
                if { blob_map.flags }.contains(BlobMapFlags::UNMAP) {
                    if let Some(ptr) = unsafe { blob_map.ptr.ptr } && let Some((offset, size)) = alloc.mapped_range() {
                        let phys = state.queue_handler.get_shmem_slice().0 + offset;
                        let mdl = MdlOwned::from_io_physical_range(phys, size)?;
                        //debug!("{}: Unmapped {:X} (offset {:X}) from address {:?}: {:?}", function!(), { blob_map.handle }, offset, ptr, alloc);
                        mm_unmap_locked_pages(&mdl, ptr);
                    }

                    //let err = if let Some(ptr) = unsafe { blob_map.ptr.ptr } {
                    //    //let
                    //
                    //    state.queue_handler.dxgk_interface().unmap_memory(ptr)
                    //} else {
                    //    Ok(())
                    //};
                    alloc.unmap_blob(&device)?;
                } else {
                    let (offset, size, map_info) = alloc.map_blob(&device)?;

                    let phys = state.queue_handler.get_shmem_slice().0 + offset;

                    let caching_type = match map_info {
                        commands::VIRTIO_GPU_MAP_CACHE_NONE => {
                            warn!("{}: map blob returned unexpected caching type VIRTIO_GPU_MAP_CACHE_NONE for {:?}", function!(), &alloc);
                            MEMORY_CACHING_TYPE::MmNonCached
                        },
                        commands::VIRTIO_GPU_MAP_CACHE_CACHED => MEMORY_CACHING_TYPE::MmCached,
                        commands::VIRTIO_GPU_MAP_CACHE_UNCACHED => MEMORY_CACHING_TYPE::MmNonCached,
                        commands::VIRTIO_GPU_MAP_CACHE_WC => MEMORY_CACHING_TYPE::MmWriteCombined,
                        _ => {
                            error!("{}: map blob returned unkown caching type 0x{:x} for {:?}", function!(), map_info, &alloc);
                            return Err(NtStatus(STATUS::IO_DEVICE_ERROR));
                        },
                    };


                    if size >= u32::MAX as _ {
                        error!("{}: mapping too big blobs of size {:?} is not implemented yet", function!(), size);
                        return Err(NtStatus(STATUS::NOT_IMPLEMENTED));
                    };

                    let mdl = MdlOwned::from_io_physical_range(phys, size)?;

                    let ptr = match microseh::try_seh(|| -> Option<NonNull<u8>> {
                        mm_map_locked_pages_specify_cache(&mdl, true, wdk::wdm::MEMORY_CACHING_TYPE(caching_type.0), None)
                    }) {
                        Ok(Some(ptr)) => ptr,
                        Ok(None) => {
                            error!("{}: Failed to map {:X} (offset {:X}) with caching type {:?}: {:?}", function!(), { blob_map.handle }, offset, caching_type, alloc);
                            return Err(NtStatus(STATUS::UNSUCCESSFUL));
                        }
                        Err(e) => {
                            error!("{}: Exception {:?} while trying to map {:X} (offset {:X}) with caching type {:?}: {:?}", function!(), e, { blob_map.handle }, offset, caching_type, alloc);
                            return Err(e.into());
                        },
                    };

                    //let ptr = state.queue_handler.dxgk_interface().map_memory(phys, size, false, true, caching_type)?;
                    debug!("{}: Mapped {:X} (offset {:X}) at address {:?} with caching type {:?}: {:?}", function!(), { blob_map.handle }, offset, ptr, caching_type, alloc);
                    blob_map.ptr.ptr = Some(ptr);
                }

                Ok(())
            },

            ESCAPE_CONTEXT_INIT_TAG => {
                let context_init = check_buffer_size!(escape.pPrivateDriverData, escape.PrivateDriverDataSize, ContextInit)?;
                trace!("{}: context init: {:?}, device: {:?}, context: {:?}", function!(), context_init, escape.hDevice, escape.hContext);
                let Some(device) = <Device as TaggedExt>::from_arc_handle_clone(escape.hDevice) else {
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                };

                device.init_context(state.supported_capsets, context_init)
            },

            ESCAPE_EXEC_BUF_TAG => {
                let exec_buffer = check_buffer_size!(escape.pPrivateDriverData, escape.PrivateDriverDataSize, ExecBuffer)?;
                let priv_size = escape.PrivateDriverDataSize as usize;

                let mut command_offset = 0;
                let cmd = exec_buffer.command_slice(priv_size);

                let (hdr, _) = CommandHeaderRaw::ref_from_prefix(cmd).map_err(|e| {
                    error!("{}: failed to read raw command header: {:?}", function!(), e);
                    STATUS::INVALID_PARAMETER
                })?;
                command_offset += size_of::<CommandHeaderRaw>();

                let hdr = hdr.try_as_hdr().map_err(|e| {
                    error!("{}: command validation failed: {}", function!(), e);
                    STATUS::INVALID_PARAMETER
                })?;

                if hdr.shadow_virgl() {
                    error!("{}: command validation failed: shadow virgl from usermode is not supported", function!());
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                }

                let body = hdr.command_slice().map_err(|e| {
                    error!("{}: command validation failed: {}", function!(), e);
                    STATUS::INVALID_PARAMETER
                })?;

                command_offset += hdr.size as usize;

                if command_offset != cmd.len() {
                    error!("{}: command validation failed: too many commands", function!());
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                }

                let CommandBody::Submit(data) = body else {
                    error!("{}: command validation failed: unsupported type: {:?}", function!(), hdr);
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                };

                let context = <DeviceContext as TaggedExt>::from_arc_handle_clone(escape.hContext).ok_or(STATUS::INVALID_PARAMETER)?;
                let ctx_id = context.device.context().ok_or(STATUS::REINITIALIZATION_NEEDED)?;

                let fence_id = state.queue_handler.chan.submit_command_buffer_with_fence(context.engine, ctx_id, hdr.ring(), data)?;

                if data.len() > 0 && let Some(CapsetId::Venus) = context.device.capset() {
                    if let Some((cmd, body)) = data.split_at_checked(size_of::<u32>()) {
                        let cmd = crate::virgl::VenusCommandType(u32::from_le_bytes(cmd.try_into().unwrap()));
                        trace!("{}: SUBMIT_3D: virtio_fence {}, ctx {}, ring {:?}, cmd {:?}, body {:X?}", function!(), fence_id, ctx_id, hdr.ring(), cmd, body);
                    } else {
                        warn!("{}: cmd: {:?}, data ({:?}@{}): {:?}", function!(), hdr, data.as_ptr(), data.len(), data);
                    }
                }

                exec_buffer.fence_id = fence_id;

                Ok(())
            }

            _ => {
                error!("unknown escape type: {:x}", escape_tag);
                Err(NtStatus(STATUS::NOT_SUPPORTED))
            }
        }
    }

    pub fn stop(&mut self) -> Result<(), NtStatus> {
        trace!("{}", function!());

        let result = if let Some(flip_timer) = self.flip_timer.take() {
            flip_timer.stop()
        } else {
            Ok(())
        };
        // take would overflow the stack lol, safe language btw
        self.state.clear();

        result
    }

    pub fn stop_and_release(&mut self) -> Result<DXGK_DISPLAY_INFORMATION, NtStatus> {
        trace!("{}", function!());
        let state = check_state!(self)?;
        let info = state.system_display_info.unwrap();

        self.stop()?;

        Ok(info)
    }

    pub fn handle_interrupt(&mut self, msg: u32) -> bool {
        //trace!("{}", function!());
        let Ok(state) = check_state_mut!(self) else {
            return false;
        };

        if state.is_msi {
            // TODO: should we implement MSI-X?
            todo!("MSI-X");
        } else {
            state.queue_handler.ack_interrupt();
            state.queue_handler.dxgk_interface().queue_dpc();

            true
        }
    }

    pub fn handle_dpc(&self) {
        //trace!("{}", function!());
        let Ok(state) = check_state!(self) else {
            return;
        };

        state.queue_handler.handle_dpc();
        state.queue_handler.dxgk_interface().notify_dpc();
    }

    pub fn queue_handler(&self) -> Option<&QueueHandler> {
        let state = self.state.as_ref()?;

        Some(&state.queue_handler)
    }

    pub fn queue_channel(&self) -> Option<GpuChannel> {
        let state = self.state.as_ref()?;

        Some(state.queue_handler.channel())
    }

    pub fn check_engine(&self, engine: Engine) -> bool {
        if let Some(state) = self.state.as_ref() {
            state.queue_handler.check_engine(engine)
        } else {
            true
        }
    }

    pub fn is_vga(&self) -> bool {
        self.state.as_ref().and_then(|s| Some(s.is_vga)).unwrap_or(false)
    }

    //pub fn capsets(&self) -> CapsetMask {
    //    self.state.as_ref().and_then(|s| Some(s.supported_capsets)).unwrap_or(CapsetMask::empty())
    //}

    pub fn kick_queue_handler(&self) {
        let Ok(state) = check_state!(self) else {
            return;
        };

        state.queue_handler.kick();
    }

    pub fn notify_queued_fences(&self) {
        let Ok(state) = check_state!(self) else {
            return;
        };

        for node_ordinal in 0..Engine::TOTAL_COUNT {
            let engine = Engine::try_from_node_ordinal(node_ordinal).unwrap();
            state.queue_handler.notify_queued_fences(engine);
        }
    }

    pub fn notify_dma_preempted(&self, engine: Engine, preemption_fence: u32, last_completed_fence: u32) {
        let Ok(state) = check_state!(self) else {
            return;
        };

        state.queue_handler.notify_dma_preempted(engine, preemption_fence, last_completed_fence);
    }

    pub fn last_completed_fence(&self, engine: Engine) -> u32 {
        let Ok(state) = check_state!(self) else {
            return 0;
        };

        state.queue_handler.last_completed_fence(engine)
    }

    pub fn last_submitted_fence(&self, engine: Engine) -> u32 {
        let Ok(state) = check_state!(self) else {
            return 0;
        };

        state.queue_handler.last_submitted_fence(engine)
    }

    pub fn submit_command(&self, submit_command: &DXGKARG_SUBMITCOMMAND) -> Result<(), NtStatus> {
        if submit_command.pDmaBufferPrivateData.is_null() {
            error!("{}: no dma private data", function!());
            return Ok(());
        }
        let state = check_state!(self)?;
        let chan = &state.queue_handler.chan;
        let fence = submit_command.SubmissionFenceId;

        let engine = if let Some(context) = <DeviceContext as TaggedExt>::from_handle_silent(unsafe { submit_command.__bindgen_anon_1.hContext }) {
            context.engine
        } else {
            Engine::try_from_node_ordinal(submit_command.NodeOrdinal).unwrap()
        };

        debug!("{}: DmaSubmit({:?}, {})", function!(), engine, submit_command.SubmissionFenceId);
        //info!("{}: engine {:?}: {:?}", function!(), engine, submit_command);

        let simultaneously_running_commands = fence - state.queue_handler.last_completed_fence(engine);
        let max_simultaneously_running_commands = state.max_simultaneously_running_commands[engine.node_ordinal() as usize].fetch_max(simultaneously_running_commands, Ordering::SeqCst);
        if max_simultaneously_running_commands < simultaneously_running_commands {
            debug!("{}: engine {:?} max simultaneously running commands: {}", function!(), engine, simultaneously_running_commands);
        }

        //info!("{}: priv: {:?}, offset: {:?}", function!(), submit_command.pDmaBufferPrivateData, submit_command.DmaBufferPrivateDataSubmissionStartOffset);

        if submit_command.Flags.Resubmission() {
            // We cannot handle this, so let's hope this will never happen
            // TODO: we might want to handle preemption as follows:
            // 1. VidSch calls submit_command with fence 1
            // 2. VidSch calls submit_command with fence 2
            // 3. VidSch calls preempt_command with preemption fence 1
            // 4. Wait for all commands to finish until some timeout.
            // 5. If completed in time - report everything as "already finished", otherwise go to the next step
            // 6. Notify about preemption and record "preempted" fence range somewhere
            // 7. Once command is finished, mark corresponding fence in range as completed
            // 8. When command is resubmited, notify about it immediately taking it from the list of "preempted" commands
            // But I'm too lazy to implement this right now. Might have some performance benefit I guess
            // UPD: can we use submit_fence for this? Maybe we should actually convert all commands into submit_fence...
            // This would give more control about timeouts and such.
            error!("{}: tried to resubmit preempted command: {:?}", function!(), submit_command);
        }

        // Paging buffers are special because there could be multiple commands written into the same buffer
        // I don't see a good (and safe) way of avoiding this monstrosity (yet)
        let priv_ptr = if submit_command.DmaBufferPrivateDataSize == 0
                || (submit_command.DmaBufferPrivateDataSubmissionStartOffset == submit_command.DmaBufferPrivateDataSubmissionEndOffset && !submit_command.Flags.Paging())
                || (submit_command.DmaBufferSubmissionStartOffset == submit_command.DmaBufferSubmissionEndOffset && submit_command.Flags.Paging())
        {
            // This is kinda expected for some of the paging commands, so no reason to warn
            if submit_command.Flags.Paging() | submit_command.Flags.Flip() {
                debug!("{}: empty dma private data for submit {:?}", function!(), submit_command);
            } else {
                warn!("{}: empty dma private data for submit {:?}", function!(), submit_command);
            }
            chan.submit_command(engine, fence, &Command::nop(), None)?;

            return Ok(())
        } else {
            submit_command.pDmaBufferPrivateData
            //unsafe {
            //    submit_command.pDmaBufferPrivateData.byte_add(submit_command.DmaBufferPrivateDataSubmissionStartOffset as _)
            //}
        };

        let (commands, allocations) = if let Some(dma_priv) = <CommandDmaPrivate as TaggedExt>::from_handle_silent_mut(priv_ptr) {
            let CommandDmaPrivate {commands, allocations, ..} = core::mem::take(dma_priv);
            (commands, allocations)
        } else {
            if !submit_command.Flags.Paging() {
                error!("{}: invalid dma private data for submit {:?}", function!(), submit_command);
            }
            chan.submit_command(engine, fence, &Command::nop(), None)?;
            return Ok(())
        };

        trace!("{}: engine {:?} total {} commands: {:?}, allocations: {:?}", function!(), engine, commands.len(), commands, allocations);

        //for cmd in &commands {
        //    if cmd.id == CommandId::MapBlob {
        //        warn!("{}: map blob: {:?}, alloc: {:?}", function!(), cmd, allocations);
        //    }
        //}

        // FIXME: pDmaBufferPrivateData contains private data for the whole dma buffer, not to the currently being submitted part
        // So we may end up submitting more than we were asked to
        // TODO: maybe use DmaBufferSubmissionStartOffset..DmaBufferSubmissionEndOffset for partial submits?
        // Something along the lines of "iterate over each command, if in range - add to batch else break"

        if commands.len() <= 1 {
            let cmd = commands.get(0).and_then(|c| Some(c.clone())).unwrap_or(Command::nop());
            //if let Some(mut dma) = cmd.dma() {
            //    let phys_mm = unsafe { MmGetPhysicalAddress(dma.as_mut().as_mut_ptr() as _).QuadPart as u64 };
            //    let phys_sc = unsafe { submit_command.DmaBufferPhysicalAddress.QuadPart as u64 } + (submit_command.DmaBufferSubmissionStartOffset as u64);
            //    debug!("{}: dma dxgk phys addr: {:X} / {:X}", function!(), phys_mm, phys_sc);
            //    assert_eq!(phys_mm, phys_sc);
            //}

            let alloc_batch = if allocations.len() > 0 {
                Some(AllocationsBatch::new(allocations))
            } else {
                None
            };

            chan.submit_command(engine, fence, &cmd, alloc_batch)?;
        } else {
            chan.submit_command_batch(engine, fence, &commands, AllocationsBatch::new(allocations))?;
        }

        Ok(())
    }

    pub fn submit_command_virtual(&self, submit_command: &DXGKARG_SUBMITCOMMANDVIRTUAL) -> Result<(), NtStatus> {
        let state = check_state!(self)?;
        let chan = &state.queue_handler.chan;
        let fence = submit_command.SubmissionFenceId;

        if submit_command.Flags.Resubmission() {
            // TODO: handle as the physical one above
            error!("{}: tried to resubmit preempted command: {:?}", function!(), submit_command);
        }

        let (engine, ctx) = if let Some(context) = <DeviceContext as TaggedExt>::from_handle_silent(submit_command.hContext) {
            (context.engine, context.device.context())
        } else {
            (Engine::try_from_node_ordinal(submit_command.NodeOrdinal).unwrap(), None)
        };

        if submit_command.pDmaBufferPrivateData.is_null() {
            error!("{}: no dma private data", function!());
            chan.submit_command(engine, fence, &Command::nop(), None)?;
            return Ok(());
        }

        debug!("{}: DmaSubmitVirtual({:?}, {})", function!(), engine, submit_command.SubmissionFenceId);
        //info!("{}: engine {:?}: {:?}", function!(), engine, submit_command);

        let simultaneously_running_commands = fence - state.queue_handler.last_completed_fence(engine);
        let max_simultaneously_running_commands = state.max_simultaneously_running_commands[engine.node_ordinal() as usize].fetch_max(simultaneously_running_commands, Ordering::SeqCst);
        if max_simultaneously_running_commands < simultaneously_running_commands {
            debug!("{}: engine {:?} max simultaneously running commands: {}", function!(), engine, simultaneously_running_commands);
        }

        let (commands, allocations) = if let Some(dma_priv) = <CommandDmaPrivate as TaggedExt>::from_handle_silent_mut(submit_command.pDmaBufferPrivateData) {
            let CommandDmaPrivate {commands, allocations, ..} = core::mem::take(dma_priv);
            (commands, allocations)
        } else if let Some(submit_usermode) = <SubmitCommand as TaggedExt>::from_handle_silent(submit_command.pDmaBufferPrivateData) {
            let priv_size = submit_command.DmaBufferUmdPrivateDataSize as usize;

            let mut command_offset = 0;
            let cmd = submit_usermode.command_slice(priv_size);

            let (hdr, _) = CommandHeaderRaw::ref_from_prefix(cmd).map_err(|e| {
                error!("{}: failed to read raw command header: {:?}", function!(), e);
                STATUS::INVALID_PARAMETER
            })?;
            command_offset += size_of::<CommandHeaderRaw>();

            let hdr = hdr.try_as_hdr().map_err(|e| {
                error!("{}: command validation failed: {}", function!(), e);
                STATUS::INVALID_PARAMETER
            })?;

            if hdr.shadow_virgl() {
                error!("{}: command validation failed: shadow virgl from usermode is not supported", function!());
                return Err(NtStatus(STATUS::INVALID_PARAMETER));
            }

            let body = hdr.command_slice().map_err(|e| {
                error!("{}: command validation failed: {}", function!(), e);
                STATUS::INVALID_PARAMETER
            })?;

            command_offset += hdr.size as usize;

            /*
            if command_offset < cmd.len() {
                let sync_info_bytes = &cmd[command_offset..];
                let (h, sync_info, f) = unsafe { sync_info_bytes.align_to::<u32>() };
                if h.len() > 0 || f.len() > 0 {
                    warn!("{}: command validation failed: unaligned sync info: before {}, after {}", function!(), h.len(), f.len());
                }
                if sync_info.len() < 2 {
                    warn!("{}: command validation failed: sync info does not have at least a header: {}", function!(), sync_info.len());
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                }
                let in_count = sync_info[0] as usize;
                let out_count = sync_info[1] as usize;
                if sync_info.len() < 2 + in_count + out_count {
                    warn!("{}: command validation failed: sync info does not have enough syncs: {} < 2 + {} + {}", function!(), sync_info.len(),  in_count, out_count);
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                }
                let syncs = &sync_info[2..];
                let in_syncs = &syncs[0..in_count];
                let out_syncs = &sync_info[in_count..in_count + out_count];

                //info!("{}: fence {}: in {:X?}", function!(), fence, in_syncs);
                //info!("{}: fence {}: out {:X?}", function!(), fence, out_syncs);

                command_offset += sync_info.len() * size_of::<u32>();
            }
            */

            // TODO: might want to introduce CommandId::AllocationList for KMD implicit sync
            // Though it's probably cleaner to handle it in UMD instead
            if command_offset != cmd.len() {
                error!("{}: command validation failed: too many commands", function!());
                return Err(NtStatus(STATUS::INVALID_PARAMETER));
            }

            match body {
                CommandBody::Submit(data) => {
                    let ctx = ctx.ok_or(STATUS::INVALID_PARAMETER).inspect_err(|e| {
                        let ctx = <DeviceContext as TaggedExt>::from_handle(submit_command.hContext);
                        error!("{}: command validation failed: invalid DeviceContext: {:?}", function!(), ctx);
                    })?;
                    debug!("{}: header: {:?}, ring: {:?}, ctx: {}", function!(), hdr, hdr.ring(), ctx);

                    return chan.submit_command_buffer(engine, fence, ctx, hdr.ring(), data);
                },
                CommandBody::Fence(virtio_fence) => {
                    trace!("{}: engine {:?}, dxgk_fence {}, virtio_fence {}", function!(), engine, fence, virtio_fence);
                    chan.submit_fence(engine, fence, virtio_fence);
                    return Ok(());
                },
                _ => {
                    error!("{}: command validation failed: unsupported type: {:?}", function!(), hdr);
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                }
            }
        } else {
            let tag = Tag::from_handle(submit_command.pDmaBufferPrivateData);
            if submit_command.Flags.Paging() {
                debug!("{}: invalid dma private data for submit {:?}: {:?}", function!(), submit_command, tag);
            } else {
                error!("{}: invalid dma private data for submit {:?}: {:?}", function!(), submit_command, tag);
            }
            chan.submit_command(engine, fence, &Command::nop(), None)?;
            return Ok(())
        };

        trace!("{}: total {} commands: {:?}, allocations: {:?}", function!(), commands.len(), commands, allocations);

        if commands.len() <= 1 {
            let cmd = commands.get(0).and_then(|c| Some(c.clone())).unwrap_or(Command::nop());
            //if let Some(mut dma) = cmd.dma() {
            //    let phys_mm = unsafe { MmGetPhysicalAddress(dma.as_mut().as_mut_ptr() as _).QuadPart as u64 };
            //    let phys_sc = unsafe { submit_command.DmaBufferPhysicalAddress.QuadPart as u64 } + (submit_command.DmaBufferSubmissionStartOffset as u64);
            //    debug!("{}: dma dxgk phys addr: {:X} / {:X}", function!(), phys_mm, phys_sc);
            //    assert_eq!(phys_mm, phys_sc);
            //}

            let alloc_batch = if allocations.len() > 0 {
                Some(AllocationsBatch::new(allocations))
            } else {
                None
            };

            chan.submit_command(engine, fence, &cmd, alloc_batch)?;
        } else {
            chan.submit_command_batch(engine, fence, &commands, AllocationsBatch::new(allocations))?;
        }

        Ok(())
    }

    pub fn submit_preemption(&self, engine: Engine, preemption_fence: u32) {
        trace!("{}", function!());
        let Ok(state) = check_state!(self) else {
            return;
        };
        state.queue_handler.chan.submit_preemption(engine, preemption_fence);
    }

    pub fn allocate_full(chan: &GpuChannel, create_allocation: &mut DXGKARG_CREATEALLOCATION) -> Result<(), NtStatus> {
        trace!("{}", function!());
        if create_allocation.NumAllocations > 1 {
            error!("{}: cannot handle multiple allocations per resource yet", function!());
            return Err(NtStatus(STATUS::NO_MEMORY));
        }

        let alloc_info = unsafe { &mut *create_allocation.pAllocationInfo };

        *alloc_info.Alignment_mut() = 0;
        alloc_info.PitchAlignedSize = 0;
        alloc_info.HintedBank.set_Value(0);
        alloc_info.AllocationPriority = D3DDDI_ALLOCATIONPRIORITY_NORMAL;
        //alloc_info.Flags_mut().set_Value(0);
        alloc_info.FlagsWddm2_mut().set_Value(0);
        *alloc_info.MaximumRenamingListLength_mut() = 0;
        alloc_info.pAllocationUsageHint = null_mut();
        *alloc_info.PhysicalAdapterIndex_mut() = 0;

        alloc_info.PreferredSegment.set_Value(0);

        let submit_3d = if create_allocation.PrivateDriverDataSize >= (size_of::<CreateResource>() as u32) && let Some(res_priv) = <CreateResource as TaggedExt>::from_handle_silent(create_allocation.pPrivateDriverData) {
            let mut command_offset = 0;
            let cmd = res_priv.command_slice(create_allocation.PrivateDriverDataSize as usize);

            if cmd.len() > 0 {
                let (hdr, _) = CommandHeaderRaw::ref_from_prefix(cmd).map_err(|e| {
                    error!("{}: failed to read raw command header: {:?}", function!(), e);
                    STATUS::INVALID_PARAMETER
                })?;
                command_offset += size_of::<CommandHeaderRaw>();

                let hdr = hdr.try_as_hdr().map_err(|e| {
                    error!("{}: command validation failed: {}", function!(), e);
                    STATUS::INVALID_PARAMETER
                })?;

                if hdr.shadow_virgl() {
                    error!("{}: command validation failed: shadow virgl from usermode is not supported", function!());
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                }

                let body = hdr.command_slice().map_err(|e| {
                    error!("{}: command validation failed: {}", function!(), e);
                    STATUS::INVALID_PARAMETER
                })?;

                command_offset += hdr.size as usize;

                if command_offset != cmd.len() {
                    error!("{}: command validation failed: too many commands", function!());
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                }

                let CommandBody::Submit(data) = body else {
                    error!("{}: command validation failed: unsupported type: {:?}", function!(), hdr);
                    return Err(NtStatus(STATUS::INVALID_PARAMETER));
                };

                let mut v = Vec::<u8, _>::try_with_capacity_in(size_of::<commands::CmdSubmit3d>() + data.len(), AlignedAlloc::<PAGE_SIZE>)?;
                v.resize(size_of::<commands::CmdSubmit3d>(), 0);
                v.extend_from_slice(data);

                Some(v.into_boxed_slice())
            } else {
                None
            }

            /*
            if len > size_of::<CreateResource>() {
                match microseh::try_seh(|| -> Result<AlignedBox<[u8]>, NtStatus> {
                    let command = slice_from_raw_parts(ptr.as_ptr(), len);
                    let mut command_offset = 0;

                    let (hdr, _) = CommandHeaderRaw::ref_from_prefix(command).map_err(|e| {
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

                    if command_offset != command.len() {
                        error!("{}: command validation failed: too many commands", function!());
                        return Err(NtStatus(STATUS::INVALID_PARAMETER));
                    }

                    let CommandBody::Submit(data) = body else {
                        error!("{}: command validation failed: invalid type: {:?}", function!(), hdr);
                        return Err(NtStatus(STATUS::INVALID_PARAMETER));
                    };

                    debug!("{}: header: {:?}", function!(), hdr);

                    let mut v = Vec::<u8, _>::try_with_capacity_in(size_of::<commands::CmdSubmit3d>() + data.len(), AlignedAlloc::<PAGE_SIZE>)?;
                    v.resize(size_of::<commands::CmdSubmit3d>(), 0);
                    v.extend_from_slice(data);

                    Ok(v.into_boxed_slice())
                }) {
                    Ok(Ok(buf)) => {
                        Some(buf)
                    },
                    Ok(Err(e)) => {
                        error!("{}: invalid buffer: {:?}", function!(), e);
                        return Err(e);
                    },
                    Err(e) => {
                        error!("{}: failed to validate buffer: {:?}", function!(), e);
                        return Err(e.into());
                    },
                }
            } else {
                None
            }*/
        } else {
            None
        };

        let alloc_priv = check_buffer_size!(alloc_info.pPrivateDriverData, alloc_info.PrivateDriverDataSize, CreateAllocation)?;

        let allocation = match alloc_priv.tag() {
            ALLOCATE_BLOB_TAG => {
                let alloc_blob = unsafe { &alloc_priv.blob };

                if { alloc_blob.flags } == BlobFlag::NONE {
                    warn!("{}: creating blob {} with no flags will probably fail", function!(), { alloc_blob.id });
                }

                if { alloc_blob.mem }.contains(BlobMem::GUEST) {
                    error!("{}: guest blobs are not supported yet", function!());
                    Err(NtStatus(STATUS::NO_MEMORY))?;
                }

                let resource_id = chan.next_resource_id().ok_or(STATUS::NO_MEMORY)?;
                let allocation = Arc::try_new(Allocation::new(resource_id, submit_3d, *alloc_blob)?)?;
                //debug!("{}: allocated blob: {:?}", function!(), allocation);

                alloc_info.PreferredSegment.set_SegmentId0(MemorySegment::BlobHost3D as _);
                alloc_info.PreferredSegment.set_Direction0(false); // Allocate from start
                alloc_info.Size = alloc_blob.size;
                alloc_info.FlagsWddm2_mut().set_CpuVisible(false);
                alloc_info.FlagsWddm2_mut().set_AccessedPhysically(true);
                *alloc_info.SupportedReadSegmentSet_mut() = MemorySegment::BlobHost3D.mask();
                alloc_info.SupportedWriteSegmentSet = MemorySegment::BlobHost3D.mask();

                /*
                if { alloc_blob.flags }.contains(BlobFlag::MAPPABLE) {
                    alloc_info.PreferredSegment.set_SegmentId0(SEGMENT_ID_BLOB_MAPPABLE);
                    alloc_info.PreferredSegment.set_Direction0(false); // Allocate from start
                    alloc_info.Flags_mut().set_CpuVisible(true);
                    *alloc_info.SupportedReadSegmentSet_mut() = 1 << (SEGMENT_ID_BLOB_MAPPABLE - 1);
                    alloc_info.SupportedWriteSegmentSet = 1 << (SEGMENT_ID_BLOB_MAPPABLE - 1);
                    alloc_info.Size = alloc_blob.size;
                } else {
                    alloc_info.PreferredSegment.set_SegmentId0(SEGMENT_ID_BLOB_HOST3D);
                    alloc_info.PreferredSegment.set_Direction0(false); // Allocate from start
                    alloc_info.Size = alloc_blob.size;

                    /* FIXME: these should not be required, but see above
                     * in the query_info for DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_QUERYSEGMENT3 */
                    alloc_info.Flags_mut().set_CpuVisible(false);
                    *alloc_info.SupportedReadSegmentSet_mut() = 1 << (SEGMENT_ID_BLOB_HOST3D - 1);
                    alloc_info.SupportedWriteSegmentSet = 1 << (SEGMENT_ID_BLOB_HOST3D - 1);
                }
                */

                Ok(allocation)
            },
            ALLOCATE_3D_TAG => {
                let alloc_3d = unsafe { &alloc_priv._3d };

                let resource_id = chan.resource_create_3d(alloc_3d)?;

                let allocation = Arc::try_new(Allocation::new(resource_id, submit_3d, *alloc_3d)?)?;

                debug!("allocated 3d: {:?}", allocation);
                alloc_info.EvictionSegmentSet = 1;
                alloc_info.PreferredSegment.set_SegmentId0(MemorySegment::Aperture3D as _);
                alloc_info.PreferredSegment.set_Direction0(false);         // Allocate from start
                alloc_info.FlagsWddm2_mut().set_CpuVisible(true);
                //alloc_info.FlagsWddm2_mut().set_AccessedPhysically(true);
                *alloc_info.SupportedReadSegmentSet_mut() = MemorySegment::Aperture3D.mask();
                alloc_info.SupportedWriteSegmentSet = MemorySegment::Aperture3D.mask();
                alloc_info.Size = alloc_3d.size;

                Ok(allocation)
            },
            _ => {
                error!("unknown allocation type: {:x}", alloc_priv.tag());
                Err(NtStatus(STATUS::INVALID_PARAMETER))
            },
        }?;

        /*
        if create_allocation.Flags.Resource() {
            create_allocation.hResource = TaggedExt::into_arc_handle(Resource::new(&allocation)?);
        }
        */

        let handle = TaggedExt::into_arc_handle(allocation);
        alloc_info.hAllocation = handle;

        if alloc_priv.tag() == ALLOCATE_BLOB_TAG {
            debug!("{}: blob handle: {:?}", function!(), handle);
        }

        if create_allocation.Flags.Resource() {
            create_allocation.hResource = handle;
        }

        Ok(())
    }

    pub fn allocate(&self, create_allocation: &mut DXGKARG_CREATEALLOCATION) -> Result<(), NtStatus> {
        trace!("{}", function!());

        let state = check_state!(self)?;

        Self::allocate_full(&state.queue_handler.chan, create_allocation)
    }

    pub fn destroy_allocation_inner(chan: &GpuChannel, alloc: &Allocation) {
        let Some(id) = alloc.id() else {
            // No need to destroy resources which were never created
            return;
        };

        //warn!("Destroying allocation: {:?}", alloc);

        if alloc.is_mapped() {
            debug!("{}: unmapping blob: {:?}", function!(), alloc);
            let offset = match alloc.resource() {
               VirtioResource::_3D {..} => {
                   unreachable!()
               },
               VirtioResource::Blob {map, ..} => {
                   map.read().unwrap()
               },
            };

            /*if let Some((offset, size)) = alloc.mapped_range() {
                let (shmem_start, _) = chan.get_shmem_slice();
                let phys =  shmem_start + offset;
                let mdl = MdlOwned::from_physical_range(phys, size)?;
                //debug!("{}: Unmapped {:X} (offset {:X}) from address {:?}: {:?}", function!(), { blob_map.handle }, offset, ptr, alloc);
                mm_unmap_locked_pages(&mdl, ptr);
            }*/

            let _ = chan.resource_unmap_blob(id, offset).inspect_err(|e|
                error!("failed to unmap blob resource {}: {:?}", id, e)
            );
        }

        let _ = chan.resource_unref(id).inspect_err(|e|
            error!("failed to unref resource {}: {:?}", id, e)
        );
    }

    pub fn destroy_allocation(chan: &GpuChannel, alloc: Arc<Allocation>) {
        debug!("{}: destroying allocation: {:?} (strong: {}, weak: {})", function!(), alloc, Arc::strong_count(&alloc), Arc::weak_count(&alloc));

        let n = alloc.attached_devices_count();
        if let Some(alloc) = Arc::into_inner(alloc) {
            if n > 0 {
                warn!("{}: allocation dropped (attached devices: {}): {:?}", function!(), alloc.attached_devices_count(), alloc);
            } else {
                trace!("{}: allocation dropped (attached devices: {}): {:?}", function!(), alloc.attached_devices_count(), alloc);
            }
            Self::destroy_allocation_inner(chan, &alloc)
        } else {
            error!("{}: allocation is still in use (attached devices: {})", function!(), n);
            // Memory leak is safe
            //Self::destroy_allocation_inner(chan, &alloc)
        }
    }

    pub fn deallocate(&self, destroy_allocation: &DXGKARG_DESTROYALLOCATION) -> Result<(), NtStatus> {
        let state = check_state!(self)?;
        let chan = &state.queue_handler.chan;

        /*
        if destroy_allocation.Flags.DestroyResource() {
            if let Some(resource) = <Resource as TaggedExt>::from_arc_handle_owned(destroy_allocation.hResource) {
                drop(resource);
            } else {
                error!("{}: invalid resource handle: {:?}", function!(), destroy_allocation.hResource);
            }
        }
        */

        let allocations = slice_from_raw_parts(destroy_allocation.pAllocationList, destroy_allocation.NumAllocations as _);

        for handle in allocations {
            let Some(alloc) = <Allocation as TaggedExt>::from_arc_handle_owned((*handle) as *mut _) else {
                continue;
            };

            if alloc.is_blob() {
                debug!("{}: blob handle: {:?}", function!(), handle);
            }

            Self::destroy_allocation(&chan, alloc);
        }

        Ok(())
    }

    pub fn num_scanouts(&self) -> Result<u8, NtStatus> {
        trace!("{}", function!());

        let state = check_state!(self)?;
        Ok(state.num_scanouts)
    }

    pub fn query_child_relations(&self, child_relations: &mut [DXGK_CHILD_DESCRIPTOR]) {
        trace!("{}", function!());

        let Ok(state) = check_state!(self) else {
            return;
        };

        let is_vga = state.is_vga;
        assert!(child_relations.len() <= (state.num_scanouts as usize));

        for (i, child) in child_relations.into_iter().enumerate() {
            debug!("{}: {} -> {:?}", function!(), i, child as *const _ as HANDLE);

            *child = unsafe { zeroed() };

            child.ChildDeviceType = DXGK_CHILD_DEVICE_TYPE::TypeVideoOutput;
            child.ChildCapabilities.HpdAwareness = if is_vga {
                DXGK_CHILD_DEVICE_HPD_AWARENESS::HpdAwarenessAlwaysConnected
            } else {
                DXGK_CHILD_DEVICE_HPD_AWARENESS::HpdAwarenessInterruptible
            };

            let video_output = unsafe { &mut child.ChildCapabilities.Type.VideoOutput};

            // WTF rust, this works:
            // child.ChildCapabilities.Type.VideoOutput.InterfaceTechnology = D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_INTERNAL;
            // and this doesn't:
            // *(&mut child.ChildCapabilities.Type.VideoOutput.InterfaceTechnology) = D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_INTERNAL;

            video_output.InterfaceTechnology = if is_vga {
                D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_INTERNAL
            } else {
                D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_OTHER
            };
            video_output.MonitorOrientationAwareness = D3DKMDT_MONITOR_ORIENTATION_AWARENESS::D3DKMDT_MOA_NONE;
            video_output.SupportsSdtvModes = false as _;
            child.AcpiUid = 0;
            child.ChildUid = i as _;
        }
    }

    pub fn is_supported_vidpn(&self, is_supported_vidpn: &mut DXGKARG_ISSUPPORTEDVIDPN) -> Result<(), NtStatus> {
        trace!("{}", function!());

        let state = check_state!(self)?;

        let vidpn = state.queue_handler.dxgk_interface().query_vidpn_interface(is_supported_vidpn.hDesiredVidPn)?;
        let topology = vidpn.get_topology()?;

        for source in 0..state.num_scanouts {
            match topology.get_num_paths_from_source(source as D3DDDI_VIDEO_PRESENT_SOURCE_ID) {
                Ok(num_paths) => {
                    if num_paths > state.num_scanouts as _ {
                        is_supported_vidpn.IsVidPnSupported = true as _;
                        return Ok(());
                    }
                },
                Err(NtStatus(STATUS::GRAPHICS_SOURCE_NOT_IN_TOPOLOGY)) => {
                    continue;
                },
                Err(e) => {
                    return Err(e);
                },
            };
        }

        is_supported_vidpn.IsVidPnSupported = true as _;
        Ok(())
    }

    pub fn get_edid(&self, scanout: usize) -> Result<&[u8], NtStatus> {
        trace!("{}", function!());

        let state = check_state!(self)?;
        if scanout >= state.num_scanouts as usize {
            error!("{}: invalid scanout {}: out of range (max {})", function!(), scanout, state.num_scanouts);
            return Err(NtStatus(STATUS::INVALID_PARAMETER));
        }

        let edid = match &state.outputs[scanout] {
            Some(info) => &info.edid.data[0..core::cmp::min(256, info.edid.size as usize)],
            None => {
                error!("{}: no EDID for scanout {}", function!(), scanout);
                return Err(NtStatus(STATUS::GRAPHICS_CHILD_DESCRIPTOR_NOT_SUPPORTED));
            }
        };

        Ok(edid)
    }

    pub fn recommend_monitor_modes(&self, recommend_monitor_modes: &DXGKARG_RECOMMENDMONITORMODES) -> Result<(), NtStatus> {
        trace!("{}", function!());

        let scanout = recommend_monitor_modes.VideoPresentTargetId as usize;
        let state = check_state!(self)?;
        if scanout >= state.num_scanouts as usize {
            error!("{}: invalid scanout {}: out of range (max {})", function!(), scanout, state.num_scanouts);
            return Err(NtStatus(STATUS::INVALID_PARAMETER));
        }

        let modes = match &state.outputs[scanout] {
            Some(info) => info.modes.iter(),
            None => {
                error!("{}: no modes for scanout {}", function!(), scanout);
                return Err(NtStatus(STATUS::GRAPHICS_CHILD_DESCRIPTOR_NOT_SUPPORTED));
            }
        };

        let mode_interface = MonitorSourceModeSetInterface::new(recommend_monitor_modes.hMonitorSourceModeSet, recommend_monitor_modes.pMonitorSourceModeSetInterface);
        for (i, mode) in modes.enumerate() {
            //let mode_info = mode_interface.create_new_mode_info().inspect_err(|e| error!("{}: failed to create mode info: {:?}", function!(), e))?;
            let mut mode_info = mode_interface.create_new_mode_info_autorelease().inspect_err(|e| error!("{}: failed to create mode info: {:?}", function!(), e))?;

            mode.fill_video_signal_info(&mut mode_info.VideoSignalInfo);

            mode_info.Origin = D3DKMDT_MONITOR_CAPABILITIES_ORIGIN::D3DKMDT_MCO_DRIVER;
            mode_info.ColorBasis = D3DKMDT_COLOR_BASIS::D3DKMDT_CB_SRGB;
            mode_info.ColorCoeffDynamicRanges.FirstChannel = 8;
            mode_info.ColorCoeffDynamicRanges.SecondChannel = 8;
            mode_info.ColorCoeffDynamicRanges.ThirdChannel = 8;
            mode_info.ColorCoeffDynamicRanges.FourthChannel = 8;
            mode_info.Preference = if i == 0 {
                D3DKMDT_MODE_PREFERENCE::D3DKMDT_MP_PREFERRED
            } else {
                D3DKMDT_MODE_PREFERENCE::D3DKMDT_MP_NOTPREFERRED
            };

            mode_interface.add_mode_autorelease(mode_info)?;
            //mode_interface.add_mode(mode_info)?;
        }

        Ok(())
    }

    pub fn targets_for_source(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID) -> Result<&[D3DDDI_VIDEO_PRESENT_TARGET_ID], NtStatus> {
        trace!("{}", function!());
        let state = check_state!(self)?;

        if source as usize >= state.source_target_map.len() {
            error!("{}: invalid source {}: out of range (max {})", function!(), source, state.source_target_map.len());
            return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_SOURCE));
        }

        let targets = &state.source_target_map[source as usize];

        if targets.len() == 0 {
            return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_SOURCE));
        }

        Ok(targets)
    }

    pub fn move_cursor(&self, set_pointer_position: &DXGKARG_SETPOINTERPOSITION) -> Result<(), NtStatus> {
        trace!("{}", function!());

        let state = check_state!(self)?;
        let chan = &state.queue_handler.chan;
        let targets = self.targets_for_source(set_pointer_position.VidPnSourceId)?;

        if set_pointer_position.Flags.Visible() {
            for &scanout in targets {
                state.outputs[scanout as usize].as_ref().unwrap().move_cursor(scanout, &chan, set_pointer_position.X, set_pointer_position.Y)?;
            }
        } else {
            for &scanout in targets {
                state.outputs[scanout as usize].as_ref().unwrap().hide_cursor(scanout, &chan, state.empty_cursor.as_ref().unwrap())?;
            }
        }

        Ok(())
    }

    pub fn update_cursor(&self, set_pointer_shape: &DXGKARG_SETPOINTERSHAPE) -> Result<(), NtStatus> {
        let state = check_state!(self)?;
        let chan = &state.queue_handler.chan;
        let targets = self.targets_for_source(set_pointer_shape.VidPnSourceId)?;

        let height = set_pointer_shape.Height as usize;
        let stride = set_pointer_shape.Pitch as usize;

        if set_pointer_shape.Flags.Color() {
            trace!("{}: color cursor: w {}, h {}, xhot {}, yhot {}", function!(), set_pointer_shape.Width, set_pointer_shape.Height, set_pointer_shape.XHot, set_pointer_shape.YHot);

            let pixels = slice_from_raw_parts(set_pointer_shape.pPixels as *const u8, height * stride);

            //info!("{}: pixels: {:?}", function!(), pixels);

            for &scanout in targets {
                state.outputs[scanout as usize].as_ref().unwrap().update_cursor_bgra(scanout, &chan, set_pointer_shape.XHot, set_pointer_shape.YHot, pixels, stride)?;
            }
        } else if set_pointer_shape.Flags.Monochrome() {
            trace!("{}: monochrome cursor: w {}, h {}, stride {}, xhot {}, yhot {}", function!(), set_pointer_shape.Width, set_pointer_shape.Height, set_pointer_shape.Pitch, set_pointer_shape.XHot, set_pointer_shape.YHot);

            let mask = slice_from_raw_parts(set_pointer_shape.pPixels as *const u8, height * stride * 2);

            for &scanout in targets {
                state.outputs[scanout as usize].as_ref().unwrap().update_cursor_mono(scanout, &chan, set_pointer_shape.XHot, set_pointer_shape.YHot, mask, stride)?;
            }
        } else if set_pointer_shape.Flags.MaskedColor() {
            trace!("{}: masked color cursor: w {}, h {}, stride {}, xhot {}, yhot {}", function!(), set_pointer_shape.Width, set_pointer_shape.Height, set_pointer_shape.Pitch, set_pointer_shape.XHot, set_pointer_shape.YHot);

            let masked_color = slice_from_raw_parts(set_pointer_shape.pPixels as *const u8, height * stride);

            for &scanout in targets {
                state.outputs[scanout as usize].as_ref().unwrap().update_cursor_masked_color(scanout, &chan, set_pointer_shape.XHot, set_pointer_shape.YHot, masked_color, stride)?;
            }

            //for y in 0..height {
            //    let row = &pixels[y * stride..(y + 1) * stride];
            //    info!("{}: {:?}", y, row);
            //}
        } else {
            warn!("{}: unsupported pointer format: {:?}", function!(), set_pointer_shape.Flags);
        }

        Ok(())
    }

    pub fn control_interrupt(&self, interrupt: DXGK_INTERRUPT_TYPE, enabled: bool) {
        trace!("{}", function!());
        match interrupt {
            DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_CRTC_VSYNC => {
                self.flip_timer.as_ref().unwrap().inner.vsync_enabled.store(enabled, Ordering::SeqCst);
            },
            _ => {
                warn!("{}: not implemented: {:?}", function!(), interrupt);
            },
        }
    }

    pub fn queue_scanout(&self, source: D3DDDI_VIDEO_PRESENT_SOURCE_ID, alloc: &Arc<Allocation>, addr: u64) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let state = check_state!(self)?;
        let targets = self.targets_for_source(source)?;

        for &scanout in targets {
            let weak = Arc::downgrade(alloc);
            if let Some((dropped_addr, dropped_alloc)) = state.outputs[scanout as usize].as_ref().unwrap().flipq.force_push((weak, addr)) {
                warn!("{}: dropped scanout: {:?}", function!(), dropped_alloc);
            }
        }

        Ok(())
    }

    pub fn update_active_present_path(&self, present_path: &D3DKMDT_VIDPN_PRESENT_PATH) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let state = check_state!(self)?;

        check_vidpn_present_path(state.num_scanouts, present_path)
    }

    // This can be mut as per the WDDM threading and synchronization model
    pub fn commit_vidpn(&mut self, commit: &DXGKARG_COMMITVIDPN) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let state = check_state_mut!(self)?;
        if commit.AffectedVidPnSourceId >= state.num_scanouts as _ {
            error!("{}: invalid scanout {}: out of range (max {})", function!(), commit.AffectedVidPnSourceId, state.num_scanouts);
            return Err(NtStatus(STATUS::INVALID_PARAMETER));
        }

        if commit.Flags.PathPoweredOff() != 0 {
            return Ok(());
        }

        let vidpn = state.queue_handler.dxgk_interface().query_vidpn_interface(commit.hFunctionalVidPn).inspect_err(|e|
            error!("{}: failed to get vidpn interface: {:?}", function!(), e)
        )?;

        let topology = vidpn.get_topology().inspect_err(|e|
            error!("{}: failed to get topology: {:?}", function!(), e)
        )?;

        let num_paths = topology.get_num_paths().inspect_err(|e|
            error!("{}: failed to get number of paths: {:?}", function!(), e)
        )?;

        trace!("{}: num paths: {}", function!(), num_paths);

        if num_paths == 0 {
            return Ok(());
        }

        let source_mode_set = vidpn.acquire_source_mode_set_autorelease(commit.AffectedVidPnSourceId).inspect_err(|e|
            error!("{}: failed to acquire source mode set: {:?}", function!(), e)
        )?;

        let Some(pinned_mode_info) = source_mode_set.acquire_pinned_mode_info_autorelease().inspect_err(|e|
            error!("{}: failed to acquire pinned mode info: {:?}", function!(), e)
        )? else {
            error!("{}: failed to acquire pinned mode info: ", function!());
            return Err(NtStatus(STATUS::INVALID_PARAMETER));
        };

        check_vidpn_source_mode(&pinned_mode_info).inspect_err(|e|
            error!("{}: pinned mode info is invalid: {:?}", function!(), e)
        )?;

        let (width, height) = {
            let graphics = unsafe { &pinned_mode_info.Format.Graphics };
            (graphics.PrimSurfSize.cx, graphics.PrimSurfSize.cy)
        };

        debug!("{}: mode {}x{}", function!(), width, height);

        let num_paths_for_source = topology.get_num_paths_from_source(commit.AffectedVidPnSourceId).inspect_err(|e|
            error!("{}: failed to get number of paths for source {}: {:?}", function!(), commit.AffectedVidPnSourceId, e)
        )?;

        debug!("{}: num paths for source {}: {}", function!(), commit.AffectedVidPnSourceId, num_paths_for_source);

        let mut targets = SmallVec::new();
        for path in 0..num_paths_for_source {
            let target = topology.enum_path_targets_from_source(commit.AffectedVidPnSourceId, path).inspect_err(|e|
                error!("{}: failed to get path target {} for source {}: {:?}", function!(), path, commit.AffectedVidPnSourceId, e)
            )?;
            debug!("{}: path {} for source {}: target {}", function!(), path, commit.AffectedVidPnSourceId, target);

            let path_info = topology.acquire_path_info_autorelease(commit.AffectedVidPnSourceId, target).inspect_err(|e|
                error!("{}: failed to get path info for source {}, target {}: {:?}", function!(), commit.AffectedVidPnSourceId, target, e)
            )?;

            check_vidpn_present_path(state.num_scanouts, &path_info).inspect_err(|e|
                error!("{}: invalid path info for source {}, target {}: {:?}", function!(), commit.AffectedVidPnSourceId, target, e)
            )?;

            targets.push(target);
            let output = state.outputs[target as usize].as_mut().ok_or(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_TARGET)?;

            let Some(current_mode) = output.modes.iter().position(|mode| mode.width == width && mode.height == height) else {
                error!("{}: mode {}x{} does not exist on target {}", function!(), width, height, target);
                return Err(NtStatus(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_SOURCE_MODE));
            };

            output.current_mode = Some(current_mode);
        }

        state.source_target_map[commit.AffectedVidPnSourceId as usize] = targets;

        Ok(())
    }

    pub fn enum_cofunc_modality(&self, enum_cofunc_modality: &DXGKARG_ENUMVIDPNCOFUNCMODALITY) -> Result<(), NtStatus> {
        trace!("{}", function!());
        let state = check_state!(self)?;

        let vidpn = state.queue_handler.dxgk_interface().query_vidpn_interface(enum_cofunc_modality.hConstrainingVidPn).inspect_err(|e|
            error!("{}: failed to get vidpn interface: {:?}", function!(), e)
        )?;

        let topology = vidpn.get_topology().inspect_err(|e|
            error!("{}: failed to get topology: {:?}", function!(), e)
        )?;

        let mut path_info_iter = VidPnPathInfoIter::new(&topology)?;
        loop {
            let Some(path) = path_info_iter.try_next()? else {
                break;
            };

            debug!("{}: source {} -> target {} ", function!(), path.VidPnSourceId, path.VidPnTargetId);

            let output = state.outputs[path.VidPnTargetId as usize].as_ref().ok_or(STATUS::GRAPHICS_INVALID_VIDEO_PRESENT_TARGET)?;

            if (enum_cofunc_modality.EnumPivotType != D3DKMDT_ENUMCOFUNCMODALITY_PIVOT_TYPE::D3DKMDT_EPT_VIDPNSOURCE) || (enum_cofunc_modality.EnumPivot.VidPnSourceId != path.VidPnSourceId) {
                let source_mode_set = vidpn.acquire_source_mode_set_autorelease(path.VidPnSourceId).inspect_err(|e|
                    error!("{}: failed to acquire source mode set for source {}: {:?}", function!(), path.VidPnSourceId, e)
                )?;

                let pinned_source_mode_info = source_mode_set.acquire_pinned_mode_info_autorelease().inspect_err(|e|
                    error!("{}: failed to acquire pinned mode info: {:?}", function!(), e)
                )?;

                if pinned_source_mode_info.is_none() {
                    debug!("{}: dropping old source modeset without a pinned mode", function!());

                    drop(pinned_source_mode_info);
                    drop(source_mode_set);

                    debug!("{}: creating new source modeset", function!());
                    let source_mode_set = vidpn.create_new_source_mode_set_autorelease(path.VidPnSourceId)?;

                    for mode in output.modes.iter() {
                        debug!("{}: creating new mode: {:?}", function!(), mode);

                        let mut mode_info = source_mode_set.create_new_mode_info_autorelease()?;
                        mode_info.Type = D3DKMDT_VIDPN_SOURCE_MODE_TYPE::D3DKMDT_RMT_GRAPHICS;
                        mode.fill_graphics_info(unsafe { &mut mode_info.Format.Graphics });
                        debug!("{}: adding new mode", function!());
                        source_mode_set.add_mode_autorelease(mode_info)?;
                    }

                    debug!("{}: assigning new source modeset", function!());
                    vidpn.assign_source_mode_set_autorelease(path.VidPnSourceId, source_mode_set)?;
                } else {
                    debug!("{}: current source modeset has a pinned mode already", function!());
                }
            }

            if (enum_cofunc_modality.EnumPivotType != D3DKMDT_ENUMCOFUNCMODALITY_PIVOT_TYPE::D3DKMDT_EPT_VIDPNTARGET) || (enum_cofunc_modality.EnumPivot.VidPnTargetId != path.VidPnTargetId) {
                let target_mode_set = vidpn.acquire_target_mode_set_autorelease(path.VidPnTargetId).inspect_err(|e|
                    error!("{}: failed to acquire target mode set for target {}: {:?}", function!(), path.VidPnTargetId, e)
                )?;

                let pinned_target_mode_info = target_mode_set.acquire_pinned_mode_info_autorelease().inspect_err(|e|
                    error!("{}: failed to acquire pinned mode info: {:?}", function!(), e)
                )?;

                if pinned_target_mode_info.is_none() {
                    debug!("{}: creating new target modeset with modes", function!());

                    drop(pinned_target_mode_info);
                    drop(target_mode_set);

                    let target_mode_set = vidpn.create_new_target_mode_set_autorelease(path.VidPnTargetId)?;

                    for (i, mode) in output.modes.iter().enumerate() {
                        let mut mode_info = target_mode_set.create_new_mode_info_autorelease()?;

                        mode.fill_video_signal_info(&mut mode_info.VideoSignalInfo);

                        let preference = if i == 0 {
                            D3DKMDT_MODE_PREFERENCE::D3DKMDT_MP_PREFERRED
                        } else {
                            D3DKMDT_MODE_PREFERENCE::D3DKMDT_MP_NOTPREFERRED
                        };

                        mode_info.Preference = preference;

                        //unsafe {
                        //    mode_info.__bindgen_anon_1.__bindgen_anon_1.set_Preference(preference);
                        //}

                        target_mode_set.add_mode_autorelease(mode_info)?;
                    }

                    vidpn.assign_target_mode_set_autorelease(path.VidPnTargetId, target_mode_set)?;
                } else {
                    debug!("{}: current target modeset has a pinned mode already", function!());
                }
            }

            let mut new_present_path: Option<D3DKMDT_VIDPN_PRESENT_PATH> = None;

            if (enum_cofunc_modality.EnumPivotType != D3DKMDT_ENUMCOFUNCMODALITY_PIVOT_TYPE::D3DKMDT_EPT_SCALING) || (enum_cofunc_modality.EnumPivot.VidPnSourceId != path.VidPnSourceId) || (enum_cofunc_modality.EnumPivot.VidPnTargetId != path.VidPnTargetId) {
                if path.ContentTransformation.Scaling == D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_UNPINNED {
                    debug!("{}: updating scaling support for the present path", function!());

                    new_present_path = new_present_path.or(Some(*path));

                    let content_transformation = &mut new_present_path.as_mut().unwrap().ContentTransformation;

                    content_transformation.ScalingSupport = unsafe { zeroed() };
                    content_transformation.ScalingSupport.set_Identity(true as _);
                    content_transformation.ScalingSupport.set_Centered(true as _);
                }
            }

            if (enum_cofunc_modality.EnumPivotType != D3DKMDT_ENUMCOFUNCMODALITY_PIVOT_TYPE::D3DKMDT_EPT_ROTATION) || (enum_cofunc_modality.EnumPivot.VidPnSourceId != path.VidPnSourceId) || (enum_cofunc_modality.EnumPivot.VidPnTargetId != path.VidPnTargetId) {
                if path.ContentTransformation.Rotation == D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_UNPINNED {
                    debug!("{}: updating rotation support for the present path", function!());

                    new_present_path = new_present_path.or(Some(*path));
                    let content_transformation = &mut new_present_path.as_mut().unwrap().ContentTransformation;

                    content_transformation.RotationSupport = unsafe { zeroed() };
                    content_transformation.RotationSupport.set_Identity(true as _);
                    content_transformation.RotationSupport.set_Rotate90(false as _);
                    content_transformation.RotationSupport.set_Rotate180(false as _);
                    content_transformation.RotationSupport.set_Rotate270(false as _);
                    content_transformation.RotationSupport.set_Offset0(true as _);
                    content_transformation.RotationSupport.set_Offset90(false as _);
                    content_transformation.RotationSupport.set_Offset180(false as _);
                    content_transformation.RotationSupport.set_Offset270(false as _);
                }
            }

            if let Some(new_present_path) = new_present_path {
                topology.update_path_support_info(&new_present_path).inspect_err(|e|
                    error!("{}: failed to update path info: {:?}", function!(), e)
                )?;
            }

            // //path_guard = topology.acquire_next_path_info_autorelease(path)?;
            // let next_guard = topology.acquire_next_path_info_autorelease(path)?;
            // // Now drop current_guard (it goes out of scope at the end of the iteration)
            // // Then set path_guard to next_guard for the next loop
            // path_guard = next_guard;
        }
        Ok(())
    }

}
