use core::{
    alloc::Layout,
    any::type_name,
    mem::{
        size_of,
        offset_of,
        transmute,
        take,
        replace,
        ManuallyDrop,
        MaybeUninit,
    },
    ffi::c_void,
    num::{
        NonZero,
    },
    sync::{
        atomic::{
            AtomicBool,
            AtomicU32,
            AtomicU64,
            Ordering,
        },
    },
    pin::{
        Pin,
        pin,
    },
    ptr::NonNull,
    cell::UnsafeCell,
    convert::Infallible,
    ops::{
        Deref,
        DerefMut,
    },
};

use alloc::{
    alloc::{
        Allocator,
        Global,
    },
    boxed::Box,
    collections::{
        VecDeque,
        BTreeSet,
    },
    sync::Arc,
    vec::Vec,
};

//use offset_allocator;
use smallvec::SmallVec;
use pin_init::*;
use zerocopy::*;
use crossbeam::{
    queue::*,
    utils::*,
};
use winresult::STATUS;
use virtio_drivers::{
    config::*,
    device::{
        gpu::*,
    },
    transport::{
        pci::{
            VirtioCapabilityInfo,
            PciTransport,
            bus::*,
        },
        DeviceType,
        Transport,
        InterruptStatus,
    },
    queue::*,
    BufferDirection,
    Hal,
    PhysAddr,
};
use spin::{
    mutex::SpinMutex,
    rwlock::RwLock,
};

use crate::adapter::*;
use crate::uapi::*;
use crate::allocation::*;
use crate::command::{*, Command};

use crate::map_virtio_error;
use crate::function;

use wdk::{
    wdm::{
        ThreadPriority,
        KeEvent,
        KeThread,
        EventType,
        NtTime,
        UINT,
        NTSTATUS,
        io_get_remaining_stack_size,
        ke_query_performance_counter,
    },
    select,
    wdm_call_status,
    dxgkrnl::DXGKRNL_INTERFACE,
    //ke_delay_execution_thread,
};

const PAGE_SIZE: usize = wdk::wdm::PAGE_SIZE as usize;

#[derive(Clone, Copy, Debug, Default)]
pub struct CapsetInfo {
    pub version: u32,
    pub size: u32,
}

//#[derive(Debug)]
//enum Message {
//    Control(commands::GpuControlCommand),
//    Cursor(commands::GpuCursorCommand),
//}

const MAX_COMMAND_SIZE:  usize = 120;
const MAX_RESPONSE_SIZE: usize = 56;
// TODO: check control command sizes
//const _: () = assert!(size_of::<commands::COMMAND_TYPE>() < MAX_COMMAND_SIZE);

const CURSOR_COMMAND_SIZE: usize = 56;
const CURSOR_RESPONSE_SIZE: usize = 24;
const _: () = assert!(size_of::<commands::UpdateCursor>() <= CURSOR_COMMAND_SIZE);
const _: () = assert!(size_of::<commands::CtrlHeader>() <= CURSOR_RESPONSE_SIZE);

#[pin_data]
struct BlockingBufferInner<T> {
    #[pin]
    event: KeEvent,
    cancel: AtomicBool,
    data: SpinMutex<Option<T>>,
}

impl<T> BlockingBufferInner<T> {
    fn init() -> impl PinInit<Self, NtStatus> {
        pin_init!(Self {
            event <- KeEvent::new(EventType::Notification, false),
            cancel: AtomicBool::new(false),
            data: SpinMutex::new(None),
        }? NtStatus)
    }
}

// TODO: async resource / context creation
#[repr(transparent)]
#[derive(Clone)]
struct BlockingBuffer<T>(Pin<Arc<BlockingBufferInner<T>>>);

impl<T> BlockingBuffer<T> {
    pub fn new() -> Result<Self, NtStatus> {
        Ok(Self(Arc::try_pin_init(BlockingBufferInner::init())?))
    }

    pub fn set(&self) {
        self.0.event.set();
    }

    pub fn wait(&self, timeout: NtTime) -> Result<(), NtStatus> {
        self.0.event.wait(timeout)?;
        Ok(())
    }

    pub fn cancel(&self) {
        self.0.cancel.store(true, Ordering::SeqCst);
    }

    pub fn cancelled(&self) -> bool {
        self.0.cancel.load(Ordering::SeqCst)
    }

    pub fn write(&self, data: T) {
        self.0.data.lock().replace(data);
    }

    pub fn read(&self) -> Option<T> {
        self.0.data.lock().take()
    }
}

//#[repr(transparent)]
//#[derive(Clone)]
//struct CancellationSignal(Arc<AtomicBool>);
//
//impl CancellationSignal {
//    pub fn new() -> Self {
//        Self(Arc::new(AtomicBool::new(false)))
//    }
//
//    pub fn cancel(&self) {
//        self.0.store(true, Ordering::SeqCst);
//    }
//
//    pub fn cancelled(&self) -> bool {
//        self.0.load(Ordering::SeqCst)
//    }
//}

#[derive(Clone, Debug)]
pub struct AllocationsBatch(SmallVec<[Arc<DeviceSpecificAllocation>; 2]>);

impl AllocationsBatch {
    pub fn new(allocations: SmallVec<[Arc<DeviceSpecificAllocation>; 2]>) -> Self {
        //for alloc in &allocations {
        //    trace!("{}: marking {:?} busy", function!(), alloc.alloc);
        //    alloc.alloc.mark_busy();
        //}

        Self(allocations)
    }
}

impl Drop for AllocationsBatch {
    fn drop(&mut self) {
        for alloc in &*self.0 {
            let device = alloc.device.upgrade().unwrap();

            if let Some(alloc) = alloc.alloc.upgrade() {
                trace!("{}: marking {:?} free", function!(), alloc);
                alloc.mark_free();
                //if true {
                //    alloc.debug_print_attached_pages(device.dxgk_interface());
                //}
            } else {
                error!("{}: trying to drop already freed allocation", function!());
            }
        }
    }
}

enum Callback<const MAX_INLINE: usize> {
    None,
    SetEvent(BlockingBuffer<MaybeInlineBuffer<MAX_INLINE>>),
    //SetEvent(NonNull<KeEvent>, Option<NonNull<MaybeUninit<MaybeInlineBuffer<MAX_INLINE>>>>, CancellationSignal),
    FreeContextId(NonZero<u32>),
    FreeResourceId(NonZero<u32>),
    DmaCompleted(Engine, u32),
    DmaCompletedWithAllocations(Engine, u32, AllocationsBatch),
    DmaCompletedBatched(Engine, u32, Arc<AllocationsBatch>),
    //AsyncCallback(dyn FnOnce() -> ()),
    // Async(...),
}
const _: () = assert!(size_of::<Callback<128>>() == 32);

impl<const MAX_INLINE: usize> Callback<MAX_INLINE> {
    fn as_dma_completed(&self) -> Option<(Engine, u32)> {
        //if true {
        if false {
            None
        } else {
            match self {
                Self::DmaCompleted(engine, fence) => Some((*engine, *fence)),
                Self::DmaCompletedWithAllocations(engine, fence, _) => Some((*engine, *fence)),
                Self::DmaCompletedBatched(engine, fence, _) => Some((*engine, *fence)),
                _ => None,
            }
        }
    }
}


impl<const MAX_INLINE: usize> Default for Callback<MAX_INLINE> {
    fn default() -> Self {
        Self::None
    }
}

//impl Callback {
//    // pub fn blocking(event: ...) -> Self
//}

const CONTROL_QUEUE_SIZE: usize = 256;
const CURSOR_QUEUE_SIZE:  usize = 16;

const CONTROL_FAST_QUEUE_SIZE: usize = CONTROL_QUEUE_SIZE * 3;
const CURSOR_FAST_QUEUE_SIZE:  usize = CURSOR_QUEUE_SIZE * 4;

const FAST_QUEUE_SIZE: usize = 2048;

/*
pub fn new_bytes<T: IntoBytes + FromBytes + KnownLayout>() -> Result<(Layout, NonNull<[u8]>), NtStatus> {
    let layout = Layout::new<T>();

    //let ptr = Box::into_raw(T::new_box_zeroed()?);
    //Ok(unsafe {
    //    Box::from_raw(from_raw_parts_mut(transmute::<*mut T, *mut u8>(ptr), len))
    //})
}
*/

#[derive(Clone, Copy)]
pub struct AlignedAlloc<const N: usize>;

unsafe impl<const N: usize> Allocator for AlignedAlloc<N> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, core::alloc::AllocError> {
        Global.allocate(layout.align_to(N).unwrap())
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe {
            Global.deallocate(ptr, layout.align_to(N).unwrap())
        }
    }
}

pub type AlignedBox<T> = Box<T, AlignedAlloc<PAGE_SIZE>>;

#[repr(align(8))]
#[derive(Clone)]
enum MaybeInlineBuffer<const MAX_INLINE: usize> {
    Inline {
        pad: [u8; 5],
        len: u16,
        data: [u8; MAX_INLINE]
    },
    Boxed {
        data: AlignedBox<[u8]>,
    },
    Dma {
        data: NonNull<[u8]>,
    },
    // TODO: find an efficient way to submit guest-only allocations as buffers
    //IoVec {
    //    pad: [u8; 5],
    //    len: u16,
    //    header: [u8; 32],
    //    alloc: [NonNull<[u8]>; 5],
    //},
    /*
    DmaOwned {
        dma: Dma,
    },
    */
}

impl<const MAX_INLINE: usize> MaybeInlineBuffer<MAX_INLINE> {
    const fn can_inline<T: Sized>() -> bool {
        size_of::<T>() <= MAX_INLINE
    }

    const fn can_handle<T: Sized>() -> bool {
        size_of::<T>() <= PAGE_SIZE
    }

    fn try_new_zeroed<T: IntoBytes + KnownLayout>() -> Result<Self, NtStatus> {
        Ok(if Self::can_inline::<T>() {
            MaybeInlineBuffer::Inline {
                pad: [0; _],
                len: size_of::<T>() as u16,
                data: [0; _],
            }
        } else if Self::can_handle::<T>() {
            let data = Box::<[u8], _>::try_new_zeroed_slice_in(size_of::<T>(), AlignedAlloc)?;
            MaybeInlineBuffer::Boxed {
                data: unsafe { data.assume_init() },
            }
        } else {
            error!("cannot handle multi-page boxes yet: type {} with size {} is too big)", type_name::<T>(), size_of::<T>());
            return Err(NtStatus(STATUS::NO_MEMORY));
        })
    }

    fn try_new_zeroed_size(size: usize) -> Result<Self, NtStatus> {
        Ok(if size <= MAX_INLINE {
            MaybeInlineBuffer::Inline {
                pad: [0; _],
                len: size as _,
                data: [0; _],
            }
        } else if size <= PAGE_SIZE {
            let data = Box::<[u8], _>::try_new_zeroed_slice_in(size, AlignedAlloc)?;
            MaybeInlineBuffer::Boxed {
                data: unsafe { data.assume_init() },
            }
        } else {
            error!("cannot handle multi-page boxes yet: size {} is too big)", size);
            return Err(NtStatus(STATUS::NO_MEMORY));
        })
    }

    fn try_from_hdr_with_body<T: IntoBytes + KnownLayout + Immutable>(hdr: T, body: &[u8]) -> Result<Self, NtStatus> {
        let size = size_of::<T>() + body.len();

        Ok(if size <= MAX_INLINE {
            MaybeInlineBuffer::Inline {
                pad: [0; _],
                len: size as _,
                data: {
                    let mut data = [0; _];
                    hdr.write_to_prefix(&mut data)?;
                    (&mut data[size_of::<T>()..size]).copy_from_slice(body);
                    data
                }
            }
        } else if size <= PAGE_SIZE {
            let mut v = Vec::<u8, _>::try_with_capacity_in(size, AlignedAlloc::<PAGE_SIZE>)?;
            v.extend_from_slice(hdr.as_bytes());
            v.extend_from_slice(body);

            MaybeInlineBuffer::Boxed {
                data: v.into_boxed_slice(),
            }
        } else {
            error!("cannot handle multi-page boxes yet: size {} is too big)", size);
            return Err(NtStatus(STATUS::NO_MEMORY));
        })
    }

