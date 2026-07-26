#![no_std]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
//#![feature(stmt_expr_attributes)]
//#![feature(ptr_alignment_type)]
#![feature(allocator_api)]
//#![feature(get_mut_unchecked)]
//#![feature(generic_const_exprs)]
//#![feature(slice_ptr_get)]
//#![feature(offset_of_enum)]
//#![feature(const_default)]
//#![feature(const_trait_impl)]
//#![feature(derive_const)]

#[macro_use]
extern crate log;

extern crate alloc;

use core::{
    panic::PanicInfo,
    ffi::*,
    ptr::*,
    mem::{
        transmute,
        zeroed,
    },
    cell::UnsafeCell,
    sync::atomic::Ordering,
};

use alloc::boxed::Box;
use alloc::sync::Arc;
use smallvec::*;

use winresult::STATUS;

use wdk::{
    dxgkrnl::*,
    wdm::{
        MdlRef,
        WdkAllocator,
        ke_query_performance_counter,
    },
    *,
};

mod adapter;
mod logger;
mod uapi;
mod queue;
mod init_option;
mod device;
mod allocation;
mod command;
mod virgl;
mod vidpn;
mod process;

// TODO: update tag to invalid for all tagged structs on drop
pub const VIRTIO_GPU_INVALID_TAG: u64 = 0xDEAD_DEAD_DEAD_DEAD;

use crate::uapi::{Tagged, TaggedExt, CreateAllocation, CreateResource, Allocate3d, Pointer64, ContextInit, ESCAPE_CONTEXT_INIT_TAG, CapsetId};
use crate::adapter::{Adapter, NtStatus, Engine};
use crate::device::{Device, DeviceContext};
use crate::allocation::Allocation;
use crate::command::{Command, CommandDmaPrivate};
use crate::virgl::*;
use crate::process::*;

macro_rules! check_arg {
    (mut $arg:expr) => {{
        if !$arg.is_null() {
            unsafe { &mut *$arg }
        } else {
            error!("{}: parameter is null", function!());
            return STATUS::INVALID_PARAMETER.to_u32()
        }
    }};
    ($arg:expr) => {{
        if !$arg.is_null() {
            unsafe { &*$arg }
        } else {
            error!("{}: parameter is null", function!());
            return STATUS::INVALID_PARAMETER.to_u32()
        }
    }};
}

macro_rules! check_handle {
    ($handle:ident : $ty:ty) => {{
        match <$ty as TaggedExt>::from_handle_mut($handle) {
            Some(v) => v,
            None => return STATUS::INVALID_PARAMETER.to_u32(),
        }
    }};
    ($struct:ident . $handle:ident : $ty:ty) => {{
        match <$ty as TaggedExt>::from_handle_mut($struct.$handle) {
            Some(v) => v,
            None => return STATUS::INVALID_PARAMETER.to_u32(),
        }
    }};
}

macro_rules! check_handle_arc {
    ($handle:ident : $ty:ty) => {{
        match <$ty as TaggedExt>::from_arc_handle_clone($handle) {
            Some(v) => v,
            None => return STATUS::INVALID_PARAMETER.to_u32(),
        }
    }};
    ($struct:ident . $handle:ident : $ty:ty) => {{
        match <$ty as TaggedExt>::from_arc_handle_clone($struct.$handle) {
            Some(v) => v,
            None => return STATUS::INVALID_PARAMETER.to_u32(),
        }
    }};
}

pub fn slice_from_raw_parts<'a, T>(ptr: *const T, size: usize) -> &'a [T] {
    unsafe {
        let ptr = if size == 0 {
            NonNull::dangling().as_ptr()
        } else {
            ptr
        };

        core::slice::from_raw_parts(ptr, size)
    }
}

pub fn slice_from_raw_parts_mut<'a, T>(ptr: *mut T, size: usize) -> &'a mut [T] {
    unsafe {
        let ptr = if size == 0 {
            NonNull::dangling().as_ptr()
        } else {
            ptr
        };

        core::slice::from_raw_parts_mut(ptr, size)
    }
}

const BUILD_VERSION: &'static str = env!("BUILD_VERSION");
const BUILD_DATE:    &'static str = env!("BUILD_DATE");

#[global_allocator]
static GLOBAL_ALLOC: WdkAllocator = WdkAllocator;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn driver_entry(
    driver: *mut DRIVER_OBJECT,
    registry_path: *mut UNICODE_STRING,
) -> NTSTATUS {
    //logger::init(log::LevelFilter::Trace).unwrap();
    logger::init(log::LevelFilter::Warn).unwrap();

    warn!("starting VirtIO GPU {BUILD_VERSION} ({BUILD_DATE})");

    const _: () = assert!(size_of::<DXGK_DRIVERCAPS>() == 0x240);

    let mut initial_data : DRIVER_INITIALIZATION_DATA = unsafe { zeroed() };
    //initial_data.Version = DXGKDDI_INTERFACE_VERSION_WDDM2_3;
    initial_data.Version = DXGKDDI_INTERFACE_VERSION_WDDM2_0;

    initial_data.DxgkDdiAddDevice = Some(add_device);
    initial_data.DxgkDdiStartDevice = Some(start_device);
    initial_data.DxgkDdiStopDevice = Some(stop_device);
    initial_data.DxgkDdiRemoveDevice = Some(remove_device);
    initial_data.DxgkDdiUnload = Some(unload);

    initial_data.DxgkDdiDispatchIoRequest = Some(dispatch_io_request);
    initial_data.DxgkDdiInterruptRoutine = Some(interrupt_routine);
    initial_data.DxgkDdiDpcRoutine = Some(dpc_routine);

    initial_data.DxgkDdiQueryChildRelations = Some(query_child_relations);
    initial_data.DxgkDdiQueryChildStatus = Some(query_child_status);
    initial_data.DxgkDdiQueryDeviceDescriptor = Some(query_device_descriptor);
    initial_data.DxgkDdiSetPowerState = Some(set_power_state);
    initial_data.DxgkDdiResetDevice = Some(reset_device);

    initial_data.DxgkDdiQueryAdapterInfo = Some(query_adapter_info);
    initial_data.DxgkDdiEscape = Some(escape);
    initial_data.DxgkDdiCreateAllocation = Some(create_allocation);
    initial_data.DxgkDdiOpenAllocation = Some(open_allocation);
    initial_data.DxgkDdiCloseAllocation = Some(close_allocation);
    initial_data.DxgkDdiDescribeAllocation = Some(describe_allocation);
    initial_data.DxgkDdiDestroyAllocation = Some(destroy_allocation);
    initial_data.DxgkDdiGetStandardAllocationDriverData = Some(get_standard_allocation_driver_data);
    initial_data.DxgkDdiBuildPagingBuffer = Some(build_paging_buffer);

    initial_data.DxgkDdiCreateDevice = Some(create_device);
    initial_data.DxgkDdiDestroyDevice = Some(destroy_device);
    initial_data.DxgkDdiStopDeviceAndReleasePostDisplayOwnership = Some(stop_device_and_release_post_display_ownership);

    // initial_data.DxgkDdiAcquireSwizzlingRange = Some(acquire_swizzling_range);
    // initial_data.DxgkDdiReleaseSwizzlingRange = Some(release_swizzling_range);

    initial_data.DxgkDdiCreateContext = Some(create_context);
    initial_data.DxgkDdiDestroyContext = Some(destroy_context);

    initial_data.DxgkDdiPresent = Some(present);
    initial_data.DxgkDdiRender = Some(render);
    initial_data.DxgkDdiPatch = Some(patch);
    initial_data.DxgkDdiSubmitCommand = Some(submit_command);

    initial_data.DxgkDdiSetPointerPosition = Some(set_pointer_position);
    initial_data.DxgkDdiSetPointerShape = Some(set_pointer_shape);
    initial_data.DxgkDdiIsSupportedVidPn = Some(is_supported_vidpn);
    initial_data.DxgkDdiRecommendFunctionalVidPn = Some(recommend_functional_vidpn);
    initial_data.DxgkDdiEnumVidPnCofuncModality = Some(enum_vidpn_cofunc_modality);
    initial_data.DxgkDdiSetVidPnSourceVisibility = Some(set_vidpn_source_visibility);
    initial_data.DxgkDdiCommitVidPn = Some(commit_vidpn);
    initial_data.DxgkDdiUpdateActiveVidPnPresentPath = Some(update_active_vidpn_present_path);
    initial_data.DxgkDdiSetVidPnSourceAddress = Some(set_vidpn_source_address);
    initial_data.DxgkDdiRecommendMonitorModes = Some(recommend_monitor_modes);
    initial_data.DxgkDdiQueryVidPnHWCapability = Some(query_vidpn_hw_capability);
    initial_data.DxgkDdiSystemDisplayEnable = Some(system_display_enable);
    initial_data.DxgkDdiSystemDisplayWrite = Some(system_display_write);

    initial_data.DxgkDdiPreemptCommand = Some(preempt_command);
    initial_data.DxgkDdiResetFromTimeout = Some(reset_from_timeout);
    initial_data.DxgkDdiRestartFromTimeout = Some(restart_from_timeout);
    initial_data.DxgkDdiQueryDependentEngineGroup = Some(query_dependent_engine_group);
    initial_data.DxgkDdiCollectDbgInfo = Some(collect_dbg_info);
    initial_data.DxgkDdiQueryCurrentFence = Some(query_current_fence);
    initial_data.DxgkDdiQueryEngineStatus = Some(query_engine_status);
    initial_data.DxgkDdiResetEngine = Some(reset_engine);
    //initial_data.DxgkDdiCancelCommand = Some(cancel_command);

    initial_data.DxgkDdiControlInterrupt = Some(control_interrupt);
    initial_data.DxgkDdiGetScanLine = Some(get_scan_line);

    initial_data.DxgkDdiSetVidPnSourceAddressWithMultiPlaneOverlay = Some(set_vidpn_source_address_with_multiplane_overlay);

    // WDDMv1.3
    initial_data.DxgkDdiGetNodeMetadata = Some(get_node_metadata);
    initial_data.DxgkDdiControlInterrupt2 = Some(control_interrupt2);
    initial_data.DxgkDdiFormatHistoryBuffer = Some(format_history_buffer);
    initial_data.DxgkDdiCalibrateGpuClock = Some(calibrate_gpu_clock);
    initial_data.DxgkDdiCheckMultiPlaneOverlaySupport = Some(check_multiplane_overlay_support);

    // WDDMv2
    //initial_data.DxgkDdiRenderGdi = Some(render_gdi);
    initial_data.DxgkDdiSubmitCommandVirtual = Some(submit_command_virtual);
    initial_data.DxgkDdiSetRootPageTable = Some(set_root_page_table);
    initial_data.DxgkDdiGetRootPageTableSize = Some(get_root_page_table_size);
    initial_data.DxgkDdiMapCpuHostAperture = Some(map_cpu_host_aperture);
    initial_data.DxgkDdiUnmapCpuHostAperture = Some(unmap_cpu_host_aperture);
    initial_data.DxgkDdiCheckMultiPlaneOverlaySupport2 = Some(check_multiplane_overlay_support2);
    initial_data.DxgkDdiCreateProcess = Some(create_process);
    initial_data.DxgkDdiDestroyProcess = Some(destroy_process);
    initial_data.DxgkDdiSetVidPnSourceAddressWithMultiPlaneOverlay2 = Some(set_vidpn_source_address_with_multiplane_overlay2);
    initial_data.DxgkDdiPowerRuntimeSetDeviceHandle = Some(power_runtime_set_device_handle);
    initial_data.DxgkDdiSetStablePowerState = Some(set_stable_power_state);
    initial_data.DxgkDdiSetVideoProtectedRegion = Some(set_video_protected_region);

    // WDDM2.2
    //initial_data.DxgkDdiCreateHwContext = Some(create_hw_context);
    //initial_data.DxgkDdiDestroyHwContext = Some(destroy_hw_context);
    //initial_data.DxgkDdiCreateHwQueue = Some(create_hw_queue);
    //initial_data.DxgkDdiDestroyHwQueue = Some(destroy_hw_queue);
    //initial_data.DxgkDdiSubmitCommandToHwQueue = Some(submit_command_to_hw_queue);
    //initial_data.DxgkDdiSwitchToHwContextList = Some(switch_to_hw_context_list);
    //initial_data.DxgkDdiResetHwEngine = Some(reset_hw_engine);

    let status = NtStatus::from(unsafe { DxgkInitialize(driver, registry_path, &mut initial_data) });

    if !status.is_success() {
        error!("failed to initialize: {:x?}", status);
    } else {
        trace!("successful dxgk init");
    }

    status.to_u32()
}

