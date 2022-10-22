//! Generic Interrupt Controller (GIC) 400 driver.
//!
//! Documentation:
//!
//! * [BCM2711 ARM Peripherals](https://datasheets.raspberrypi.com/bcm2711/bcm2711-peripherals.pdf)
//!   6.3 and 6.5.1
//! * [CoreLink GIC-400 Generic Interrupt Controller Technical Reference Manual](https://developer.arm.com/documentation/ddi0471/b)
//! * [ARM Generic Interrupt Controller Architecture Specification](https://developer.arm.com/documentation/ihi0048/b)

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::ops::{Index, IndexMut};
use core::ptr::{read_volatile, write_volatile};

use crate::sync::{Lazy, Lock};
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
/// IRQ minimum priority register.
const GICC_PMR: *mut u32 = (GIC_BASE + 0x2004) as _;
/// IRQ acknowledge register.
const GICC_IAR: *mut u32 = (GIC_BASE + 0x200C) as _;
/// IRQ dismissal register.
const GICC_EOIR: *mut u32 = (GIC_BASE + 0x2010) as _;

/// Global interrupt controller driver.
pub static IRQ: Lazy<Lock<Irq>> = Lazy::new(Irq::new);

/// IRQ driver.
pub struct Irq
{
    /// Registered handlers.
    handlers: BTreeMap<usize, Vec<Handler>>,
}

type Handler = fn(usize) -> ();

impl Irq
{
    /// Creates and initializes a new interrupt controller driver.
    ///
    /// Returns the newly created driver.
    fn new() -> Lock<Self>
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
            // Make all SPIs edge triggered.
            (*GICD_ICFGR).iter_mut()
                         .skip(1)
                         .for_each(|element| write_volatile(element, 0xFFFFFFFF));
            // Deliver all SPIs to all cores..
            (*GICD_ITARGETSR).iter_mut()
                             .skip(8)
                             .for_each(|element| write_volatile(element, 0xF));
        }
        let this = Self { handlers: BTreeMap::new() };
        Lock::new(this)
    }

    /// Listens for the specified IRQ, and installs a handler for it.
    pub fn listen(&mut self, id: usize, handler: Handler)
    {
        assert!(id < IRQ_COUNT, "IRQ #{id} is out of range");
        if self.handlers.get(&id).is_none() {
            // Figure out which register and bit to enable for the given IRQ.
            let val = (0x1 << (id & 0x1F)) as u32;
            let idx = id >> 5;
            unsafe { write_volatile((*GICD_ISENABLER).index_mut(idx), val) };
            // Set the IRQ to be level-triggered.
            let bit = (0x2 << (id << 1 & 0x1F)) as u32;
            let idx = id >> 4;
            let val = unsafe { read_volatile((*GICD_ICFGR).index(idx)) };
            let val = val & !bit;
            unsafe { write_volatile((*GICD_ICFGR).index_mut(idx), val) };
            self.handlers.insert(id, vec![handler]);
        } else {
            self.handlers.get_mut(&id).unwrap().push(handler);
        }
    }

    /// Checks for and processes all pending IRQs.
    ///
    /// This function is intended to be called once every main loop.
    pub fn handle(&self)
    {
        loop {
            let val = unsafe { (GICC_IAR as *const u32).read_volatile() as usize };
            let id = val & 0x3FF; // Strip sender info from SGIs.
            if id >= IRQ_COUNT {
                break;
            }
            if let Some(list) = self.handlers.get(&id) {
                list.iter().for_each(|handler| handler(id));
            } else {
                panic!("Missing handler for IRQ #{id}")
            };
            unsafe { (GICC_EOIR as *mut u32).write_volatile(val as _) };
        }
    }
}