    fn try_from_value<T: IntoBytes + KnownLayout + Immutable>(value: T) -> Result<Self, NtStatus> {
        let size = size_of::<T>();

        Ok(if size <= MAX_INLINE {
            MaybeInlineBuffer::Inline {
                pad: [0; _],
                len: size as _,
                data: {
                    let mut data = [0; _];
                    value.write_to_prefix(&mut data)?;
                    data
                }
            }
        } else if size <= PAGE_SIZE {
            let mut v = Vec::<u8, _>::try_with_capacity_in(size, AlignedAlloc::<PAGE_SIZE>)?;
            v.extend_from_slice(value.as_bytes());

            MaybeInlineBuffer::Boxed {
                data: v.into_boxed_slice(),
            }
        } else {
            error!("cannot handle multi-page boxes yet: size {} is too big)", size);
            return Err(NtStatus(STATUS::NO_MEMORY));
        })
    }

    fn as_mut(&mut self) -> &mut [u8] {
        match self {
            MaybeInlineBuffer::Inline {len, data, ..} => {
                let len = *len as usize;
                &mut data[0..len]
            },
            MaybeInlineBuffer::Boxed {data} => {
                &mut data[0..]
            },
            MaybeInlineBuffer::Dma {data} => {
                unsafe {
                    data.as_mut()
                }
            },
            //MaybeInlineBuffer::IoVec { .. } => todo!(),
        }
    }

    fn as_ref(&self) -> &[u8] {
        match self {
            MaybeInlineBuffer::Inline {len, data, ..} => {
                let len = *len as usize;
                &data[0..len]
            },
            MaybeInlineBuffer::Boxed {data} => {
                &data[0..]
            },
            MaybeInlineBuffer::Dma {data} => {
                unsafe {
                    data.as_ref()
                }
            },
            //MaybeInlineBuffer::IoVec { .. } => todo!(),
        }
    }

    fn into_boxed(self, strip_prefix: usize) -> Result<Box<[u8]>, NtStatus> {
        let data = Box::try_from(&self.as_ref()[strip_prefix..])?;
        Ok(data)
    }
}

fn box_try_init_in<T, E, A: Allocator>(init: impl Init<T, E>, alloc: A) -> Result<Box<T, A>, NtStatus> where NtStatus: From<E> {
    let mut uninit: Box<MaybeUninit<T>, A> = Box::try_new_uninit_in(alloc)?;
    let slot = uninit.as_mut_ptr();
    unsafe { init.__init(slot)? };

    Ok(unsafe { uninit.assume_init() })
}

fn slice_in_one_page(slice: &[u8]) -> bool {
    if slice.is_empty() {
        return true;
    }

    let start_addr = slice.as_ptr() as usize;
    let end_addr = start_addr + slice.len() - 1;

    // Compute the page index for the start and end addresses.
    let start_page = start_addr / PAGE_SIZE;
    let end_page = end_addr / PAGE_SIZE;

    start_page == end_page
}


struct Buffer<const IN: usize, const OUT: usize> {
    input: MaybeInlineBuffer<IN>,
    output: MaybeInlineBuffer<OUT>,
    callback: Callback<OUT>,
}
//const _: () = assert!(size_of::<Option<Buffer<128, 64>>>() == 240);

struct Buffers<const BUFFERS: usize, const IN: usize, const OUT: usize> {
    inputs: AlignedBox<[Option<MaybeInlineBuffer<IN>>; BUFFERS]>,
    outputs: AlignedBox<[Option<MaybeInlineBuffer<OUT>>; BUFFERS]>,
    callbacks: [Callback<OUT>; BUFFERS],
    indices: [Option<usize>; BUFFERS],
}

impl<const BUFFERS: usize, const IN: usize, const OUT: usize> Buffers<BUFFERS, IN, OUT> {
    pub fn new() -> impl Init<Self, NtStatus> {
        init!(Self {
            inputs: box_try_init_in(init_array_from_fn(|i| None), AlignedAlloc)?,
            outputs: box_try_init_in(init_array_from_fn(|i| None), AlignedAlloc)?,
            callbacks <- init_array_from_fn(|i| Callback::None),
            indices <- init_array_from_fn(|i| None),
        }? NtStatus)
    }
}

// FIXME: check for race conditions
struct ArraySegQueue<T> {
    fast: ArrayQueue<T>,
    slow: SegQueue<T>,
}

impl<T> ArraySegQueue<T> {
    fn new(cap: usize) -> Self {
        Self {
            fast: ArrayQueue::new(cap),
            slow: SegQueue::new(),
        }
    }

    fn push(&self, value: T) {
        if let Err(value) = self.fast.push(value) {
            self.slow.push(value);
        }
    }

    fn pop(&self) -> Option<T> {
        if let Some(v) = self.slow.pop() {
            if let Some(v) = self.fast.force_push(v) {
                Some(v)
            } else {
                self.fast.pop()
            }
        } else {
            self.fast.pop()
        }
    }
}

#[pin_data]
struct AsyncQueue<T> {
    #[pin]
    event: KeEvent,

    q: ArraySegQueue<T>,
}

impl<T> AsyncQueue<T> {
    fn try_new(cap: usize) -> Result<Pin<Arc<Self>>, NtStatus> {
        let q: Pin<Arc<Self>> = Arc::try_pin_init(Self::init(cap))?;
        Ok(q)
    }

    fn init(cap: usize) -> impl PinInit<Self, NtStatus> {
        pin_init!(Self {
            event <- KeEvent::new(EventType::Synchronization, false),
            q: ArraySegQueue::new(cap),
        }? NtStatus)
    }

    fn push(&self, value: T) {
        self.q.push(value);
        self.event.set();
    }

    fn pop(&self) -> Option<T> {
        self.q.pop()
    }

    fn event(self: &Pin<Arc<Self>>) -> Pin<&KeEvent> {
        unsafe { self.as_ref().map_unchecked(|q| &q.event) }
    }
}

struct Queue<const Q: u16, const SIZE: usize, const FAST: usize, const IN: usize, const OUT: usize> {
    queue: VirtQueue<DxgkInterface, SIZE>,
    buffers: Buffers<SIZE, IN, OUT>,
    free: ArrayQueue<usize>,
    //nop: SegQueue<u32>,

    //chan: Pin<Arc<AsyncQueue<Buffer<IN, OUT>>>>,
    chan: QueueChannel<IN, OUT>,

    //last_submitted_fence: AtomicU64,
}

const DEFAULT_BLOCKING_TIMEOUT: NtTime = NtTime::relative_ms(5_000);

#[derive(Clone)]
struct QueueChannel<const IN: usize, const OUT: usize>(Pin<Arc<AsyncQueue<Buffer<IN, OUT>>>>);

impl<const IN: usize, const OUT: usize> QueueChannel<IN, OUT> {
    fn try_new(cap: usize) -> Result<Self, NtStatus> {
        Ok(Self(AsyncQueue::try_new(cap)?))
    }

    pub fn request_async_buf(&self, input: MaybeInlineBuffer<IN>, output: MaybeInlineBuffer<OUT>, callback: Callback<OUT>) -> Result<(), NtStatus> {
        self.0.push(Buffer { input, output, callback });
        Ok(())
    }

    pub fn request_async_into_buf<Req: IntoBytes + Immutable + KnownLayout>(&self, req: Req, output: MaybeInlineBuffer<OUT>, callback: Callback<OUT>) -> Result<(), NtStatus> {
        let input = MaybeInlineBuffer::try_from_value(req)?;
        self.request_async_buf(input, output, callback)
    }

    pub fn request_async<Req: IntoBytes + Immutable + KnownLayout, Rsp: FromBytes + IntoBytes + KnownLayout>(&self, req: Req, callback: Callback<OUT>) -> Result<(), NtStatus> {
        let output = MaybeInlineBuffer::try_new_zeroed::<Rsp>()?;
        self.request_async_into_buf(req, output, callback)
    }

    pub fn request_blocking_buf(&self, input: MaybeInlineBuffer<IN>, output: MaybeInlineBuffer<OUT>, timeout: NtTime) -> Result<MaybeInlineBuffer<OUT>, NtStatus> {
        let block = BlockingBuffer::<MaybeInlineBuffer<OUT>>::new()?;
        let callback = Callback::SetEvent(block.clone());

        //let signal = CancellationSignal::new();
        //let mut response = pin!(MaybeUninit::<MaybeInlineBuffer<OUT>>::uninit());
        //stack_pin_init!(let block = KeEvent::new(EventType::Notification, false));
        //let callback = unsafe {
        //    Callback::SetEvent(
        //        NonNull::new_unchecked(block.as_mut().get_unchecked_mut()),
        //        Some(NonNull::new_unchecked(response.as_mut().get_unchecked_mut())),
        //        signal.clone()
        //    )
        //};

        self.request_async_buf(input, output, callback)?;

        block.wait(timeout).inspect_err(|e| {
            block.cancel();
            error!("failed to wait for request: {:?}", e)
        })?;

        block.read().ok_or(NtStatus(STATUS::IO_DEVICE_ERROR)).inspect_err(|_|
            error!("failed to get for responce: data is None")
        )
    }

    pub fn request_blocking_into_buf<Req: IntoBytes + Immutable + KnownLayout>(&self, req: Req, output: MaybeInlineBuffer<OUT>) -> Result<MaybeInlineBuffer<OUT>, NtStatus> {
        let input = MaybeInlineBuffer::try_from_value(req)?;
        self.request_blocking_buf(input, output, DEFAULT_BLOCKING_TIMEOUT)
    }

    pub fn request_blocking<Req: IntoBytes + Immutable + KnownLayout, Rsp: FromBytes + IntoBytes + KnownLayout>(&self, req: Req) -> Result<Rsp, NtStatus> {
        let output = MaybeInlineBuffer::try_new_zeroed::<Rsp>()?;
        let response = self.request_blocking_into_buf(req, output)?;
        let rsp = Rsp::read_from_prefix(response.as_ref()).unwrap().0;

        Ok(rsp)
    }

    pub fn request_blocking_with_timeout<Req: IntoBytes + Immutable + KnownLayout, Rsp: FromBytes + IntoBytes + KnownLayout>(&self, req: Req, timeout: NtTime) -> Result<Rsp, NtStatus> {
        let input = MaybeInlineBuffer::try_from_value(req)?;
        let output = MaybeInlineBuffer::try_new_zeroed::<Rsp>()?;
        let response = self.request_blocking_buf(input, output, timeout)?;
        let rsp = Rsp::read_from_prefix(response.as_ref()).unwrap().0;

        Ok(rsp)
    }

    pub fn request_async_dma<Rsp: FromBytes + IntoBytes + KnownLayout>(&self, dma: NonNull<[u8]>, engine: Engine, fence: u32, allocations: Option<AllocationsBatch>) -> Result<(), NtStatus> {
        let input = MaybeInlineBuffer::Dma { data: dma };
        let output = MaybeInlineBuffer::try_new_zeroed::<Rsp>()?;

        let callback = if let Some(allocations) = allocations {
            Callback::DmaCompletedWithAllocations(engine, fence, allocations)
        } else {
            Callback::DmaCompleted(engine, fence)
        };

        self.request_async_buf(input, output, callback)
    }