unsafe extern "C" fn unload() {
    warn!("unloading VirtIO GPU driver");
    logger::deinit();
}

unsafe extern "C" fn add_device(device: *mut DEVICE_OBJECT, adapter: *mut HANDLE) -> NTSTATUS {
    info!("{}", function!());
    let Some(dev) = NonNull::new(device) else {
        error!("physical device object is null");
        return STATUS::INVALID_PARAMETER.to_u32();
    };
    let gpu: Box<Adapter> = match Adapter::new(dev.cast::<UnsafeCell<_>>()) {
        Ok(gpu) => gpu,
        Err(code) => return code.to_u32(),
    };

    unsafe { *adapter = TaggedExt::into_handle(gpu); }

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn remove_device(adapter: HANDLE) -> NTSTATUS {
    info!("{}", function!());

    if adapter.is_null() {
        error!("{}: adapter handle is null", function!());
        return STATUS::INVALID_PARAMETER.to_u32();
    }

    unsafe { drop(Box::from_raw(adapter as *mut Adapter)); }

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn start_device(adapter: HANDLE, start_info: *mut DXGK_START_INFO, interface: *mut DXGKRNL_INTERFACE, num_outputs: *mut ULONG, num_children: *mut ULONG) -> NTSTATUS {
    info!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let interface = *check_arg!(interface);
    let start_info = check_arg!(start_info);
    let num_outputs = check_arg!(mut num_outputs);
    let num_children = check_arg!(mut num_children);

    match gpu.start(start_info, interface) {
        Ok(n_scanouts) => {
            info!("{}: Detected {} scanouts", function!(), n_scanouts);

            *num_outputs  = n_scanouts as _;
            *num_children = n_scanouts as _;

            STATUS::SUCCESS
        }
        Err(status) => {
            error!("{}: failed to start device: {:?}", function!(), status);
            status.0
        },
    }.to_u32()


}

unsafe extern "C" fn stop_device(adapter: HANDLE) -> NTSTATUS {
    info!("{}", function!());
    let gpu = check_handle!(adapter: Adapter);

    match gpu.stop() {
        Ok(()) => {
            warn!("{}: device stopped", function!());
            STATUS::SUCCESS
        }
        Err(status) => {
            error!("{}: failed to stop device: {:?}", function!(), status);
            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn dispatch_io_request(
    adapter: HANDLE,
    vidpn_source_id: ULONG,
    video_request_packet: *mut VIDEO_REQUEST_PACKET,
) -> NTSTATUS {
    trace!("{}", function!());

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn set_power_state(
    adapter: HANDLE,
    hardware_uid: ULONG,
    device_power_state: DEVICE_POWER_STATE,
    action_type: POWER_ACTION,
) -> NTSTATUS {
    warn!("{}", function!());

    //STATUS::NOT_IMPLEMENTED.to_u32()
    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn query_child_relations(adapter: HANDLE, child_relations: *mut DXGK_CHILD_DESCRIPTOR, child_relations_size: ULONG) -> NTSTATUS {
    trace!("{}: {:?}@{}", function!(), child_relations, child_relations_size);
    let gpu = check_handle!(adapter: Adapter);

    const _: () = assert!(size_of::<DXGK_CHILD_DESCRIPTOR>() == 28);

    let child_relations_count = (child_relations_size as usize) / size_of::<DXGK_CHILD_DESCRIPTOR>() - 1;
    let child_relations = slice_from_raw_parts_mut(child_relations, child_relations_count);

    gpu.query_child_relations(child_relations);

    /*
    for child in child_relations {
        warn!("ChildDeviceType: {:?}", child.ChildDeviceType);
        warn!("ChildCapabilities.HpdAwareness: {:?}", child.ChildCapabilities.HpdAwareness);
        let video_output = unsafe { &mut child.ChildCapabilities.Type.VideoOutput};
        warn!("ChildCapabilities.Type.VideoOutput.InterfaceTechnology: {:?}", video_output.InterfaceTechnology);
        warn!("ChildCapabilities.Type.VideoOutput.MonitorOrientationAwareness: {:?}", video_output.MonitorOrientationAwareness);
        warn!("ChildCapabilities.Type.VideoOutput.SupportsSdtvModes: {:?}", video_output.SupportsSdtvModes);
        warn!("AcpiUid: {:?}", child.AcpiUid);
        warn!("ChildUid: {:?}", child.ChildUid);
    }
    */

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn query_child_status(adapter: HANDLE, child_status: *mut DXGK_CHILD_STATUS, non_destructive_only: BOOLEAN) -> NTSTATUS {
    info!("{}", function!());
    let gpu = check_handle!(adapter: Adapter);
    let child_status = check_arg!(mut child_status);

    let num_scanouts = match gpu.num_scanouts() {
        Ok(num_scanouts) => num_scanouts as u32,
        Err(e) => return e.0.to_u32(),
    };

    assert!(child_status.ChildUid < num_scanouts);

    match child_status.Type {
        DXGK_CHILD_STATUS_TYPE::StatusConnection => {
            child_status.__bindgen_anon_1.HotPlug.Connected = gpu.queue_handler().is_some() as _;
        },
        _ => {
            error!("{}: invalid child status query: {:?}", function!(), child_status.Type);
        }
    }

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn query_device_descriptor(adapter: HANDLE, child_uid: ULONG, device_descriptor: *mut DXGK_DEVICE_DESCRIPTOR) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let device_descriptor = check_arg!(mut device_descriptor);

    let edid = match gpu.get_edid(child_uid as usize) {
        Ok(edid) => edid,
        Err(e) => return e.to_u32(),
    };

    let offset = device_descriptor.DescriptorOffset as usize;

    debug!("{}: edid len: {}, offset: {}, space: {}", function!(), edid.len(), offset, device_descriptor.DescriptorLength);

    if offset >= edid.len() {
        return STATUS::MONITOR_NO_MORE_DESCRIPTOR_DATA.to_u32();
    }

    let len = core::cmp::min(device_descriptor.DescriptorLength as usize, edid.len() - offset);
    let descriptor = slice_from_raw_parts_mut(device_descriptor.DescriptorBuffer as *mut u8, len);

    descriptor.copy_from_slice(&edid[offset..offset+len]);
    device_descriptor.DescriptorLength = len as u32;

    debug!("{}: copied range: {}..{}", function!(), offset, offset+len);

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn reset_device(adapter: HANDLE) {
    info!("{}", function!());
}

unsafe extern "C" fn interrupt_routine(adapter: HANDLE, message_number: ULONG) -> BOOLEAN {
    // Logging here is a bad idea, apparently
    //info!("{}", function!());

    let Some(gpu): Option<&mut Adapter> = TaggedExt::from_handle_mut(adapter) else {
        return false as _;
    };

    gpu.handle_interrupt(message_number) as _
}

unsafe extern "C" fn dpc_routine(context: HANDLE) {
    //info!("{}", function!());

    let Some(gpu): Option<&mut Adapter> = TaggedExt::from_handle_mut(context) else {
        return;
    };

    gpu.handle_dpc();
}

unsafe extern "C" fn query_adapter_info(adapter: HANDLE, query_adapter_info: *const DXGKARG_QUERYADAPTERINFO) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let query_info = check_arg!(query_adapter_info);

    match gpu.query_info(query_info) {
        Ok(()) => {
            //if unsafe { (*query_adapter_info).Type } == DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_QUERYSEGMENT3 {
            //    let segment_info = unsafe { core::mem::transmute::<_, &DXGK_QUERYSEGMENTOUT3>((*query_adapter_info).pOutputData) };
            //
            //    debug!("segment_info.NbSegment: {}", segment_info.NbSegment);
            //    debug!("segment_info.PagingBufferPrivateDataSize: {}", segment_info.PagingBufferPrivateDataSize);
            //    debug!("segment_info.PagingBufferSegmentId: {}", segment_info.PagingBufferSegmentId);
            //    debug!("segment_info.PagingBufferSize: {}", segment_info.PagingBufferSize);
            //
            //    if !segment_info.pSegmentDescriptor.is_null() {
            //        let descriptors = unsafe {
            //            let descriptors: &[DXGK_SEGMENTDESCRIPTOR3] = core::slice::from_raw_parts(segment_info.pSegmentDescriptor, segment_info.NbSegment as _);
            //
            //            descriptors
            //        };
            //
            //        for (i, descriptor) in descriptors.into_iter().enumerate() {
            //            debug!("segment_info.pSegmentDescriptor[{}]: addr = {}, size = {}, commit_limit = {}, flags = {}", i, unsafe { descriptor.BaseAddress.QuadPart }, descriptor.Size, descriptor.CommitLimit,
            //                unsafe { descriptor.Flags.__bindgen_anon_1.Value },
            //            );
            //        }
            //    } else {
            //        debug!("segment_info.pSegmentDescriptor: <empty>");
            //    }
            //} else
            //if unsafe { (*query_adapter_info).Type } == DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_DRIVERCAPS{
            //    let driver_caps = unsafe { core::mem::transmute::<_, &DXGK_DRIVERCAPS>((*query_adapter_info).pOutputData) };
            //
            //    warn!("InterruptMessageNumber: {:?}", driver_caps.InterruptMessageNumber);
            //    warn!("WDDMVersion: {:?}", driver_caps.WDDMVersion);
            //    warn!("HighestAcceptableAddress: {}", unsafe { driver_caps.HighestAcceptableAddress.QuadPart });
            //    warn!("PreemptionCaps.GraphicsPreemptionGranularity: {:?}", driver_caps.PreemptionCaps.GraphicsPreemptionGranularity);
            //    warn!("PreemptionCaps.ComputePreemptionGranularity: {:?}", driver_caps.PreemptionCaps.ComputePreemptionGranularity);
            //    warn!("FlipCaps: {}", unsafe { driver_caps.FlipCaps.__bindgen_anon_1.Value });
            //    warn!("MaxQueuedFlipOnVSync: {}", driver_caps.MaxQueuedFlipOnVSync);
            //    warn!("MemoryManagementCaps: {}", unsafe { driver_caps.MemoryManagementCaps.__bindgen_anon_1.Value });
            //    warn!("SupportDirectFlip: {}", driver_caps.SupportDirectFlip);
            //    warn!("SchedulingCaps: {}", unsafe { driver_caps.SchedulingCaps.__bindgen_anon_1.Value });
            //    warn!("GpuEngineTopology.NbAsymetricProcessingNodes: {}", driver_caps.GpuEngineTopology.NbAsymetricProcessingNodes);
            //    warn!("SupportSmoothRotation: {}", driver_caps.SupportSmoothRotation);
            //    warn!("SupportNonVGA: {}", driver_caps.SupportNonVGA);
            //}

            STATUS::SUCCESS
        },
        Err(NtStatus(STATUS::NOT_SUPPORTED)) => STATUS::NOT_SUPPORTED,
        Err(status) => {
            error!("failed to query adapter info: {:?}", status);
            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn get_node_metadata(adapter: HANDLE, node_ordinal: UINT, get_node_metadata: *mut DXGKARG_GETNODEMETADATA) -> NTSTATUS {
    trace!("{}", function!());

    let get_node_metadata = check_arg!(mut get_node_metadata);

    let Some(engine) = Engine::try_from_node_ordinal(node_ordinal) else {
        trace!("unsupported node: {}", node_ordinal);
        return STATUS::INVALID_PARAMETER.to_u32();
    };
    *get_node_metadata = unsafe { zeroed() };
    engine.fill_metadata(get_node_metadata);
    trace!("report node {}: {:?}", node_ordinal, engine);

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn set_pointer_position(adapter: HANDLE, set_pointer_position: *const DXGKARG_SETPOINTERPOSITION) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let set_pointer_position = check_arg!(set_pointer_position);

    match gpu.move_cursor(set_pointer_position) {
        Ok(()) => STATUS::SUCCESS,
        Err(e) => {
            error!("failed to move cursor: {:?}", e);
            e.0
        },
    }.to_u32()
}

unsafe extern "C" fn set_pointer_shape(adapter: HANDLE, set_pointer_shape: *const DXGKARG_SETPOINTERSHAPE) -> NTSTATUS {
    trace!("{}", function!());
    let gpu = check_handle!(adapter: Adapter);
    let set_pointer_shape = check_arg!(set_pointer_shape);

    //info!("{}: {} -> {}x{} / {}x{} {:?}", function!(), set_pointer_shape.VidPnSourceId, set_pointer_shape.Width, set_pointer_shape.Height, set_pointer_shape.XHot, set_pointer_shape.YHot, set_pointer_shape.Flags);

    match gpu.update_cursor(set_pointer_shape) {
        Ok(()) => STATUS::SUCCESS,
        Err(e) => {
            error!("failed to update cursor: {:?}", e);
            e.0
        },
    }.to_u32()
}

unsafe extern "C" fn escape(adapter: HANDLE, escape: *const DXGKARG_ESCAPE) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let escape = check_arg!(escape);

    match gpu.escape(escape) {
        Ok(()) => STATUS::SUCCESS,
        Err(NtStatus(STATUS::NOT_SUPPORTED)) => STATUS::NOT_SUPPORTED,
        Err(status) => {
            /* You cannot escape lol */
            if escape.PrivateDriverDataSize as usize >= core::mem::size_of::<u64>() {
                let escape_tag = uapi::Tag::from_handle(escape.pPrivateDriverData);
                error!("failed to escape {:?}: {:?}", escape_tag, status);
            } else {
                error!("failed to escape (not enough data: {}): {:?}", escape.PrivateDriverDataSize, status);
            }

            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn create_allocation(adapter: HANDLE, create_allocation: *mut DXGKARG_CREATEALLOCATION) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let create_allocation = check_arg!(mut create_allocation);

    match gpu.allocate(create_allocation) {
        Ok(()) => {
            let alloc_info = unsafe { &*create_allocation.pAllocationInfo };
            debug!("{}: allocation info: {:?}", function!(), alloc_info);

            STATUS::SUCCESS
        },
        Err(status) => {
            error!("failed to allocate: {:?}", status);
            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn open_allocation(device: HANDLE, open_allocation: *const DXGKARG_OPENALLOCATION) -> NTSTATUS {
    trace!("{}", function!());
    let device = check_handle_arc!(device: Device);
    let open_allocation = check_arg!(open_allocation);
    let allocations = slice_from_raw_parts_mut(open_allocation.pOpenAllocation, open_allocation.NumAllocations as _);

    /*
    if open_allocation.Flags.Create() {
        let mut alloc_infos = SmallVec::<[DXGK_ALLOCATIONINFO; 1]>::new();

        for alloc in allocations.iter() {
            let alloc_info = DXGK_ALLOCATIONINFO {
                pPrivateDriverData: alloc.pPrivateDriverData,
                PrivateDriverDataSize: alloc.PrivateDriverDataSize,
                ..unsafe {zeroed()}
            };

            alloc_infos.push(alloc_info);
        }

        let mut create_allocation = DXGKARG_CREATEALLOCATION {
            pPrivateDriverData: open_allocation.pPrivateDriverData,
            PrivateDriverDataSize: open_allocation.PrivateDriverSize,
            NumAllocations: open_allocation.NumAllocations,
            pAllocationInfo: alloc_infos.as_mut_ptr(),
            hResource: null_mut(),
            Flags: unsafe { zeroed() },
        };

        warn!("{}: Allocating in open_allocation, alloc count: {}", function!(), allocations.len());

        match device.allocate(&mut create_allocation, allocations) {
            Ok(()) => {
                debug!("{}: creating device allocation succeded!", function!());
                STATUS::SUCCESS
            },
            Err(status) => {
                error!("failed to create device allocation: {:?}", status);
                status.0
            },
        }.to_u32()
    } else {
    */
        match device.open_allocation(open_allocation.Flags, allocations) {
            Ok(()) => {
                debug!("{}: open allocation succeded!", function!());
                STATUS::SUCCESS
            },
            Err(status) => {
                error!("failed to open allocation: {:?}", status);
                status.0
            },
        }.to_u32()
    //}
}

unsafe extern "C" fn close_allocation(device: HANDLE, close_allocation: *const DXGKARG_CLOSEALLOCATION) -> NTSTATUS {
    trace!("{}", function!());
    let device = check_handle_arc!(device: Device);
    let close_allocation = check_arg!(close_allocation);
    let device_allocations = slice_from_raw_parts(close_allocation.pOpenHandleList, close_allocation.NumAllocations as _);

    match device.close_allocation(device_allocations) {
        Ok(()) => STATUS::SUCCESS,
        Err(status) => {
            error!("failed to close allocation: {:?}", status);
            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn describe_allocation(adapter: HANDLE, describe_allocation: *mut DXGKARG_DESCRIBEALLOCATION) -> NTSTATUS {
    trace!("{}", function!());
    let describe_allocation = check_arg!(mut describe_allocation);
    let alloc = check_handle_arc!(describe_allocation.hAllocation: Allocation);
    match alloc.description() {
        Ok(desc) => {
            describe_allocation.Width = desc.width;
            describe_allocation.Height = desc.height;
            describe_allocation.Format = desc.format;
            describe_allocation.MultisampleMethod = D3DDDI_MULTISAMPLINGMETHOD { NumSamples: 1, NumQualityLevels: 1 };
            describe_allocation.RefreshRate = D3DDDI_RATIONAL { Numerator: 148500000, Denominator: 2475000 };
            describe_allocation.PrivateDriverFormatAttribute = 0;
            STATUS::SUCCESS
        }
        Err(e) => {
            error!("failed to close allocation: {:?}", e);
            e.0
        }
    }.to_u32()
}

unsafe extern "C" fn destroy_allocation(adapter: HANDLE, destroy_allocation: *const DXGKARG_DESTROYALLOCATION) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let destroy_allocation = check_arg!(destroy_allocation);

    match gpu.deallocate(destroy_allocation) {
        Ok(()) => STATUS::SUCCESS,
        Err(status) => {
            error!("failed to deallocate: {:?}", status);
            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn get_standard_allocation_driver_data(adapter: HANDLE, standard_allocation: *mut DXGKARG_GETSTANDARDALLOCATIONDRIVERDATA) -> NTSTATUS {
    trace!("{}", function!());
    let standard_allocation = check_arg!(mut standard_allocation);

    if standard_allocation.pResourcePrivateDriverData.is_null() ||
       standard_allocation.pAllocationPrivateDriverData.is_null() ||
       (standard_allocation.ResourcePrivateDriverDataSize as usize) < size_of::<CreateResource>() ||
       (standard_allocation.AllocationPrivateDriverDataSize as usize) < size_of::<CreateAllocation>()
    {
        standard_allocation.pResourcePrivateDriverData = null_mut();
        standard_allocation.pAllocationPrivateDriverData = null_mut();
        standard_allocation.ResourcePrivateDriverDataSize = size_of::<CreateResource>() as _;
        standard_allocation.AllocationPrivateDriverDataSize = size_of::<CreateAllocation>() as _;
        return STATUS::SUCCESS.to_u32();
    }

    let (width, height, size, format, flags) = match standard_allocation.StandardAllocationType {
        D3DKMDT_STANDARDALLOCATION_TYPE::D3DKMDT_STANDARDALLOCATION_SHAREDPRIMARYSURFACE => {
            let surface_data = unsafe { &*standard_allocation.__bindgen_anon_1.pCreateSharedPrimarySurfaceData };
            trace!("{}: {:?}", function!(), surface_data);

            let w = surface_data.Width;
            let h = surface_data.Height;
            let format = VirglFormat::from(surface_data.Format);
            /* All supported formats are 32-bit for now */
            let size = (w as u64) * (h as u64) * 4;
            let flags = VirglFlags::empty();
            //warn!("{}: {:?} / {:?}", function!(), surface_data, format);

            (w, h, size, format.into(), flags.bits())
        },
        D3DKMDT_STANDARDALLOCATION_TYPE::D3DKMDT_STANDARDALLOCATION_SHADOWSURFACE => {
            let surface_data = unsafe { &mut *standard_allocation.__bindgen_anon_1.pCreateShadowSurfaceData };
            trace!("{}: {:?}", function!(), surface_data);

            let w = surface_data.Width;
            let h = surface_data.Height;
            let format = VirglFormat::from(surface_data.Format);
            /* All supported formats are 32-bit for now */
            let size = (w as u64) * (h as u64) * 4;
            surface_data.Pitch = w * 4;
            let flags = VirglFlags::MAP_COHERENT;
            //warn!("{}: {:?} / {:?}", function!(), surface_data, format);

            (w, h, size, format.into(), flags.bits())
        },
        D3DKMDT_STANDARDALLOCATION_TYPE::D3DKMDT_STANDARDALLOCATION_STAGINGSURFACE => {
            let surface_data = unsafe { &mut *standard_allocation.__bindgen_anon_1.pCreateStagingSurfaceData };
            trace!("{}: {:?}", function!(), surface_data);

            let w = surface_data.Width;
            let h = surface_data.Height;
            let format = VirglFormat(VIRGL_FORMAT_B8G8R8X8_UNORM);
            let size = (w as u64) * (h as u64) * 4;
            surface_data.Pitch = w * 4;
            let flags = VirglFlags::MAP_COHERENT;
            //warn!("{}: {:?} / {:?}", function!(), surface_data, format);

            (w, h, size, format.into(), flags.bits())
        },
        _ => {
            error!("{}: unsupported standard allocation type {:?}", function!(), standard_allocation.StandardAllocationType);
            return STATUS::NOT_SUPPORTED.to_u32();
        },
    };

    let resource_priv = unsafe { transmute::<_, &mut CreateResource>(standard_allocation.pResourcePrivateDriverData) };
    *resource_priv = CreateResource { tag: uapi::CREATE_RESOURCE_TAG, cmd: [] };

    let alloc_priv = unsafe { transmute::<_, &mut CreateAllocation>(standard_allocation.pAllocationPrivateDriverData) };

    let alloc_3d = unsafe { &mut alloc_priv._3d };
    *alloc_3d = Allocate3d {
        tag: uapi::ALLOCATE_3D_TAG,
        target: VIRGL_TARGET_TEXTURE_2D,
        format,
        bind: (VirglBind::RENDER_TARGET | VirglBind::SAMPLER_VIEW | VirglBind::DISPLAY_TARGET | VirglBind::SCANOUT).bits(),
        width,
        height,
        depth: 1,
        array_size: 1,
        last_level: 0,
        nr_samples: 0,
        flags,
        size,
    };

    debug!("{}: standard allocation -> {:?}", function!(), allocation::VirtioResource::from(unsafe { alloc_priv._3d }));

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn build_paging_buffer(adapter: HANDLE, build_paging_buffer: *mut DXGKARG_BUILDPAGINGBUFFER) -> NTSTATUS {
    //warn!("{}: {:?}", function!(), unsafe { (*build_paging_buffer).Operation });
    //let guard = logger::set_log_level_temp(log::LevelFilter::Trace);

    trace!("{}: {:?}", function!(), unsafe { (*build_paging_buffer).Operation });
    let gpu = check_handle!(adapter: Adapter);
    let build_paging_buffer = check_arg!(mut build_paging_buffer);

    if build_paging_buffer.pDmaBuffer.is_null() {
        warn!("{} ({:?}): dma buffer is null", function!(), build_paging_buffer.Operation);
        return STATUS::SUCCESS.to_u32();
    }

    if (build_paging_buffer.DmaBufferPrivateDataSize as usize) < size_of::<CommandDmaPrivate>() {
        debug!("{}: paging buffer has not enough space for private data: {} < size_of::<CommandDmaPrivate>())", function!(), build_paging_buffer.DmaBufferPrivateDataSize);
        return STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER.to_u32();
    }

    //info!("{}: {:?}, priv: {:?}", function!(), build_paging_buffer.Operation, build_paging_buffer.pDmaBufferPrivateData);

    let dma_priv = if let Some(dma_priv) = <CommandDmaPrivate as TaggedExt>::from_handle_silent_mut(build_paging_buffer.pDmaBufferPrivateData) {
        dma_priv
    } else {
        unsafe {
            let dma_priv_ptr = transmute::<_, *mut CommandDmaPrivate>(build_paging_buffer.pDmaBufferPrivateData);
            dma_priv_ptr.write(CommandDmaPrivate::new());
            &mut *dma_priv_ptr
        }
    };

    //warn!("{}: {:?}", function!(), build_paging_buffer.Operation);

    let dmabuf = slice_from_raw_parts_mut(build_paging_buffer.pDmaBuffer as *mut u8, build_paging_buffer.DmaSize as _);
    let result = match build_paging_buffer.Operation {
        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_MAP_APERTURE_SEGMENT => {
            let map_aperture_segment = unsafe { (*build_paging_buffer).__bindgen_anon_1.MapApertureSegment };
            if map_aperture_segment.hAllocation.is_null() {
                warn!("{} ({:?}): allocation is null", function!(), build_paging_buffer.Operation);
                return STATUS::SUCCESS.to_u32();
            }

            let alloc = check_handle_arc!(map_aperture_segment.hAllocation: Allocation);
            let Some(res_id) = alloc.id() else {
                error!("{}: cannot attach backing from resource without id: {:?}", function!(), alloc);
                return STATUS::INVALID_PARAMETER.to_u32();
            };
            if !alloc.can_attach() {
                warn!("{}: cannot attach backing for unsupported resource: {:?}", function!(), alloc);
                return STATUS::SUCCESS.to_u32();
            }

            let mdl = MdlRef(map_aperture_segment.pMdl as _);
            let page_offset = map_aperture_segment.MdlOffset as _;
            let n_pages = map_aperture_segment.NumberOfPages as _;

            let needed = Command::attach_backing_dma_len(n_pages);
            if needed >= dmabuf.len() {
                //error!("{}: attaching backing needs {} bytes, but only {} bytes are available", function!(), needed, dmabuf.len());
                debug!("{}: attaching backing needs {} bytes, but only {} bytes are available", function!(), needed, dmabuf.len());
                return STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER.to_u32();
            }

            build_paging_buffer.pDmaBuffer = unsafe { build_paging_buffer.pDmaBuffer.byte_add(needed) };
            let cmd = Command::attach_backing(&gpu.queue_channel().unwrap(), res_id, mdl, page_offset, n_pages, dmabuf);

            trace!("{} ({:?}): writing {:?} to {:?} (resource {}, n_pages {})", function!(), build_paging_buffer.Operation, cmd, build_paging_buffer.pDmaBufferPrivateData, res_id, n_pages);

            if true {
                dma_priv.commands.push(cmd);
            } else {
                build_paging_buffer.pDmaBuffer = dmabuf.as_ptr() as _;
                if let Err(e) = gpu.queue_channel().unwrap().submit_command_sync(&cmd) {
                    error!("{} ({:?}): failed to submit: {:?}", function!(), build_paging_buffer.Operation, e);
                }
            }

            STATUS::SUCCESS
        },

        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_UNMAP_APERTURE_SEGMENT => {
            let unmap_aperture_segment = unsafe { (*build_paging_buffer).__bindgen_anon_1.UnmapApertureSegment };
            if unmap_aperture_segment.hAllocation.is_null() {
                warn!("{} ({:?}): allocation is null", function!(), build_paging_buffer.Operation);
                return STATUS::SUCCESS.to_u32();
            }

            let alloc = check_handle_arc!(unmap_aperture_segment.hAllocation: Allocation);
            let Some(res_id) = alloc.id() else {
                error!("{}: cannot detach backing from resource without id {:?}", function!(), alloc);
                return STATUS::INVALID_PARAMETER.to_u32();
            };
            if !alloc.can_attach() {
                warn!("{}: cannot detach backing for unsupported resource: {:?}", function!(), alloc);
                return STATUS::SUCCESS.to_u32();
            }

            let needed = Command::detach_backing_dma_len();
            if needed >= dmabuf.len() {
                //error!("{}: detaching backing needs {} bytes, but only {} bytes are available", function!(), needed, dmabuf.len());
                debug!("{}: detaching backing needs {} bytes, but only {} bytes are available", function!(), needed, dmabuf.len());
                return STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER.to_u32();
            }

            build_paging_buffer.pDmaBuffer = unsafe { build_paging_buffer.pDmaBuffer.byte_add(needed) };
            let cmd = Command::detach_backing(&gpu.queue_channel().unwrap(), res_id, dmabuf);

            trace!("{} ({:?}): writing {:?} to {:?} (resource {})", function!(), build_paging_buffer.Operation, cmd, build_paging_buffer.pDmaBufferPrivateData, res_id);

            if true {
                dma_priv.commands.push(cmd);
            } else {
                build_paging_buffer.pDmaBuffer = dmabuf.as_ptr() as _;
                if let Err(e) = gpu.queue_channel().unwrap().submit_command_sync(&cmd) {
                    error!("{} ({:?}): failed to submit: {:?}", function!(), build_paging_buffer.Operation, e);
                }
            }

            STATUS::SUCCESS
        },
        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_FILL => {
            let fill = unsafe { build_paging_buffer.__bindgen_anon_1.Fill };
            if fill.hAllocation.is_null() {
                warn!("{} ({:?}): allocation is null", function!(), build_paging_buffer.Operation);
                return STATUS::SUCCESS.to_u32();
            }

            let alloc = check_handle_arc!(fill.hAllocation: Allocation);
            debug!("{}: fill {:?} for {:?}", function!(), fill, alloc);

            STATUS::SUCCESS
        },

        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_DISCARD_CONTENT => {
            let discard_content = unsafe { (*build_paging_buffer).__bindgen_anon_1.DiscardContent };

            if discard_content.hAllocation.is_null() {
                warn!("{} ({:?}): allocation is null", function!(), build_paging_buffer.Operation);
                return STATUS::SUCCESS.to_u32();
            }

            let alloc = check_handle_arc!(discard_content.hAllocation: Allocation);

            debug!("{}: discard_content for {:?}", function!(), alloc);

            STATUS::SUCCESS
        },

        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_TRANSFER => {
            let transfer = unsafe { (*build_paging_buffer).__bindgen_anon_1.Transfer };

            if transfer.hAllocation.is_null() {
                warn!("{} ({:?}): allocation is null", function!(), build_paging_buffer.Operation);
                return STATUS::SUCCESS.to_u32();
            }

            let alloc = check_handle_arc!(transfer.hAllocation: Allocation);

            debug!("{}: transfer {:?} for alloc: {:?}", function!(), transfer, alloc);

            STATUS::SUCCESS
        },

        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_VIRTUAL_TRANSFER => {
            let transfer = unsafe { (*build_paging_buffer).__bindgen_anon_1.TransferVirtual };

            if transfer.hAllocation.is_null() {
                warn!("{} ({:?}): allocation is null", function!(), build_paging_buffer.Operation);
                return STATUS::SUCCESS.to_u32();
            }

            let alloc = check_handle_arc!(transfer.hAllocation: Allocation);

            debug!("{}: virtual transfer {:?} for alloc: {:?}", function!(), transfer, alloc);

            STATUS::SUCCESS
        },

        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_VIRTUAL_FILL => {
            let fill = unsafe { build_paging_buffer.__bindgen_anon_1.FillVirtual };
            if fill.hAllocation.is_null() {
                warn!("{} ({:?}): allocation is null", function!(), build_paging_buffer.Operation);
                return STATUS::SUCCESS.to_u32();
            }

            let alloc = check_handle_arc!(fill.hAllocation: Allocation);
            debug!("{}: fill {:?} for {:?}", function!(), fill, alloc);

            STATUS::SUCCESS
        },

        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_UPDATE_PAGE_TABLE => {
            let update = unsafe { (*build_paging_buffer).__bindgen_anon_1.UpdatePageTable };

            trace!("{}: update page table: {:?}", function!(), update);
            if update.hAllocation.is_null() {
                return STATUS::SUCCESS.to_u32();
            }

            let alloc = check_handle_arc!(update.hAllocation: Allocation);

            // TODO: handle unmap / remap

            // TODO: check that system memory pages are attached in order
            let entries = slice_from_raw_parts(update.pPageTableEntries, update.NumPageTableEntries as _);

            let has_invalid_entries = entries.iter().fold(false, |result, entry| result | !entry.Flags().Valid());
            if has_invalid_entries {
                error!("{}: UNHANDLED DETACH BACKING!!!", function!());
            }

            debug!("{}: update page table: {:?}", function!(), update);
            debug!("{}: update page table for allocation: {:?}", function!(), alloc);
            debug!("{}: alloc attached {}, size {}", function!(), alloc.total_attached_bytes(), alloc.size());

            if /*(update.Flags.InitialUpdate() != 0 &&*/ alloc.can_attach() && alloc.total_attached_bytes() < alloc.size() {
                debug!("{}: update page table for allocation: {:?}", function!(), alloc);
                debug!("{}: page table (alloc byte offset {} / {}): {:?}", function!(), update.AllocationOffsetInBytes, alloc.size(), entries);
                //warn!("{}: update for (alloc byte offset {} / total size {}): resource id {:?}", function!(), update.AllocationOffsetInBytes, alloc.size(), alloc.id());

                if alloc.attach_pages(update.AllocationOffsetInBytes as u64, entries) {
                    let needed = Command::attach_backing_virtual_dma_len(&alloc);
                    if needed >= dmabuf.len() {
                        debug!("{}: attaching backing needs {} bytes, but only {} bytes are available", function!(), needed, dmabuf.len());
                        return STATUS::GRAPHICS_INSUFFICIENT_DMA_BUFFER.to_u32();
                    }

                    build_paging_buffer.pDmaBuffer = unsafe { build_paging_buffer.pDmaBuffer.byte_add(needed) };
                    let Some(cmd) = Command::attach_backing_virtual(&gpu.queue_channel().unwrap(), &alloc, dmabuf) else {
                        error!("{}: failed to attach backing for {:?}", function!(), alloc);
                        return STATUS::INVALID_PARAMETER.to_u32();
                    };

                    //warn!("{} ({:?}): writing {:?} to {:?} (alloc {:?})", function!(), build_paging_buffer.Operation, cmd, build_paging_buffer.pDmaBufferPrivateData, alloc);

                    dma_priv.commands.push(cmd);
                    //warn!("{} ({:?}): saved command {:?} for resource {:?}", function!(), build_paging_buffer.Operation, dma_priv.commands.last(), alloc.id());

                } else {
                    debug!("{}: incomplete update page table for allocation: {:?}", function!(), alloc);
                }
            } else if alloc.can_attach() {
                debug!("{}: secondary update for alloc (n_pages: {}): {:?}", function!(), alloc.num_attached_pages(), alloc);
                debug!("{}: secondary page table (alloc byte offset {}): {:?}", function!(), update.AllocationOffsetInBytes, entries);
            }

            STATUS::SUCCESS
        },

        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_FLUSH_TLB => {
            let flush = unsafe { (*build_paging_buffer).__bindgen_anon_1.FlushTlb };
            debug!("{}: flush tlb: {:?}", function!(), flush);

            STATUS::SUCCESS
        },

        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_COPY_PAGE_TABLE_ENTRIES => {
            let copy = unsafe { (*build_paging_buffer).__bindgen_anon_1.CopyPageTableEntries };
            let ranges = slice_from_raw_parts(copy.pRanges, copy.NumRanges as _);
            debug!("{}: copy page table entries: {:?}", function!(), ranges);

            STATUS::SUCCESS
        },

        DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_NOTIFY_RESIDENCY => {
            let notify = unsafe { (*build_paging_buffer).__bindgen_anon_1.NotifyResidency };
            warn!("{}: notify residency: {:?}", function!(), notify);

            STATUS::SUCCESS
        },

        _ => {
            error!("{}: unknown paging operation: {:?}", function!(), build_paging_buffer.Operation);
            STATUS::NOT_SUPPORTED
        },
    };

    if dma_priv.commands.len() > 0 || dma_priv.allocations.len() > 0 {
        unsafe {
            build_paging_buffer.pDmaBufferPrivateData = build_paging_buffer.pDmaBufferPrivateData.byte_add(size_of::<CommandDmaPrivate>());
            build_paging_buffer.DmaBufferPrivateDataSize -= size_of::<CommandDmaPrivate>() as u32;
        }
    }

    result.to_u32()
}

/*unsafe extern "C" fn acquire_swizzling_range(adapter: HANDLE, acquire_swizzling_range: *const DXGKARG_ACQUIRESWIZZLINGRANGE) -> NTSTATUS {
    info!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn release_swizzling_range(adapter: HANDLE, release_swizzling_range: *const DXGKARG_RELEASESWIZZLINGRANGE) -> NTSTATUS {
    info!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}*/

unsafe extern "C" fn create_device(adapter: HANDLE, create_device: *mut DXGKARG_CREATEDEVICE) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);

    let device: Arc<Device> = match Device::new(gpu) {
        Ok(device) => device,
        Err(code) => return code.to_u32(),
    };

    unsafe { (*create_device).hDevice = TaggedExt::into_arc_handle(device); }

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn destroy_device(device: HANDLE) -> NTSTATUS {
    trace!("{}", function!());

    if device.is_null() {
        error!("{}: device handle is null", function!());
        return STATUS::INVALID_PARAMETER.to_u32();
    }

    unsafe { drop(Arc::from_raw(device as *mut Device)); }

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn create_context(device: HANDLE, create_context: *mut DXGKARG_CREATECONTEXT) -> NTSTATUS {
    trace!("{}", function!());
    let device = check_handle_arc!(device: Device);
    // Wouldn't it be nice to have some private data here?
    // If only we had something...
    let create_context = check_arg!(mut create_context);

    let Some(engine) = Engine::try_from_node_ordinal(create_context.NodeOrdinal) else {
        error!("{}: unsupported engine: {}", function!(), create_context.NodeOrdinal);
        return STATUS::INVALID_PARAMETER.to_u32();
    };

    let context = match device.dxgk_context(engine) {
        Ok(context) => context,
        Err(e) => {
            error!("{}: failed to create dxgk context for {:?}: {:?}", function!(), engine, e);
            return e.to_u32();
        },
    };

    //if matches!(engine, Engine::Other(_)) {
    //    info!("{}: context type: {:?}, engine: {:?}", function!(), create_context.Flags, engine);
    //} else {
    //    trace!("{}: context type: {:?}, engine: {:?}", function!(), create_context.Flags, engine);
    //}
    trace!("{}: context type: {:?}, engine: {:?}", function!(), create_context.Flags, engine);

    if create_context.Flags.GdiContext() || create_context.Flags.SystemContext() {
        create_context.hContext = TaggedExt::into_arc_handle(context);
        create_context.ContextInfo.DmaBufferSegmentSet = 0;
        create_context.ContextInfo.DmaBufferSize = 1024 * 1024;
        create_context.ContextInfo.DmaBufferPrivateDataSize = 128;
        create_context.ContextInfo.AllocationListSize = DXGK_ALLOCATION_LIST_SIZE_GDICONTEXT;
        create_context.ContextInfo.PatchLocationListSize = DXGK_ALLOCATION_LIST_SIZE_GDICONTEXT;

        let context_init = ContextInit {
            tag: ESCAPE_CONTEXT_INIT_TAG,
            capset_id: CapsetId::Virgl2,
            num_rings: 64,
            debug_name: {
                const NAME: &str = "virgl-system-win32";

                let mut buffer = [0u8; 64];
                buffer[..NAME.len()].copy_from_slice(&NAME.as_bytes());

                buffer
            },
        };

        if let Err(e) = device.init_context(uapi::CapsetMask::VIRGL2, &context_init) {
            error!("{}: failed to initialize system virgl context: {:?}", function!(), e);
            return e.to_u32();
        }
    } else {
        match engine {
            Engine::Graphics | Engine::PhysicalOther => {
                create_context.hContext = TaggedExt::into_arc_handle(context);
                create_context.ContextInfo.DmaBufferSegmentSet = 0;
                create_context.ContextInfo.DmaBufferSize = /*8 **/ 1024 * 1024;
                create_context.ContextInfo.DmaBufferPrivateDataSize = 4096;
                create_context.ContextInfo.AllocationListSize = 1024;
                create_context.ContextInfo.PatchLocationListSize = 1024;
            },
           Engine::Copy | Engine::Other(_) => {
                create_context.hContext = TaggedExt::into_arc_handle(context);
                create_context.ContextInfo.DmaBufferSegmentSet = 0;
                create_context.ContextInfo.DmaBufferSize = 0;
                create_context.ContextInfo.DmaBufferPrivateDataSize = uapi::MAX_SUBMIT_COMMAND_VIRTUAL_SIZE;
                create_context.ContextInfo.AllocationListSize = 0;
                create_context.ContextInfo.PatchLocationListSize = 0;
                create_context.ContextInfo.Caps.set_NoPatchingRequired(true);
                create_context.ContextInfo.Caps.set_DriverManagesResidency(true);
            },
        }
    }

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn destroy_context(context: HANDLE) -> NTSTATUS {
    trace!("{}", function!());

    if context.is_null() {
        error!("{}: context handle is null", function!());
        return STATUS::INVALID_PARAMETER.to_u32();
    }

    unsafe { drop(Arc::from_raw(context as *mut DeviceContext)); }

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn present(context: HANDLE, present: *mut DXGKARG_PRESENT) -> NTSTATUS {
    trace!("{}", function!());
    let context = check_handle_arc!(context: DeviceContext);
    let present = check_arg!(mut present);
    //debug!("{}: {:?}", function!(), present);

    match context.device.present(present) {
        Ok(()) => STATUS::SUCCESS,
        Err(status) => {
            error!("failed to present: {:?}", status);
            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn render(context: HANDLE, render: *mut DXGKARG_RENDER) -> NTSTATUS {
    trace!("{}", function!());

    let context = check_handle_arc!(context: DeviceContext);
    let render = check_arg!(mut render);

    if render.DmaBufferSegmentId != 0 {
        error!("{}: expected dma buffer to be a physically contiguous block of system memory, but it's in segment {} instead", function!(), render.DmaBufferSegmentId);
        return STATUS::INVALID_PARAMETER.to_u32();
    }

    let pre = render.clone();

    match context.device.render(render) {
        Ok(()) => {
            debug!("{}: render pre: {:?}", function!(), pre);
            debug!("{}: render post: {:?}", function!(), render);

            STATUS::SUCCESS
        },
        Err(status) => {
            error!("failed to render: {:?}", status);
            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn patch(adapter: HANDLE, patch: *const DXGKARG_PATCH) -> NTSTATUS {
    trace!("{}", function!());

    let adapter = check_handle!(adapter: Adapter);
    let patch = check_arg!(patch);

    if patch.DmaBufferSegmentId != 0 {
        error!("{}: expected dma buffer to be a physically contiguous block of system memory, but it's in a segment {} instead", function!(), patch.DmaBufferSegmentId);
        return STATUS::INVALID_PARAMETER.to_u32();
    }

    if patch.pAllocationList.is_null() {
        trace!("{}: allocation list is empty", function!());
        return STATUS::SUCCESS.to_u32();
    }
    if patch.pPatchLocationList.is_null() {
        trace!("{}: patch location list is empty", function!());
        return STATUS::SUCCESS.to_u32();
    }

    if patch.pDmaBufferPrivateData.is_null() {
        trace!("{}: no dma private data", function!());
        return STATUS::SUCCESS.to_u32();
    }

    let Some(dma_priv): Option<&mut CommandDmaPrivate> = TaggedExt::from_handle_mut(patch.pDmaBufferPrivateData) else {
        error!("{}: invalid dma private data", function!());
        // this probably should be an error (use check_handle)
        return STATUS::SUCCESS.to_u32();
    };

    let cmds = &mut dma_priv.commands;
    debug!("{}: {:?} (total {} commands to patch)", function!(), patch, cmds.len());

    let allocations = slice_from_raw_parts(patch.pAllocationList, patch.AllocationListSize as _);

    for cmd in cmds {
        cmd.patch(allocations);
    }

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn submit_command(adapter: HANDLE, submit_command: *const DXGKARG_SUBMITCOMMAND) -> NTSTATUS {
    let gpu = check_handle!(adapter: Adapter);
    let submit_command = check_arg!(submit_command);

    trace!("{}: fence {}", function!(), submit_command.SubmissionFenceId);

    match gpu.submit_command(submit_command) {
        Ok(()) => STATUS::SUCCESS,
        Err(status) => {
            error!("failed to submit command: {:?}", status);
            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn is_supported_vidpn(adapter: HANDLE, is_supported_vidpn: *mut DXGKARG_ISSUPPORTEDVIDPN) -> NTSTATUS {
    trace!("{}", function!());
    let gpu = check_handle!(adapter: Adapter);
    let is_supported_vidpn = check_arg!(mut is_supported_vidpn);

    if is_supported_vidpn.hDesiredVidPn.is_null() {
        is_supported_vidpn.IsVidPnSupported = true as _;

        return STATUS::SUCCESS.to_u32();
    }

    is_supported_vidpn.IsVidPnSupported = false as _;

    match gpu.is_supported_vidpn(is_supported_vidpn) {
        Ok(()) => STATUS::SUCCESS,
        Err(status) => {
            error!("{}: failed to check if vidpn is supported: {:?}", function!(), status);
            status.0
        },
    }.to_u32()
}

unsafe extern "C" fn recommend_functional_vidpn(adapter: HANDLE, recommend_functional_vidpn: *const DXGKARG_RECOMMENDFUNCTIONALVIDPN) -> NTSTATUS {
    info!("{}", function!());

    STATUS::GRAPHICS_NO_RECOMMENDED_FUNCTIONAL_VIDPN.to_u32()
}

unsafe extern "C" fn recommend_vidpn_topology(adapter: HANDLE, recommend_vidpn_topology: *const DXGKARG_RECOMMENDVIDPNTOPOLOGY) -> NTSTATUS {
    info!("{}", function!());

    STATUS::GRAPHICS_NO_RECOMMENDED_VIDPN_TOPOLOGY.to_u32()
}

unsafe extern "C" fn recommend_monitor_modes(adapter: HANDLE, recommend_monitor_modes: *const DXGKARG_RECOMMENDMONITORMODES) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let recommend_monitor_modes = check_arg!(recommend_monitor_modes);

    match gpu.recommend_monitor_modes(recommend_monitor_modes) {
        Ok(()) => STATUS::SUCCESS,
        Err(e) => {
            error!("{}: failed to get monitor modes: {:?}", function!(), e);
            e.0
        },
    }.to_u32()
}

unsafe extern "C" fn enum_vidpn_cofunc_modality(adapter: HANDLE, enum_cofunc_modality: *const DXGKARG_ENUMVIDPNCOFUNCMODALITY) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let enum_cofunc_modality = check_arg!(enum_cofunc_modality);

    match gpu.enum_cofunc_modality(enum_cofunc_modality) {
        Ok(()) => STATUS::SUCCESS,
        Err(e) => {
            error!("{}: failed to enum cofunc modality: {:?}", function!(), e);
            e.0
        },
    }.to_u32()
}

unsafe extern "C" fn set_vidpn_source_visibility(adapter: HANDLE, set_vidpn_source_visibility: *const DXGKARG_SETVIDPNSOURCEVISIBILITY) -> NTSTATUS {
    debug!("{}: not implemented", function!());

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn set_vidpn_source_address(adapter: HANDLE, set_vidpn_source_address: *const DXGKARG_SETVIDPNSOURCEADDRESS) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let set_vidpn_source_address = check_arg!(set_vidpn_source_address);
    let alloc = check_handle_arc!(set_vidpn_source_address.hAllocation: Allocation);
    let source = set_vidpn_source_address.VidPnSourceId;
    let seg = set_vidpn_source_address.PrimarySegment;
    let addr = unsafe { set_vidpn_source_address.PrimaryAddress.QuadPart } as u64;

    //warn!("{}: queue scanout {}: alloc id {:?} / phys {}:{}, strong: {}, weak: {}", function!(), source, alloc.id(), seg, addr, Arc::strong_count(&alloc), Arc::weak_count(&alloc));

    match gpu.queue_scanout(source, &alloc, addr) {
        Ok(()) => STATUS::SUCCESS,
        Err(e) => {
            error!("{}: failed to queue scanout: {:?}", function!(), e);
            e.0
        },
    }.to_u32()
}

unsafe extern "C" fn commit_vidpn(adapter: HANDLE, commit_vidpn: *const DXGKARG_COMMITVIDPN) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let commit_vidpn = check_arg!(commit_vidpn);

    match gpu.commit_vidpn(commit_vidpn) {
        Ok(()) => STATUS::SUCCESS,
        Err(e) => {
            error!("{}: failed to commit vidpn: {:?}", function!(), e);
            e.0
        },
    }.to_u32()

    //STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn update_active_vidpn_present_path(adapter: HANDLE, update_active_vidpn_present_path: *const DXGKARG_UPDATEACTIVEVIDPNPRESENTPATH) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let update_active_vidpn_present_path = check_arg!(update_active_vidpn_present_path);

    let path = &update_active_vidpn_present_path.VidPnPresentPathInfo;

    match gpu.update_active_present_path(path) {
        Ok(()) => STATUS::SUCCESS,
        Err(e) => {
            error!("{}: failed to queue scanout: {:?}", function!(), e);
            e.0
        },
    }.to_u32()

    //STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn query_vidpn_hw_capability(adapter: HANDLE, vidpn_hw_caps: *mut DXGKARG_QUERYVIDPNHWCAPABILITY) -> NTSTATUS {
    trace!("{}", function!());
    let gpu = check_handle!(adapter: Adapter);
    let vidpn_hw_caps = check_arg!(mut vidpn_hw_caps);

    let num_scanouts = match gpu.num_scanouts() {
        Ok(num_scanouts) => num_scanouts as u32,
        Err(e) => return e.0.to_u32(),
    };

    assert!(vidpn_hw_caps.SourceId < num_scanouts);
    assert!(vidpn_hw_caps.TargetId < num_scanouts);

    // FIXME: we cannot really do any of these
    //vidpn_hw_caps.VidPnHWCaps.set_DriverRotation(true as _);
    vidpn_hw_caps.VidPnHWCaps.set_DriverRotation(false as _);
    vidpn_hw_caps.VidPnHWCaps.set_DriverScaling(false as _);
    vidpn_hw_caps.VidPnHWCaps.set_DriverCloning(false as _);
    vidpn_hw_caps.VidPnHWCaps.set_DriverColorConvert(true as _);
    vidpn_hw_caps.VidPnHWCaps.set_DriverLinkedAdapaterOutput(false as _);
    vidpn_hw_caps.VidPnHWCaps.set_DriverRemoteDisplay(false as _);
    vidpn_hw_caps.VidPnHWCaps.set_Reserved(0);

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn control_interrupt(adapter: HANDLE, interrupt_type: DXGK_INTERRUPT_TYPE, enable_interrupt: BOOLEAN) -> NTSTATUS {
    trace!("{}: interrupt_type: {:?}, enable: {}", function!(), interrupt_type, enable_interrupt != 0);
    let gpu = check_handle!(adapter: Adapter);

    gpu.control_interrupt(interrupt_type, enable_interrupt != 0);

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn get_scan_line(adapter: HANDLE, get_scan_line: *mut DXGKARG_GETSCANLINE) -> NTSTATUS {
    error!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn stop_device_and_release_post_display_ownership(adapter: HANDLE, target_id: D3DDDI_VIDEO_PRESENT_TARGET_ID, display_info: *mut DXGK_DISPLAY_INFORMATION) -> NTSTATUS {
    warn!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let display_info = check_arg!(mut display_info);

    match gpu.stop_and_release() {
        Ok(info) => {
            *display_info = DXGK_DISPLAY_INFORMATION { TargetId: target_id, ..info };

            STATUS::SUCCESS
        },
        Err(e) => {
            error!("{}: failed to queue scanout: {:?}", function!(), e);
            e.0
        },
    }.to_u32()
}

unsafe extern "C" fn system_display_enable(adapter: HANDLE, target_id: D3DDDI_VIDEO_PRESENT_TARGET_ID, flags: PDXGKARG_SYSTEM_DISPLAY_ENABLE_FLAGS, width: *mut UINT, height: *mut UINT, color_format: *mut D3DDDIFORMAT) -> NTSTATUS {
    error!("{}: not implemented", function!());

    STATUS::NOT_SUPPORTED.to_u32()
}

unsafe extern "C" fn system_display_write(adapter: HANDLE, source: HANDLE, source_width: UINT, source_height: UINT, source_stride: UINT, position_x: UINT, position_y: UINT) {
    error!("{}: not implemented", function!());
}

unsafe extern "C" fn preempt_command(adapter: HANDLE, preempt_command: *const DXGKARG_PREEMPTCOMMAND) -> NTSTATUS {
    let gpu = check_handle!(adapter: Adapter);
    let preempt_command = check_arg!(preempt_command);
    let engine = Engine::try_from_node_ordinal(preempt_command.NodeOrdinal).unwrap();

    gpu.kick_queue_handler();
    gpu.notify_queued_fences();

    let last_completed = gpu.last_completed_fence(engine);
    let last_submitted = gpu.last_submitted_fence(engine);

    info!("{}: engine {:?}, preemption fence {}, last completed {}, last submitted {}", function!(), engine, preempt_command.PreemptionFenceId, last_completed, last_submitted);

    if last_completed == last_submitted {
        gpu.notify_dma_preempted(engine, preempt_command.PreemptionFenceId, last_completed);
    } else {
        assert!(preempt_command.PreemptionFenceId != 0);
        gpu.submit_preemption(engine, preempt_command.PreemptionFenceId);
        gpu.kick_queue_handler();
        gpu.notify_queued_fences();
    }

    STATUS::SUCCESS.to_u32()
}

/*
unsafe extern "C" fn cancel_command(adapter: HANDLE, cancel_command: *const DXGKARG_CANCELCOMMAND) -> NTSTATUS {
    error!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}
*/

unsafe extern "C" fn query_current_fence(adapter: HANDLE, current_fence: *mut DXGKARG_QUERYCURRENTFENCE) -> NTSTATUS {
    //info!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let current_fence = check_arg!(mut current_fence);
    let engine = Engine::try_from_node_ordinal(current_fence.NodeOrdinal).unwrap();

    gpu.kick_queue_handler();
    gpu.notify_queued_fences();

    let last_completed = gpu.last_completed_fence(engine);
    let last_submitted = gpu.last_submitted_fence(engine);

    current_fence.CurrentFence = last_completed;
    info!("{}: engine {:?}, last completed: {}, last submitted: {}", function!(), engine, last_completed, last_submitted);

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn reset_engine(adapter: HANDLE, reset_engine: *mut DXGKARG_RESETENGINE) -> NTSTATUS {
    error!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn query_engine_status(adapter: HANDLE, query: *mut DXGKARG_QUERYENGINESTATUS) -> NTSTATUS {
    trace!("{}", function!());
    let gpu = check_handle!(adapter: Adapter);
    let query = check_arg!(mut query);

    let engine = Engine::try_from_node_ordinal(query.NodeOrdinal).unwrap();
    let responsive = gpu.check_engine(engine);
    info!("{}: engine {:?} is responsive: {}", function!(), engine, responsive);

    query.EngineStatus.set_Responsive(responsive);

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn query_dependent_engine_group(adapter: HANDLE, query: *mut DXGKARG_QUERYDEPENDENTENGINEGROUP) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let query = check_arg!(mut query);
    let engine = Engine::try_from_node_ordinal(query.NodeOrdinal).unwrap();

    gpu.kick_queue_handler();
    gpu.notify_queued_fences();

    let last_completed = gpu.last_completed_fence(engine);
    let last_submitted = gpu.last_submitted_fence(engine);

    info!("{}: engine {:?} last completed: {}, last submitted: {}", function!(), engine, last_completed, last_submitted);

    query.DependentNodeOrdinalMask = u64::MAX;

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn collect_dbg_info(adapter: HANDLE, collect_dbg_info: *const DXGKARG_COLLECTDBGINFO) -> NTSTATUS {
    error!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn reset_from_timeout(adapter: HANDLE) -> NTSTATUS {
    error!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn restart_from_timeout(adapter: HANDLE) -> NTSTATUS {
    error!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn control_interrupt2(adapter: HANDLE, interrupt_control: DXGKARG_CONTROLINTERRUPT2) -> NTSTATUS {
    trace!("{}: {:?}", function!(), interrupt_control);
    let gpu = check_handle!(adapter: Adapter);

    let enable = match interrupt_control.InterruptType {
        DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_CRTC_VSYNC => {
            unsafe { interrupt_control.__bindgen_anon_1.CrtcVsyncState == DXGK_CRTC_VSYNC_STATE::DXGK_VSYNC_ENABLE }
        },
        _ => {
            unsafe { interrupt_control.__bindgen_anon_1.InterruptState == DXGK_INTERRUPT_STATE::DXGK_INTERRUPT_ENABLE }
        },
    };

    gpu.control_interrupt(interrupt_control.InterruptType, enable);

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn format_history_buffer(adapter: HANDLE, format_data: *mut DXGKARG_FORMATHISTORYBUFFER) -> NTSTATUS {
    error!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn calibrate_gpu_clock(adapter: HANDLE, node: u32, engine: u32, clock_calibration: *mut DXGKARG_CALIBRATEGPUCLOCK) -> NTSTATUS {
    trace!("{}: not implemented", function!());

    //let clock_calibration = check_arg!(mut clock_calibration);
    //let (ts, freq) = ke_query_performance_counter();
    //clock_calibration.GpuFrequency = freq;
    //clock_calibration.GpuClockCounter = ts;
    //clock_calibration.CpuClockCounter = ts;

    STATUS::NOT_SUPPORTED.to_u32()
    //STATUS::SUCCESS.to_u32()
}

/*
unsafe extern "C" fn render_gdi(device: HANDLE, render_gdi: *mut DXGKARG_RENDERGDI) -> NTSTATUS {
    error!("{}: not implemented", function!());

    STATUS::NOT_IMPLEMENTED.to_u32()
}
*/

unsafe extern "C" fn submit_command_virtual(adapter: HANDLE, submit_command_virtual: *const DXGKARG_SUBMITCOMMANDVIRTUAL) -> NTSTATUS {
    trace!("{}", function!());

    let gpu = check_handle!(adapter: Adapter);
    let submit_command_virtual = check_arg!(submit_command_virtual);

    match gpu.submit_command_virtual(submit_command_virtual) {
        Ok(()) => STATUS::SUCCESS,
        Err(status) => {
            error!("failed to submit command virtual: {:?}", status);
            status.0
        },
    }.to_u32()
}

/*
pub unsafe extern "C" fn create_hw_context(device: HANDLE, create_hw_context: *mut DXGKARG_CREATEHWCONTEXT) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

pub unsafe extern "C" fn destroy_hw_context(context: HANDLE) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

pub unsafe extern "C" fn create_hw_queue(context: HANDLE, create_hw_queue: *mut DXGKARG_CREATEHWQUEUE) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

pub unsafe extern "C" fn destroy_hw_queue(hw_queue: HANDLE) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

pub unsafe extern "C" fn submit_command_to_hw_queue(adapter: HANDLE, submit_command_to_hw_queue: *const DXGKARG_SUBMITCOMMANDTOHWQUEUE) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

pub unsafe extern "C" fn switch_to_hw_context_list(adapter: HANDLE, switch_to_hw_context_list: *const DXGKARG_SWITCHTOHWCONTEXTLIST) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

pub unsafe extern "C" fn reset_hw_engine(adapter: HANDLE, reset_hw_engine: *mut DXGKARG_RESETHWENGINE) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}
*/

unsafe extern "C" fn set_root_page_table(adapter: HANDLE, set_page_table: *const DXGKARG_SETROOTPAGETABLE) {
    if set_page_table.is_null() {
        warn!("{}: set_page_table is null", function!());
        return;
    }

    let set_page_table = unsafe { *set_page_table };
    debug!("{}: stub: {:?}", function!(), set_page_table);
}

unsafe extern "C" fn get_root_page_table_size(adapter: HANDLE, get_root_page_table_size: *mut DXGKARG_GETROOTPAGETABLESIZE) -> u64 {
    warn!("{}: stub", function!());
    0
}

unsafe extern "C" fn map_cpu_host_aperture(adapter: HANDLE, map_aperture: *const DXGKARG_MAPCPUHOSTAPERTURE) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn unmap_cpu_host_aperture(adapter: HANDLE, unmap_aperture: *const DXGKARG_UNMAPCPUHOSTAPERTURE) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn check_multiplane_overlay_support(adapter: HANDLE, check_multiplane_overlay_support: *mut DXGKARG_CHECKMULTIPLANEOVERLAYSUPPORT) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn check_multiplane_overlay_support2(adapter: HANDLE, check_multiplane_overlay_support: *mut DXGKARG_CHECKMULTIPLANEOVERLAYSUPPORT2) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn create_process(adapter: HANDLE, create_process: *mut DXGKARG_CREATEPROCESS) -> NTSTATUS {
    trace!("{}", function!());
    let gpu = check_handle!(adapter: Adapter);
    let create_process = check_arg!(mut create_process);

    let process = match Process::new(create_process.hDxgkProcess) {
        Ok(process) => process,
        Err(e) => {
            error!("failed to create process: {:?}", e);
            return e.to_u32()
        },
    };

    create_process.hKmdProcess = TaggedExt::into_arc_handle(process);

    debug!("{}: {:?}", function!(), create_process);

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn destroy_process(adapter: HANDLE, process: HANDLE) -> NTSTATUS {
    trace!("{}", function!());

    if process.is_null() {
        error!("{}: process handle is null", function!());
        return STATUS::INVALID_PARAMETER.to_u32();
    }

    unsafe { drop(Arc::from_raw(process as *mut Process)); }

    STATUS::SUCCESS.to_u32()
}

unsafe extern "C" fn set_vidpn_source_address_with_multiplane_overlay(adapter: HANDLE, set_vidpn_source_address_with_multiplane_overlay: *const DXGKARG_SETVIDPNSOURCEADDRESSWITHMULTIPLANEOVERLAY) -> NTSTATUS {
    let set_vidpn_source_address_with_multiplane_overlay = check_arg!(set_vidpn_source_address_with_multiplane_overlay);
    error!("{}: not implemented: {:?}", function!(), set_vidpn_source_address_with_multiplane_overlay);

    let planes = slice_from_raw_parts(set_vidpn_source_address_with_multiplane_overlay.pPlanes, set_vidpn_source_address_with_multiplane_overlay.PlaneCount as _);

    let disabling_planes = planes.iter().fold(true, |d, plane| d && plane.Enabled == 0);

    for plane in planes {
        error!("{}: plane {:?}", function!(), plane);
    }

    // This started failing in WDDM2.something on driver unload for whatever reason
    if disabling_planes {
        return STATUS::SUCCESS.to_u32()
    }

    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn set_vidpn_source_address_with_multiplane_overlay2(adapter: HANDLE, set_vidpn_source_address_with_multiplane_overlay2: *const DXGKARG_SETVIDPNSOURCEADDRESSWITHMULTIPLANEOVERLAY2) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn power_runtime_set_device_handle(adapter: HANDLE, power_runtime: HANDLE) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

unsafe extern "C" fn set_stable_power_state(adapter: HANDLE, stable_power_state: *const DXGKARG_SETSTABLEPOWERSTATE) {
    error!("{}: not implemented", function!());
}

unsafe extern "C" fn set_video_protected_region(adapter: HANDLE, set_video_protected_region: *const DXGKARG_SETVIDEOPROTECTEDREGION) -> NTSTATUS {
    error!("{}: not implemented", function!());
    STATUS::NOT_IMPLEMENTED.to_u32()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("{}", info);
    unsafe { logger::force_flush(); }

    unsafe { KeBugCheck(0xDEADDEAD); }
}
