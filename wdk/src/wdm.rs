#![allow(clashing_extern_declarations)]

use core::{
    alloc::{
        GlobalAlloc,
        Layout
    },
    cell::UnsafeCell,
    convert::Infallible,
    mem::{
        zeroed,
        transmute,
    },
    marker::PhantomPinned,
    ptr::{
        NonNull,
        addr_of,
        null_mut,
    },
    fmt,
    pin::Pin,
    ops::{
        Add,
        BitOr,
        BitOrAssign,
    },
    cmp::{
        max,
        min,
    },
};

extern crate alloc;

use alloc::boxed::Box;

use pin_init::*;

mod sys {
    pub type NTSTATUS = u32;
    pub type PNTSTATUS = *mut NTSTATUS;
    pub type PCNTSTATUS = *const NTSTATUS;

    include!(concat!(env!("OUT_DIR"), "/wdm-bindings.rs"));

    #[link(name = "ntoskrnl")]
    unsafe extern "C" {}

    #[link(name = "ntstrsafe")]
    unsafe extern "C" {}

    #[link(name = "hal")]
    unsafe extern "C" {}

    #[link(name = "bufferoverflowfastfailk")]
    unsafe extern "C" {}
}

pub use sys::*;

use log::error;

#[macro_export]
macro_rules! wdm_call_status_unchecked {
    ($func:ident ( $($args:expr),* )) => {{
        let status = winresult::NtStatus::from(unsafe { $func($($args),*) });
        if status.is_success() && status != winresult::STATUS::TIMEOUT {
            Ok(())
        } else {
            //error!(concat!("func ", stringify!($func), " failed: {:?}"), status);
            Err(status)
        }
    }};
}

#[macro_export]
macro_rules! assert_irql {
    ($op:tt $irql:ident) => {{
        let current_irql = unsafe { KeGetCurrentIrql() } as u32;
        if !(current_irql $op $irql) {
            error!("current irql: {}, limit: {} {}", current_irql, stringify!($op), $irql);
        }

        assert!(current_irql $op $irql);
    }};
}

#[macro_export]
macro_rules! wdm_call_status {
    //($irql:ident | $func:ident ( $($args:expr),* )) => {{
    //    assert_irql!(<= $irql);
    //    wdm_call_status_unchecked!($func($($args),*))
    //}};
    ($op:tt $irql:ident | $func:ident ( $($args:expr),* )) => {{
        assert_irql!($op $irql);
        wdm_call_status_unchecked!($func($($args),*))
    }};
}

const ALLOC_TAG: u32 = u32::from_ne_bytes(*b"VIRT");

const POOL_FLAG_UNINITIALIZED: u64 = 0x0000000000000002u64;
const POOL_FLAG_NON_PAGED:     u64 = 0x0000000000000040u64;

const DEFAULT_ALIGN: usize = size_of::<*mut u8>() * 2;

pub struct WdkAllocator;

fn default_layout_alignment_ok(layout: &Layout) -> bool {
    layout.align() <= if layout.size() < PAGE_SIZE as _ { DEFAULT_ALIGN } else { PAGE_SIZE as _ }
}

unsafe impl GlobalAlloc for WdkAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = if default_layout_alignment_ok(&layout) {
            assert_irql!(<= DISPATCH_LEVEL);
            unsafe {
                ExAllocatePool2(POOL_FLAG_UNINITIALIZED | POOL_FLAG_NON_PAGED, layout.size() as _, ALLOC_TAG)
            }
        } else {
            let size = layout.size() + layout.align();
            let mask = !(layout.align() - 1);

            assert_irql!(<= DISPATCH_LEVEL);
            let p = unsafe {
                ExAllocatePool2(POOL_FLAG_UNINITIALIZED | POOL_FLAG_NON_PAGED, size as _, ALLOC_TAG)
            };
            let p = if !p.is_null() {
                let q = ((p as usize & mask) + layout.align()) as *mut *mut u8;
                unsafe {
                    q.sub(1).write(p as _);
                }
                q as _
            } else {
                p
            };
            //log::trace!("GlobalAlloc::allocate: allocated memory {:?} for layout {:?}", p, layout);
            p
        };
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        ptr as _
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let p = if default_layout_alignment_ok(&layout) {
            ptr as _
        } else {
            let q: *mut *mut u8 = ptr as _;
            unsafe { q.sub(1).read() }
        };

        assert_irql!(<= DISPATCH_LEVEL);
        unsafe {
            ExFreePoolWithTag(p as _, ALLOC_TAG);
        }
        //log::trace!("GlobalAlloc::dealloc: freed memory {:?} for layout {:?}", p, layout);
    }
}

