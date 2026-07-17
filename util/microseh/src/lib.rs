#![allow(dead_code)]
#![cfg_attr(not(feature = "std"), no_std)]

use core::{ffi::c_void, mem::MaybeUninit};

mod code;
mod exception;
mod registers;

pub use code::ExceptionCode;
pub use exception::Exception;
pub use registers::Registers;

const MS_SUCCEEDED: u32 = 0x0;

/// Type alias for a function that converts a pointer to a function and executes it.
type ProcExecutor = unsafe extern "system" fn(*mut c_void);

/// Internal function that converts a pointer to a function and executes it.
///
/// # Arguments
///
/// * `proc` - A pointer to the procedure to execute.
#[inline(always)]
unsafe extern "system" fn proc_executor<F>(proc: *mut c_void)
where
    F: FnMut(),
{
    // The procedure may be equal to std::ptr::null_mut() if the compiler optimized it away.
    // This also means that if you have some code that is optimized away, any exception it
    // contained will not get thrown.
    if let Some(proc) = proc.cast::<F>().as_mut() {
        proc();
    }
}

#[cfg(all(windows, not(docsrs)))]
extern "C" {
    /// External function that is responsible for handling exceptions.
    ///
    /// # Arguments
    ///
    /// * `proc_executor` - The wrapper function that will execute the procedure.
    /// * `proc` - A pointer to the procedure to be executed within the handled context.
    /// * `exception` - Where the exception information will be stored if one occurs.
    ///
    /// # Returns
    ///
    /// * `0x0` - If the procedure executed without throwing any exceptions.
    /// * `0x1` - If an exception occurred during the execution of the procedure.
    #[link_name = "__microseh_HandlerStub"]
    fn handler_stub(
        proc_executor: ProcExecutor,
        proc: *mut c_void,
        exception: *mut Exception,
    ) -> u32;
}

/// Primary execution orchestrator that calls the exception handling stub.
///
/// # Arguments
///
/// * `proc` - The procedure to be executed within the handled context.
///
/// # Returns
///
/// * `Ok(())` - If the procedure executed without throwing any exceptions.
/// * `Err(Exception)` - If an exception occurred during the execution of the procedure.
#[cfg(all(windows, not(docsrs)))]
fn do_call_stub<F>(mut proc: F) -> Result<(), Exception>
where
    F: FnMut(),
{
    let mut exception = Exception::empty();
    let proc = &mut proc as *mut _ as *mut c_void;

    match unsafe { handler_stub(proc_executor::<F>, proc, &mut exception) } {
        MS_SUCCEEDED => Ok(()),
        _ => Err(exception),
    }
}

/// Fallback execution orchestrator to be used when exception handling is disabled.
///
/// # Panics
///
/// This function will always panic, notifying the user that exception handling is not
/// available in the current build.
#[cfg(any(not(windows), docsrs))]
fn do_call_stub<F>(_proc: F) -> Result<(), Exception>
where
    F: FnMut(),
{
    panic!("exception handling is not available in this build of microseh")
}