    // Caller MUST call the corresponding KeEvent::set after queuing all the commands
    pub fn request_async_dma_batched<Rsp: FromBytes + IntoBytes + KnownLayout>(&self, dma: NonNull<[u8]>, engine: Engine, fence: u32, allocations: Arc<AllocationsBatch>) -> Result<(), NtStatus> {
        let input = MaybeInlineBuffer::Dma { data: dma };
        let output = MaybeInlineBuffer::try_new_zeroed::<Rsp>()?;
        let callback = Callback::DmaCompletedBatched(engine, fence, allocations);

        self.0.push(Buffer { input, output, callback });
        Ok(())
    }

    pub fn pop_queued_request(&self) -> Option<Buffer<IN, OUT>> {
        self.0.pop()
    }

    pub fn get_queue_event(&self) -> Pin<&KeEvent> {
        self.0.event()
    }
}

impl<const Q: u16, const SIZE: usize, const FAST: usize, const IN: usize, const OUT: usize> Queue<Q, SIZE, FAST, IN, OUT> {
    const IN_OK: () = assert!(size_of::<MaybeInlineBuffer<IN>>().is_power_of_two());
    const OUT_OK: () = assert!(size_of::<MaybeInlineBuffer<OUT>>().is_power_of_two());

    pub fn new(pci_transport: &mut PciTransport, access_platform: bool, indirect: bool, event_idx: bool) -> impl Init<Self, NtStatus> {
        let _ = Self::IN_OK;
        let _ = Self::OUT_OK;

        let init_free = unsafe {
            init_from_closure(move |slot: *mut ArrayQueue<usize>| {
                slot.write_volatile(ArrayQueue::new(SIZE));
                Ok::<_, Infallible>(())
            })
        }.chain(|free| Ok(for i in 0..SIZE {
            free.push(i).unwrap();
        }));

        let init_queue = VirtQueue::<DxgkInterface, SIZE>::init(
            pci_transport,
            Q,
            indirect,
            event_idx,
            access_platform,
        ).chain(|queue| Ok(queue.set_dev_notify(true)));

        init!(Self {
            queue <- init_queue,
            buffers <- Buffers::new(),
            chan: QueueChannel::try_new(FAST)?,
            //fast: ArrayQueue::new(FAST),
            //slow: SegQueue::new(),
            //nop: SegQueue::new(),
            free <- init_free,
            //last_submitted_fence: AtomicU64::new(0),
        }? NtStatus)
    }

    fn push_to_device(&mut self, pci_transport: &mut PciTransport, i: usize, buffer: Buffer<IN, OUT>) -> Result<(), NtStatus> {
        //trace!("{}", function!());

        let input_is_dma = matches!(buffer.input, MaybeInlineBuffer::Dma{..});
        let output_is_dma = matches!(buffer.output, MaybeInlineBuffer::Dma{..});

        // DEBUG
        if false {
            let data = buffer.input.as_ref();
            let hdr = commands::CtrlHeader::read_from_prefix(data).unwrap().0;
            debug!("{}: -- starting processing command (len {}): {:?}", function!(), data.len(), hdr);
        }

        // DEBUG
        if false && input_is_dma {
            let data = buffer.input.as_ref();
            let hdr = commands::CtrlHeader::read_from_prefix(data).unwrap().0;
            if hdr.hdr_type == commands::Command::SUBMIT_3D {
                let hdr = commands::CmdSubmit3d::read_from_prefix(data).unwrap().0;
                if hdr.size == 0 {
                    warn!("{}: timestamp {:?}", function!(), ke_query_performance_counter());
                }
            }
        }

        // DEBUG
        if false && input_is_dma {
            let data = buffer.input.as_ref();
            let hdr = commands::CtrlHeader::read_from_prefix(data).unwrap().0;
            debug!("{}: -- starting decoding command (len {}): {:?}", function!(), data.len(), hdr);

            /*if hdr.hdr_type == commands::Command::SUBMIT_3D {
                match &buffer.callback {
                    Callback::DmaCompleted(u32) => {
                        error!("no allocations!");
                    }
                    Callback::DmaCompletedWithAllocations(_, allocations) => {
                        warn!(" --- allocations:");
                        for alloc in allocations.0.iter() {
                            warn!("{:?}", alloc.alloc);
                        }
                    },
                    Callback::DmaCompletedBatched(_, allocations) => {
                        warn!(" --- allocations:");
                        for alloc in allocations.0.iter() {
                            warn!("{:?}", alloc.alloc);
                        }
                    },
                    _ => {},
                }
                let hdr = commands::CmdSubmit3d::read_from_prefix(data).unwrap().0;
                let body = &data[size_of::<commands::CmdSubmit3d>()..];

                let mut offset = 0;
                let typed_data = unsafe {
                    core::slice::from_raw_parts(body.as_ptr() as *const u32, body.len() / size_of::<u32>())
                };

                while offset < typed_data.len() {
                    let hdr = typed_data[offset];
                    let cmd = crate::virgl::VirglCommand::from((hdr & 0xff) as u8);
                    let obj = crate::virgl::VirglObject::from(((hdr >> 8) & 0xff) as u8);
                    let len = ((hdr >> 16) & 0xffff) as usize;
                    if cmd.0 == 43 {
                        // TRANSFER3D
                        let body: [u32; 13] = typed_data[offset..offset+1+len].try_into().unwrap();
                        let cmd = crate::virgl::VirglTransfer3d::from(body);
                        debug!("decoded: cmd: {:?}, obj: {:?}, len: {}", cmd, obj, len);
                    } else {
                        debug!("decoded: cmd: {:?}, obj: {:?}, len: {}", cmd, obj, len);
                    }
                    offset += 1 + len;
                }
            } else*/ if hdr.hdr_type == commands::Command::RESOURCE_ATTACH_BACKING {
                let cmd = commands::ResourceAttachBacking::read_from_prefix(data).unwrap().0;
                debug!("{}: decoded: cmd: {:?}", function!(), cmd);
            }
            debug!("{}: -- finished decoding command: {:?}", function!(), hdr);
        }

        // DEBUG!!!!
        if false && let Some((Engine::Other(_), fence)) = buffer.callback.as_dma_completed() {
            let data = buffer.input.as_ref();
            let hdr = commands::CtrlHeader::read_from_prefix(data).unwrap().0;
            if hdr.hdr_type == commands::Command::SUBMIT_3D {
                let body = &data[size_of::<commands::CmdSubmit3d>()..];
                let offset = 0;
                let typed_data = unsafe {
                    core::slice::from_raw_parts(body.as_ptr() as *const u32, body.len() / size_of::<u32>())
                };
                let cmd = crate::virgl::VenusCommandType(typed_data[0]);
                info!("{}: SUBMIT_3D: dxgk_fence {}, flags {}, fence {}, ctx {}, ring {}, cmd {:?}, body {:X?}", function!(), fence, hdr.flags, hdr.fence_id, hdr.ctx_id, hdr.ring_idx, cmd, typed_data);
            }
        }

        self.buffers.inputs[i] = Some(buffer.input);
        self.buffers.outputs[i] = Some(buffer.output);
        self.buffers.callbacks[i] = buffer.callback;

        let input = self.buffers.inputs[i].as_ref().unwrap().as_ref();
        let output = self.buffers.outputs[i].as_mut().unwrap().as_mut();

        // TODO: surely this is always true...
        if !input_is_dma {
            assert!(slice_in_one_page(input), "multi-page non-dma virtio buffers are not yet supported");
        }
        if !output_is_dma {
            assert!(slice_in_one_page(output), "multi-page non-dma virtio buffers are not yet supported");
        }

        //trace!("{}: sending buffer to device", function!());
        let token = map_virtio_error!(unsafe { self.queue.add(&[input], &mut [output]) })?;
        self.buffers.indices[token as usize] = Some(i);

        if self.queue.should_notify() {
            //trace!("{}: notifying the device", function!());
            pci_transport.notify(Q);
        }

        Ok(())
    }

    fn pop_from_device(&mut self) -> Result<Option<(usize, u32)>, NtStatus> {
        //trace!("{}", function!());

        let Some(token) = self.queue.peek_used() else {
            return Ok(None);
        };

        let i = self.buffers.indices[token as usize].ok_or(STATUS::INVALID_DEVICE_STATE)?;

        let input = self.buffers.inputs[i].as_ref().ok_or(STATUS::INVALID_DEVICE_STATE)?.as_ref();
        let output = self.buffers.outputs[i].as_mut().ok_or(STATUS::INVALID_DEVICE_STATE)?.as_mut();

        let len = map_virtio_error!(unsafe { self.queue.pop_used(token, &[input], &mut [output]) })?;

        self.buffers.indices[token as usize] = None;

        Ok(Some((i, len)))
    }

    fn mark_free(&mut self, i: usize) {
        self.buffers.inputs[i] = None;
        self.buffers.outputs[i] = None;
        self.buffers.callbacks[i] = Callback::None;

        //debug!("{}: pushing {} to the list of free virtio buffers ({} / {})", function!(), i, self.free.len(), SIZE);
        self.free.push(i).unwrap();
    }

    fn read_req_header(&self, i: usize) -> commands::CtrlHeader {
        let input = self.buffers.inputs[i].as_ref().unwrap().as_ref();
        commands::CtrlHeader::read_from_prefix(input).unwrap().0
    }

    fn read_rsp_header(&self, i: usize) -> commands::CtrlHeader {
        let output = self.buffers.outputs[i].as_ref().unwrap().as_ref();
        commands::CtrlHeader::read_from_prefix(output).unwrap().0
    }

    fn read_completed_fence(&self, i: usize) -> Option<u64> {
        let rsp = self.read_rsp_header(i);
        let req = self.read_req_header(i);

        if !req.hdr_type.is_cursor() && rsp.flags & commands::GPU_FLAG_FENCE != req.flags & commands::GPU_FLAG_FENCE {
            warn!("{}: req {:?}, rsp {:?}, fence flag is different", function!(), req, rsp);
            //return None;
        }

        if rsp.flags & commands::GPU_FLAG_FENCE != 0 {
            Some(rsp.fence_id)
        } else if req.flags & commands::GPU_FLAG_FENCE != 0 {
            Some(req.fence_id)
        } else {
            //if req.hdr_type == commands::Command::SUBMIT_3D {
            //    let input = self.buffers.inputs[i].as_ref().unwrap().as_ref();
            //    let req = commands::CmdSubmit3d::read_from_prefix(input).unwrap().0;
            //    if let Some((cmd, body)) = (&input[size_of::<commands::CmdSubmit3d>()..]).split_at_checked(size_of::<u32>()) {
            //        let cmd = crate::virgl::VenusCommandType(u32::from_le_bytes(cmd.try_into().unwrap()));
            //        warn!("{}: cmd: {:?}, body: {:?}, no fence for submit", function!(), cmd, body);
            //    } else {
            //        warn!("{}: req {:?}, rsp {:?}, no fence for submit", function!(), req, rsp);
            //    }
            //}

            None
        }
    }

    fn check_response(&self, i: usize) -> bool {
        let rsp = self.read_rsp_header(i);
        let req = self.read_req_header(i);
        if rsp.hdr_type.is_error() {
            if req.hdr_type == commands::Command::RESOURCE_UNREF {
                let req = commands::ResourceUnref::read_from_prefix(self.buffers.inputs[i].as_ref().unwrap().as_ref()).unwrap().0;
                error!("{}: request {:?} failed with error {:?}", function!(), req, rsp.hdr_type);
            } else {
                error!("{}: request {:?} failed with error {:?}", function!(), req, rsp.hdr_type);
            }
            false
        } else if !rsp.hdr_type.is_response() && !req.hdr_type.is_cursor() {
            error!("{}: received unexpected response for request {:?}: {:?}", function!(), req, rsp.hdr_type);
            false
        } else {
            true
        }
    }