pub struct NtTime(Option<i64>);

impl NtTime {
    pub const INFINITE: NtTime = NtTime::infinite();

    pub const fn absolute_ms(ms: u32) -> Self {
        let v = ms as i64 * 10_000;

        Self(Some(v))
    }

    pub const fn relative_ms(ms: u32) -> Self {
        let v = ms as i64 * -10_000;

        Self(Some(v))
    }

    pub const fn fps(fps: u8) -> Self {
        let v = 10_000_000i64 / fps as i64;

        Self(Some(v))
    }

    pub const fn infinite() -> Self {
        Self(None)
    }

    pub const fn get(&mut self) -> PLARGE_INTEGER {
        if let Some(timeout) = &mut self.0 {
            (timeout as PLONGLONG).cast()
        } else {
            null_mut()
        }
    }

    fn get_max_irql_for_wait(&self) -> u32 {
        if let Some(timeout) = self.0 && timeout == 0 {
            DISPATCH_LEVEL
        } else {
            APC_LEVEL
        }
    }
}

pub fn ke_delay_execution_thread(mut timeout: NtTime) -> Result<(), winresult::NtStatus> {
    wdm_call_status!(<= APC_LEVEL | KeDelayExecutionThread(MODE::KernelMode.0 as _, false as _, timeout.get()))?;
    Ok(())
}

pub fn ke_get_current_irql() -> u32 {
    unsafe { KeGetCurrentIrql() as u32 }
}

pub enum EventType {
    Notification,
    Synchronization,
}

impl Into<EVENT_TYPE> for EventType {
    fn into(self) -> EVENT_TYPE {
        match self {
            EventType::Notification => EVENT_TYPE::NotificationEvent,
            EventType::Synchronization => EVENT_TYPE::SynchronizationEvent,
        }
    }
}

#[pin_data]
#[repr(transparent)]
pub struct KeEvent {
    #[pin]
    event: UnsafeCell<KEVENT>,
    #[pin]
    pin: PhantomPinned,
}

const _: () = assert!(core::mem::size_of::<KeEvent>() == core::mem::size_of::<KEVENT>());

unsafe impl Sync for KeEvent {}

impl KeEvent {
    pub fn new(event_type: EventType, signalled: bool) -> impl PinInit<Self> {
        unsafe {
            pin_init_from_closure(move |slot: *mut Self| {
                KeInitializeEvent((*slot).event.get(), event_type.into(), signalled as _);
                Ok(())
            })
        }
    }

    pub fn from_usermode_handle(handle: HANDLE) -> Result<NonNull<Self>, winresult::NtStatus> {
        let mut ptr: PKEVENT = null_mut();

        wdm_call_status!(<= PASSIVE_LEVEL | ObReferenceObjectByHandle(handle, SYNCHRONIZE | EVENT_MODIFY_STATE, *ExEventObjectType, MODE::UserMode.0 as _, &mut ptr as *mut _ as *mut *mut _, null_mut()))?;
        NonNull::new(ptr as _).ok_or(winresult::STATUS::UNSUCCESSFUL)
    }

    pub unsafe fn destroy(ptr: NonNull<Self>) {
        unsafe {
            assert_irql!(<= DISPATCH_LEVEL);
            ObfDereferenceObject(ptr.as_ref().event.get() as _);
        }
    }

    pub fn from_raw(raw: PKEVENT) -> &'static Self {
        unsafe { &*raw.cast() }
    }

    pub fn wait_usermode(&self, mut timeout: NtTime) -> Result<(), winresult::NtStatus> {
        let max_irql = timeout.get_max_irql_for_wait();
        wdm_call_status!(<= max_irql | KeWaitForSingleObject(self.event.get() as _, KWAIT_REASON::UserRequest, MODE::KernelMode.0 as i8, false as _, timeout.get()))?;
        Ok(())
    }

    pub fn wait(&self, mut timeout: NtTime) -> Result<(), winresult::NtStatus> {
        let max_irql = timeout.get_max_irql_for_wait();
        wdm_call_status!(<= max_irql | KeWaitForSingleObject(self.event.get() as _, KWAIT_REASON::Executive, MODE::KernelMode.0 as i8, false as _, timeout.get()))?;
        Ok(())
    }

    pub fn set(&self) -> bool {
        assert_irql!(<= DISPATCH_LEVEL);
        unsafe {
            //KeSetEvent(self.event.get() as _, IO_NO_INCREMENT as _, false as _) != 0
            KeSetEvent(self.event.get() as _, IO_VIDEO_INCREMENT as _, false as _) != 0
        }
    }

    pub fn clear(&self) {
        assert_irql!(<= DISPATCH_LEVEL);
        unsafe {
            KeClearEvent(self.event.get() as _);
        }
    }

    pub fn is_signalled(&self) -> bool {
        assert_irql!(<= DISPATCH_LEVEL);
        unsafe {
            KeReadStateEvent(self.event.get() as _) != 0
        }
    }

    pub fn get(self: Pin<&Self>) -> PKEVENT {
        self.event.get()
    }
}

