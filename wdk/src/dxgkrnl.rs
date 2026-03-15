#![allow(clashing_extern_declarations)]

use core::{
    cmp::{
        max,
        min,
    },
    fmt,
    ops::{
        Add,
        BitOrAssign,
    },
};

mod sys {
    pub type NTSTATUS = u32;
    pub type PNTSTATUS = *mut NTSTATUS;
    pub type PCNTSTATUS = *const NTSTATUS;

    include!(concat!(env!("OUT_DIR"), "/dxgkrnl-bindings.rs"));

    #[link(name = "displib")]
    unsafe extern "C" {}
}

pub use sys::*;

impl DXGK_VIDSCHCAPS {
    #[inline]
    pub fn set_MultiEngineAware(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_MultiEngineAware(val as _) };
    }

    #[inline]
    pub fn set_PreemptionAware(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_PreemptionAware(val as _) };
    }

    //#[inline]
    //pub fn set_HwQueuePacketCap(&mut self, val: u8) {
    //    unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_HwQueuePacketCap(val as _) };
    //}
}

impl DXGK_FLIPCAPS {
    #[inline]
    pub fn set_FlipOnVSyncMmIo(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_FlipOnVSyncMmIo(val as _) };
    }
    #[inline]
    pub fn set_FlipOnVSyncWithNoWait(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_FlipOnVSyncWithNoWait(val as _) };
    }
    #[inline]
    pub fn set_FlipInterval(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_FlipInterval(val as _) };
    }
    #[inline]
    pub fn set_FlipImmediateMmIo(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_FlipImmediateMmIo(val as _) };
    }
    #[inline]
    pub fn set_FlipIndependent(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_FlipIndependent(val as _) };
    }
}

impl DXGK_VIDMMCAPS {
    #[inline]
    pub fn set_SectionBackedPrimary(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_SectionBackedPrimary(val as _) };
    }
    #[inline]
    pub fn set_VirtualAddressingSupported(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_VirtualAddressingSupported(val as _) };
    }
    #[inline]
    pub fn set_GpuMmuSupported(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_GpuMmuSupported(val as _) };
    }
    #[inline]
    pub fn set_IoMmuSupported(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_IoMmuSupported(val as _) };
    }
}

impl DXGK_SEGMENTFLAGS {
    #[inline]
    pub fn set_Aperture(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_Aperture(val as _) };
    }

    #[inline]
    pub fn set_CacheCoherent(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_CacheCoherent(val as _) };
    }

    #[inline]
    pub fn set_CpuVisible(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_CpuVisible(val as _) };
    }

    #[inline]
    pub fn set_DirectFlip(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_DirectFlip(val as _) };
    }
}

impl DXGK_CREATECONTEXTFLAGS {
    #[inline]
    pub fn SystemContext(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.SystemContext() != 0 }
    }
    #[inline]
    pub fn set_SystemContext(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_SystemContext(val as _) };
    }

    #[inline]
    pub fn GdiContext(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.GdiContext() != 0 }
    }
    #[inline]
    pub fn set_GdiContext(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_GdiContext(val as _) };
    }

    #[inline]
    pub fn VirtualAddressing(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.VirtualAddressing() != 0 }
    }
    #[inline]
    pub fn set_VirtualAddressing(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_VirtualAddressing(val as _) };
    }

}

impl DXGK_CONTEXTINFO_CAPS {
    #[inline]
    pub fn set_NoPatchingRequired(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_NoPatchingRequired(val as _) };
    }
    #[inline]
    pub fn set_DriverManagesResidency(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_DriverManagesResidency(val as _) };
    }
    #[inline]
    pub fn set_UseIoMmu(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_UseIoMmu(val as _) };
    }
}

impl D3DDDI_PATCHLOCATIONLIST {
    #[inline]
    pub fn set_SlotId(&mut self, val: usize) {
        unsafe {
            self.__bindgen_anon_1.__bindgen_anon_1.set_SlotId(val as _);
        }
    }
}

impl DXGK_ALLOCATIONLIST {
    #[inline]
    pub fn SegmentId(&self) -> u8 {
        unsafe {
            (self.__bindgen_anon_1.SegmentId() & 0x1f) as _
        }
    }
}

impl DXGK_CREATEALLOCATIONFLAGS {
    #[inline]
    pub fn Resource(&self) -> bool {
        unsafe {
            self.__bindgen_anon_1.__bindgen_anon_1.Resource() != 0
        }
    }
}

impl DXGK_DESTROYALLOCATIONFLAGS {
    #[inline]
    pub fn DestroyResource(&self) -> bool {
        unsafe {
            self.__bindgen_anon_1.__bindgen_anon_1.DestroyResource() != 0
        }
    }
}

impl DXGK_ALLOCATIONINFO {
    #[inline]
    pub fn Flags_mut(&mut self) -> &mut DXGK_ALLOCATIONINFOFLAGS {
        unsafe {
            &mut self.__bindgen_anon_4.Flags
        }
    }

    #[inline]
    pub fn FlagsWddm2_mut(&mut self) -> &mut DXGK_ALLOCATIONINFOFLAGS_WDDM2_0 {
        unsafe {
            &mut self.__bindgen_anon_4.FlagsWddm2
        }
    }

    #[inline]
    pub fn SupportedReadSegmentSet_mut(&mut self) -> &mut UINT {
        unsafe {
            &mut self.__bindgen_anon_2.SupportedReadSegmentSet
        }
    }

    #[inline]
    pub fn Alignment_mut(&mut self) -> &mut UINT {
        unsafe {
            &mut self.__bindgen_anon_1.Alignment
        }
    }

    #[inline]
    pub fn MaximumRenamingListLength_mut(&mut self) -> &mut UINT {
        unsafe {
            &mut self.__bindgen_anon_3.MaximumRenamingListLength
        }
    }

    #[inline]
    pub fn PhysicalAdapterIndex_mut(&mut self) -> &mut UINT {
        unsafe {
            &mut self.__bindgen_anon_3.PhysicalAdapterIndex
        }
    }
}

impl fmt::Debug for DXGK_ALLOCATIONINFO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DXGK_ALLOCATIONINFO")
            .field("Size", &self.Size)
            .field("HintedBank", &unsafe { self.HintedBank.__bindgen_anon_1.Value })
            .field("PreferredSegment", &unsafe { self.PreferredSegment.__bindgen_anon_1.Value })
            .field("SupportedReadSegmentSet", &unsafe { self.__bindgen_anon_2.SupportedReadSegmentSet })
            .field("SupportedWriteSegmentSet", &self.SupportedWriteSegmentSet)
            .field("Allocation", &self.hAllocation)
            .field("Flags", &unsafe { self.__bindgen_anon_4.Flags.__bindgen_anon_1.Value })
            .field("AllocationPriority", &self.AllocationPriority)
            .finish()
    }
}


impl DXGK_ALLOCATIONINFOFLAGS {
    pub fn set_Value(&mut self, val: u32) {
        unsafe {
            self.__bindgen_anon_1.Value = val;
        }
    }

    #[inline]
    pub fn set_CpuVisible(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_CpuVisible(val as _); }
    }

    //#[inline]
    //pub fn set_AccessedPhysically(&mut self, val: bool) {
    //    unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_AccessedPhysically(val as _); }
    //}
    //
    //#[inline]
    //pub fn set_ExplicitResidencyNotification(&mut self, val: bool) {
    //    unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_ExplicitResidencyNotification(val as _); }
    //}
}

impl DXGK_ALLOCATIONINFOFLAGS_WDDM2_0 {
    pub fn set_Value(&mut self, val: u32) {
        unsafe {
            self.__bindgen_anon_1.Value = val;
        }
    }

    #[inline]
    pub fn set_CpuVisible(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_CpuVisible(val as _); }
    }

    #[inline]
    pub fn set_AccessedPhysically(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_AccessedPhysically(val as _); }
    }

    //#[inline]
    //pub fn set_ExplicitResidencyNotification(&mut self, val: bool) {
    //    unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_ExplicitResidencyNotification(val as _); }
    //}
}

impl DXGK_SEGMENTPREFERENCE {
    pub fn set_Value(&mut self, val: u32) {
        unsafe {
            self.__bindgen_anon_1.Value = val;
        }
    }

    #[inline]
    pub fn set_SegmentId0(&mut self, val: u8) {
        unsafe {
            self.__bindgen_anon_1.__bindgen_anon_1.set_SegmentId0((val & 0x1f) as _);
        }
    }

    #[inline]
    pub fn set_Direction0(&mut self, end: bool) {
        unsafe {
            self.__bindgen_anon_1.__bindgen_anon_1.set_Direction0(end as _);
        }
    }
}

impl DXGK_SEGMENTBANKPREFERENCE {
    pub fn set_Value(&mut self, val: u32) {
        unsafe {
            self.__bindgen_anon_1.Value = val;
        }
    }
}

impl DXGK_ENGINESTATUS {
    pub fn set_Responsive(&mut self, val: bool) {
        unsafe {
            self.__bindgen_anon_1.__bindgen_anon_1.set_Responsive(val as _);
        }
    }
}

impl DXGKARG_RENDER {
    pub fn dma_buf(&self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(self.pDmaBuffer as *mut u8, self.DmaSize as _)
        }
    }

    pub fn command_buf(&self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(self.pCommand as *mut u8, self.CommandLength as _)
        }
    }
}

