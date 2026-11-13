use core::{
    fmt,
    ops::{
        Deref,
        DerefMut,
    },
};

use spin::{
    mutex,
    rwlock,
    Spin,
    RelaxStrategy,
};

use crate::wdm::*;

pub struct SpinMutex<T: ?Sized, R = Spin>(mutex::SpinMutex<T, R>);
pub struct SpinMutexGuard<'a, T: ?Sized + 'a, R = Spin>(mutex::SpinMutexGuard<'a, T, R>, Option<IrqlGuard>);
unsafe impl<T: ?Sized + Send, R> Sync for SpinMutex<T, R> {}
unsafe impl<T: ?Sized + Send, R> Send for SpinMutex<T, R> {}
unsafe impl<T: ?Sized, R> Sync for SpinMutexGuard<'_, T, R> where for<'a> &'a mut T: Sync {}
unsafe impl<T: ?Sized, R> Send for SpinMutexGuard<'_, T, R> where for<'a> &'a mut T: Send {}

impl<T, R> SpinMutex<T, R> {
    #[inline(always)]
    pub const fn new(data: T) -> Self {
        Self(mutex::SpinMutex::new(data))
    }

    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }

    #[inline(always)]
    pub fn as_mut_ptr(&self) -> *mut T {
        self.0.as_mut_ptr()
    }
}

impl<T: ?Sized, R: RelaxStrategy> SpinMutex<T, R> {
    #[inline(always)]
    pub fn lock(&self) -> SpinMutexGuard<'_, T, R> {
        let irql = IrqlGuard::raise(DISPATCH_LEVEL);
        let guard = self.0.lock();
        SpinMutexGuard(guard, irql)
    }
}

impl<T: ?Sized, R> SpinMutex<T, R> {
    #[inline(always)]
    pub fn is_locked(&self) -> bool {
        self.0.is_locked()
    }

    #[inline(always)]
    pub unsafe fn force_unlock(&self) {
        unsafe {
            self.0.force_unlock();
        }
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<SpinMutexGuard<'_, T, R>> {
        let irql = IrqlGuard::raise(DISPATCH_LEVEL);
        if let Some(guard) = self.0.try_lock() {
            Some(SpinMutexGuard(guard, irql))
        } else {
            drop(irql);
            None
        }
    }

    #[inline(always)]
    pub fn try_lock_weak(&self) -> Option<SpinMutexGuard<'_, T, R>> {
        let irql = IrqlGuard::raise(DISPATCH_LEVEL);
        if let Some(guard) = self.0.try_lock_weak() {
            Some(SpinMutexGuard(guard, irql))
        } else {
            drop(irql);
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }
}

impl<T: ?Sized + fmt::Debug, R> fmt::Debug for SpinMutex<T, R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => write!(f, "Mutex {{ data: ")
                .and_then(|()| (*guard.0).fmt(f))
                .and_then(|()| write!(f, " }}")),
            None => write!(f, "Mutex {{ <locked> }}"),
        }
    }
}

impl<'a, T: ?Sized, R> Deref for SpinMutexGuard<'a, T, R> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0.deref()
    }
}

impl<'a, T: ?Sized, R> DerefMut for SpinMutexGuard<'a, T, R> {
    fn deref_mut(&mut self) -> &mut T {
        self.0.deref_mut()
    }
}