impl fmt::Debug for KeEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let signalled = self.is_signalled();
        f.debug_struct("KeEvent")
            .field("signalled", &signalled)
            .finish()
    }
}

fn box_into_inner<T>(boxed: Box<T>) -> T {
    *boxed
}

#[repr(u32)]
pub enum ThreadPriority {
    Low = LOW_PRIORITY,
    LowRealtime = LOW_REALTIME_PRIORITY,
    High = HIGH_PRIORITY,
    Max = MAXIMUM_PRIORITY,
}

#[repr(transparent)]
pub struct KeThread(PKTHREAD);

impl KeThread {
    pub fn spawn<T: Send>(f: fn(&mut T), ctx: *mut T) -> Result<Self, winresult::NtStatus> {
        extern "C" fn thread_trampoline_with_more_stack<T>(context: PVOID) {
            let (f, ctx) = unsafe {
                box_into_inner(Box::from_raw(transmute::<_, *mut (fn(&mut T), &mut T)>(context)))
            };

            f(ctx);
        }

        extern "C" fn thread_trampoline<T>(context: PVOID) {
            assert_irql!(<= APC_LEVEL);
            let status = winresult::NtStatus::from(unsafe {
               KeExpandKernelStackAndCalloutEx(Some(thread_trampoline_with_more_stack::<T>), context, MAXIMUM_EXPANSION_SIZE as _, true as _, null_mut())
            });

            let _ = KeThread::terminate(status);
        }

        let mut handle = null_mut();
        let context = Box::into_raw(Box::try_new((f, ctx)).map_err(|_| winresult::STATUS::NO_MEMORY)?);
        wdm_call_status!(<= PASSIVE_LEVEL | PsCreateSystemThread(&mut handle, 0, null_mut(), null_mut(), null_mut(), Some(thread_trampoline::<T>), context as _))?;

        let mut thread = null_mut();
        assert_irql!(<= DISPATCH_LEVEL);
        unsafe {
            ObReferenceObjectByHandle(handle, THREAD_ALL_ACCESS, null_mut(), MODE::KernelMode.0 as _, &mut thread, null_mut());
        }

        assert_irql!(== PASSIVE_LEVEL);
        unsafe {
            ZwClose(handle);
        }

        Ok(Self(thread as PKTHREAD))
    }

    pub fn set_priority(&self, priority: ThreadPriority) {
        assert_irql!(<= PASSIVE_LEVEL);
        unsafe {
            KeSetPriorityThread(self.0, priority as _);
        }
    }

    pub fn terminate(status: winresult::NtStatus) -> Result<(), winresult::NtStatus> {
        wdm_call_status!(<= PASSIVE_LEVEL | PsTerminateSystemThread(status.to_u32()))?;
        Ok(())
    }

    pub fn join(&self, mut timeout: NtTime) -> Result<(), winresult::NtStatus> {
        let max_irql = timeout.get_max_irql_for_wait();
        wdm_call_status!(<= max_irql | KeWaitForSingleObject(self.0 as _, KWAIT_REASON::Executive, MODE::KernelMode.0 as _, false as _, timeout.get()))?;

        Ok(())
    }
}

impl Drop for KeThread {
    fn drop(&mut self) {
        assert_irql!(<= DISPATCH_LEVEL);
        // TODO: is this required?
        unsafe {
            ObfDereferenceObject(self.0 as _);
        }
    }
}

//pub fn spawn<T: Send>(f: fn(&mut T), ctx: *mut T) -> Result<Self, winresult::NtStatus> {
pub struct KeTimer<T: Send>(PEX_TIMER, *mut (fn(&T), *const T));