impl fmt::Debug for LARGE_INTEGER {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", unsafe { self.QuadPart })
    }
}

impl fmt::Debug for DXGKARG_RENDER {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DXGKARG_RENDER")
            .field("pCommand", &self.pCommand)
            .field("CommandLength", &self.CommandLength)
            .field("pDmaBuffer", &self.pDmaBuffer)
            .field("DmaSize", &self.DmaSize)
            .field("DmaBufferSegmentId", &self.DmaBufferSegmentId)
            .field("DmaBufferPhysicalAddress", &self.DmaBufferPhysicalAddress)
            .field("pPatchLocationListIn", &self.pPatchLocationListIn)
            .field("PatchLocationListInSize", &self.PatchLocationListInSize)
            .field("pPatchLocationListOut", &self.pPatchLocationListOut)
            .field("PatchLocationListOutSize", &self.PatchLocationListOutSize)
            .field("AllocationListSize", &self.AllocationListSize)
            .field("pAllocationList", &self.pAllocationList)
            .finish()
    }
}

impl fmt::Debug for DXGKARG_PATCH {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DXGKARG_PATCH")
            .field("pDmaBuffer", &self.pDmaBuffer)
            .field("DmaSize", &self.DmaBufferSize)
            .field("DmaBufferSegmentId", &self.DmaBufferSegmentId)
            .field("DmaBufferPhysicalAddress", &self.DmaBufferPhysicalAddress)
            .field("pPatchLocationList", &self.pPatchLocationList)
            .field("PatchLocationListSize", &self.PatchLocationListSize)
            .field("AllocationListSize", &self.AllocationListSize)
            .field("pAllocationList", &self.pAllocationList)
            .finish()
    }
}

impl fmt::Debug for DXGKARG_SUBMITCOMMAND {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DXGKARG_SUBMITCOMMAND")
            .field("SubmissionFenceId", &self.SubmissionFenceId)
            .field("Flags", &self.Flags)
            .field("EngineOrdinal", &self.EngineOrdinal)
            .field("NodeOrdinal", &self.NodeOrdinal)
            .field("hContext", unsafe { &self.__bindgen_anon_1.hContext })
            .field("DmaBufferPhysicalAddress", &format_args!("{:X}", unsafe { &self.DmaBufferPhysicalAddress.QuadPart }))
            .field("DmaBufferRange", &format_args!("{}..{}", &self.DmaBufferSubmissionStartOffset, &self.DmaBufferSubmissionEndOffset))
            .field("pDmaBufferPrivateData", &self.pDmaBufferPrivateData)
            .field("DmaBufferPrivateDataSize", &self.DmaBufferPrivateDataSize)
            .field("DmaBufferPrivateRange", &format_args!("{}..{}", &self.DmaBufferPrivateDataSubmissionStartOffset, &self.DmaBufferPrivateDataSubmissionEndOffset))
            .finish()
    }
}

impl fmt::Debug for DXGKARG_SUBMITCOMMANDVIRTUAL {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DXGKARG_SUBMITCOMMANDVIRTUAL")
            .field("SubmissionFenceId", &self.SubmissionFenceId)
            .field("Flags", &self.Flags)
            .field("EngineOrdinal", &self.EngineOrdinal)
            .field("NodeOrdinal", &self.NodeOrdinal)
            .field("hContext", &self.hContext)
            .field("DmaBufferVirtualAddress", &self.DmaBufferVirtualAddress)
            .field("DmaBufferSize", &self.DmaBufferSize)
            .field("pDmaBufferPrivateData", &self.pDmaBufferPrivateData)
            .field("DmaBufferPrivateDataSize", &self.DmaBufferPrivateDataSize)
            .field("DmaBufferUmdPrivateDataSize", &self.DmaBufferUmdPrivateDataSize)
            .finish()
    }
}

impl fmt::Debug for D3DDDI_PATCHLOCATIONLIST {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("D3DDDI_PATCHLOCATIONLIST")
            .field("AllocationIndex", &self.AllocationIndex)
            .field("SlotId", &unsafe { self.__bindgen_anon_1.Value })
            .field("DriverId", &self.DriverId)
            .field("AllocationOffset", &self.AllocationOffset)
            .field("PatchOffset", &self.PatchOffset)
            .field("SplitOffset", &self.SplitOffset)
            .finish()
    }
}

impl DXGK_SUBMITCOMMANDFLAGS {
    #[inline]
    pub fn Paging(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Paging() != 0 }
    }
    #[inline]
    pub fn Present(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Present() != 0  }
    }
    #[inline]
    pub fn RedirectedPresent(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.RedirectedPresent() != 0  }
    }
    #[inline]
    pub fn NullRendering(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.NullRendering()  != 0 }
    }
    #[inline]
    pub fn Flip(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Flip() != 0  }
    }
    #[inline]
    pub fn FlipWithNoWait(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.FlipWithNoWait() != 0 }
    }
    #[inline]
    pub fn ContextSwitch(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.ContextSwitch() != 0 }
    }
    #[inline]
    pub fn Resubmission(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Resubmission() != 0 }
    }
}

impl fmt::Debug for DXGK_SUBMITCOMMANDFLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("Paging", self.Paging());
        write_flag!("Present", self.Present());
        write_flag!("RedirectedPresent", self.RedirectedPresent());
        write_flag!("NullRendering", self.NullRendering());
        write_flag!("Flip", self.Flip());
        write_flag!("FlipWithNoWait", self.FlipWithNoWait());
        write_flag!("ContextSwitch", self.ContextSwitch());
        write_flag!("Resubmission", self.Resubmission());

        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

impl fmt::Debug for DXGK_CREATECONTEXTFLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("SystemContext", self.SystemContext());
        write_flag!("GdiContext", self.GdiContext());
        write_flag!("VirtualAddressing", self.VirtualAddressing());

        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

//impl DXGK_NODEMETADATA_FLAGS {
//    #[inline]
//    pub fn set_ContextSchedulingSupported(&mut self, val: bool) {
//        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_ContextSchedulingSupported(val as _) };
//    }
//}

