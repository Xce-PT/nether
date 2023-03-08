//! System timer driver.
//!
//! Provides time information backed by the system timer as described in the
//! BCM2711 peripherals datasheet [1].  The clock frequency was obtained by
//! following the device tree source includes for the Raspberry Pi 4 B in the
//! Linux source code [2].
//!
//! [1]: https://datasheets.raspberrypi.com/bcm2711/bcm2711-peripherals.pdf
//! [2]: https://github.com/raspberrypi/linux/blob/rpi-5.15.y/arch/arm/boot/dts/bcm283x.dtsi

use crate::PERRY_RANGE;

/// System timer base address.
const BASE: usize = PERRY_RANGE.start + 0x2003000;
/// System timer current time lower 32 bit register.
const CLO: *const u32 = (BASE + 0x4) as _;
/// System timer current time higher 32 bit register.
const CHI: *const u32 = (BASE + 0x8) as _;
/// System timer frequency.
const FREQ: u64 = 1000000;

/// Returns the current system time in milliseconds.
pub fn now() -> u64
{
    unsafe { (((CHI.read_volatile() as u64) << 32) | CLO.read_volatile() as u64) / (FREQ / 1000) }
}