impl<T: Send> KeTimer<T> {
    pub fn new_hires(f: fn(&T), ctx: *const T) -> Result<Self, winresult::NtStatus> {
        extern "C" fn timer_trampoline<T>(timer: PEX_TIMER, context: PVOID) {
            let (f, ctx) = unsafe { *(context as *mut _ as *const (fn(&T), *const T)) };
            f(unsafe { &*ctx });
        }

        let context = Box::into_raw(Box::try_new((f, ctx as *const _)).map_err(|_| winresult::STATUS::NO_MEMORY)?);

        assert_irql!(<= DISPATCH_LEVEL);
        let timer = unsafe { ExAllocateTimer(Some(timer_trampoline::<T>), context as *mut _, EX_TIMER_HIGH_RESOLUTION) };
        if timer.is_null() {
            return Err(winresult::STATUS::UNSUCCESSFUL);
        }

        Ok(Self(timer, context))
    }

    pub fn start_periodic(&self, period: NtTime) -> Result<(), winresult::NtStatus> {
        let period = period.0.ok_or(winresult::STATUS::INVALID_PARAMETER)?;

        assert_irql!(<= DISPATCH_LEVEL);
        unsafe { ExSetTimer(self.0, -period, period, null_mut()) == 0 };

        Ok(())
    }

    pub fn cancel(&self) -> bool {
        assert_irql!(<= DISPATCH_LEVEL);
        unsafe { ExCancelTimer(self.0, null_mut()) != 0 }
    }
}

impl<T: Send> Drop for KeTimer<T> {
    fn drop(&mut self) {
        self.cancel();

        drop(unsafe { Box::from_raw(self.1) });

        assert_irql!(<= APC_LEVEL);
        unsafe {
            let mut parameters = core::mem::zeroed();
            ExDeleteTimer(self.0, true as _, true as _, &mut parameters as *mut _);
        }
    }
}

/*
pub struct KeTimer(PEX_TIMER, PVOID);

impl KeTimer {
    pub fn new_hires(f: &dyn Fn()) -> Result<Self, winresult::NtStatus> {
        extern "C" fn timer_trampoline(timer: PEX_TIMER, context: PVOID) {
            let f = unsafe { &*(context as *mut _ as *const dyn Fn()) };
            f();
        }

        let context: *mut dyn Fn() = Box::into_raw(Box::try_new(f).map_err(|_| winresult::STATUS::NO_MEMORY)?);
        const _: () = assert!(size_of::<*mut &dyn Fn()>() == 8);

        assert_irql!(<= DISPATCH_LEVEL);
        let timer = unsafe { ExAllocateTimer(Some(timer_trampoline), context as *mut _, EX_TIMER_HIGH_RESOLUTION) };
        if timer.is_null() {
            return Err(winresult::STATUS::UNSUCCESSFUL);
        }

        Ok(Self(timer, context as *mut _))
    }

    pub fn start_periodic(&self, period: NtTime) -> Result<(), winresult::NtStatus> {
        assert_irql!(<= DISPATCH_LEVEL);
        let period = period.0.ok_or(winresult::STATUS::INVALID_PARAMETER)?;

        unsafe { ExSetTimer(self.0, -period, period, null_mut()) == 0 };

        Ok(())
    }

    pub fn cancel(&self) -> Result<(), winresult::NtStatus> {
        assert_irql!(<= DISPATCH_LEVEL);
        if unsafe { ExCancelTimer(self.0, null_mut()) == 0 } {
            return Err(winresult::STATUS::UNSUCCESSFUL);
        }

        Ok(())
    }
}

impl Drop for KeTimer {
    fn drop(&mut self) {
        let _ = self.cancel().inspect_err(|e|
            error!("failed to cancel timer: {:?}", e)
        );

        unsafe { drop(Box::from_raw(self.1 as *mut &dyn Fn())) };

        assert_irql!(<= APC_LEVEL);
        unsafe {
            let mut parameters = core::mem::zeroed();
            ExDeleteTimer(self.0, true as _, true as _, &mut parameters as *mut _);
        }
    }
}
*/

#[macro_export]
macro_rules! count {
    () => { 0 };
    ($first:expr $(, $rest:expr)*) => {
        1 + $crate::count!($($rest),*)
    };
}