impl fmt::Debug for MEMORY_CACHING_TYPE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            MEMORY_CACHING_TYPE::MmNonCached              => write!(f, "MmNonCached"),
            MEMORY_CACHING_TYPE::MmCached                 => write!(f, "MmCached"),
            MEMORY_CACHING_TYPE::MmWriteCombined          => write!(f, "MmWriteCombined"),
            MEMORY_CACHING_TYPE::MmHardwareCoherentCached => write!(f, "MmHardwareCoherentCached"),
            MEMORY_CACHING_TYPE::MmNonCachedUnordered     => write!(f, "MmNonCachedUnordered"),
            MEMORY_CACHING_TYPE::MmUSWCCached             => write!(f, "MmUSWCCached"),
            MEMORY_CACHING_TYPE::MmMaximumCacheType       => write!(f, "MmMaximumCacheType"),
            MEMORY_CACHING_TYPE::MmNotMapped              => write!(f, "MmNotMapped"),
            _                                             => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for DXGK_QUERYADAPTERINFOTYPE {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_UMDRIVERPRIVATE              => write!(f, "UMDRIVERPRIVATE"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_DRIVERCAPS                   => write!(f, "DRIVERCAPS"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_QUERYSEGMENT                 => write!(f, "QUERYSEGMENT"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_RESERVED                     => write!(f, "RESERVED"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_QUERYSEGMENT2                => write!(f, "QUERYSEGMENT2"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_QUERYSEGMENT3                => write!(f, "QUERYSEGMENT3"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_NUMPOWERCOMPONENTS           => write!(f, "NUMPOWERCOMPONENTS"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_POWERCOMPONENTINFO           => write!(f, "POWERCOMPONENTINFO"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_PREFERREDGPUNODE             => write!(f, "PREFERREDGPUNODE"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_POWERCOMPONENTPSTATEINFO     => write!(f, "POWERCOMPONENTPSTATEINFO"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_HISTORYBUFFERPRECISION       => write!(f, "HISTORYBUFFERPRECISION"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_QUERYSEGMENT4                => write!(f, "QUERYSEGMENT4"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_SEGMENTMEMORYSTATE           => write!(f, "SEGMENTMEMORYSTATE"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_GPUMMUCAPS                   => write!(f, "GPUMMUCAPS"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_PAGETABLELEVELDESC           => write!(f, "PAGETABLELEVELDESC"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_PHYSICALADAPTERCAPS          => write!(f, "PHYSICALADAPTERCAPS"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_DISPLAY_DRIVERCAPS_EXTENSION => write!(f, "DISPLAY_DRIVERCAPS_EXTENSION"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_64BITONLYCAPS                => write!(f, "64BITONLYCAPS"),
            DXGK_QUERYADAPTERINFOTYPE::DXGKQAITYPE_PAGINGPROCESSGPUVASIZE       => write!(f, "PAGINGPROCESSGPUVASIZE"),
            _                                                                   => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DKMDT_STANDARDALLOCATION_TYPE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DKMDT_STANDARDALLOCATION_TYPE::D3DKMDT_STANDARDALLOCATION_SHAREDPRIMARYSURFACE => write!(f, "SHARED_PRIMARY_SURFACE"),
            D3DKMDT_STANDARDALLOCATION_TYPE::D3DKMDT_STANDARDALLOCATION_SHADOWSURFACE        => write!(f, "SHADOW_SURFACE"),
            D3DKMDT_STANDARDALLOCATION_TYPE::D3DKMDT_STANDARDALLOCATION_STAGINGSURFACE       => write!(f, "STAGING_SURFACE"),
            D3DKMDT_STANDARDALLOCATION_TYPE::D3DKMDT_STANDARDALLOCATION_GDISURFACE           => write!(f, "GDI_SURFACE"),
            D3DKMDT_STANDARDALLOCATION_TYPE::D3DKMDT_STANDARDALLOCATION_FENCESTORAGE         => write!(f, "FENCE_STORAGE"),
            _                                                                                => write!(f, "Unknown({})", self.0),
        }
    }
}

impl fmt::Debug for D3DKMDT_GRAPHICS_PREEMPTION_GRANULARITY {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            D3DKMDT_GRAPHICS_PREEMPTION_GRANULARITY::D3DKMDT_GRAPHICS_PREEMPTION_NONE                => write!(f, "NONE"),
            D3DKMDT_GRAPHICS_PREEMPTION_GRANULARITY::D3DKMDT_GRAPHICS_PREEMPTION_DMA_BUFFER_BOUNDARY => write!(f, "DMA_BUFFER"),
            D3DKMDT_GRAPHICS_PREEMPTION_GRANULARITY::D3DKMDT_GRAPHICS_PREEMPTION_PRIMITIVE_BOUNDARY  => write!(f, "PRIMITIVE"),
            D3DKMDT_GRAPHICS_PREEMPTION_GRANULARITY::D3DKMDT_GRAPHICS_PREEMPTION_TRIANGLE_BOUNDARY   => write!(f, "TRIANGLE"),
            D3DKMDT_GRAPHICS_PREEMPTION_GRANULARITY::D3DKMDT_GRAPHICS_PREEMPTION_PIXEL_BOUNDARY      => write!(f, "PIXEL"),
            D3DKMDT_GRAPHICS_PREEMPTION_GRANULARITY::D3DKMDT_GRAPHICS_PREEMPTION_SHADER_BOUNDARY     => write!(f, "SHADER"),
            _                                                                                        => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DKMDT_COMPUTE_PREEMPTION_GRANULARITY {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            D3DKMDT_COMPUTE_PREEMPTION_GRANULARITY::D3DKMDT_COMPUTE_PREEMPTION_NONE                  => write!(f, "NONE"),
            D3DKMDT_COMPUTE_PREEMPTION_GRANULARITY::D3DKMDT_COMPUTE_PREEMPTION_DMA_BUFFER_BOUNDARY   => write!(f, "DMA_BUFFER"),
            D3DKMDT_COMPUTE_PREEMPTION_GRANULARITY::D3DKMDT_COMPUTE_PREEMPTION_DISPATCH_BOUNDARY     => write!(f, "DISPATCH"),
            D3DKMDT_COMPUTE_PREEMPTION_GRANULARITY::D3DKMDT_COMPUTE_PREEMPTION_THREAD_GROUP_BOUNDARY => write!(f, "THREAD_GROUP"),
            D3DKMDT_COMPUTE_PREEMPTION_GRANULARITY::D3DKMDT_COMPUTE_PREEMPTION_THREAD_BOUNDARY       => write!(f, "THREAD"),
            D3DKMDT_COMPUTE_PREEMPTION_GRANULARITY::D3DKMDT_COMPUTE_PREEMPTION_SHADER_BOUNDARY       => write!(f, "SHADER"),
            _                                                                                        => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for DXGK_WDDMVERSION {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_WDDMVERSION::DXGKDDI_WDDMv1      => write!(f, "v1"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv1_2    => write!(f, "v1.2"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv1_3    => write!(f, "v1.3"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2      => write!(f, "v2"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_1    => write!(f, "v2.1"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_1_5  => write!(f, "v2.1.5"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_1_6  => write!(f, "v2.1.6"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_2    => write!(f, "v2.2"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_3    => write!(f, "v2.3"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_4    => write!(f, "v2.4"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_5    => write!(f, "v2.5"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_6    => write!(f, "v2.6"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_7    => write!(f, "v2.7"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_8    => write!(f, "v2.8"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv2_9    => write!(f, "v2.9"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv3_0    => write!(f, "v3.0"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv3_1    => write!(f, "v3.1"),
            DXGK_WDDMVERSION::DXGKDDI_WDDMv3_2    => write!(f, "v3.2"),
            DXGK_WDDMVERSION::DXGKDDI_WDDM_LATEST => write!(f, "latest"),
            _ => write!(f, "unknown({})", self.0),
        }
    }
}

impl fmt::Debug for DXGK_BUILDPAGINGBUFFER_OPERATION {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_TRANSFER                  => write!(f, "TRANSFER"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_FILL                      => write!(f, "FILL"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_DISCARD_CONTENT           => write!(f, "DISCARD_CONTENT"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_READ_PHYSICAL             => write!(f, "READ_PHYSICAL"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_WRITE_PHYSICAL            => write!(f, "WRITE_PHYSICAL"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_MAP_APERTURE_SEGMENT      => write!(f, "MAP_APERTURE_SEGMENT"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_UNMAP_APERTURE_SEGMENT    => write!(f, "UNMAP_APERTURE_SEGMENT"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_SPECIAL_LOCK_TRANSFER     => write!(f, "SPECIAL_LOCK_TRANSFER"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_VIRTUAL_TRANSFER          => write!(f, "VIRTUAL_TRANSFER"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_VIRTUAL_FILL              => write!(f, "VIRTUAL_FILL"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_INIT_CONTEXT_RESOURCE     => write!(f, "INIT_CONTEXT_RESOURCE"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_UPDATE_PAGE_TABLE         => write!(f, "UPDATE_PAGE_TABLE"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_FLUSH_TLB                 => write!(f, "FLUSH_TLB"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_UPDATE_CONTEXT_ALLOCATION => write!(f, "UPDATE_CONTEXT_ALLOCATION"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_COPY_PAGE_TABLE_ENTRIES   => write!(f, "COPY_PAGE_TABLE_ENTRIES"),
            DXGK_BUILDPAGINGBUFFER_OPERATION::DXGK_OPERATION_NOTIFY_RESIDENCY          => write!(f, "NOTIFY_RESIDENCY"),
            _                                                                          => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DDDI_FLIPINTERVAL_TYPE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DDDI_FLIPINTERVAL_TYPE::D3DDDI_FLIPINTERVAL_IMMEDIATE               => write!(f, "IMMEDIATE"),
            D3DDDI_FLIPINTERVAL_TYPE::D3DDDI_FLIPINTERVAL_ONE                     => write!(f, "ONE"),
            D3DDDI_FLIPINTERVAL_TYPE::D3DDDI_FLIPINTERVAL_TWO                     => write!(f, "TWO"),
            D3DDDI_FLIPINTERVAL_TYPE::D3DDDI_FLIPINTERVAL_THREE                   => write!(f, "THREE"),
            D3DDDI_FLIPINTERVAL_TYPE::D3DDDI_FLIPINTERVAL_FOUR                    => write!(f, "FOUR"),
            D3DDDI_FLIPINTERVAL_TYPE::D3DDDI_FLIPINTERVAL_IMMEDIATE_ALLOW_TEARING => write!(f, "IMMEDIATE_ALLOW_TEARING"),
            _                                                                     => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for DXGK_INTERRUPT_TYPE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_DMA_COMPLETED                      => write!(f, "DMA_COMPLETED"),
            DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_DMA_PREEMPTED                      => write!(f, "DMA_PREEMPTED"),
            DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_CRTC_VSYNC                         => write!(f, "CRTC_VSYNC"),
            DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_DMA_FAULTED                        => write!(f, "DMA_FAULTED"),
            DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_DISPLAYONLY_VSYNC                  => write!(f, "DISPLAYONLY_VSYNC"),
            DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_DISPLAYONLY_PRESENT_PROGRESS       => write!(f, "DISPLAYONLY_PRESENT_PROGRESS"),
            DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_CRTC_VSYNC_WITH_MULTIPLANE_OVERLAY => write!(f, "CRTC_VSYNC_WITH_MULTIPLANE_OVERLAY"),
            DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_MICACAST_CHUNK_PROCESSING_COMPLETE => write!(f, "MICACAST_CHUNK_PROCESSING_COMPLETE"),
            _                                                                      => write!(f, "unknown({})", self.0),
        }
    }
}


impl fmt::Debug for D3DKMDT_VIDPN_SOURCE_MODE_TYPE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DKMDT_VIDPN_SOURCE_MODE_TYPE::D3DKMDT_RMT_UNINITIALIZED                 => write!(f, "UNINITIALIZED"),
            D3DKMDT_VIDPN_SOURCE_MODE_TYPE::D3DKMDT_RMT_GRAPHICS                      => write!(f, "GRAPHICS"),
            D3DKMDT_VIDPN_SOURCE_MODE_TYPE::D3DKMDT_RMT_TEXT                          => write!(f, "TEXT"),
            D3DKMDT_VIDPN_SOURCE_MODE_TYPE::D3DKMDT_RMT_GRAPHICS_STEREO               => write!(f, "GRAPHICS_STEREO"),
            D3DKMDT_VIDPN_SOURCE_MODE_TYPE::D3DKMDT_RMT_GRAPHICS_STEREO_ADVANCED_SCAN => write!(f, "GRAPHICS_STEREO_ADVANCED_SCAN"),
            _                                                                         => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DKMDT_COLOR_BASIS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DKMDT_COLOR_BASIS::D3DKMDT_CB_UNINITIALIZED => write!(f, "UNINITIALIZED"),
            D3DKMDT_COLOR_BASIS::D3DKMDT_CB_INTENSITY     => write!(f, "INTENSITY"),
            D3DKMDT_COLOR_BASIS::D3DKMDT_CB_SRGB          => write!(f, "SRGB"),
            D3DKMDT_COLOR_BASIS::D3DKMDT_CB_SCRGB         => write!(f, "SCRGB"),
            D3DKMDT_COLOR_BASIS::D3DKMDT_CB_YCBCR         => write!(f, "YCBCR"),
            D3DKMDT_COLOR_BASIS::D3DKMDT_CB_YPBPR         => write!(f, "YPBPR"),
            _                                             => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DDDI_GAMMARAMP_TYPE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DDDI_GAMMARAMP_TYPE::D3DDDI_GAMMARAMP_UNINITIALIZED => write!(f, "UNINITIALIZED"),
            D3DDDI_GAMMARAMP_TYPE::D3DDDI_GAMMARAMP_DEFAULT       => write!(f, "DEFAULT"),
            D3DDDI_GAMMARAMP_TYPE::D3DDDI_GAMMARAMP_RGB256x3x16   => write!(f, "RGB256x3x16"),
            D3DDDI_GAMMARAMP_TYPE::D3DDDI_GAMMARAMP_DXGI_1        => write!(f, "DXGI_1"),
            _                                                     => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DKMDT_VIDPN_PRESENT_PATH_SCALING {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_UNINITIALIZED          => write!(f, "UNINITIALIZED"),
            D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_IDENTITY               => write!(f, "IDENTITY"),
            D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_CENTERED               => write!(f, "CENTERED"),
            D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_STRETCHED              => write!(f, "STRETCHED"),
            D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_ASPECTRATIOCENTEREDMAX => write!(f, "ASPECTRATIOCENTEREDMAX"),
            D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_CUSTOM                 => write!(f, "CUSTOM"),
            D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_RESERVED1              => write!(f, "RESERVED1"),
            D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_UNPINNED               => write!(f, "UNPINNED"),
            D3DKMDT_VIDPN_PRESENT_PATH_SCALING::D3DKMDT_VPPS_NOTSPECIFIED           => write!(f, "NOTSPECIFIED"),
            _                                                                       => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DKMDT_VIDPN_PRESENT_PATH_ROTATION {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_UNINITIALIZED => write!(f, "UNINITIALIZED"),
            D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_IDENTITY      => write!(f, "IDENTITY"),
            D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_ROTATE90      => write!(f, "ROTATE90"),
            D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_ROTATE180     => write!(f, "ROTATE180"),
            D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_ROTATE270     => write!(f, "ROTATE270"),
            D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_UNPINNED      => write!(f, "UNPINNED"),
            D3DKMDT_VIDPN_PRESENT_PATH_ROTATION::D3DKMDT_VPPR_NOTSPECIFIED  => write!(f, "NOTSPECIFIED"),
            _                                                               => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DKMDT_PIXEL_VALUE_ACCESS_MODE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DKMDT_PIXEL_VALUE_ACCESS_MODE::D3DKMDT_PVAM_UNINITIALIZED   => write!(f, "UNINITIALIZED"),
            D3DKMDT_PIXEL_VALUE_ACCESS_MODE::D3DKMDT_PVAM_DIRECT          => write!(f, "DIRECT"),
            D3DKMDT_PIXEL_VALUE_ACCESS_MODE::D3DKMDT_PVAM_PRESETPALETTE   => write!(f, "PRESETPALETTE"),
            D3DKMDT_PIXEL_VALUE_ACCESS_MODE::D3DKMDT_PVAM_SETTABLEPALETTE => write!(f, "SETTABLEPALETTE"),
            _ =>                                                             write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DDDIFORMAT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DDDIFORMAT::D3DDDIFMT_UNKNOWN                 => write!(f, "UNKNOWN"),
            D3DDDIFORMAT::D3DDDIFMT_R8G8B8                  => write!(f, "R8G8B8"),
            D3DDDIFORMAT::D3DDDIFMT_A8R8G8B8                => write!(f, "A8R8G8B8"),
            D3DDDIFORMAT::D3DDDIFMT_X8R8G8B8                => write!(f, "X8R8G8B8"),
            D3DDDIFORMAT::D3DDDIFMT_R5G6B5                  => write!(f, "R5G6B5"),
            D3DDDIFORMAT::D3DDDIFMT_X1R5G5B5                => write!(f, "X1R5G5B5"),
            D3DDDIFORMAT::D3DDDIFMT_A1R5G5B5                => write!(f, "A1R5G5B5"),
            D3DDDIFORMAT::D3DDDIFMT_A4R4G4B4                => write!(f, "A4R4G4B4"),
            D3DDDIFORMAT::D3DDDIFMT_R3G3B2                  => write!(f, "R3G3B2"),
            D3DDDIFORMAT::D3DDDIFMT_A8                      => write!(f, "A8"),
            D3DDDIFORMAT::D3DDDIFMT_A8R3G3B2                => write!(f, "A8R3G3B2"),
            D3DDDIFORMAT::D3DDDIFMT_X4R4G4B4                => write!(f, "X4R4G4B4"),
            D3DDDIFORMAT::D3DDDIFMT_A2B10G10R10             => write!(f, "A2B10G10R10"),
            D3DDDIFORMAT::D3DDDIFMT_A8B8G8R8                => write!(f, "A8B8G8R8"),
            D3DDDIFORMAT::D3DDDIFMT_X8B8G8R8                => write!(f, "X8B8G8R8"),
            D3DDDIFORMAT::D3DDDIFMT_G16R16                  => write!(f, "G16R16"),
            D3DDDIFORMAT::D3DDDIFMT_A2R10G10B10             => write!(f, "A2R10G10B10"),
            D3DDDIFORMAT::D3DDDIFMT_A16B16G16R16            => write!(f, "A16B16G16R16"),
            D3DDDIFORMAT::D3DDDIFMT_A8P8                    => write!(f, "A8P8"),
            D3DDDIFORMAT::D3DDDIFMT_P8                      => write!(f, "P8"),
            D3DDDIFORMAT::D3DDDIFMT_L8                      => write!(f, "L8"),
            D3DDDIFORMAT::D3DDDIFMT_A8L8                    => write!(f, "A8L8"),
            D3DDDIFORMAT::D3DDDIFMT_A4L4                    => write!(f, "A4L4"),
            D3DDDIFORMAT::D3DDDIFMT_V8U8                    => write!(f, "V8U8"),
            D3DDDIFORMAT::D3DDDIFMT_L6V5U5                  => write!(f, "L6V5U5"),
            D3DDDIFORMAT::D3DDDIFMT_X8L8V8U8                => write!(f, "X8L8V8U8"),
            D3DDDIFORMAT::D3DDDIFMT_Q8W8V8U8                => write!(f, "Q8W8V8U8"),
            D3DDDIFORMAT::D3DDDIFMT_V16U16                  => write!(f, "V16U16"),
            D3DDDIFORMAT::D3DDDIFMT_W11V11U10               => write!(f, "W11V11U10"),
            D3DDDIFORMAT::D3DDDIFMT_A2W10V10U10             => write!(f, "A2W10V10U10"),
            D3DDDIFORMAT::D3DDDIFMT_UYVY                    => write!(f, "UYVY"),
            D3DDDIFORMAT::D3DDDIFMT_R8G8_B8G8               => write!(f, "R8G8_B8G8"),
            D3DDDIFORMAT::D3DDDIFMT_YUY2                    => write!(f, "YUY2"),
            D3DDDIFORMAT::D3DDDIFMT_G8R8_G8B8               => write!(f, "G8R8_G8B8"),
            D3DDDIFORMAT::D3DDDIFMT_DXT1                    => write!(f, "DXT1"),
            D3DDDIFORMAT::D3DDDIFMT_DXT2                    => write!(f, "DXT2"),
            D3DDDIFORMAT::D3DDDIFMT_DXT3                    => write!(f, "DXT3"),
            D3DDDIFORMAT::D3DDDIFMT_DXT4                    => write!(f, "DXT4"),
            D3DDDIFORMAT::D3DDDIFMT_DXT5                    => write!(f, "DXT5"),
            D3DDDIFORMAT::D3DDDIFMT_D16_LOCKABLE            => write!(f, "D16_LOCKABLE"),
            D3DDDIFORMAT::D3DDDIFMT_D32                     => write!(f, "D32"),
            D3DDDIFORMAT::D3DDDIFMT_D15S1                   => write!(f, "D15S1"),
            D3DDDIFORMAT::D3DDDIFMT_D24S8                   => write!(f, "D24S8"),
            D3DDDIFORMAT::D3DDDIFMT_D24X8                   => write!(f, "D24X8"),
            D3DDDIFORMAT::D3DDDIFMT_D24X4S4                 => write!(f, "D24X4S4"),
            D3DDDIFORMAT::D3DDDIFMT_D16                     => write!(f, "D16"),
            D3DDDIFORMAT::D3DDDIFMT_D32F_LOCKABLE           => write!(f, "D32F_LOCKABLE"),
            D3DDDIFORMAT::D3DDDIFMT_D24FS8                  => write!(f, "D24FS8"),
            D3DDDIFORMAT::D3DDDIFMT_D32_LOCKABLE            => write!(f, "D32_LOCKABLE"),
            D3DDDIFORMAT::D3DDDIFMT_S8_LOCKABLE             => write!(f, "S8_LOCKABLE"),
            D3DDDIFORMAT::D3DDDIFMT_S1D15                   => write!(f, "S1D15"),
            D3DDDIFORMAT::D3DDDIFMT_S8D24                   => write!(f, "S8D24"),
            D3DDDIFORMAT::D3DDDIFMT_X8D24                   => write!(f, "X8D24"),
            D3DDDIFORMAT::D3DDDIFMT_X4S4D24                 => write!(f, "X4S4D24"),
            D3DDDIFORMAT::D3DDDIFMT_L16                     => write!(f, "L16"),
            D3DDDIFORMAT::D3DDDIFMT_G8R8                    => write!(f, "G8R8"),
            D3DDDIFORMAT::D3DDDIFMT_R8                      => write!(f, "R8"),
            D3DDDIFORMAT::D3DDDIFMT_VERTEXDATA              => write!(f, "VERTEXDATA"),
            D3DDDIFORMAT::D3DDDIFMT_INDEX16                 => write!(f, "INDEX16"),
            D3DDDIFORMAT::D3DDDIFMT_INDEX32                 => write!(f, "INDEX32"),
            D3DDDIFORMAT::D3DDDIFMT_Q16W16V16U16            => write!(f, "Q16W16V16U16"),
            D3DDDIFORMAT::D3DDDIFMT_MULTI2_ARGB8            => write!(f, "MULTI2_ARGB8"),
            D3DDDIFORMAT::D3DDDIFMT_R16F                    => write!(f, "R16F"),
            D3DDDIFORMAT::D3DDDIFMT_G16R16F                 => write!(f, "G16R16F"),
            D3DDDIFORMAT::D3DDDIFMT_A16B16G16R16F           => write!(f, "A16B16G16R16F"),
            D3DDDIFORMAT::D3DDDIFMT_R32F                    => write!(f, "R32F"),
            D3DDDIFORMAT::D3DDDIFMT_G32R32F                 => write!(f, "G32R32F"),
            D3DDDIFORMAT::D3DDDIFMT_A32B32G32R32F           => write!(f, "A32B32G32R32F"),
            D3DDDIFORMAT::D3DDDIFMT_CxV8U8                  => write!(f, "CxV8U8"),
            D3DDDIFORMAT::D3DDDIFMT_A1                      => write!(f, "A1"),
            D3DDDIFORMAT::D3DDDIFMT_A2B10G10R10_XR_BIAS     => write!(f, "A2B10G10R10_XR_BIAS"),
            D3DDDIFORMAT::D3DDDIFMT_DXVACOMPBUFFER_BASE     => write!(f, "DXVACOMPBUFFER_BASE"),
            D3DDDIFORMAT::D3DDDIFMT_PICTUREPARAMSDATA       => write!(f, "PICTUREPARAMSDATA"),
            D3DDDIFORMAT::D3DDDIFMT_MACROBLOCKDATA          => write!(f, "MACROBLOCKDATA"),
            D3DDDIFORMAT::D3DDDIFMT_RESIDUALDIFFERENCEDATA  => write!(f, "RESIDUALDIFFERENCEDATA"),
            D3DDDIFORMAT::D3DDDIFMT_DEBLOCKINGDATA          => write!(f, "DEBLOCKINGDATA"),
            D3DDDIFORMAT::D3DDDIFMT_INVERSEQUANTIZATIONDATA => write!(f, "INVERSEQUANTIZATIONDATA"),
            D3DDDIFORMAT::D3DDDIFMT_SLICECONTROLDATA        => write!(f, "SLICECONTROLDATA"),
            D3DDDIFORMAT::D3DDDIFMT_BITSTREAMDATA           => write!(f, "BITSTREAMDATA"),
            D3DDDIFORMAT::D3DDDIFMT_MOTIONVECTORBUFFER      => write!(f, "MOTIONVECTORBUFFER"),
            D3DDDIFORMAT::D3DDDIFMT_FILMGRAINBUFFER         => write!(f, "FILMGRAINBUFFER"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED9          => write!(f, "DXVA_RESERVED9"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED10         => write!(f, "DXVA_RESERVED10"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED11         => write!(f, "DXVA_RESERVED11"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED12         => write!(f, "DXVA_RESERVED12"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED13         => write!(f, "DXVA_RESERVED13"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED14         => write!(f, "DXVA_RESERVED14"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED15         => write!(f, "DXVA_RESERVED15"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED16         => write!(f, "DXVA_RESERVED16"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED17         => write!(f, "DXVA_RESERVED17"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED18         => write!(f, "DXVA_RESERVED18"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED19         => write!(f, "DXVA_RESERVED19"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED20         => write!(f, "DXVA_RESERVED20"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED21         => write!(f, "DXVA_RESERVED21"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED22         => write!(f, "DXVA_RESERVED22"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED23         => write!(f, "DXVA_RESERVED23"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED24         => write!(f, "DXVA_RESERVED24"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED25         => write!(f, "DXVA_RESERVED25"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED26         => write!(f, "DXVA_RESERVED26"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED27         => write!(f, "DXVA_RESERVED27"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED28         => write!(f, "DXVA_RESERVED28"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED29         => write!(f, "DXVA_RESERVED29"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED30         => write!(f, "DXVA_RESERVED30"),
            D3DDDIFORMAT::D3DDDIFMT_DXVA_RESERVED31         => write!(f, "DXVA_RESERVED31"),
            D3DDDIFORMAT::D3DDDIFMT_DXVACOMPBUFFER_MAX      => write!(f, "DXVACOMPBUFFER_MAX"),
            D3DDDIFORMAT::D3DDDIFMT_BINARYBUFFER            => write!(f, "BINARYBUFFER"),
            _ => write!(f, "D3DDDIFORMAT({})", self.0),
        }
    }
}

impl fmt::Debug for D3DDDI_RATIONAL {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("D3DDDI_RATIONAL")
            .field("Numerator", &self.Numerator)
            .field("Denominator", &self.Denominator)
            .finish()
    }
}

impl From<(u32, u32)> for D3DDDI_RATIONAL {
    fn from(value: (u32, u32)) -> Self {
        Self {
            Numerator: value.0,
            Denominator: value.1,
        }
    }
}

impl fmt::Debug for D3DKMDT_SHAREDPRIMARYSURFACEDATA {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("D3DKMDT_SHAREDPRIMARYSURFACEDATA")
            .field("Width", &self.Width)
            .field("Height", &self.Height)
            .field("Format", &self.Format)
            .field("RefreshRate", &self.RefreshRate)
            .field("VidPnSourceId", &self.VidPnSourceId)
            .finish()
    }
}

impl fmt::Debug for D3DKMDT_SHADOWSURFACEDATA {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("D3DKMDT_SHADOWSURFACEDATA")
            .field("Width", &self.Width)
            .field("Height", &self.Height)
            .field("Format", &self.Format)
            .field("Pitch", &self.Pitch)
            .finish()
    }
}

impl fmt::Debug for D3DKMDT_STAGINGSURFACEDATA {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("D3DKMDT_STAGINGSURFACEDATA")
            .field("Width", &self.Width)
            .field("Height", &self.Height)
            .field("Pitch", &self.Pitch)
            .finish()
    }
}

impl DXGK_SETVIDPNSOURCEADDRESS_FLAGS {
    #[inline]
    pub fn ModeChange(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.ModeChange() != 0 }
    }
    #[inline]
    pub fn FlipImmediate(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.FlipImmediate() != 0 }
    }
    #[inline]
    pub fn FlipOnNextVSync(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.FlipOnNextVSync() != 0 }
    }
    #[inline]
    pub fn FlipStereo(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.FlipStereo() != 0 }
    }
    #[inline]
    pub fn FlipStereoTemporaryMono(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.FlipStereoTemporaryMono() != 0 }
    }
    #[inline]
    pub fn FlipStereoPreferRight(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.FlipStereoPreferRight() != 0 }
    }
    #[inline]
    pub fn SharedPrimaryTransition(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.SharedPrimaryTransition() != 0 }
    }
    #[inline]
    pub fn IndependentFlipExclusive(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.IndependentFlipExclusive() != 0 }
    }
    //#[inline]
    //pub fn MoveFlip(&self) -> bool {
    //    unsafe { self.__bindgen_anon_1.__bindgen_anon_1.MoveFlip() != 0 }
    //}
}

impl fmt::Debug for DXGK_SETVIDPNSOURCEADDRESS_FLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("ModeChange", self.ModeChange());
        write_flag!("FlipImmediate", self.FlipImmediate());
        write_flag!("FlipOnNextVSync", self.FlipOnNextVSync());
        write_flag!("FlipStereo", self.FlipStereo());
        write_flag!("FlipStereoTemporaryMono", self.FlipStereoTemporaryMono());
        write_flag!("FlipStereoPreferRight", self.FlipStereoPreferRight());
        write_flag!("SharedPrimaryTransition", self.SharedPrimaryTransition());
        write_flag!("IndependentFlipExclusive", self.IndependentFlipExclusive());
        //write_flag!("MoveFlip", self.MoveFlip());

        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

impl DXGK_MULTIPLANE_OVERLAY_FLAGS {
    #[inline]
    pub fn VerticalFlip(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.VerticalFlip() != 0 }
    }
    #[inline]
    pub fn HorizontalFlip(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.HorizontalFlip() != 0 }
    }
}

impl fmt::Debug for DXGK_MULTIPLANE_OVERLAY_FLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("VerticalFlip", self.VerticalFlip());
        write_flag!("HorizontalFlip", self.HorizontalFlip());

        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

impl DXGK_MULTIPLANE_OVERLAY_BLEND {
    #[inline]
    pub fn AlphaBlend(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.AlphaBlend() != 0 }
    }
}

impl fmt::Debug for DXGK_MULTIPLANE_OVERLAY_BLEND {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("AlphaBlend", self.AlphaBlend());

        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

impl fmt::Debug for D3DDDI_ROTATION {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DDDI_ROTATION::D3DDDI_ROTATION_IDENTITY => write!(f, "IDENTITY"),
            D3DDDI_ROTATION::D3DDDI_ROTATION_90       => write!(f, "90"),
            D3DDDI_ROTATION::D3DDDI_ROTATION_180      => write!(f, "180"),
            D3DDDI_ROTATION::D3DDDI_ROTATION_270      => write!(f, "270"),
            _ => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for DXGKARG_SETVIDPNSOURCEADDRESSWITHMULTIPLANEOVERLAY {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DXGKARG_SETVIDPNSOURCEADDRESSWITHMULTIPLANEOVERLAY")
            .field("ContextCount", &self.ContextCount)
            .field("Flags", &self.Flags)
            .field("VidPnSourceId", &self.VidPnSourceId)
            .field("PlaneCount", &self.PlaneCount)
            .finish()
    }
}

impl fmt::Debug for DXGK_MULTIPLANE_OVERLAY_PLANE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OVERLAY_PLANE")
            .field("LayerIndex", &self.LayerIndex)
            .field("Enabled", &{ self.Enabled != 0 })
            .field("AllocationSegment", &self.AllocationSegment)
            .field("AllocationAddress", &self.AllocationAddress)
            .field("hAllocation", &self.hAllocation)
            .field("PlaneAttributes", &self.PlaneAttributes)
            .finish()
    }
}

impl fmt::Debug for DXGK_MULTIPLANE_OVERLAY_ATTRIBUTES {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OVERLAY_ATTRIBUTES")
            .field("Flags", &self.Flags)
            .field("SrcRect", &self.SrcRect)
            .field("DstRect", &self.DstRect)
            .field("ClipRect", &self.ClipRect)
            .field("Rotation", &self.Rotation)
            .field("Blend", &self.Blend)
            .finish()
    }
}

impl fmt::Debug for RECT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RECT")
            .field("left", &self.left)
            .field("top", &self.top)
            .field("right", &self.right)
            .field("bottom", &self.bottom)
            .finish()
    }
}

//impl BitOr for RECT {
//    type Output = RECT;
//
//    fn bitor(self, rhs: Self) -> RECT {
//        let left = min(self.left, rhs.left);
//        let top = min(self.top, rhs.top);
//
//        let right = max(self.right, rhs.right);
//        let bottom = max(self.bottom, rhs.bottom);
//
//        RECT { left, top, right, bottom }
//    }
//}

impl BitOrAssign for RECT {
    fn bitor_assign(&mut self, rhs: Self) {
        self.left = min(self.left, rhs.left);
        self.top = min(self.top, rhs.top);
        self.right = max(self.right, rhs.right);
        self.bottom = max(self.bottom, rhs.bottom);
    }
}

impl RECT {
    pub fn top_left(&self) -> (u32, u32, u32) {
        (self.left as _, self.top as _, 0)
    }
    pub fn dimensions(&self) -> (u32, u32, u32) {
        ((self.right - self.left) as _, (self.bottom - self.top) as _, 1)
    }
}

impl Add<(i32, i32)> for RECT {
    type Output = RECT;

    fn add(mut self, val: (i32, i32)) -> RECT {
        self.left   += val.0;
        self.right  += val.0;
        self.top    += val.1;
        self.bottom += val.1;

        self
    }
}


impl fmt::Debug for DXGK_INTERRUPT_STATE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_INTERRUPT_STATE::DXGK_INTERRUPT_ENABLE  => f.write_str("ENABLE"),
            DXGK_INTERRUPT_STATE::DXGK_INTERRUPT_DISABLE => f.write_str("DISABLE"),
            _                                            => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for DXGK_CRTC_VSYNC_STATE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_CRTC_VSYNC_STATE::DXGK_VSYNC_ENABLE             => f.write_str("VSYNC_ENABLE"),
            DXGK_CRTC_VSYNC_STATE::DXGK_VSYNC_DISABLE_KEEP_PHASE => f.write_str("VSYNC_DISABLE_KEEP_PHASE"),
            DXGK_CRTC_VSYNC_STATE::DXGK_VSYNC_DISABLE_NO_PHASE   => f.write_str("VSYNC_DISABLE_NO_PHASE"),
            _                                                    => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for DXGKARG_CONTROLINTERRUPT2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.InterruptType == DXGK_INTERRUPT_TYPE::DXGK_INTERRUPT_CRTC_VSYNC {
            f.debug_struct("DXGKARG_CONTROLINTERRUPT2")
                .field("InterruptType", &self.InterruptType)
                .field("CrtcVsyncState", &unsafe { self.__bindgen_anon_1.CrtcVsyncState })
                .finish()
        } else {
            f.debug_struct("DXGKARG_CONTROLINTERRUPT2")
                .field("InterruptType", &self.InterruptType)
                .field("InterruptState", &unsafe { self.__bindgen_anon_1.InterruptState })
                .finish()
        }
    }
}

impl DXGKCB_GETHANDLEDATAFLAGS {
    #[inline]
    pub fn set_DeviceSpecific(&mut self, val: bool) {
        unsafe {
            self.__bindgen_anon_1.__bindgen_anon_1.set_DeviceSpecific(val as _);
        }
    }
}

impl DXGK_OPENALLOCATIONFLAGS {
    #[inline]
    pub fn Create(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Create() != 0 }
    }
    #[inline]
    pub fn ReadOnly(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.ReadOnly() != 0 }
    }
}

impl fmt::Debug for DXGK_OPENALLOCATIONFLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("Create", self.Create());
        write_flag!("ReadOnly", self.ReadOnly());

        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

impl DXGK_PRESENTFLAGS {
    #[inline]
    pub fn Blt(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Blt() != 0 }
    }
    #[inline]
    pub fn ColorFill(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.ColorFill() != 0 }
    }
    #[inline]
    pub fn Flip(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Flip() != 0 }
    }
    #[inline]
    pub fn FlipWithNoWait(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.FlipWithNoWait() != 0 }
    }
    #[inline]
    pub fn SrcColorKey(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.SrcColorKey() != 0 }
    }
    #[inline]
    pub fn DstColorKey(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.DstColorKey() != 0 }
    }
    #[inline]
    pub fn LinearToSrgb(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.LinearToSrgb() != 0 }
    }
    #[inline]
    pub fn Rotate(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Rotate() != 0 }
    }
}

impl fmt::Debug for DXGK_PRESENTFLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("Blt", self.Blt());
        write_flag!("ColorFill", self.ColorFill());
        write_flag!("Flip", self.Flip());
        write_flag!("FlipWithNoWait", self.FlipWithNoWait());
        write_flag!("SrcColorKey", self.SrcColorKey());
        write_flag!("DstColorKey", self.DstColorKey());
        write_flag!("LinearToSrgb", self.LinearToSrgb());
        write_flag!("Rotate", self.Rotate());

        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

impl fmt::Debug for DXGKARG_PRESENT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DXGKARG_PRESENT")
            //.field("pDmaBuffer", &self.pDmaBuffer)
            .field("DmaSize", &self.DmaSize)
            //.field("pDmaBufferPrivateData", &self.pDmaBufferPrivateData)
            .field("DmaBufferPrivateDataSize", &self.DmaBufferPrivateDataSize)
            //.field("pPatchLocationListOut", &self.pPatchLocationListOut)
            //.field("PatchLocationListOutSize", &self.PatchLocationListOutSize)
            //.field("MultipassOffset", &self.MultipassOffset)
            //.field("Color", &self.Color)
            .field("DstRect", &self.DstRect)
            .field("SrcRect", &self.SrcRect)
            .field("SubRectCnt", &self.SubRectCnt)
            .field("FlipInterval", &self.FlipInterval)
            .field("Flags", &self.Flags)
            .finish()
    }
}

impl fmt::Debug for _DXGKARG_BUILDPAGINGBUFFER__bindgen_ty_1__bindgen_ty_2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fill")
            .field("hAllocation", &self.hAllocation)
            .field("FillSize", &self.FillSize)
            .field("FillPattern", &self.FillPattern)
            .finish()
    }
}

impl DXGK_TRANSFERFLAGS {
    #[inline]
    pub fn Swizzle(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Swizzle() != 0 }
    }

    #[inline]
    pub fn Unswizzle(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Unswizzle() != 0 }
    }

    #[inline]
    pub fn AllocationIsIdle(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.AllocationIsIdle() != 0 }
    }

    #[inline]
    pub fn TransferStart(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.TransferStart() != 0 }
    }

    #[inline]
    pub fn TransferEnd(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.TransferEnd() != 0 }
    }
}

impl fmt::Debug for DXGK_TRANSFERFLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("Swizzle", self.Swizzle());
        write_flag!("Unswizzle", self.Unswizzle());
        write_flag!("AllocationIsIdle", self.AllocationIsIdle());
        write_flag!("TransferStart", self.TransferStart());
        write_flag!("TransferEnd", self.TransferEnd());
        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

impl fmt::Debug for _DXGKARG_BUILDPAGINGBUFFER__bindgen_ty_1__bindgen_ty_1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transfer")
            .field("hAllocation", &self.hAllocation)
            .field("TransferOffset", &self.TransferOffset)
            .field("TransferSize", &self.TransferSize)
            .field("Source", &self.Source.SegmentId)
            .field("Destination", &self.Destination.SegmentId)
            .field("Flags", &self.Flags)
            .finish()
    }
}

impl fmt::Debug for DXGK_PTE_PAGE_SIZE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_PTE_PAGE_SIZE::DXGK_PTE_PAGE_TABLE_PAGE_4KB  => write!(f, "4KiB"),
            DXGK_PTE_PAGE_SIZE::DXGK_PTE_PAGE_TABLE_PAGE_64KB => write!(f, "64KiB"),
            _                                                 => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl _DXGK_PTE__bindgen_ty_1 {
    #[inline]
    pub fn Valid(&self) -> bool {
        unsafe { self.__bindgen_anon_1.Valid() != 0 }
    }

    #[inline]
    pub fn Zero(&self) -> bool {
        unsafe { self.__bindgen_anon_1.Zero() != 0 }
    }

    #[inline]
    pub fn CacheCoherent(&self) -> bool {
        unsafe { self.__bindgen_anon_1.CacheCoherent() != 0 }
    }

    #[inline]
    pub fn ReadOnly(&self) -> bool {
        unsafe { self.__bindgen_anon_1.ReadOnly() != 0 }
    }

    #[inline]
    pub fn NoExecute(&self) -> bool {
        unsafe { self.__bindgen_anon_1.NoExecute() != 0 }
    }

    #[inline]
    pub fn LargePage(&self) -> bool {
        unsafe { self.__bindgen_anon_1.LargePage() != 0 }
    }

    #[inline]
    pub fn Segment(&self) -> u8 {
        unsafe { self.__bindgen_anon_1.Segment() as u8 }
    }

    #[inline]
    pub fn PhysicalAdapterIndex(&self) -> u8 {
        unsafe { self.__bindgen_anon_1.PhysicalAdapterIndex() as u8 }
    }

    #[inline]
    pub fn PageTablePageSize(&self) -> DXGK_PTE_PAGE_SIZE {
        unsafe { DXGK_PTE_PAGE_SIZE(self.__bindgen_anon_1.PageTablePageSize() as _) }
    }
}

impl DXGK_PTE {
    pub fn Flags(&self) -> _DXGK_PTE__bindgen_ty_1 {
        self.__bindgen_anon_1
    }

    pub fn PageAddress(&self) -> u64 {
        unsafe { self.__bindgen_anon_2.PageAddress }
    }

    pub fn Len(&self) -> u64 {
        match self.__bindgen_anon_1.PageTablePageSize() {
            DXGK_PTE_PAGE_SIZE::DXGK_PTE_PAGE_TABLE_PAGE_4KB  => 4 * 1024,
            DXGK_PTE_PAGE_SIZE::DXGK_PTE_PAGE_TABLE_PAGE_64KB => 64 * 1024,
            _                                                 => 0,
        }
    }
}

impl fmt::Debug for _DXGK_PTE__bindgen_ty_1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("Valid", self.Valid());
        write_flag!("Zero", self.Zero());
        write_flag!("CacheCoherent", self.CacheCoherent());
        write_flag!("ReadOnly", self.ReadOnly());
        write_flag!("NoExecute", self.NoExecute());
        write_flag!("LargePage", self.LargePage());

        Ok(())
    }
}

impl fmt::Debug for DXGK_PTE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageTableEntry")
            .field("Flags", &self.Flags())
            .field("Segment", &self.Flags().Segment())
            .field("PhysicalAdapterIndex", &self.Flags().PhysicalAdapterIndex())
            .field("PageTablePageSize", &self.Flags().PageTablePageSize())
            .field("PageAddress", &self.PageAddress())
            .finish()
    }
}
impl fmt::Debug for DXGK_UPDATEPAGETABLEFLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("Repeat", self.Repeat() != 0);
        write_flag!("InitialUpdate", self.InitialUpdate() != 0);
        write_flag!("NotifyEviction", self.NotifyEviction() != 0);
        write_flag!("Use64KBPages", self.Use64KBPages() != 0);

        Ok(())
    }
}

impl fmt::Debug for DXGK_PAGETABLEUPDATEMODE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_PAGETABLEUPDATEMODE::DXGK_PAGETABLEUPDATE_CPU_VIRTUAL  => write!(f, "CPU_VIRTUAL"),
            DXGK_PAGETABLEUPDATEMODE::DXGK_PAGETABLEUPDATE_GPU_VIRTUAL  => write!(f, "GPU_VIRTUAL"),
            DXGK_PAGETABLEUPDATEMODE::DXGK_PAGETABLEUPDATE_GPU_PHYSICAL => write!(f, "GPU_PHYSICAL"),
            _                                                           => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for DXGK_BUILDPAGINGBUFFER_UPDATEPAGETABLE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdatePageTable")
            .field("PageTableLevel", &self.PageTableLevel)
            .field("hAllocation", &self.hAllocation)
            .field("PageTableAddress", &unsafe { self.PageTableAddress.__bindgen_anon_1.CpuVirtual })
            .field("pPageTableEntries", &self.pPageTableEntries)
            .field("StartIndex", &self.StartIndex)
            .field("NumPageTableEntries", &self.NumPageTableEntries)
            .field("Flags", &self.Flags)
            .field("DriverProtection", &self.DriverProtection)
            .field("AllocationOffsetInBytes", &self.AllocationOffsetInBytes)
            .field("hProcess", &self.hProcess)
            .field("UpdateMode", &self.UpdateMode)
            .field("pPageTableEntries64KB", &self.pPageTableEntries64KB)
            .field("FirstPteVirtualAddress", &self.FirstPteVirtualAddress)
            .finish()
    }
}

impl fmt::Debug for DXGK_BUILDPAGINGBUFFER_FLUSHTLB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlushTlb")
            .field("RootPageTableAddress", &self.RootPageTableAddress)
            .field("hProcess", &self.hProcess)
            .field("StartVirtualAddress", &self.StartVirtualAddress)
            .field("EndVirtualAddress", &self.EndVirtualAddress)
            .finish()
    }
}

impl fmt::Debug for DXGK_BUILDPAGINGBUFFER_COPY_RANGE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CopyRange")
            .field("NumPageTableEntries", &self.NumPageTableEntries)
            .field("SrcPageTableAddress", &self.SrcPageTableAddress)
            .field("DstPageTableAddress", &self.DstPageTableAddress)
            .field("SrcStartPteIndex", &self.SrcStartPteIndex)
            .field("DstStartPteIndex", &self.DstStartPteIndex)
            .finish()
    }
}

impl fmt::Debug for DXGK_BUILDPAGINGBUFFER_FILLVIRTUAL {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FillVirtual")
            .field("hAllocation", &self.hAllocation)
            .field("AllocationOffsetInBytes", &self.AllocationOffsetInBytes)
            .field("FillSizeInBytes", &self.FillSizeInBytes)
            .field("FillPattern", &self.FillPattern)
            .field("DestinationVirtualAddress", &self.DestinationVirtualAddress)
            .finish()
    }
}


impl fmt::Debug for DXGK_MEMORY_TRANSFER_DIRECTION {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_MEMORY_TRANSFER_DIRECTION::DXGK_MEMORY_TRANSFER_LOCAL_TO_SYSTEM => write!(f, "LOCAL_TO_SYSTEM"),
            DXGK_MEMORY_TRANSFER_DIRECTION::DXGK_MEMORY_TRANSFER_SYSTEM_TO_LOCAL => write!(f, "SYSTEM_TO_LOCAL"),
            DXGK_MEMORY_TRANSFER_DIRECTION::DXGK_MEMORY_TRANSFER_LOCAL_TO_LOCAL  => write!(f, "LOCAL_TO_LOCAL"),
            _                                                                    => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl DXGK_TRANSFERVIRTUALFLAGS {
    #[inline]
    pub fn Src64KBPages(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Src64KBPages() != 0 }
    }

    #[inline]
    pub fn Dst64KBPages(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Dst64KBPages() != 0 }
    }
}

impl fmt::Debug for DXGK_TRANSFERVIRTUALFLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("Src64KBPages", self.Src64KBPages());
        write_flag!("Dst64KBPages", self.Dst64KBPages());
        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Flags })
    }
}

impl fmt::Debug for DXGK_BUILDPAGINGBUFFER_TRANSFERVIRTUAL {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransferVirtual")
            .field("hAllocation", &self.hAllocation)
            .field("AllocationOffsetInBytes", &self.AllocationOffsetInBytes)
            .field("TransferSizeInBytes", &self.TransferSizeInBytes)
            .field("SourceVirtualAddress", &self.SourceVirtualAddress)
            .field("SourcePageTable", &self.SourcePageTable)
            .field("DestinationVirtualAddress", &self.DestinationVirtualAddress)
            .field("DestinationPageTable", &self.DestinationPageTable)
            .field("TransferDirection", &self.TransferDirection)
            .field("Flags", &self.Flags)
            .finish()
    }
}

impl fmt::Debug for _DXGK_BUILDPAGINGBUFFER_NOTIFYRESIDENCY__bindgen_ty_1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("Resident", self.Resident() != 0);

        Ok(())
    }
}

impl fmt::Debug for DXGK_BUILDPAGINGBUFFER_NOTIFYRESIDENCY {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NotifyResidency")
            .field("hAllocation", &self.hAllocation)
            .field("PhysicalAddress", &self.PhysicalAddress)
            .field("Flags", &self.__bindgen_anon_1)
            .finish()
    }
}

