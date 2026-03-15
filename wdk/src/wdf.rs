#![allow(
    clashing_extern_declarations,
    hidden_glob_reexports,
)]

use winresult::STATUS;

mod sys {
    pub type NTSTATUS = u32;
    pub type PNTSTATUS = *mut NTSTATUS;
    pub type PCNTSTATUS = *const NTSTATUS;

    include!(concat!(env!("OUT_DIR"), "/wdf-bindings.rs"));

    #[link(name = "wdfldr")]
    unsafe extern "C" {}

    #[link(name = "wdfdriverentry")]
    unsafe extern "C" {}
}

pub use sys::*;

use crate::{
    assert_irql,
    wdm_call_status_unchecked,
    wdm_call_status,
};

use log::error;

#[unsafe(no_mangle)]
static WdfMinimumVersionRequired: ULONG = sys::WdfMinimumVersionRequired;

pub fn WdfDriverCreate(DriverObject: PDRIVER_OBJECT, RegistryPath: PCUNICODE_STRING, DriverAttributes: PWDF_OBJECT_ATTRIBUTES, DriverConfig: PWDF_DRIVER_CONFIG) -> Result<(), winresult::NtStatus> {
    let pfn = unsafe { core::mem::transmute::<_, PFN_WDFDRIVERCREATE>(*WdfFunctions_01027.add(WDFFUNCENUM::WdfDriverCreateTableIndex.0 as _)) }.unwrap();
    wdm_call_status!(== PASSIVE_LEVEL | pfn(WdfDriverGlobals, DriverObject, RegistryPath, DriverAttributes, DriverConfig, core::ptr::null_mut()))?;
    Ok(())
}

pub const fn ctl_code(device_type: u32, function: u32, access: u32, method: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | (method)
}