#[macro_export]
macro_rules! select_if_chain {
    ($index:expr, $counter:expr, $handle:expr => $block:expr) => {
        if $index == $counter {
            $block
        } else {
            unreachable!()
        }
    };
    ($index:expr, $counter:expr, $handle:expr => $block:expr, $($rest_handle:expr => $rest_block:expr),+) => {
        if $index == $counter {
            $block
        } else {
            $crate::select_if_chain!($index, $counter + 1, $($rest_handle => $rest_block),+)
        }
    };
}

#[macro_export]
macro_rules! select {
    (
        $($handle:expr => $block:expr),+ $(,)?
        ;
        error($err:ident) => $err_block:expr $(,)?
    ) => {{
        use core::{
            marker::PhantomPinned,
            mem::MaybeUninit,
        };
        use wdk::{
            wdm::{
                NtTime,
                PVOID,
                KeWaitForMultipleObjects,
                WAIT_TYPE,
                KWAIT_REASON,
                KWAIT_BLOCK,
                THREAD_WAIT_OBJECTS,
                MAXIMUM_WAIT_OBJECTS,
                MODE,
                KeGetCurrentIrql,
                APC_LEVEL,
            },
            count,
            select_if_chain,
            assert_irql,
        };
        use winresult::STATUS;

        const N_OBJECTS: usize = count!($($handle),+);
        const WAIT_OBJECT_0: u32 = STATUS::WAIT_0.to_u32();
        const WAIT_OBJECT_N: u32 = STATUS::WAIT_0.to_u32() + (N_OBJECTS as u32);

        const _: () = assert!(N_OBJECTS as u32 <= MAXIMUM_WAIT_OBJECTS);

        let mut handles: [PVOID; N_OBJECTS] = [ $($handle),+ ].map(|h| h.get() as PVOID);
        let mut blocks = pin!(([MaybeUninit::<KWAIT_BLOCK>::uninit(); N_OBJECTS], PhantomPinned));

        assert_irql!(<= APC_LEVEL);
        let status = unsafe {
            KeWaitForMultipleObjects(
                handles.len() as _, handles.as_mut_ptr(),
                WAIT_TYPE::WaitAny as _,
                KWAIT_REASON::Executive,
                MODE::KernelMode.0 as _,
                false as _,
                NtTime::INFINITE.get(),
                blocks.get_unchecked_mut().0.as_ptr() as *mut _
            )
        };

        if status >= WAIT_OBJECT_0 && status < WAIT_OBJECT_N {
            let index = (status - WAIT_OBJECT_0) as usize;
            select_if_chain!(index, 0, $($handle => $block),+)
        } else {
            let $err = winresult::NtStatus::from(status);
            $err_block
        }
    }};
}

/*pub struct IrqlGuard {
    old_irql: KIRQL,
}

impl IrqlGuard {
    pub fn raise() -> Option<Self> {
        //let old_irql = unsafe { KfRaiseIrql(HIGH_LEVEL as _) };
        let old_irql = unsafe { KeGetCurrentIrql() };
        if old_irql <= DISPATCH_LEVEL as _ {
            let old_irql = unsafe { KfRaiseIrql(DISPATCH_LEVEL as _) };
            Some(IrqlGuard { old_irql })
        } else {
            None
        }
    }
}

impl Drop for IrqlGuard {
    fn drop(&mut self) {
        unsafe { KeLowerIrql(self.old_irql) };
    }
}*/

pub fn io_get_remaining_stack_size() -> usize {
    let mut top = 0u64;
    let mut bottom = 0u64;
    unsafe { IoGetStackLimits(&mut bottom, &mut top); }

    let (size, overflow) = (top as usize).overflowing_sub(bottom as usize);
    if overflow {
        error!("overflow subtraction: {} - {}", top, bottom);
    }
    size
}

pub fn ke_query_performance_counter() -> (u64, u64) {
    unsafe {
        let mut freq = LARGE_INTEGER { QuadPart: 0 };
        let ts = KeQueryPerformanceCounter(&mut freq as *mut _);
        (ts.QuadPart as _, freq.QuadPart as _)
    }
}

#[inline]
pub fn mm_get_physical_address(base: PVOID) -> u64 {
    unsafe { MmGetPhysicalAddress(base).QuadPart as u64 }
}

#[inline]
fn mm_get_mdl_pfn_array(mdl: *mut MDL) -> PPFN_NUMBER {
    unsafe {
        let addr = mdl.add(1);
        transmute(addr)
    }
}