    fn request(&mut self, pci_transport: &mut PciTransport) -> Result<Option<()>, NtStatus> {
        let Some(i) = self.free.pop() else {
            debug!("{}: virtio queue is full: {} / {}", function!(), self.free.len(), SIZE);
            return Ok(None);
        };

        let Some(buffer) = self.chan.0.pop() else {
            self.free.push(i).unwrap();
            return Ok(None);
        };

        //warn!("{}: popped {} from the list of free virtio buffers ({} / {})", function!(), i, self.free.len(), SIZE);
        //trace!("{}: got new buffer from queue", function!());
        self.push_to_device(pci_transport, i, buffer)?;
        //trace!("{}: pushed buffer to device", function!());

        Ok(Some(()))
    }

    pub fn handle_requests(&mut self, pci_transport: &mut PciTransport) {
        loop {
            match self.request(pci_transport) {
                Ok(None) => return,
                Ok(Some(())) => continue,
                Err(e) => {
                    error!("{}: failed handling request: {:?}", function!(), e);
                    continue
                },
            }
        }
    }

    fn response(&mut self, data: &GpuData) -> Result<Option<()>, NtStatus> {
        let Some((i, len)) = self.pop_from_device()? else {
            return Ok(None);
        };

        //trace!("response: device wrote {} bytes into buffer {}", len, i);

        let callback = take(&mut self.buffers.callbacks[i]);

        /*
        if let Some((Engine::Other(_), fence)) = callback.as_dma_completed() {
            let input = self.buffers.inputs[i].as_ref().unwrap().as_ref();
            let hdr = commands::CtrlHeader::read_from_prefix(input).unwrap().0;
            info!("{}: {:?}: dxgk_fence {} (completed)", function!(), hdr.hdr_type, fence);
        } else if let Callback::None = callback {
            let input = self.buffers.inputs[i].as_ref().unwrap().as_ref();
            let hdr = commands::CtrlHeader::read_from_prefix(input).unwrap().0;
            if hdr.hdr_type == commands::Command::SUBMIT_3D {
                let body = &input[size_of::<commands::CmdSubmit3d>()..];

                let offset = 0;
                let typed_data = unsafe {
                    core::slice::from_raw_parts(body.as_ptr() as *const u32, body.len() / size_of::<u32>())
                };
                let cmd = crate::virgl::VenusCommandType(typed_data[0]);
                info!("{}: SUBMIT_3D: (completed) flags {}, virtio_fence {}, ctx {}, ring {}, cmd {:?}, body {:X?}", function!(), hdr.flags, hdr.fence_id, hdr.ctx_id, hdr.ring_idx, cmd, typed_data);
            }
        }
        */

        self.check_response(i);

        if let Some(virtio_fence) = self.read_completed_fence(i) {
            data.fence.signal(virtio_fence);
        }

        match callback {
            Callback::None => {
                self.mark_free(i);
            },

            Callback::SetEvent(block) => {
                //trace!("response: setting event");
                let output = self.buffers.outputs[i].take().unwrap();
                if !block.cancelled() {
                    block.write(output);
                }
                self.mark_free(i);
                block.set();
            },

            Callback::FreeResourceId(id) => {
                data.resource_id.free(id.get());
                self.mark_free(i);
            },

            Callback::FreeContextId(id) => {
                data.context_id.free(id.get());
                self.mark_free(i);
            },

            Callback::DmaCompleted(engine, fence) => {
                trace!("response: dma completed: {}", fence);
                data.notify_dma_completed(engine, fence);
                self.mark_free(i);
            },

            Callback::DmaCompletedWithAllocations(engine, fence, allocations) => {
                trace!("response: dma completed: {} / {:?}", fence, allocations);
                drop(allocations);
                data.notify_dma_completed(engine, fence);
                self.mark_free(i);
            },

            Callback::DmaCompletedBatched(engine, fence, allocations) => {
                trace!("response: batched dma completed: SubmissionFenceId: {} (left {})", fence, Arc::strong_count(&allocations));

                if let Some(alloc) = Arc::into_inner(allocations) {
                    trace!("response: batched dma completed (last): {:?}", alloc);
                    drop(alloc);
                    data.notify_dma_completed(engine, fence)
                }

                self.mark_free(i);
            },
        };

        Ok(Some(()))
    }

    pub fn handle_responses(&mut self, data: &GpuData) {
        loop {
            match self.response(data) {
                Ok(None) => return,
                Ok(Some(())) => continue,
                Err(e) => {
                    error!("{}: failed to handle response: {:?}", function!(), e);
                    continue
                },
            };
        }
    }

    /*pub fn handle_nop(&mut self, notify_dma: &dyn Fn(u32) -> Result<(), NtStatus>) {
        loop {
            let Some(fence) = self.nop.pop() else {
                return;
            };
            match notify_dma(fence) {
                Ok(()) => continue,
                Err(e) => {
                    error!("{}: failed to handle nop: {:?}", function!(), e);
                    continue
                },
            }
        }
    }*/

    //fn push_to_handler(&self, input: MaybeInlineBuffer<IN>, output: MaybeInlineBuffer<OUT>, callback: Callback<OUT>) -> Result<(), NtStatus> {
    //    self.chan.push(Buffer { input, output, callback });
    //
    //    Ok(())
    //}
    //
    //fn pop_in_handler(&self) -> Option<Buffer<IN, OUT>> {
    //    self.chan.pop()
    //}

    /*

    pub fn request_async_buf(&self, input: MaybeInlineBuffer<IN>, output: MaybeInlineBuffer<OUT>, callback: Callback<OUT>) -> Result<(), NtStatus> {
        //trace!("{}", function!());
        self.chan.push(Buffer { input, output, callback });
        Ok(())
    }

    pub fn request_async_into_buf<Req: IntoBytes + Immutable + KnownLayout>(&self, req: Req, output: MaybeInlineBuffer<OUT>, callback: Callback<OUT>) -> Result<(), NtStatus> {
        //trace!("{}", function!());

        let input = MaybeInlineBuffer::try_from_value(req)?;
        self.request_async_buf(input, output, callback)
    }

    pub fn request_async<Req: IntoBytes + Immutable + KnownLayout, Rsp: FromBytes + IntoBytes + KnownLayout>(&self, req: Req, callback: Callback<OUT>) -> Result<(), NtStatus> {
        //trace!("{}", function!());

        let output = MaybeInlineBuffer::try_new_zeroed::<Rsp>()?;
        self.request_async_into_buf(req, output, callback)
    }

    pub fn request_blocking_buf(&self, input: MaybeInlineBuffer<IN>, output: MaybeInlineBuffer<OUT>) -> Result<MaybeInlineBuffer<OUT>, NtStatus> {
        let signal = CancellationSignal::new();
        let mut response = pin!(MaybeUninit::<MaybeInlineBuffer<OUT>>::uninit());
        stack_pin_init!(let block = KeEvent::new(EventType::Notification, false));
        let callback = unsafe {
            Callback::SetEvent(
                NonNull::new_unchecked(block.as_mut().get_unchecked_mut()),
                Some(NonNull::new_unchecked(response.as_mut().get_unchecked_mut())),
                signal.clone()
            )
        };

        self.request_async_buf(input, output, callback)?;

        block.wait(NtTime::relative_ms(1_000)).inspect_err(|e| {
            signal.cancel();
            error!("failed to wait for request: {:?}", e)
        })?;

        let buffer = replace(response.as_mut().get_mut(), MaybeUninit::uninit());
        /* Response should be initialized by this point */
        let output = unsafe { buffer.assume_init() };

        Ok(output)
    }

    pub fn request_blocking_into_buf<Req: IntoBytes + Immutable + KnownLayout>(&self, req: Req, output: MaybeInlineBuffer<OUT>) -> Result<MaybeInlineBuffer<OUT>, NtStatus> {
        //trace!("{}", function!());

        let input = MaybeInlineBuffer::try_from_value(req)?;
        self.request_blocking_buf(input, output)

        /*let signal = CancellationSignal::new();
        let mut response = pin!(MaybeUninit::<MaybeInlineBuffer<OUT>>::uninit());
        stack_pin_init!(let block = KeEvent::new(EventType::Notification, false));
        let callback = unsafe {
            Callback::SetEvent(
                NonNull::new_unchecked(block.as_mut().get_unchecked_mut()),
                Some(NonNull::new_unchecked(response.as_mut().get_unchecked_mut())),
                signal.clone()
            )
        };

        self.request_async_into_buf(handler_event, req, output, callback)?;

        block.wait(NtTime::relative_ms(1_000)).inspect_err(|e| {
            signal.cancel();
            error!("failed to wait for request {}: {:?}", core::any::type_name::<Req>(), e)
        })?;

        let buffer = core::mem::replace(response.as_mut().get_mut(), MaybeUninit::uninit());
        /* Response should be initialized by this point */
        let output = unsafe { buffer.assume_init() };

        Ok(output)*/
    }

    pub fn request_blocking<Req: IntoBytes + Immutable + KnownLayout, Rsp: FromBytes + IntoBytes + KnownLayout>(&self, req: Req) -> Result<Rsp, NtStatus> {
        //trace!("{}", function!());

        let output = MaybeInlineBuffer::try_new_zeroed::<Rsp>()?;
        let response = self.request_blocking_into_buf(req, output)?;
        let rsp = Rsp::read_from_prefix(response.as_ref()).unwrap().0;

        Ok(rsp)
    }

    pub fn request_async_dma<Rsp: FromBytes + IntoBytes + KnownLayout>(&self, dma: NonNull<[u8]>, fence: u32, allocations: Option<AllocationsBatch>) -> Result<(), NtStatus> {
        let input = MaybeInlineBuffer::Dma { data: dma };
        let output = MaybeInlineBuffer::try_new_zeroed::<Rsp>()?;

        let callback = if let Some(allocations) = allocations {
            Callback::DmaCompletedWithAllocations(fence, allocations)
        } else {
            Callback::DmaCompleted(fence)
        };

        self.request_async_buf(input, output, callback)
    }

    // Caller MUST call the corresponding KeEvent::set after queuing all the commands
    pub fn request_async_dma_batched<Rsp: FromBytes + IntoBytes + KnownLayout>(&self, dma: NonNull<[u8]>, fence: u32, allocations: Arc<AllocationsBatch>) -> Result<(), NtStatus> {
        let input = MaybeInlineBuffer::Dma { data: dma };
        let output = MaybeInlineBuffer::try_new_zeroed::<Rsp>()?;
        let callback = Callback::DmaCompletedBatched(fence, allocations);

        self.chan.push(Buffer { input, output, callback });
        Ok(())
    }

    */

    //pub fn submit_nop(&self, handler_event: Pin<&KeEvent>, fence: u32) {
    //    self.nop.push(fence);
    //    handler_event.set();
    //}
}

type ControlQueue = Queue<QUEUE_TRANSMIT, CONTROL_QUEUE_SIZE, CONTROL_FAST_QUEUE_SIZE, MAX_COMMAND_SIZE, MAX_RESPONSE_SIZE>;
type CursorQueue = Queue<QUEUE_CURSOR, CURSOR_QUEUE_SIZE, CURSOR_FAST_QUEUE_SIZE, CURSOR_COMMAND_SIZE, CURSOR_RESPONSE_SIZE>;
type ControlChannel = QueueChannel<MAX_COMMAND_SIZE, MAX_RESPONSE_SIZE>;
type CursorChannel = QueueChannel<CURSOR_COMMAND_SIZE, CURSOR_RESPONSE_SIZE>;

