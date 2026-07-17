use core::ffi::c_void;

use crate::{code::ExceptionCode, registers::Registers};

/// Represents an exception that occurs during program execution, along with additional
/// context information.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Exception {
    code: ExceptionCode,
    address: *mut c_void,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
    registers: Registers,
}

impl Exception {
    /// Creates a new exception with default values.
    ///
    /// Exceptions created with this function are to be considered invalid, and should
    /// only be used as a placeholder.
    pub(crate) fn empty() -> Self {
        Self {
            code: ExceptionCode::Invalid,
            address: core::ptr::null_mut(),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
            registers: Registers::empty(),
        }
    }

    /// # Returns
    ///
    /// The system-specific code of the exception.
    pub fn code(&self) -> ExceptionCode {
        self.code
    }

    /// # Returns
    ///
    /// A pointer to the memory address where the exception occurred.
    pub fn address(&self) -> *mut c_void {
        self.address
    }

    /// # Returns
    ///
    /// The dump of the CPU registers at the time of the exception.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
    pub fn registers(&self) -> &Registers {
        &self.registers
    }
}

impl core::fmt::Display for Exception {
    /// Formats the exception into a human-readable string.
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write to.
    ///
    /// # Returns
    ///
    /// Whether the formatting operation succeeded.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.code)
    }
}

/// In case the `std` feature is enabled, this implementation allows the exception to be
/// treated as a standard error.
#[cfg(feature = "std")]
impl std::error::Error for Exception {}
