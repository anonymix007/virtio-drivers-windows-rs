const INVALID_PTR: *mut i32 = core::mem::align_of::<i32>() as _;

unsafe fn unsafe_func() {
    INVALID_PTR.write_volatile(0);
}

fn rust_func() {
    unsafe { unsafe_func() };
}

extern "system" fn system_func() {
    rust_func();
}

fn main() {
    // You can pass in closures:
    let ex = microseh::try_seh(|| unsafe { INVALID_PTR.write_volatile(0) });
    if let Err(ex) = ex {
        println!("closure: {:?}", ex);
    }

    // Or functions:
    let ex = microseh::try_seh(rust_func);
    if let Err(ex) = ex {
        println!("rust_func: {:?}", ex);
    }

    // But if you want to use it with FFI:
    let ex = microseh::try_seh(|| system_func());
    if let Err(ex) = ex {
        println!("system_func: {:?}", ex);
    }

    // Or you want to call an unsafe function:
    let ex = microseh::try_seh(|| unsafe { unsafe_func() });
    if let Err(ex) = ex {
        println!("unsafe_func: {:?}", ex);
    }

    // And you can also pass any return value:
    let ex = microseh::try_seh(|| 1337);
    if let Ok(val) = ex {
        println!("ret_val: {}", val);
    }
}