//const _: () = assert!(size_of::<ControlQueue>() == 17664);
//const _: () = assert!(size_of::<CursorQueue>() == 2304);
const _: () = assert!(size_of::<ArrayQueue<usize>>() == 384);

// TODO: surely there's a better way
struct SimpleIdAllocator {
    next: AtomicU32,
    free: SpinMutex<VecDeque<u32>>,
}

impl SimpleIdAllocator {
    /* Can return [initial; u32::MAX) */
    pub fn new(initial: u32) -> Self {
        Self {
            next: AtomicU32::new(initial),
            free: SpinMutex::new(VecDeque::new()),
        }
    }

    pub fn next(&self) -> Option<u32> {
        let mut free = self.free.lock();
        if let Some(id) = free.pop_front() {
            return Some(id);
        }
        drop(free);

        let id = self.next.fetch_add(1, Ordering::SeqCst);
        if id != u32::MAX {
            Some(id)
        } else {
            self.next.store(u32::MAX, Ordering::SeqCst);
            None
        }
    }

    pub fn free(&self, id: u32) {
        let mut free = self.free.lock();
        free.push_back(id);
    }
}

const _: () = assert!(size_of::<SimpleIdAllocator>() == 48);

#[pin_data]
struct Thread {
    thread: Option<KeThread>,

    #[pin]
    control_irq: KeEvent,
    //#[pin]
    //control_msg: KeEvent,

    #[pin]
    cursor_irq: KeEvent,
    //#[pin]
    //cursor_msg: KeEvent,

    #[pin]
    config_irq: KeEvent,

    //#[pin]
    //nop_msg: KeEvent,

    #[pin]
    stop: KeEvent,
}

struct ThreadEvents<'t> {
    control_irq: Pin<&'t KeEvent>,
    //control_msg: Pin<&'t KeEvent>,
    cursor_irq: Pin<&'t KeEvent>,
    //cursor_msg: Pin<&'t KeEvent>,
    config_irq: Pin<&'t KeEvent>,
    //nop_msg: Pin<&'t KeEvent>,
    stop: Pin<&'t KeEvent>,
}

impl Thread {
    fn new() -> impl PinInit<Self, NtStatus> {
        pin_init!(Self {
            thread: None,
            control_irq <- KeEvent::new(EventType::Synchronization, false),
            //control_msg <- KeEvent::new(EventType::Synchronization, false),
            cursor_irq <- KeEvent::new(EventType::Synchronization, false),
            //cursor_msg <- KeEvent::new(EventType::Synchronization, false),
            config_irq <- KeEvent::new(EventType::Synchronization, false),
            //nop_msg <- KeEvent::new(EventType::Synchronization, false),
            stop <- KeEvent::new(EventType::Synchronization, false),
        }? NtStatus)
    }

    fn start<T: Send>(self: &mut Pin<Box<Self>>, f: fn(&mut T), ctx: *mut T) -> Result<(), NtStatus> {
        let thread = self.thread();
        *thread = Some(KeThread::spawn(f, ctx)?);
        //thread.as_ref().unwrap().set_priority(ThreadPriority::LowRealtime);
        thread.as_ref().unwrap().set_priority(ThreadPriority::High);

        Ok(())
    }

    fn stop(self: &Pin<Box<Self>>){
        self.stop.set();
    }

    fn thread(self: &mut Pin<Box<Self>>) -> &mut Option<KeThread> {
        unsafe {
            self.as_mut().map_unchecked_mut(|t| &mut t.thread).get_unchecked_mut()
        }
    }

    //fn thread(self: &mut Pin<Box<Self>>) -> &mut Option<KeThread> {
    //    unsafe {
    //        self.as_mut().map_unchecked_mut(|t| &mut t.thread).get_unchecked_mut()
    //    }
    //}

    fn events<'t>(self: &'t Pin<Box<Self>>) -> ThreadEvents<'t> {
        ThreadEvents {
            control_irq: unsafe { self.as_ref().map_unchecked(|t| &t.control_irq) },
            //control_msg: unsafe { self.as_ref().map_unchecked(|t| &t.control_msg) },
            cursor_irq: unsafe { self.as_ref().map_unchecked(|t| &t.cursor_irq) },
            //cursor_msg: unsafe { self.as_ref().map_unchecked(|t| &t.cursor_msg) },
            config_irq: unsafe { self.as_ref().map_unchecked(|t| &t.config_irq) },
            //nop_msg: unsafe { self.as_ref().map_unchecked(|t| &t.nop_msg) },
            stop: unsafe { self.as_ref().map_unchecked(|t| &t.stop) },
        }
    }
}

#[derive(Debug)]
struct EngineState {
    last_submitted_fence: AtomicU32,
    last_completed_fence: AtomicU32,
    last_preemption_fence: AtomicU32,
    pending_fences: SpinMutex<BTreeSet<u32>>,
    last_submitted_timestamp: AtomicU64,
    last_completed_timestamp: AtomicU64,
}

impl EngineState {
    const RESPONSIVE_TIMEOUT_SEC: u64 = 30;

    fn submit(&self, fence: u32) {
        self.last_submitted_fence.store(fence, Ordering::SeqCst);
        let (now, _) = ke_query_performance_counter();
        self.last_submitted_timestamp.store(now, Ordering::SeqCst);
    }

    fn complete(&self, fence: u32) {
        self.last_completed_fence.store(fence, Ordering::SeqCst);
        let (now, _) = ke_query_performance_counter();
        self.last_completed_timestamp.store(now, Ordering::SeqCst);
    }

    fn is_responsive(&self) -> bool {
        let last_submitted_fence = self.last_submitted_fence.load(Ordering::SeqCst);
        let last_completed_fence = self.last_completed_fence.load(Ordering::SeqCst);

        if last_completed_fence == last_submitted_fence {
            /* No work is pending, assume responsive */
            return true;
        }

        let last_submitted_timestamp = self.last_submitted_timestamp.load(Ordering::SeqCst);
        let last_completed_timestamp = self.last_completed_timestamp.load(Ordering::SeqCst);

        if last_submitted_timestamp == 0 || last_completed_timestamp == 0 {
            return true;
        }

        let (now, freq) = ke_query_performance_counter();

        if (now - last_submitted_timestamp) > EngineState::RESPONSIVE_TIMEOUT_SEC * freq {
            warn!("{}: not responsive: {:?}, now {}, last submitted {}, threshold {}", function!(), self, now, last_submitted_timestamp, EngineState::RESPONSIVE_TIMEOUT_SEC * freq);
        }

        /*now - last_completed_timestamp <= EngineState::RESPONSIVE_TIMEOUT_SEC * freq ||*/ (now - last_submitted_timestamp) <= EngineState::RESPONSIVE_TIMEOUT_SEC * freq
    }
}

struct FenceTracker{
    next_fence: AtomicU64,
    unsignaled: RwLock<SmallVec<[u64; CONTROL_QUEUE_SIZE]>>,
}

impl FenceTracker {
    pub fn new() -> Self {
        Self{
            next_fence: AtomicU64::new(0),
            unsignaled: RwLock::new(SmallVec::new()),
        }
    }

    pub fn next_fence(&self) -> u64 {
        let fence = self.next_fence.fetch_add(1, Ordering::SeqCst);
        self.unsignaled.write().push(fence);

        fence
    }

    pub fn signal(&self, fence: u64) {
        let next_fence = self.next_fence.load(Ordering::SeqCst);
        let mut unsignaled = self.unsignaled.write();
        assert!(fence < next_fence);
        if let Some(position) = unsignaled.iter().position(|&x| x == fence) {
            unsignaled.swap_remove(position);
        } else {
            warn!("{}: fence {} signaled multiple times", function!(), fence);
        }
    }

    pub fn query(&self, fence: u64) -> bool {
        let next_fence = self.next_fence.load(Ordering::SeqCst);
        fence < next_fence && !self.unsignaled.read().contains(&fence)
    }
}

#[derive(Debug)]
struct FenceSubmission {
    engine: Engine,
    dxgk_fence: u32,
    virtio_fence: u64,
    timestamp: u64,
}

impl FenceSubmission {
    pub fn new(engine: Engine, dxgk_fence: u32, virtio_fence: u64) -> Self {
        let (timestamp, _) = ke_query_performance_counter();

        Self {
            engine,
            dxgk_fence,
            virtio_fence,
            timestamp,
        }
    }

    pub fn is_ready(&self, tracker: &FenceTracker) -> bool {
        tracker.query(self.virtio_fence)
    }

    // Only makes sense for not yet ready fence submission
    pub fn is_timeout(&self) -> bool {
        let (now, freq) = ke_query_performance_counter();

        let dt = now - self.timestamp;

        if dt >= EngineState::RESPONSIVE_TIMEOUT_SEC * freq {
            warn!("{}: timeout {:?}: now {}, freq {}, dt {}", function!(), self, now, freq, dt);
            true
        } else {
            false
        }
    }
}

struct GpuData {
    pub interface: DxgkInterface,
    pub shmem: VirtioCapabilityInfo,

    resource_id: SimpleIdAllocator,
    context_id: SimpleIdAllocator,
    offset_allocator: SpinMutex<offset_allocator::Allocator>,
    irq: AtomicU32,
    //fence: CachePadded<AtomicU64>,
    fence: FenceTracker,

    fence_submissions: SpinMutex<SmallVec<[FenceSubmission; 16]>>,

    engines: [EngineState; Engine::TOTAL_COUNT as usize],
}

impl GpuData {
    fn new(interface: DxgkInterface, shmem: VirtioCapabilityInfo) -> impl Init<Self, NtStatus> {
        let shmem_pages = (shmem.length / (PAGE_SIZE as u64)) as u32;

        init!(Self {
            interface,
            shmem,
            resource_id: SimpleIdAllocator::new(1),
            context_id: SimpleIdAllocator::new(1),
            offset_allocator: SpinMutex::new(offset_allocator::Allocator::with_max_allocs(shmem_pages, 1024 * 1024)),
            fence: FenceTracker::new(),
            //fence: CachePadded::new(AtomicU64::new(0)),
            irq: AtomicU32::new(0),
            engines <- init_array_from_fn(|_| EngineState {
                last_submitted_fence: AtomicU32::new(0),
                last_completed_fence: AtomicU32::new(0),
                pending_fences: SpinMutex::new(BTreeSet::new()),
                last_preemption_fence: AtomicU32::new(0),
                last_submitted_timestamp: AtomicU64::new(0),
                last_completed_timestamp: AtomicU64::new(0),
            }),
            fence_submissions: SpinMutex::new(SmallVec::new()),
        }? NtStatus)
    }

    fn notify_fence(&self, engine: Engine, fence: u32, notify_cb: &dyn Fn(Engine, u32)) {
        let engine_state = &self.engines[engine.node_ordinal() as usize];

        let last_completed = engine_state.last_completed_fence.load(Ordering::SeqCst);

        if last_completed + 1 == fence {
            debug!("{}: notify immediately fence {} (last completed {})", function!(), fence, last_completed);
            notify_cb(engine, fence);
            engine_state.complete(fence);
        } else {
            debug!("{}: enqueue fence {} (last completed {})", function!(), fence, last_completed);
            engine_state.pending_fences.lock().insert(fence);
        };

        self.notify_queued_fences(engine);
    }

    fn notify_dma_completed(&self, engine: Engine, fence: u32) {
        self.notify_fence(engine, fence, &|engine, fence| {
            let _ = self.interface.notify_interrupt_synchronized(Interrupt::DmaCompleted(engine, fence));
        });
    }

    fn notify_dma_faulted(&self, engine: Engine, fence: u32) {
        self.notify_fence(engine, fence, &|engine, fence| {
            let _ = self.interface.notify_interrupt_synchronized(Interrupt::DmaFaulted(engine, fence));
        });
    }

