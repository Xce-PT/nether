//! Generic Interrupt Controller (GIC) 400) driver.
//!
//! Documentation:
//!
//! * [BCM2711 ARM Peripherals](https://datasheets.raspberrypi.com/bcm2711/bcm2711-peripherals.pdf)
//!   6.3 and 6.5.1
//! * [CoreLink GIC-400 Generic Interrupt Controller Technical Reference Manual](https://developer.arm.com/documentation/ddi0471/b)
//! * [ARM Generic Interrupt Controller Architecture Specification](https://developer.arm.com/documentation/ihi0048/b)

extern crate alloc;

use alloc::collections::BTreeMap;
use core::arch::asm;
use core::ptr::write_volatile;
use core::sync::atomic::{fence, Ordering};

use crate::sync::{Lazy, RwLock};
use crate::PERRY_RANGE;

/// Number of SPIs on the BCM2711.
const SPI_COUNT: usize = 192;
/// Total number of IRQs on the BCM2711.
const IRQ_COUNT: usize = SPI_COUNT + 32;
/// Base address of theGIC 400.
const GIC_BASE: usize = 0x3840000 + PERRY_RANGE.start;
/// IRQ set enable registers.
const GICD_ISENABLER: *mut [u32; IRQ_COUNT >> 5] = (GIC_BASE + 0x1100) as _;
/// IRQ clear enable registers.
const GICD_ICENABLER: *mut [u32; IRQ_COUNT >> 5] = (GIC_BASE + 0x1180) as _;
/// IRQ priority registers.
const GICD_IPRIORITYR: *mut [u8; IRQ_COUNT] = (GIC_BASE + 0x1400) as _;
/// IRQ target CPU registers.
const GICD_ITARGETSR: *mut [u8; IRQ_COUNT] = (GIC_BASE + 0x1800) as _;
/// IRQ trigger configuration registers.
const GICD_ICFGR: *mut [u32; IRQ_COUNT >> 4 /* Two bits per field */] = (GIC_BASE + 0x1c00) as _;
/// Software Generated IRQ register.
const GICD_SGIR: *mut u32 = (GIC_BASE + 0x1F00) as _;
/// IRQ minimum priority register.
const GICC_PMR: *mut u32 = (GIC_BASE + 0x2004) as _;
/// IRQ acknowledge register.
const GICC_IAR: *mut u32 = (GIC_BASE + 0x200C) as _;
/// IRQ dismissal register.
const GICC_EOIR: *mut u32 = (GIC_BASE + 0x2010) as _;

/// Global interrupt controller driver.
pub static IRQ: Lazy<Irq> = Lazy::new(Irq::new);

/// IRQ driver.
pub struct Irq
{
    /// Registered handlers.
    handlers: RwLock<BTreeMap<u32, fn()>>,
}

impl Irq
{
    /// Creates and initializes a new interrupt controller driver.
    ///
    /// Returns the newly created driver.
    fn new() -> Self
    {
        unsafe {
            // Disable all IRQs.
            (*GICD_ICENABLER).iter_mut()
                             .for_each(|element| write_volatile(element, 0xFFFFFFFF));
            // Set the minimum priority level (higher values correspond to lower priority
            // levels).
            GICC_PMR.write_volatile(0xFF);
            // Raise the priority of every IRQ as matching the lowest priority level masks
            // them.
            (*GICD_IPRIORITYR).iter_mut()
                              .for_each(|element| write_volatile(element, 0x7F));
            // Make all IRQs level triggered.
            (*GICD_ICFGR).iter_mut()
                         .for_each(|element| write_volatile(element, 0x55555555));
            // Deliver all SPIs to all cores.
            (*GICD_ITARGETSR).iter_mut()
                             .skip(32)
                             .for_each(|element| write_volatile(element, 0xFF));
        }
        Self { handlers: RwLock::new(BTreeMap::new()) }
    }

    /// Registers a handler to be called when the specified IRQ is triggered.
    ///
    /// * `irq`: IRQ to wait for.
    /// * `handler`: Handler function to register.
    ///
    /// Panics if a handler is already registered for the specified IRQ.
    #[track_caller]
    pub fn register(&self, irq: u32, handler: fn())
    {
        assert!((irq as usize) < IRQ_COUNT, "IRQ #{irq} is out of range");
        let mut handlers = self.handlers.wlock();
        assert!(handlers.insert(irq, handler).is_none(),
                "Attempted to add a second handler for IRQ {irq}");
        // Figure out which register and bit to enable for the given IRQ.
        let val = 0x1 << (irq & 0x1F);
        let idx = irq as usize >> 5;
        unsafe { write_volatile((*GICD_ISENABLER).get_mut(idx).unwrap(), val) };
    }

    /// Raises the specified Software Generated Interrupt on all CPUs.
    ///
    /// * `irq`: IRQ to raise.
    ///
    /// Panics if an attempt is made to raise an IRQ of any other kind.
    pub fn notify_all(&self, irq: u32)
    {
        assert!(irq < 16,
                "Attempted to trigger a Software Generated Interrupt outside of the valid range");
        let val = 0xFF8000 | irq; // Target all CPUs.
        unsafe { GICD_SGIR.write_volatile(val) };
    }

    /// Raises a Software Generated Interrupt on all CPUs except the one that is
    /// calling this function.
    ///
    /// * `irq`: IRQ to raise.
    ///
    /// Panics if an attempt is made to raise an IRQ of any other kind.
    pub fn notify_others(&self, irq: u32)
    {
        assert!(irq < 16,
                "Attempted to trigger a Software Generated Interrupt outside of the valid range");
        let val = 0x1008000 | irq; // Target this CPU.
        unsafe { GICD_SGIR.write_volatile(val) };
    }

    /// Raises a Software Generated Interrupt on the same CPU that is calling
    /// this function.
    ///
    /// * `irq`: IRQ to raise.
    ///
    /// Panics if an attempt is made to raise an IRQ of any other kind.
    pub fn notify_self(&self, irq: u32)
    {
        assert!(irq < 16,
                "Attempted to trigger a Software Generated Interrupt outside of the valid range");
        let val = 0x2008000 | irq; // Target this CPU.
        unsafe { GICD_SGIR.write_volatile(val) };
    }

    /// Checks for and processes pending IRQs in an infinite loop.
    pub fn dispatch(&self) -> !
    {
        loop {
            let val = unsafe { GICC_IAR.read_volatile() };
            let irq = val & 0x3FF; // Strip sender info from SGIs.
            if irq as usize >= IRQ_COUNT {
                unsafe { asm!("msr daifclr, #0x3", "wfi", options(nomem, nostack, preserves_flags)) };
                continue;
            }
            fence(Ordering::SeqCst);
            let handler = *self.handlers
                               .rlock()
                               .get(&irq)
                               .expect("Received an IRQ without a handler");
            handler();
            fence(Ordering::SeqCst);
            unsafe { GICC_EOIR.write_volatile(val as _) };
        }
    }
}