/// Executes the provided procedure in a context where exceptions are handled, catching any\
/// hardware exceptions that occur.
///
/// Any value returned by the procedure is returned by this function, if no exceptions occur.
///
/// # Arguments
///
/// * `proc` - The procedure to be executed within the handled context.
///
/// # Returns
///
/// * `Ok(R)` - If the procedure executed without throwing any exceptions.
/// * `Err(Exception)` - If an exception occurred during the execution of the procedure.
///
/// # Examples
///
/// ```
/// use microseh::try_seh;
///
/// if let Err(e) = try_seh(|| unsafe {
///     core::ptr::read_volatile(core::mem::align_of::<i32>() as *const i32);
/// }) {
///     println!("an exception occurred: {:?}", e);
/// }
/// ```
///
/// # Caveats
///
/// If an exception occours within the procedure, resources that require cleanup via\
/// the `Drop` trait will not be released.
///
/// As a rule of thumb, it's recommended not to define resources that implement\
/// the `Drop` trait inside the procedure. Instead, allocate and manage these resources\
/// outside, ensuring proper cleanup even if an exception occurs.
///
/// # Panics
///
/// If exception handling is disabled in the build, which occurs when the library is\
/// not built on Windows with Microsoft Visual C++.
#[inline(always)]
pub fn try_seh<F, R>(mut proc: F) -> Result<R, Exception>
where
    F: FnMut() -> R,
{
    let mut ret_val = MaybeUninit::<R>::uninit();
    do_call_stub(|| {
        ret_val.write(proc());
    })
    // SAFETY: We should only reach this point if the inner closure has returned
    //         without throwing an exception, so `ret_val` should be initialized.
    .map(|_| unsafe { ret_val.assume_init() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVALID_PTR: *mut i32 = core::mem::align_of::<i32>() as _;

    #[test]
    #[cfg(feature = "std")]
    fn all_good() {
        let ex = try_seh(|| {
            let _ = *Box::new(1337);
        });

        assert_eq!(ex.is_ok(), true);
    }

    #[test]
    fn access_violation_rs() {
        let ex = try_seh(|| unsafe {
            INVALID_PTR.read_volatile();
        });

        assert_eq!(ex.is_err(), true);
        assert_eq!(ex.unwrap_err().code(), ExceptionCode::AccessViolation);
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn access_violation_asm() {
        let ex = try_seh(|| unsafe {
            core::arch::asm!("mov eax, DWORD PTR [0]");
        });

        assert_eq!(ex.is_err(), true);
        assert_eq!(ex.unwrap_err().code(), ExceptionCode::AccessViolation);
    }

    #[test]
    #[cfg(target_arch = "aarch64")]
    fn access_violation_asm() {
        let ex = try_seh(|| unsafe {
            core::arch::asm!("ldr x0, xzr");
        });

        assert_eq!(ex.is_err(), true);
        assert_eq!(ex.unwrap_err().code(), ExceptionCode::AccessViolation);
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn breakpoint() {
        let ex = try_seh(|| unsafe {
            core::arch::asm!("int3");
        });

        assert_eq!(ex.is_err(), true);
        assert_eq!(ex.unwrap_err().code(), ExceptionCode::Breakpoint);
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn illegal_instruction() {
        let ex = try_seh(|| unsafe {
            core::arch::asm!("ud2");
        });

        assert_eq!(ex.is_err(), true);
        assert_eq!(ex.unwrap_err().code(), ExceptionCode::IllegalInstruction);
    }

    #[test]
    #[cfg(target_arch = "aarch64")]
    fn illegal_instruction() {
        let ex = try_seh(|| unsafe {
            core::arch::asm!("udf #0");
        });

        assert_eq!(ex.is_err(), true);
        assert_eq!(
            ex.as_ref().unwrap_err().code(),
            ExceptionCode::IllegalInstruction
        );
    }

    #[test]
    #[cfg(target_arch = "x86")]
    fn reg_state_check() {
        let ex = try_seh(|| unsafe {
            core::arch::asm!("mov eax, 0xbadc0de", "ud2");
        });

        assert_eq!(ex.is_err(), true);
        assert_eq!(
            ex.as_ref().unwrap_err().code(),
            ExceptionCode::IllegalInstruction
        );

        assert_eq!(ex.unwrap_err().registers().eax(), 0xbadc0de);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn reg_state_check() {
        let ex = try_seh(|| unsafe {
            core::arch::asm!("mov rax, 0xbadc0debabefffff", "ud2");
        });

        assert_eq!(ex.is_err(), true);
        assert_eq!(
            ex.as_ref().unwrap_err().code(),
            ExceptionCode::IllegalInstruction
        );

        assert_eq!(ex.unwrap_err().registers().rax(), 0xbadc0debabefffff);
    }

    #[test]
    #[cfg(target_arch = "aarch64")]
    fn reg_state_check() {
        let ex = try_seh(|| unsafe {
            core::arch::asm!(
                "movz x0, #0xbadc, LSL 48",
                "movk x0, #0x0deb, LSL 32",
                "movk x0, #0xabef, LSL 16",
                "movk x0, #0xffff",
                "udf #0"
            );
        });

        assert_eq!(ex.is_err(), true);
        assert_eq!(
            ex.as_ref().unwrap_err().code(),
            ExceptionCode::IllegalInstruction
        );

        assert_eq!(ex.unwrap_err().registers().x0(), 0xbadc0debabefffff);
    }

    #[test]
    fn ret_vals() {
        let a = try_seh(|| 1337);
        assert_eq!(a.unwrap(), 1337);

        let b = try_seh(|| "hello");
        assert_eq!(b.unwrap(), "hello");

        let c = try_seh(|| {});
        assert_eq!(core::mem::size_of_val(&c.unwrap()), 0x0);
    }
}
