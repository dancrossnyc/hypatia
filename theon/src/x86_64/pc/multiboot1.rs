// Copyright 2021  The Hypatia Authors
// All rights reserved
//
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

use crate::theon;
use crate::x86_64::memory;
use alloc::vec::Vec;
use core::cell::SyncUnsafeCell;
use multiboot::information::{MemoryManagement, MemoryType, Multiboot, PAddr};

unsafe fn phys_to_slice(phys_addr: PAddr, len: usize) -> Option<&'static [u8]> {
    Some(unsafe {
        let p = theon::VZERO.add(phys_addr as usize);
        core::slice::from_raw_parts(p, len)
    })
}

struct MM;

impl MemoryManagement for MM {
    unsafe fn paddr_to_slice(&self, phys_addr: PAddr, len: usize) -> Option<&'static [u8]> {
        unsafe { phys_to_slice(phys_addr, len) }
    }

    unsafe fn allocate(&mut self, _len: usize) -> Option<(PAddr, &mut [u8])> {
        None
    }

    unsafe fn deallocate(&mut self, addr: PAddr) {
        if addr != 0 {
            unimplemented!();
        }
    }
}

fn theon_region() -> memory::Region {
    let start = 0x0000_0000_0010_0000_u64;
    let phys_end = unsafe { theon::end_addr().offset_from_unsigned(theon::VZERO) } as u64;
    memory::Region { start, end: phys_end, typ: memory::Type::Loader }
}

fn parse_memory(mb: &Multiboot<'_, '_>) -> Option<Vec<memory::Region>> {
    Some(
        mb.memory_regions()?
            .map(|r| memory::Region {
                start: r.base_address(),
                end: r.base_address().wrapping_add(r.length()),
                typ: match r.memory_type() {
                    MemoryType::Available => memory::Type::RAM,
                    MemoryType::Reserved => memory::Type::Reserved,
                    MemoryType::ACPI => memory::Type::ACPI,
                    MemoryType::NVS => memory::Type::NonVolatile,
                    MemoryType::Defect => memory::Type::Defective,
                },
            })
            .collect(),
    )
}

pub(crate) struct MultibootModule<'a> {
    pub bytes: &'a [u8],
    pub name: Option<&'a str>,
}

impl MultibootModule<'_> {
    fn region(&self) -> memory::Region {
        let phys_start = unsafe { self.bytes.as_ptr().offset_from_unsigned(theon::VZERO) };
        let phys_end = phys_start.wrapping_add(self.bytes.len());
        memory::Region { start: phys_start as u64, end: phys_end as u64, typ: memory::Type::Module }
    }
}

fn parse_modules<'a>(mb: &'a Multiboot<'_, '_>) -> Option<Vec<MultibootModule<'a>>> {
    Some(
        mb.modules()?
            .map(|m| MultibootModule {
                bytes: unsafe { phys_to_slice(m.start, (m.end - m.start) as usize).unwrap() },
                name: m.string.map(|name| name.split('/').next_back().unwrap()),
            })
            .collect(),
    )
}

pub(crate) struct InitInfo<'a> {
    pub memory_regions: Vec<memory::Region>,
    pub regions: Vec<memory::Region>,
    pub modules: Vec<MultibootModule<'a>>,
}

pub(crate) struct Multiboot1 {
    multiboot: Multiboot<'static, 'static>,
}

impl Multiboot1 {
    pub(crate) fn new(mbinfo_phys: u64) -> Multiboot1 {
        let multiboot = unsafe {
            static MULTIBOOT_MM: SyncUnsafeCell<MM> = SyncUnsafeCell::new(MM {});
            let mm = &mut *MULTIBOOT_MM.get();
            Multiboot::from_ptr(mbinfo_phys as PAddr, mm).unwrap()
        };
        Multiboot1 { multiboot }
    }

    pub(crate) fn info(&self) -> InitInfo<'_> {
        let (memory_regions, regions, modules) = init_memory_regions(&self.multiboot);
        InitInfo { memory_regions, regions, modules }
    }
}

pub(crate) fn init(mbinfo_phys: u64) -> Multiboot1 {
    uart::panic_println!("mbinfo: {:08x}", mbinfo_phys);
    Multiboot1::new(mbinfo_phys)
}

fn init_memory_regions<'a>(
    mb: &'a Multiboot<'_, '_>,
) -> (Vec<memory::Region>, Vec<memory::Region>, Vec<MultibootModule<'a>>) {
    let memory_regions = parse_memory(mb).unwrap();
    let modules = parse_modules(mb).expect("could not find modules");
    let regions = usable_regions(memory_regions.clone(), &modules);
    (memory_regions, regions, modules)
}

fn usable_regions(
    mut regions: Vec<memory::Region>,
    modules: &[MultibootModule<'_>],
) -> Vec<memory::Region> {
    regions.push(theon_region());
    for module in modules {
        regions.push(module.region());
    }
    regions.sort_by(memory::Region::cmp);
    fix_overlap(regions)
}

fn fix_overlap(mut overlapping_regions: Vec<memory::Region>) -> Vec<memory::Region> {
    // Split regions to ensure no overlap.
    let mut regions = Vec::new();
    let mut prev = overlapping_regions.pop().unwrap();
    while let Some(mut region) = overlapping_regions.pop() {
        if prev.start == region.start && prev.end < region.end {
            region.start = prev.end;
        } else if region.start < prev.end {
            regions.push(memory::Region { start: prev.start, end: region.start, typ: prev.typ });
            if region.end < prev.end {
                regions.push(region);
            }
            prev.start = region.end;
            continue;
        }
        regions.push(prev);
        prev = region;
    }
    regions.push(prev);
    regions
}