impl fmt::Debug for D3DGPU_PHYSICAL_ADDRESS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuPhysicalAddress")
            .field("SegmentId", &self.SegmentId)
            .field("SegmentOffset", &self.SegmentOffset)
            .finish()
    }
}

impl DXGK_SETPOINTERPOSITIONFLAGS {
    #[inline]
    pub fn Visible(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Visible() != 0 }
    }
}

impl DXGK_GPUMMUCAPS {
    #[inline]
    pub fn set_ExplicitPageTableInvalidation(&mut self, val: bool) {
        unsafe {
            self.__bindgen_anon_1.__bindgen_anon_1.set_ExplicitPageTableInvalidation(val as _);
        }
    }

    #[inline]
    pub fn set_CacheCoherentMemorySupported(&mut self, val: bool) {
        unsafe {
            self.__bindgen_anon_1.__bindgen_anon_1.set_CacheCoherentMemorySupported(val as _);
        }
    }

}

impl DXGK_FAULT_ERROR_CODE {
    #[inline]
    pub fn set_IsDeviceSpecificCode(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_IsDeviceSpecificCode(val as _) };
    }

    #[inline]
    pub fn set_GeneralErrorCode(&mut self, val: DXGK_GENERAL_ERROR_CODE) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_GeneralErrorCode(val) };
    }
}