    fn notify_dma_preempted(&self, engine: Engine, preemption_fence: u32, last_completed_fence: u32) {
        let _ = self.interface.notify_interrupt_synchronized(Interrupt::DmaPreempted(engine, preemption_fence, last_completed_fence));
    }

    fn notify_queued_fences(&self, engine: Engine) {
        let engine_state = &self.engines[engine.node_ordinal() as usize];

        let mut last_completed = engine_state.last_completed_fence.load(Ordering::SeqCst);
        let mut pending = engine_state.pending_fences.lock();

        loop {
            let Some(&next_fence) = pending.first() else {
                break;
            };
            if last_completed + 1 == next_fence {
                let _ = pending.pop_first();
                let _ = self.interface.notify_interrupt_synchronized(Interrupt::DmaCompleted(engine, next_fence));
                engine_state.complete(next_fence);
                last_completed = next_fence;
            } else if last_completed + 1 > next_fence {
                // VidSch will bugcheck if we report a missed fence
                // Is this even possible with the current logic?
                panic!("{}: missed fence: {} (last completed {})", function!(), next_fence, last_completed);
            } else /* if last completed + 1 < next_fence */ {
                // Haven't completed the last fence yet
                break;
            }
        }

        let last_submitted = engine_state.last_submitted_fence.load(Ordering::SeqCst);
        if last_completed == last_submitted {
           let last_preemption_fence = engine_state.last_preemption_fence.swap(0, Ordering::SeqCst);
           if last_preemption_fence != 0 {
               self.notify_dma_preempted(engine, last_preemption_fence, last_completed);
           }
        }
    }

    fn handle_fence_submissions(&self) {
        let mut resubmit = SmallVec::<[FenceSubmission; 8]>::new();

        self.fence_submissions.lock().retain(|fence| {
            //warn!("{}: engine {:?}, dxgk_fence {}, virtio_fence {}", function!(), *engine, *dxgk_fence, *virtio_fence);
            if fence.is_ready(&self.fence) {
                //warn!("{}: virtio fence {} is finished, notify engine {:?}, dxgk_fence {}", function!(), *virtio_fence, *engine, *dxgk_fence);
                /* If virtio fence is completed, dxgk fence can be notified, and there's no longer need to keep it here */
                self.notify_dma_completed(fence.engine, fence.dxgk_fence);
                false
            } else if fence.is_timeout() {
                warn!("{}: fence timed out: {:?}", function!(), fence);
                warn!("{}: engine state: {:?}", function!(), self.engines[fence.engine.node_ordinal() as usize]);
                // DEBUG: it's better to misrender rather than hang forever
                if false {
                    // TODO: DXGK_INTERRUPT_DMA_PAGE_FAULTED does not seem to unblock VidSch from waiting
                    // We want something that would both unblock it and crash the userspace caller
                    // But misrendering is probably better than hang
                    self.notify_dma_completed(fence.engine, fence.dxgk_fence);
                    //self.notify_dma_faulted(fence.engine, fence.dxgk_fence);
                    false
                } else {
                    resubmit.push(FenceSubmission::new(fence.engine, fence.dxgk_fence, fence.virtio_fence));
                    false
                }
            } else {
                true
            }
        });

        self.fence_submissions.lock().extend(resubmit);

        /*
        if self.fence_submissions.lock().len() > 0 || self.fence.unsignaled.read().len() > 0 {
            for (i, state) in self.engines.iter().enumerate() {
                let engine = Engine::try_from_node_ordinal(i as _).unwrap();
                if !state.is_responsive() {
                    let (now, freq) = ke_query_performance_counter();
                    warn!("{}: Engine {:?} is not responsive: {:?}, now: {}, dt: {}, threshold: {}", function!(), engine, state, now, now - state.last_submitted_timestamp.load(Ordering::SeqCst), EngineState::RESPONSIVE_TIMEOUT_SEC * freq);
                }
            }
        }
        */

        //let pending = self.fence_submissions.lock();
        //let unsignaled = &self.fence.0.read().unsignaled;
        //if pending.len() > 0 || unsignaled.len() > 0 {
        //    warn!("{}: pending: {:?}", function!(), pending);
        //    warn!("{}: unsignaled: {:?}", function!(), unsignaled);
        //}
    }
}

#[derive(Clone)]
pub struct GpuChannel {
    control: ControlChannel,
    cursor: CursorChannel,
    data: Arc<GpuData>,
}

impl GpuChannel {
    fn next_fence(&self) -> u64 {
        self.data.fence.next_fence()
    }

    pub fn next_resource_id(&self) -> Option<NonZero<u32>> {
        self.data.resource_id.next().map(NonZero::new).flatten()
    }

    fn next_context_id(&self) -> Option<NonZero<u32>> {
        self.data.context_id.next().map(NonZero::new).flatten()
    }

    pub fn dxgk_interface(&self) -> &DxgkInterface {
        &self.data.interface
    }

    pub fn kick(&self) {
        self.control.get_queue_event().set();
        self.cursor.get_queue_event().set();
    }

    pub fn new_header(&self, hdr_type: commands::Command, fence: bool, context: Option<NonZero<u32>>, ring: Option<u8>) -> commands::CtrlHeader {
        let mut flags: u32 = 0;

        let fence_id = if fence {
            flags |= commands::GPU_FLAG_FENCE;
            self.data.fence.next_fence()
        } else {
            0
        };

        let ring_idx = if let Some(ring_idx) = ring {
            flags |= commands::GPU_FLAG_RING_INDEX;
            ring_idx
        } else {
            0
        };

        let ctx_id = if let Some(ctx_id) = context {
            ctx_id.get()
        } else {
            0
        };

        commands::CtrlHeader {
            hdr_type,
            flags,
            fence_id,
            ctx_id,
            ring_idx,
            _padding: [0; 3],
        }
    }

    fn get_capset_info(&self, capset_index: u32) -> Result<(u32, CapsetInfo), NtStatus> {
        trace!("{}", function!());

        let cmd = commands::GetCapsetInfo {
            header: self.new_header(commands::Command::GET_CAPSET_INFO, true, None, None),
            capset_index,
            _padding: 0,
        };

        let resp: commands::RespCapsetInfo = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.header.check_type(commands::Command::OK_CAPSET_INFO))?;

        let id = resp.capset_id;
        let version = resp.capset_max_version;
        let size = resp.capset_max_size;

