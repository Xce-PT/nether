//! Mini UART driver.
//!
//! Documentation:
//!
//! * [BCM2711 ARM Peripherals](https://datasheets.raspberrypi.com/bcm2711/bcm2711-peripherals.pdf)
//!   2 and 5

use core::fmt::{Result as FormatResult, Write};
use core::hint::spin_loop;
use core::marker::PhantomData;

use crate::sync::{Lazy, Lock};
use crate::PERRY_RANGE;

/// Base of the auxiliary peripheral configuration registers
const AUX_BASE: usize = 0x2215000 + PERRY_RANGE.start;
/// Auxiliary peripheral enabler register.
const AUX_ENABLES: *mut u32 = (AUX_BASE + 0x4) as _;
/// Input / output Mini UART register.
const AUX_MU_IO: *mut u32 = (AUX_BASE + 0x40) as _;
/// Data status Mini UART register.
const AUX_MU_LCR: *mut u32 = (AUX_BASE + 0x4C) as _;
/// Control MiniUART register.
const AUX_MU_CNTL: *mut u32 = (AUX_BASE + 0x60) as _;
/// Mini UART status register.
const AUX_MU_STAT: *const u32 = (AUX_BASE + 0x64) as _;
/// Mini UART BAUD rate divisor.
const AUX_MU_BAUD: *mut u32 = (AUX_BASE + 0x68) as _;
/// Base address of the GPIO registers.
const GPIO_BASE: usize = 0x2200000 + PERRY_RANGE.start;
/// GPIO function selection register 1.
const GPIO_FSEL1: *mut u32 = (GPIO_BASE + 0x4) as _;
/// GPIO pull-up / pull-down register 0.
const GPIO_PUPD0: *mut u32 = (GPIO_BASE + 0xE4) as _;

/// Global UART driver instance.
pub static UART: Lazy<Lock<Uart>> = Lazy::new(Uart::new);

/// Send formatted diagnostic messages over the Mini UART.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut uart = $crate::uart::UART.lock();
        writeln!(uart, $($arg)*).unwrap();
    }};
}

/// Mini UART driver.
#[derive(Debug)]
pub struct Uart
{
    /// Phantom field just to prevent public initialization.
    _dummy: PhantomData<()>,
}

impl Uart
{
    /// Creates and initializes a new Mini UART driver instance.
    ///
    /// Returns the newly created Mini UART driver instance.
    fn new() -> Lock<Self>
    {
        unsafe {
            AUX_ENABLES.write_volatile(0x1); // Enable the Mini UART.
            AUX_MU_CNTL.write_volatile(0x0); // Temporarily disable transmission and reception..
            let val = GPIO_FSEL1.read_volatile();
            GPIO_FSEL1.write_volatile(val & 0xFFFC0FFF | 0x12000); // Set alt function 5 for GPIOs 14 and 15.
            let val = GPIO_PUPD0.read_volatile();
            GPIO_PUPD0.write_volatile(val & 0xFFFFFF); // Set neither pull-up nor pull-down state for GPIOs 14 and 15.
            AUX_MU_LCR.write_volatile(0x3); // Set data bits to 8 (the documentation is wrong).
            AUX_MU_BAUD.write_volatile(500000000 / 115200 / 8 - 1); // Set the BAUD rate to 115200.
            AUX_MU_CNTL.write_volatile(0x3); // Enable the transmitter and
                                             // receiver.
        }
        let this = Self { _dummy: PhantomData };
        Lock::new(this)
    }
}

impl Write for Uart
{
    fn write_str(&mut self, msg: &str) -> FormatResult
    {
        for byte in msg.as_bytes() {
            while unsafe { AUX_MU_STAT.read_volatile() } & 0x20 != 0 {
                spin_loop()
            } // FIFO full.
            unsafe { AUX_MU_IO.write_volatile(*byte as _) };
        }
        Ok(())
    }
}