impl DXGK_POINTERFLAGS {
    #[inline]
    pub fn set_Monochrome(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_Monochrome(val as _) };
    }

    #[inline]
    pub fn set_Color(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_Color(val as _) };
    }

    #[inline]
    pub fn set_MaskedColor(&mut self, val: bool) {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.set_MaskedColor(val as _) };
    }

    #[inline]
    pub fn Monochrome(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Monochrome() != 0 }
    }

    #[inline]
    pub fn Color(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.Color() != 0 }
    }

    #[inline]
    pub fn MaskedColor(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.MaskedColor() != 0 }
    }
}

impl fmt::Debug for DXGK_POINTERFLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("Monochrome", self.Monochrome());
        write_flag!("Color", self.Color());
        write_flag!("MaskedColor", self.MaskedColor());
        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

impl DXGK_CREATEPROCESSFLAGS {
    #[inline]
    pub fn SystemProcess(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.SystemProcess() != 0 }
    }
    #[inline]
    pub fn GdiProcess(&self) -> bool {
        unsafe { self.__bindgen_anon_1.__bindgen_anon_1.GdiProcess() != 0 }
    }
}

impl fmt::Debug for DXGK_CREATEPROCESSFLAGS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;

        macro_rules! write_flag {
            ($name:expr, $cond:expr) => {
                if $cond {
                    if !first {
                        write!(f, " | ")?;
                    }
                    first = false;
                    write!(f, "{}", $name)?;
                }
            };
        }

        write_flag!("SystemProcess", self.SystemProcess());
        write_flag!("GdiProcess", self.GdiProcess());
        write!(f, "({:x})", unsafe { self.__bindgen_anon_1.Value })
    }
}