        Ok((id, CapsetInfo { version, size }))
    }

    pub fn get_capset_infos(&self, num_capsets: u32) -> Result<(CapsetMask, [CapsetInfo; 64]), NtStatus> {
        trace!("{}", function!());

        let mut infos = [CapsetInfo::default(); 64];
        let mut mask = CapsetMask::default();

        for capset_index in 0..num_capsets {
            match self.get_capset_info(capset_index).map(|(id, info)| {
                CapsetId::try_from(id).and_then(|capset_id| Ok((id as usize, capset_id, info)))
            }).flatten() {
                Ok((index, id, info)) => {
                    if index > infos.len() {
                        error!("unknown capset {}", index);
                        continue;
                    }
                    debug!("Found capset {:?}: {:?}", id, info);
                    mask = mask.union(CapsetMask::from(id));

                    infos[index] = info;
                }
                Err(e) => {
                    error!("failed to get capset info: {:?}", e);
                }
            }
        }

        if mask.is_empty() {
            error!("no capsets supported");
            return Err(NtStatus(STATUS::UNSUCCESSFUL));
        }

        debug!("supported capsets: {:?}", mask);

        Ok((mask, infos))
    }

    pub fn get_capset(&self, id: CapsetId, info: &CapsetInfo) -> Result<Box<[u8]>, NtStatus> {
        trace!("{}", function!());

        let cmd = commands::GetCapset {
            header: self.new_header(commands::Command::GET_CAPSET, true, None, None),
            capset_id: id as _,
            capset_version: info.version,
        };

        let output = MaybeInlineBuffer::try_new_zeroed_size(size_of::<commands::RespCapset>() + (info.size as usize))?;
        let output = self.control.request_blocking_into_buf(cmd, output)?;

        let hdr: commands::CtrlHeader = commands::CtrlHeader::read_from_prefix(output.as_ref()).unwrap().0;
        map_virtio_error!(hdr.check_type(commands::Command::OK_CAPSET))?;

        output.into_boxed(size_of::<commands::CtrlHeader>())
    }

    pub fn get_display_info(&self) -> Result<[commands::DisplayOne; 16], NtStatus> {
        trace!("{}", function!());

        let cmd = self.new_header(commands::Command::GET_DISPLAY_INFO, true, None, None);

        let resp: commands::RespDisplayInfo = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.header.check_type(commands::Command::OK_DISPLAY_INFO))?;

        Ok(resp.pmodes)
    }

    pub fn context_create(&self, capset_id: CapsetId, name: &str) -> Result<NonZero<u32>, NtStatus> {
        trace!("{}", function!());

        /* We should really never run out of IDs */
        let ctx_id = self.next_context_id().ok_or(STATUS::INSUFFICIENT_RESOURCES)?;

        if name.len() > 64 {
            return Err(NtStatus(STATUS::BUFFER_TOO_SMALL));
        }

        let cmd = commands::CtxCreate {
            header: self.new_header(commands::Command::CTX_CREATE, true, Some(ctx_id), None),
            nlen: name.len() as u32,
            context_init: capset_id as u32,
            debug_name: {
                let mut buffer = [0u8; 64];
                buffer[..name.len()].copy_from_slice(&name.as_bytes());

                buffer
            }
        };

        let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.check_type(commands::Command::OK_NODATA))?;

        Ok(ctx_id)
    }

    pub fn context_destroy(&self, id: NonZero<u32>) -> Result<(), NtStatus> {
        let cmd = commands::CtxDestroy {
            header: self.new_header(commands::Command::CTX_DESTROY, true, Some(id), None),
        };
        self.control.request_async::<_, commands::CtrlHeader>(cmd, Callback::FreeContextId(id))?;

        Ok(())
    }

    pub fn resource_create_2d(&self, width: u32, height: u32, format: commands::Format) -> Result<NonZero<u32>, NtStatus> {
        let resource_id = self.next_resource_id().ok_or(STATUS::NO_MEMORY)?;
        let cmd = commands::ResourceCreate2d {
            header: self.new_header(commands::Command::RESOURCE_CREATE_2D, true, None, None),
            resource_id: resource_id.get(),
            format: format as u32,
            width,
            height,
        };

        let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_| {
            warn!("{}: failed to create resource 2d {}", function!(), resource_id.get());
            self.data.resource_id.free(resource_id.get())
        })?;

        Ok(resource_id)
    }

    pub fn resource_create_3d(&self, info: &Allocate3d) -> Result<NonZero<u32>, NtStatus> {
        let resource_id = self.next_resource_id().ok_or(STATUS::NO_MEMORY)?;
        let cmd = commands::ResourceCreate3d {
            header: self.new_header(commands::Command::RESOURCE_CREATE_3D, true, None, None),
            resource_id: resource_id.get(),
            target: info.target,
            format: info.format,
            bind: info.bind,
            width: info.width,
            height: info.height,
            depth: info.depth,
            array_size: info.array_size,
            last_level: info.last_level,
            nr_samples: info.nr_samples,
            flags: info.flags,
            _padding: 0,
        };

        let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_| {
            warn!("{}: failed to create resource 3d {}", function!(), resource_id.get());
            self.data.resource_id.free(resource_id.get())
        })?;

        Ok(resource_id)
    }

    pub fn context_attach_resource(&self, ctx_id: NonZero<u32>, res_id: NonZero<u32>) -> Result<(), NtStatus> {
        let cmd = commands::CtxResource {
            header: self.new_header(commands::Command::CTX_ATTACH_RESOURCE, true, Some(ctx_id), None),
            resource_id: res_id.get(),
            _padding: 0,
        };
        let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_|
            warn!("{}: failed to attach resource {} to context {}", function!(), res_id.get(), ctx_id.get())
        )
    }

    pub fn context_detach_resource(&self, ctx_id: NonZero<u32>, res_id: NonZero<u32>) -> Result<(), NtStatus> {
        let cmd = commands::CtxResource {
            header: self.new_header(commands::Command::CTX_DETACH_RESOURCE, true, Some(ctx_id), None),
            resource_id: res_id.get(),
            _padding: 0,
        };
        let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_|
            warn!("{}: failed to detach resource {} from context {}", function!(), res_id.get(), ctx_id.get())
        )
    }

    pub fn resource_create_blob(&self, ctx_id: NonZero<u32>, res_id: NonZero<u32>, blob_id: u64, mem: BlobMem, flags: BlobFlag, size: u64) -> Result<(), NtStatus> {
        let cmd = commands::ResourceCreateBlob {
            header: self.new_header(commands::Command::RESOURCE_CREATE_BLOB, true, Some(ctx_id), None),
            resource_id: res_id.get(),
            blob_mem: mem.bits(),
            blob_flags: flags.bits(),
            nr_entries: 0, // We probably don't need guest blobs
            blob_id,
            size,
        };

        let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_|
            warn!("{}: failed to create resource blob {} ({})", function!(), res_id.get(), blob_id)
        )?;

        Ok(())
    }

    pub fn resource_map_blob(&self, ctx_id: NonZero<u32>, res_id: NonZero<u32>, size: u64) -> Result<(offset_allocator::Allocation, u64, u32), NtStatus> {
        let pages = (size / (PAGE_SIZE as u64)) as u32;

        let offset = self.data.offset_allocator.lock().allocate(pages).ok_or(STATUS::NO_MEMORY)?;
        let bar_offset = (offset.offset as u64) * (PAGE_SIZE as u64);

        let cmd = commands::ResourceMapBlob {
            header: self.new_header(commands::Command::RESOURCE_MAP_BLOB, true, Some(ctx_id), None),
            resource_id: res_id.get(),
            _padding: 0,
            offset: bar_offset,
        };

        let map_info = self.control.request_blocking::<_, commands::RespMapInfo>(cmd)?;
        map_virtio_error!(map_info.header.check_type(commands::Command::OK_MAP_INFO)).inspect_err(|_|
            warn!("{}: failed to map blob {}", function!(), res_id.get())
        )?;

        Ok((offset, bar_offset, map_info.map_info))
    }

    pub fn resource_unmap_blob(&self, id: NonZero<u32>, offset: offset_allocator::Allocation) -> Result<(), NtStatus> {
        self.data.offset_allocator.lock().free(offset);

        let cmd = commands::ResourceUnmapBlob {
            header: self.new_header(commands::Command::RESOURCE_UNMAP_BLOB, true, None, None),
            resource_id: id.get(),
            _padding: 0,
        };
        self.control.request_blocking::<_, commands::CtrlHeader>(cmd)?;

        Ok(())
    }

    // TODO: maybe add a special async callback type for set_cursor / set_scanout + flush
    pub fn resource_transfer_to_host_2d(&self, rect: commands::Rect, offset: u64, resource_id: NonZero<u32>) -> Result<(), NtStatus> {
        let cmd = commands::TransferToHost2d {
            header: self.new_header(commands::Command::TRANSFER_TO_HOST_2D, true, None, None),
            rect,
            offset,
            resource_id: resource_id.get(),
            _padding: 0,
        };

        let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_| {
            warn!("{}: failed to transfer resource 2d {}", function!(), resource_id.get());
            self.data.resource_id.free(resource_id.get())
        })?;

        Ok(())
    }

    pub fn resource_flush(&self, id: NonZero<u32>, rect: commands::Rect) -> Result<(), NtStatus> {
        let cmd = commands::ResourceFlush {
            header: self.new_header(commands::Command::RESOURCE_FLUSH, false, None, None),
            resource_id: id.get(),
            rect,
            _padding: 0,
        };
        self.control.request_async::<_, commands::CtrlHeader>(cmd, Callback::None)?;

        Ok(())
    }

    pub fn resource_detach_backing(&self, id: NonZero<u32>) -> Result<(), NtStatus> {
        let cmd = commands::ResourceDetachBacking {
            header: self.new_header(commands::Command::RESOURCE_DETACH_BACKING, true, None, None),
            resource_id: id.get(),
            _padding: 0,
        };

        let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_|
            warn!("{}: failed to detach backing from resource {}", function!(), id.get())
        )?;

        Ok(())
    }

    pub fn resource_unref(&self, id: NonZero<u32>) -> Result<(), NtStatus> {
        let cmd = commands::ResourceUnref {
            header: self.new_header(commands::Command::RESOURCE_UNREF, true, None, None),
            resource_id: id.get(),
            _padding: 0,
        };
        self.control.request_async::<_, commands::CtrlHeader>(cmd, Callback::FreeResourceId(id))?;

        Ok(())
    }

    pub fn submit_async(&self, data: AlignedBox<[u8]>) -> Result<(), NtStatus> {
        let input = MaybeInlineBuffer::Boxed { data };
        let output = MaybeInlineBuffer::try_new_zeroed::<commands::CtrlHeader>()?;
        let callback = Callback::None;
        self.control.request_async_buf(input, output, callback)
    }

    pub fn submit_command_sync(&self, cmd: &Command) -> Result<(), NtStatus> {
        assert!(cmd.id != CommandId::Nop);

        let dma = cmd.dma().ok_or(STATUS::UNSUCCESSFUL).inspect_err(|e|
            error!("{}: failed to get dma from command {:?}. Does it still needs patching?", function!(), cmd)
        )?;

        let input = MaybeInlineBuffer::Dma { data: dma };
        let output = MaybeInlineBuffer::try_new_zeroed::<commands::CtrlHeader>()?;
        //let output = if cmd.id == CommandId::MapBlob {
        //    MaybeInlineBuffer::try_new_zeroed::<commands::RespMapInfo>()?
        //} else {
        //    MaybeInlineBuffer::try_new_zeroed::<commands::CtrlHeader>()?
        //};

        self.control.request_blocking_buf(input, output, DEFAULT_BLOCKING_TIMEOUT)?;

        Ok(())
    }

    pub fn submit_fence(&self, engine: Engine, dxgk_fence: u32, virtio_fence: u64) {
        self.data.engines[engine.node_ordinal() as usize].submit(dxgk_fence);

        if self.data.fence.query(virtio_fence) {
            trace!("{}: notify immediately: engine {:?}, dxgk_fence {}, virtio_fence {}", function!(), engine, dxgk_fence, virtio_fence);
            self.data.notify_dma_completed(engine, dxgk_fence);
        } else {
            trace!("{}: queue notification: engine {:?}, dxgk_fence {}, virtio_fence {}", function!(), engine, dxgk_fence, virtio_fence);
            self.data.fence_submissions.lock().push(FenceSubmission::new(engine, dxgk_fence, virtio_fence));
            self.data.handle_fence_submissions();
        }
    }

    pub fn submit_preemption(&self, engine: Engine, preemption_fence: u32) {
        self.data.engines[engine.node_ordinal() as usize].last_preemption_fence.store(preemption_fence, Ordering::SeqCst);
    }

    pub fn submit_command(&self, engine: Engine, fence: u32, cmd: &Command, allocations: Option<AllocationsBatch>) -> Result<(), NtStatus> {
        //warn!("{}: engine: {:?} fence: {}, timestamp {:?}", function!(), engine, fence, ke_query_performance_counter());
        self.data.engines[engine.node_ordinal() as usize].submit(fence);

        if cmd.id == CommandId::Nop {
            self.data.notify_dma_completed(engine, fence);

            return Ok(());
        }

        debug!("{}: Sending async dma command: {:?}", function!(), cmd);

        let dma = cmd.dma().ok_or(STATUS::UNSUCCESSFUL).inspect_err(|e|
            error!("{}: failed to get dma from command {:?}. Does it still needs patching?", function!(), cmd)
        )?;

        //if cmd.id == CommandId::MapBlob {
        //    self.control.request_async_dma::<commands::RespMapInfo>(dma, fence, allocations)
        //} else {
        //    self.control.request_async_dma::<commands::CtrlHeader>(dma, fence, allocations)
        //}
        self.control.request_async_dma::<commands::CtrlHeader>(dma, engine, fence, allocations)
    }

    pub fn submit_command_batch(&self, engine: Engine, fence: u32, cmds: &[Command], allocations: AllocationsBatch) -> Result<(), NtStatus> {
        //warn!("{}: engine: {:?}, fence: {}, timestamp {:?}", function!(), engine, fence, ke_query_performance_counter());

        assert!(cmds.len() > 1);

        self.data.engines[engine.node_ordinal() as usize].submit(fence);

        let allocations = Arc::new(allocations);
        for (i, cmd) in cmds.iter().enumerate() {
            assert!(cmd.id != CommandId::Nop);
            let dma = cmd.dma().unwrap();
            debug!("{}: Sending async dma command batch ({}): {:?}", function!(), Arc::strong_count(&allocations), cmd);
            //if cmd.id == CommandId::MapBlob {
            //    self.control.request_async_dma_batched::<commands::RespMapInfo>(dma, fence, allocations.clone())?;
            //} else {
            //    self.control.request_async_dma_batched::<commands::CtrlHeader>(dma, fence, allocations.clone())?;
            //}
            self.control.request_async_dma_batched::<commands::CtrlHeader>(dma, engine, fence, allocations.clone())?;
        }
        self.control.get_queue_event().set();

        // This shouldn't really happen, but just in case
        if let Some(alloc) = Arc::into_inner(allocations) {
            drop(alloc);
            self.data.notify_dma_completed(engine, fence);
        }

        Ok(())
    }

    pub fn submit_command_buffer(&self, engine: Engine, fence: u32, ctx_id: NonZero<u32>, ring: Option<u8>, data: &[u8]) -> Result<(), NtStatus> {
        trace!("{}: engine {:?}, fence {}, ctx {}, ring {:?}, data {:?}", function!(), engine, fence, ctx_id, ring, data);
        self.data.engines[engine.node_ordinal() as usize].submit(fence);

        let hdr = commands::CmdSubmit3d {
            header: self.new_header(commands::Command::SUBMIT_3D, true, Some(ctx_id), ring),
            size: data.len() as _,
            _padding: 0,
        };

        let input = MaybeInlineBuffer::try_from_hdr_with_body(hdr, data)?;
        let output = MaybeInlineBuffer::try_new_zeroed::<commands::CtrlHeader>()?;

        let callback = Callback::DmaCompleted(engine, fence);

        self.control.request_async_buf(input, output, callback)
    }

    pub fn submit_command_buffer_with_fence(&self, engine: Engine, ctx_id: NonZero<u32>, ring: Option<u8>, data: &[u8]) -> Result<u64, NtStatus> {
        trace!("{}: engine {:?}, ctx {}, ring {:?}, data {:?}", function!(), engine, ctx_id, ring, data);

        let hdr = commands::CmdSubmit3d {
            header: self.new_header(commands::Command::SUBMIT_3D, true, Some(ctx_id), ring),
            size: data.len() as _,
            _padding: 0,
        };

        let input = MaybeInlineBuffer::try_from_hdr_with_body(hdr, data)?;
        let output = MaybeInlineBuffer::try_new_zeroed::<commands::CtrlHeader>()?;
        self.control.request_async_buf(input, output, Callback::None)?;

        Ok(hdr.header.fence_id)
    }

    pub fn get_edid(&self, scanout: u32) -> Result<Box<Edid>, NtStatus> {
        let cmd = commands::GetEdid {
            header: self.new_header(commands::Command::GET_EDID, true, None, None),
            scanout,
            _padding: 0,
        };

        let out = self.control.request_blocking_into_buf(cmd, MaybeInlineBuffer::try_new_zeroed::<commands::RespGetEdid>()?)?;

        let resp = commands::RespGetEdid::ref_from_prefix(out.as_ref()).map_err(|e| {
            error!("{}: failed to request EDID for scanout {}: {:?}", function!(), scanout, e);
            STATUS::IO_DEVICE_ERROR
        })?.0;

        map_virtio_error!(resp.header.check_type(commands::Command::OK_EDID)).inspect_err(|_|
            error!("{}: failed to request EDID for scanout {}: invalid response type", function!(), scanout)
        )?;

        let edid = Box::try_new(Edid {
            data: resp.edid,
            size: resp.size,
        })?;
        Ok(edid)
    }

    pub fn set_scanout(&self, rect: commands::Rect, scanout: u32, res_id: NonZero<u32>) -> Result<(), NtStatus> {
        let cmd = commands::SetScanout {
            header: self.new_header(commands::Command::SET_SCANOUT, false, None, None),
            rect,
            scanout_id: scanout,
            resource_id: res_id.get(),
        };

        // FIXME: we cannot really do blocking stuff here. Timer callbacks seem to be called with an IRQL that is too high
        // let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        // map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_|
        //     warn!("{}: failed to set scanout {} to {}", function!(), scanout, res_id.get())
        // )?;

        self.control.request_async::<_, commands::CtrlHeader>(cmd, Callback::None)?;

        Ok(())
    }

    pub fn set_scanout_blob(&self, rect: commands::Rect, scanout: u32, res_id: NonZero<u32>, info: BlobInfo) -> Result<(), NtStatus> {
        let cmd = commands::SetScanoutBlob {
            header: self.new_header(commands::Command::SET_SCANOUT_BLOB, false, None, None),
            rect,
            scanout_id: scanout,
            resource_id: res_id.get(),
            width: info.width,
            height: info.height,
            format: info.format,
            _padding: 0,
            strides: info.strides,
            offsets: info.offsets,
        };

        // FIXME: we cannot really do blocking stuff here. Timer callbacks seem to be called with an IRQL that is too high
        // let resp: commands::CtrlHeader = self.control.request_blocking(cmd)?;
        // map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_|
        //     warn!("{}: failed to set scanout {} to {}", function!(), scanout, res_id.get())
        // )?;

        self.control.request_async::<_, commands::CtrlHeader>(cmd, Callback::None)?;

        Ok(())
    }

    pub fn move_cursor(&self, scanout_id: u32, x: u32, y: u32) -> Result<(), NtStatus> {
        let cmd = commands::UpdateCursor {
            header: self.new_header(commands::Command::MOVE_CURSOR, true, None, None),
            pos: commands::CursorPos {
                scanout_id,
                x,
                y,
                _padding: 0,
            },
            /* Should be ignored by the device */
            resource_id: 0,
            hot_x: 0,
            hot_y: 0,
            _padding: 0,
        };


        // FIXME: it seems that QEMU does not handle the response properly

        self.cursor.request_async::<_, commands::CtrlHeader>(cmd, Callback::None)?;

        //let resp: commands::CtrlHeader = self.cursor.request_blocking(cmd)?;
        // FIXME: this errors out even though cursor is set just fine
        // map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_|
        //     warn!("{}: failed to move cursor on {}", function!(), scanout_id)
        // )?;

        Ok(())
    }

    pub fn update_cursor(&self, scanout_id: u32, resource_id: NonZero<u32>, hot_x: u32, hot_y: u32, x: u32, y: u32) -> Result<(), NtStatus> {
        let cmd = commands::UpdateCursor {
            header: self.new_header(commands::Command::UPDATE_CURSOR, true, None, None),
            pos: commands::CursorPos {
                scanout_id,
                x,
                y,
                _padding: 0,
            },
            resource_id: resource_id.get(),
            hot_x,
            hot_y,
            _padding: 0,
        };

        // FIXME: it seems that QEMU does not handle the response properly

        self.cursor.request_async::<_, commands::CtrlHeader>(cmd, Callback::None)?;

        //let resp: commands::CtrlHeader = self.cursor.request_blocking(cmd)?;
        // FIXME: this errors out even though cursor is set just fine
        // map_virtio_error!(resp.check_type(commands::Command::OK_NODATA)).inspect_err(|_|
        //     warn!("{}: failed to update cursor on {}", function!(), scanout_id)
        // )?;

        Ok(())
    }
}

