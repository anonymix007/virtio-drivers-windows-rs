use core::fmt::{Write, Error};
use core::cell::UnsafeCell;

use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use wdk::spin::{
    TimeoutMutex,
};

// TODO: DbgPrintEx logger

#[cfg(target_arch = "x86_64")]
mod x86_64 {
    use core::fmt::{Write, Error};

    use uart_16550::{Config, Uart16550Tty, backend::PioBackend};

    const SERIAL_PORT: u16 = 0x3f8;

    #[repr(transparent)]
    pub struct SerialPort(Uart16550Tty<PioBackend>);

    impl SerialPort {
        pub fn new() -> Self {
            Self(unsafe { Uart16550Tty::new_port(SERIAL_PORT, Config::default()) }.unwrap())
        }
    }

    impl Write for SerialPort {
        fn write_str(&mut self, v: &str) -> Result<(), Error> {
            self.0.write_str(v)
        }
    }
}

#[cfg(target_arch = "x86_64")]
use x86_64::SerialPort;

#[cfg(target_arch = "aarch64")]
mod aarch64 {
    use core::{
        fmt::{Write, Error},
        ptr::NonNull,
    };
    use arm_pl011_uart::{PL011Registers, Uart, UniqueMmioPointer};

    const PL011_BASE_ADDRESS: *mut PL011Registers = 0x900_0000 as _;

    #[repr(transparent)]
    pub struct SerialPort(Uart<'static>);

    impl SerialPort {
        pub fn new() -> Self {
            Self(Uart::new(unsafe { UniqueMmioPointer::new(NonNull::new(PL011_BASE_ADDRESS).unwrap()) }))
        }
    }

    impl Write for SerialPort {
        fn write_str(&mut self, v: &str) -> Result<(), Error> {
            self.0.write_str(v)
        }
    }
}

#[cfg(target_arch = "aarch64")]
use aarch64::SerialPort;

use crossbeam_queue::ArrayQueue;

pub struct BufferWriter<const MAX_LEN: usize> {
    len: u16,
    buf: [u8; MAX_LEN],
}

type LogEntry = BufferWriter<1022>;

const _: () = assert!(size_of::<LogEntry>() == 1024);

impl<const MAX_LEN: usize> Default for BufferWriter<MAX_LEN> {
    fn default() -> Self {
        Self {
            len: 0,
            buf: [0; _],
        }
    }
}

impl<const MAX_LEN: usize> Write for BufferWriter<MAX_LEN> {
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

impl<const MAX_LEN: usize> BufferWriter<MAX_LEN> {
    pub fn as_str(&self) -> &str {
        let bytes = &self.buf[..self.len as usize];
        str::from_utf8(&bytes).unwrap()
    }
}

struct Logger {
    uart: TimeoutMutex<Option<SerialPort>>,
    // TODO: thingbuf::StaticThingBuf with push_ref might be more efficient
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
    let uart = SerialPort::new();

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
                LOGGER.as_logger().uart.force_unlock();
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
    fn try_flush(&self, entry: Option<LogEntry>) {
        let mut uart = if wdk::wdm::ke_get_current_irql() <= wdk::wdm::DISPATCH_LEVEL && let Some(entry) = entry {
            self.uart.lock()
        } else if let Some(uart) = self.uart.try_lock_timeout(10_000) {
            uart
        } else {
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

        let old_entry = self.msgs.as_ref().unwrap().force_push(entry);
        self.try_flush(old_entry);

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