pub struct RwLock<T: ?Sized, R = Spin>(rwlock::RwLock<T, R>);
pub struct RwLockReadGuard<'a, T: 'a + ?Sized, R = Spin>(rwlock::RwLockReadGuard<'a, T, R>, Option<IrqlGuard>);
pub struct RwLockWriteGuard<'a, T: 'a + ?Sized, R = Spin>(rwlock::RwLockWriteGuard<'a, T, R>, Option<IrqlGuard>);
unsafe impl<T: ?Sized + Send, R> Send for RwLock<T, R> {}
unsafe impl<T: ?Sized + Send + Sync, R> Sync for RwLock<T, R> {}
unsafe impl<T: ?Sized, R> Sync for RwLockWriteGuard<'_, T, R> where for<'a> &'a mut T: Sync {}
unsafe impl<T: ?Sized, R> Send for RwLockWriteGuard<'_, T, R> where for<'a> &'a mut T: Send {}
unsafe impl<T: ?Sized, R> Sync for RwLockReadGuard<'_, T, R> where for<'a> &'a T: Sync {}
unsafe impl<T: ?Sized, R> Send for RwLockReadGuard<'_, T, R> where for<'a> &'a T: Send {}

impl<T, R> RwLock<T, R> {
    #[inline]
    pub const fn new(data: T) -> Self {
        Self(rwlock::RwLock::new(data))
    }

    #[inline]
    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }

    #[inline(always)]
    pub fn as_mut_ptr(&self) -> *mut T {
        self.0.as_mut_ptr()
    }
}

impl<T: ?Sized, R: RelaxStrategy> RwLock<T, R> {
    #[inline]
    pub fn read(&self) -> RwLockReadGuard<'_, T, R> {
        let irql = IrqlGuard::raise(DISPATCH_LEVEL);
        let guard = self.0.read();
        RwLockReadGuard(guard, irql)
    }

    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<'_, T, R> {
        let irql = IrqlGuard::raise(DISPATCH_LEVEL);
        let guard = self.0.write();
        RwLockWriteGuard(guard, irql)
    }
}

impl<T: ?Sized, R> RwLock<T, R> {
    #[inline]
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T, R>> {
        let irql = IrqlGuard::raise(DISPATCH_LEVEL);
        let guard = self.0.try_read()?;
        Some(RwLockReadGuard(guard, irql))
    }

    #[inline]
    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T, R>> {
        let irql = IrqlGuard::raise(DISPATCH_LEVEL);
        let guard = self.0.try_write()?;
        Some(RwLockWriteGuard(guard, irql))
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }
}

impl<T: ?Sized + fmt::Debug, R> fmt::Debug for RwLock<T, R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_read() {
            Some(guard) => write!(f, "RwLock {{ data: ")
                .and_then(|()| (*guard.0).fmt(f))
                .and_then(|()| write!(f, " }}")),
            None => write!(f, "RwLock {{ <locked> }}"),
        }
    }
}

impl<T: Default, R> Default for RwLock<T, R> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T, R> From<T> for RwLock<T, R> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

impl<'rwlock, T: ?Sized, R> Deref for RwLockReadGuard<'rwlock, T, R> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0.deref()
    }
}

impl<'rwlock, T: ?Sized, R> Deref for RwLockWriteGuard<'rwlock, T, R> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0.deref()
    }
}

impl<'rwlock, T: ?Sized, R> DerefMut for RwLockWriteGuard<'rwlock, T, R> {
    fn deref_mut(&mut self) -> &mut T {
        self.0.deref_mut()
    }
}

pub struct TimeoutMutex<T>(SpinMutex<T>);

impl<T> TimeoutMutex<T> {
    pub const fn new(value: T) -> Self {
        Self(SpinMutex::new(value))
    }

    pub fn lock(&self) -> SpinMutexGuard<'_, T> {
        self.0.lock()
    }

    pub unsafe fn force_unlock(&self) {
        unsafe {
            self.0.force_unlock();
        }
    }

    pub fn is_locked(&self) -> bool {
        self.0.is_locked()
    }

    pub fn try_lock(&self) -> Option<SpinMutexGuard<'_, T>> {
        self.0.try_lock()
    }

    pub fn try_lock_weak(&self) -> Option<SpinMutexGuard<'_, T>> {
        self.0.try_lock_weak()
    }

    pub fn try_lock_timeout(&self, iterations: usize) -> Option<SpinMutexGuard<'_, T>> {
        for _ in 0..5 {
            if let Some(guard) = self.try_lock_weak() {
                return Some(guard);
            }

            for _ in 0..iterations {
                if !self.is_locked() {
                    break;
                }

                core::hint::spin_loop();
            }
        }
        None
    }
}