#[inline]
fn mm_get_mdl_virtual_address(mdl: *mut MDL) -> PVOID {
    unsafe {
        let addr = (*mdl).StartVa;
        let offs = (*mdl).ByteOffset;

        addr.byte_add(offs as _)
    }
}

#[inline]
fn mm_get_mdl_byte_count(mdl: *mut MDL) -> usize {
    unsafe {
        let count = (*mdl).ByteCount;
        count as _
    }
}

fn byte_offset(ptr: PVOID) -> usize {
    let addr: usize = unsafe { transmute(ptr) };
    addr & ((PAGE_SIZE - 1) as usize)
}

#[inline]
fn mm_get_n_pages(mdl: *mut MDL) -> usize {
    let addr = mm_get_mdl_virtual_address(mdl);
    let size = mm_get_mdl_byte_count(mdl);

    (byte_offset(addr) + size + ((PAGE_SIZE - 1) as usize)) >> PAGE_SHIFT
}

#[repr(transparent)]
pub struct MdlRef(pub *mut MDL);

impl MdlRef {
    pub fn physical_pages(&self) -> &'static [u64] {
        let addr = mm_get_mdl_pfn_array(self.0);
        let len = mm_get_n_pages(self.0);

        unsafe {
            core::slice::from_raw_parts(addr, len)
        }
    }
}


#[repr(transparent)]
pub struct MdlOwned(pub *mut MDL);

impl MdlOwned {
    pub fn from_io_physical_range(start: u64, size: u64) -> Result<Self, winresult::NtStatus> {
        let mut mdl = null_mut();
        let mut addr = MM_PHYSICAL_ADDRESS_LIST {
            PhysicalAddress: PHYSICAL_ADDRESS {
                QuadPart: start as _,
            },
            NumberOfBytes: size,
        };

        wdm_call_status!(<= DISPATCH_LEVEL | MmAllocateMdlForIoSpace(&mut addr as _, 1, &mut mdl as _))?;

        Ok(Self(mdl))
    }

    pub fn from_physical_range2(start: u64, size: u64) -> Result<Self, winresult::NtStatus> {
        let mut mdl = null_mut();
        let mut addr = MM_PHYSICAL_ADDRESS_LIST {
            PhysicalAddress: PHYSICAL_ADDRESS {
                QuadPart: start as _,
            },
            NumberOfBytes: size,
        };

        wdm_call_status!(<= DISPATCH_LEVEL | MmAllocateMdlForIoSpace(&mut addr as _, 1, &mut mdl as _))?;

        Ok(Self(mdl))
    }
    pub fn physical_pages(&self) -> &[u64] {
        MdlRef(self.0).physical_pages()
    }
}

pub fn mm_map_locked_pages_specify_cache(mdl: &MdlOwned, user: bool, cache: MEMORY_CACHING_TYPE, addr: Option<NonNull<u8>>) -> Option<NonNull<u8>> {
    let mode = if user {
        assert_irql!(<= APC_LEVEL);
        MODE::UserMode
    } else {
        assert_irql!(<= DISPATCH_LEVEL);
        MODE::KernelMode
    };

    let addr = addr.and_then(|a| Some(a.as_ptr())).unwrap_or(null_mut());
    let priority = _MM_PAGE_PRIORITY::NormalPagePriority.0 as u32 | MdlMappingNoExecute;

    NonNull::new(unsafe { MmMapLockedPagesSpecifyCache(mdl.0, mode.0 as _, cache, addr as _, false as _, priority) as _ })
}

impl Drop for MdlOwned {
    fn drop(&mut self) {
        unsafe { IoFreeMdl(self.0) }
    }
}

pub fn mm_unmap_locked_pages(mdl: &MdlOwned, addr: NonNull<u8>) {
    unsafe { MmUnmapLockedPages(addr.as_ptr() as _, mdl.0) };
}

pub fn mm_map_io_space(addr: u64, len: u64, cache: MEMORY_CACHING_TYPE) -> Option<NonNull<u8>> {
    let addr = PHYSICAL_ADDRESS {
        QuadPart: addr as _,
    };

    assert_irql!(<= DISPATCH_LEVEL);
    NonNull::new(unsafe { MmMapIoSpace(addr, len, cache) as *mut _ })
}

pub fn mm_unmap_io_space(addr: NonNull<u8>, len: u64) {
    assert_irql!(<= DISPATCH_LEVEL);
    unsafe { MmUnmapIoSpace(addr.as_ptr() as *mut _, len) };
}