impl fmt::Debug for DXGKARG_CREATEPROCESS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateProcess")
            .field("hDxgkProcess", &self.hDxgkProcess)
            .field("hKmdProcess", &self.hKmdProcess)
            .field("Flags", &self.Flags)
            .field("NumPasid", &self.NumPasid)
            .field("pPasid", &self.pPasid)
            .finish()
    }
}

impl fmt::Debug for DXGKARG_SETROOTPAGETABLE{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetRootPageTable")
            .field("hContext", &self.hContext)
            .field("Address", &self.Address)
            .field("NumEntries", &self.NumEntries)
            .finish()
    }
}


impl fmt::Debug for DXGK_CHILD_STATUS_TYPE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_CHILD_STATUS_TYPE::StatusUninitialized      => write!(f, "StatusUninitialized"),
            DXGK_CHILD_STATUS_TYPE::StatusConnection         => write!(f, "StatusConnection"),
            DXGK_CHILD_STATUS_TYPE::StatusRotation           => write!(f, "StatusRotation"),
            DXGK_CHILD_STATUS_TYPE::StatusMiracastConnection => write!(f, "StatusMiracastConnection"),
            _                                                => write!(f, "Unknown({})", self.0),
        }
    }
}

impl fmt::Debug for DXGK_CHILD_DEVICE_TYPE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_CHILD_DEVICE_TYPE::TypeUninitialized     => write!(f, "Uninitialized"),
            DXGK_CHILD_DEVICE_TYPE::TypeVideoOutput       => write!(f, "VideoOutput"),
            DXGK_CHILD_DEVICE_TYPE::TypeOther             => write!(f, "Other"),
            DXGK_CHILD_DEVICE_TYPE::TypeIntegratedDisplay => write!(f, "IntegratedDisplay"),
            DXGK_CHILD_DEVICE_TYPE::TypeLogicalGpu        => write!(f, "LogicalGpu"),
            _                                             => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for DXGK_CHILD_DEVICE_HPD_AWARENESS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DXGK_CHILD_DEVICE_HPD_AWARENESS::HpdAwarenessUninitialized   => write!(f, "Uninitialized"),
            DXGK_CHILD_DEVICE_HPD_AWARENESS::HpdAwarenessAlwaysConnected => write!(f, "AlwaysConnected"),
            DXGK_CHILD_DEVICE_HPD_AWARENESS::HpdAwarenessNone            => write!(f, "None"),
            DXGK_CHILD_DEVICE_HPD_AWARENESS::HpdAwarenessPolled          => write!(f, "Polled"),
            DXGK_CHILD_DEVICE_HPD_AWARENESS::HpdAwarenessInterruptible   => write!(f, "Interruptible"),
            _                                                            => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_UNINITIALIZED        => write!(f, "UNINITIALIZED"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_OTHER                => write!(f, "OTHER"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_HD15                 => write!(f, "HD15"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_SVIDEO               => write!(f, "SVIDEO"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_COMPOSITE_VIDEO      => write!(f, "COMPOSITE_VIDEO"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_COMPONENT_VIDEO      => write!(f, "COMPONENT_VIDEO"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_DVI                  => write!(f, "DVI"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_HDMI                 => write!(f, "HDMI"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_LVDS                 => write!(f, "LVDS"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_D_JPN                => write!(f, "D_JPN"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_SDI                  => write!(f, "SDI"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_DISPLAYPORT_EXTERNAL => write!(f, "DISPLAYPORT_EXTERNAL"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_DISPLAYPORT_EMBEDDED => write!(f, "DISPLAYPORT_EMBEDDED"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_UDI_EXTERNAL         => write!(f, "UDI_EXTERNAL"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_UDI_EMBEDDED         => write!(f, "UDI_EMBEDDED"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_SDTVDONGLE           => write!(f, "SDTVDONGLE"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_MIRACAST             => write!(f, "MIRACAST"),
            D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY::D3DKMDT_VOT_INTERNAL             => write!(f, "INTERNAL"),
            _                                                                 => write!(f, "UNKNOWN({})", self.0),
        }
    }
}

impl fmt::Debug for D3DKMDT_MONITOR_ORIENTATION_AWARENESS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            D3DKMDT_MONITOR_ORIENTATION_AWARENESS::D3DKMDT_MOA_UNINITIALIZED => write!(f, "UNINITIALIZED"),
            D3DKMDT_MONITOR_ORIENTATION_AWARENESS::D3DKMDT_MOA_NONE          => write!(f, "NONE"),
            D3DKMDT_MONITOR_ORIENTATION_AWARENESS::D3DKMDT_MOA_POLLED        => write!(f, "POLLED"),
            D3DKMDT_MONITOR_ORIENTATION_AWARENESS::D3DKMDT_MOA_INTERRUPTIBLE => write!(f, "INTERRUPTIBLE"),
            _                                                                => write!(f, "UNKNOWN({})", self.0),
        }
    }
}