const _: () = assert!(size_of::<GpuChannel>() == 24);

pub struct QueueHandler {
    pci_transport: PciTransport,
    control: ControlQueue,
    cursor: CursorQueue,
    data: Arc<GpuData>,
    pub chan: GpuChannel,
    thread: Pin<Box<Thread>>,
}

const _: () = assert!(size_of::<QueueHandler>() <= 21504*2);

unsafe impl Send for QueueHandler {}

impl QueueHandler {
    pub fn new(mut pci_transport: PciTransport, interface: DxgkInterface, negotiated_features: Features, shmem: VirtioCapabilityInfo) -> impl Init<Self, NtStatus> {
        //trace!("{}: {}", function!(), io_get_remaining_stack_size());

        let access_platform = negotiated_features.contains(Features::ACCESS_PLATFORM);
        let indirect = negotiated_features.contains(Features::RING_INDIRECT_DESC);
        let event_idx = negotiated_features.contains(Features::RING_EVENT_IDX);

        init_scope(move || {
            Ok(init!(Self {
                control <- ControlQueue::new(&mut pci_transport, access_platform, indirect, event_idx),
                cursor <- CursorQueue::new(&mut pci_transport, access_platform, indirect, event_idx),
                pci_transport: pci_transport,
                thread: Box::try_pin_init(Thread::new())?,
                data: Arc::try_init(GpuData::new(interface, shmem))?,
                chan: GpuChannel {
                    control: control.chan.clone(),
                    cursor: cursor.chan.clone(),
                    data: data.clone(),
                },
            }? NtStatus))
        })
    }

    pub fn channel(&self) -> GpuChannel {
        GpuChannel {
            control: self.control.chan.clone(),
            cursor: self.cursor.chan.clone(),
            data: self.data.clone(),
        }
    }

    pub fn get_shmem_slice(&self) -> (u64, u64) {
        let phys = self.data.interface.get_physical_bar_address(self.data.shmem.bar).unwrap() + self.data.shmem.offset;
        let size = self.data.shmem.length;
        (phys, size)
    }

    pub fn dxgk_interface(&self) -> &DxgkInterface {
        &self.data.interface
    }

    pub fn check_engine(&self, engine: Engine) -> bool {
        self.data.engines[engine.node_ordinal() as usize].is_responsive()
    }

    pub fn last_completed_fence(&self, engine: Engine) -> u32 {
        self.data.engines[engine.node_ordinal() as usize].last_completed_fence.load(Ordering::SeqCst)
    }

    pub fn last_submitted_fence(&self, engine: Engine) -> u32 {
        self.data.engines[engine.node_ordinal() as usize].last_submitted_fence.load(Ordering::SeqCst)
    }

    fn handler_thread_routine(&mut self) {
        trace!("{}", function!());

        let chan = self.channel();

        let control_msg = chan.control.get_queue_event();
        let cursor_msg = chan.cursor.get_queue_event();
        let events = self.thread.events();

        //let last_completed_fence = &self.last_completed_fence;
        //let fence_queue = &self.fence_queue;
        //let interface = &self.data.interface;

        //let notify_dma = |engine: Engine, fence: u32| {
        //    debug!("{}: engine {:?} fence completed: {}", function!(), engine, fence);
        //    chan.data.notify_dma_completed(engine, fence);
        //    Ok(())
        //};

        loop {
            // TODO: timeout (1s) => handle all in case something was missed
            select! {
                events.control_irq => {
                    //trace!("- new control response");
                    self.control.handle_responses(&chan.data);
                    // We need this in case queue was full last time we got a control request
                    self.control.handle_requests(&mut self.pci_transport);
                },
                control_msg => {
                    //trace!("- new control request");
                    //warn!("{}: select: timestamp {:?}", function!(), ke_query_performance_counter());
                    self.control.handle_requests(&mut self.pci_transport);
                },
                events.cursor_irq => {
                    //trace!("- new cursor response");
                    self.cursor.handle_responses(&chan.data);
                    // We need this in case queue was full last time we got a cursor request
                    self.cursor.handle_requests(&mut self.pci_transport);
                },
                cursor_msg => {
                    //trace!("- new cursor request");
                    self.cursor.handle_responses(&chan.data);
                },
                events.config_irq => {
                    // TODO
                    //info!("- new config change");
                },
                events.stop => {
                    info!("- stopping thread");
                    return;
                };
                error(e) => {
                    error!("{}: error: {:?}", function!(), e);
                },
            };

            chan.data.handle_fence_submissions();

            //trace!("- waiting for next message");
        }
    }

    pub fn start_handler_thread(&mut self) -> Result<(), NtStatus> {
        let thread_routine = |h: &mut Self| {
            info!("running queue handler thread");
            h.handler_thread_routine();
            //warn!("exiting queue handler thread");
            //let err = KeThread::terminate(STATUS::SUCCESS);
            //warn!("failed to exit handler thread: {:?}", err);
        };
        let context = self as *mut Self;
        self.thread.start(thread_routine, context)?;

        Ok(())
    }

    pub fn ack_interrupt(&mut self) {
        let irq = self.pci_transport.ack_interrupt();
        self.data.irq.fetch_or(irq.bits(), Ordering::SeqCst);
    }

    pub fn kick(&self) {
        self.control.chan.get_queue_event().set();
        self.cursor.chan.get_queue_event().set();
        self.thread.events().control_irq.set();
        self.thread.events().cursor_irq.set();
        self.thread.events().config_irq.set();
    }

    pub fn handle_dpc(&self) {
        let irq = InterruptStatus::from_bits_truncate(self.data.irq.load(Ordering::SeqCst));

        if irq.contains(InterruptStatus::QUEUE_INTERRUPT) {
            self.thread.events().control_irq.set();
            self.thread.events().cursor_irq.set();
        }
        if irq.contains(InterruptStatus::DEVICE_CONFIGURATION_INTERRUPT) {
            self.thread.events().config_irq.set();
        }
    }

    pub fn notify_dma_preempted(&self, engine: Engine, preemption_fence: u32, last_completed_fence: u32) {
        self.data.notify_dma_preempted(engine, preemption_fence, last_completed_fence);
    }

    pub fn notify_queued_fences(&self, engine: Engine) {
        self.data.notify_queued_fences(engine);
    }
}

impl Drop for QueueHandler {
    fn drop(&mut self) {
        trace!("{}", function!());
        self.thread.stop();

        //for res_id in self.data.resource_id.free.lock().iter() {
        //    warn!("{}: used resource id: {}", function!(), res_id);
        //}
        //
        //for ctx_id in self.data.context_id.free.lock().iter() {
        //    warn!("{}: used context id: {}", function!(), ctx_id);
        //}

        if let Some(thread) = self.thread.thread().take() {
            info!("{}: waiting for queue handler thread to finish...", function!());
            let _ = thread.join(NtTime::INFINITE).inspect_err(|e|
                error!("{}: failed to wait for thread to finish: {:?}", function!(), e)
            );
        }
        info!("{}: queue handler thread stopped", function!());

        self.pci_transport.queue_unset(QUEUE_TRANSMIT);
        self.pci_transport.queue_unset(QUEUE_CURSOR);
    }
}
