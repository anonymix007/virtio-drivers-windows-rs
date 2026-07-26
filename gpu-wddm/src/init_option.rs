use core::mem::{
    needs_drop,
    MaybeUninit,
};

use pin_init::*;
use crate::function;

pub struct InitOption<T> {
    value: MaybeUninit<T>,
    init: bool,
}

impl<T> InitOption<T> {
    pub fn none() -> impl Init<Self> {
        init!(Self {
            value: MaybeUninit::uninit(),
            init: false,
        })
    }

    pub fn some<E>(init: impl Init<T, E>) -> impl Init<Self, E> {
        unsafe {
            init_from_closure(move |slot: *mut Self| {
                let slot = &mut *slot;

                init.__init(slot.value.as_mut_ptr())?;
                slot.init = true;

                Ok(())
            })
        }
    }

    fn drop_if_init(&mut self) {
        trace!("{}", function!());
        if needs_drop::<T>() && self.init {
            self.init = false;
            unsafe { self.value.assume_init_drop(); }
        }
    }

    pub fn write_init<E>(&mut self, init: impl Init<T, E>) -> Result<(), E> {
        self.drop_if_init();

        unsafe {
             init.__init(self.value.as_mut_ptr())?;
        }
        self.init = true;

        Ok(())
    }

    pub fn write<E>(&mut self, init: impl Init<Self, E>) -> Result<(), E> {
        self.drop_if_init();

        unsafe {
             init.__init(self as *mut Self)?;
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.write(Self::none()).unwrap();
    }

    //pub fn take(&mut self)

    /*pub fn take(&mut self) -> Option<T> {
        if self.init {
            self.init = false;
            Some(unsafe {
                let value = self.value.assume_init_read();
                self.value = zeroed();

                value
            })
        } else {
            None
        }
    }*/

    pub fn as_ref(&self) -> Option<&T> {
        if self.init {
            Some(unsafe { self.value.assume_init_ref() })
        } else {
            None
        }
    }

    pub fn as_mut(&mut self) -> Option<&mut T> {
        if self.init {
            Some(unsafe { self.value.assume_init_mut() })
        } else {
            None
        }
    }

    pub fn is_some(&self) -> bool {
        self.init
    }
}

impl<T> Drop for InitOption<T> {
    fn drop(&mut self) {
        trace!("{}", function!());
        self.drop_if_init();
    }
}

