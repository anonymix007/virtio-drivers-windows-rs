use core::fmt::{Write, Error};
use core::cell::UnsafeCell;

use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use spin::Spin;
use spin::mutex::spin::{SpinMutex, SpinMutexGuard};
// TODO: aarch64
use uart_16550::SerialPort;

use crossbeam::queue::ArrayQueue;

struct TimeoutMutex<T>(SpinMutex<T>);

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

#[repr(packed)]
struct LogEntry {
    len: u16,
    buf: [u8; 1022],
}
const _: () = assert!(size_of::<LogEntry>() == 1024);

impl Default for LogEntry {
    fn default() -> Self {
        Self {
            len: 0,
            buf: [0; _],
        }
    }
}

impl Write for LogEntry {
    fn write_str(&mut self, s: &str) -> Result<(), Error> {
        let free = &mut self.buf[self.len as usize..];
        let bytes = {
            let bytes = s.as_bytes();
            if bytes.len() <= free.len() {
                bytes
            } else {
                &bytes[..free.len()]
            }
        };
        let free = &mut free[..bytes.len()];
        free.copy_from_slice(bytes);
        self.len += bytes.len() as u16;

        Ok(())
    }
}

impl LogEntry {
    fn as_str(&self) -> &str {
        let bytes = &self.buf[..self.len as usize];
        str::from_utf8(&bytes).unwrap()
    }
}

const SERIAL_PORT: u16 = 0x3f8;
struct Logger {
    uart: TimeoutMutex<Option<SerialPort>>,
    msgs: Option<ArrayQueue<LogEntry>>,
}

struct GlobalLogger(UnsafeCell<Logger>);

impl GlobalLogger {
    fn replace(&self, new: Logger) -> Logger {
        // Replace will only be called once on init, so this is safe
        unsafe {
            let old = self.0.get().read();
            self.0.get().write(new);

            old
        }
    }

    fn as_logger(&self) -> &Logger {
        // There should be no more mutable references at this point
        unsafe {
            &*self.0.get()
        }
    }
}

static LOGGER: GlobalLogger = GlobalLogger(UnsafeCell::new(Logger {
    uart: TimeoutMutex::new(None),
    msgs: None,
}));

unsafe impl Sync for GlobalLogger {}

/*
pub struct LogLevelGuard(LevelFilter);

impl LogLevelGuard {
    fn new(max_level: LevelFilter) -> Self {
        let guard = Self(log::max_level());

        log::set_max_level(max_level);

        guard
    }
}

impl Drop for LogLevelGuard {
    fn drop(&mut self) {
        log::set_max_level(self.0);
    }
}

pub fn set_log_level_temp(max_level: LevelFilter) -> LogLevelGuard {
    LogLevelGuard::new(max_level)
}*/

pub fn init(max_level: LevelFilter) -> Result<(), SetLoggerError> {
    let mut uart = unsafe { SerialPort::new(SERIAL_PORT) };
    uart.init();

    LOGGER.replace(Logger {
        uart: TimeoutMutex::new(Some(uart)),
        msgs: Some(ArrayQueue::new(4*1024)),
    });

    log::set_logger(LOGGER.as_logger())?;
    log::set_max_level(max_level);
    Ok(())
}

pub fn deinit() {
    LOGGER.replace(Logger {
        uart: TimeoutMutex::new(None),
        msgs: None,
    });
}

pub unsafe fn force_flush() {
    if LOGGER.as_logger().uart.is_locked() {
        let mut uart = if let Some(uart) = LOGGER.as_logger().uart.try_lock_timeout(1_000_000) {
            uart
        } else {
            unsafe {
                LOGGER.as_logger().uart.0.force_unlock();
            }
            LOGGER.as_logger().uart.lock()
        };

        loop {
            let Some(entry) = LOGGER.as_logger().msgs.as_ref().unwrap().pop() else {
                break;
            };
            writeln!(
                uart.as_mut().unwrap(),
                "[FORCE] [BUFFERED] {}",
                entry.as_str(),
            )
            .unwrap();
        }
    } else {
        LOGGER.as_logger().flush();
    }
}

impl Logger {
    fn try_flush(&self) {
        let Some(mut uart) = self.uart.try_lock_timeout(10_000) else {
            return;
        };
        let len = self.msgs.as_ref().unwrap().len();
        for i in 0..len {
            let Some(entry) = self.msgs.as_ref().unwrap().pop() else {
                break;
            };
            writeln!(
                uart.as_mut().unwrap(),
                "{}",
                entry.as_str(),
            )
            .unwrap();
        }
    }
}

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let mut entry = LogEntry::default();
        //let irql = wdk::wdm::ke_get_current_irql();
        //write!(
        //    &mut entry,
        //    "[{:02} {}] {}",
        //    irql,
        //    record.level(),
        //    record.args()
        //).unwrap();

        write!(
            &mut entry,
            "[{}] {}",
            record.level(),
            record.args()
        ).unwrap();

        self.msgs.as_ref().unwrap().force_push(entry);

        self.try_flush();

        /*

        let Some(mut uart) = self.uart.try_lock_timeout(100_000) else {
            let mut entry = LogEntry::default();
            write!(
                &mut entry,
                "[{}] {}",
                record.level(),
                record.args()
            ).unwrap();

            self.msgs.as_ref().unwrap().force_push(entry);

            return;
        };

        let len = self.msgs.as_ref().unwrap().len();
        for i in 0..len {
            let Some(entry) = self.msgs.as_ref().unwrap().pop() else {
                break;
            };
            writeln!(
                uart.as_mut().unwrap(),
                "{}",
                entry.as_str(),
            )
            .unwrap();
        }

        writeln!(
            uart.as_mut().unwrap(),
            "[{}] {}",
            record.level(),
            record.args()
        )
        .unwrap();
        drop(uart);

        let len = self.msgs.as_ref().unwrap().len();
        for i in 0..len {
            let Some(mut uart) = self.uart.try_lock_timeout(10_000) else {
                break;
            };
            let Some(entry) = self.msgs.as_ref().unwrap().pop() else {
                break;
            };
            writeln!(
                uart.as_mut().unwrap(),
                "{}",
                entry.as_str(),
            )
            .unwrap();
        }
        */
    }

    fn flush(&self) {
        let len = self.msgs.as_ref().unwrap().len();
        for i in 0..len {
            let Some(entry) = self.msgs.as_ref().unwrap().pop() else {
                break;
            };
            writeln!(
                self.uart.lock().as_mut().unwrap(),
                "{}",
                entry.as_str(),
            )
            .unwrap();
        }
    }
}
