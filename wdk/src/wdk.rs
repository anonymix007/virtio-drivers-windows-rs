#![no_std]

#![feature(allocator_api)]

#![allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    unused,
    dead_code,
    unnecessary_transmutes,
    clashing_extern_declarations,
)]

pub mod wdm;
pub mod dxgkrnl;
pub mod wdf;
