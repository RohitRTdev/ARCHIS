extern crate alloc;

use common::{*, elf::*};
use log::*;
use alloc::vec::Vec;
use core::mem::size_of;
use core::ptr::copy_nonoverlapping;
use core::alloc::Layout;


#[derive(Debug, Clone)]
struct MapRegion {
    src_addr: usize,
    dest_addr: usize,
    src_size: usize,
    dest_size: usize
}

impl MapRegion {
    pub fn create_array_rgn(base: *const u8, offset: u64, size: u64, entry_size: u64) -> Self {
        Self {
            src_addr: unsafe {
                base.add(offset as usize) as usize
            },
            dest_addr: 0, src_size: size as usize, dest_size: entry_size as usize
        }
    }
    
    pub fn create_map_rgn(base: *const u8, offset: u64, size: u64) -> Self {
        Self {
            src_addr: unsafe {
                base.add(offset as usize) as usize
            },
            dest_addr: 0, src_size: size as usize, dest_size: size as usize
        }
    }
}


extern "Rust" {
    fn loader_alloc(layout: Layout) -> *mut u8;
}

#[cfg(target_arch="x86_64")]
pub fn canonicalize(address: usize) -> u64 {
    let mut addr = address as u64;
    if addr & (1 << 47) != 0 {
        addr |= (0xffff as u64) << 48;
    }
    else {
        addr &= !((0xffff as u64) << 48);
    }

    addr
}

fn load_aux_tables(reloc_sections: &mut Vec<MapRegion>, symtab: &mut Option<MapRegion>, symstr: &mut Option<MapRegion>, dynsymtab: &mut Option<MapRegion>, dynstr: &mut Option<MapRegion>, aux_base: usize, aux_alignment: usize) {
    // Now map the symbol table and relocation sections
    test_log!("Loading reloc sections and symbol table");
    let mut current_load_ptr = aux_base as *mut u8;
    for (_idx, shn) in reloc_sections.iter_mut().enumerate() {
        unsafe {
            copy_nonoverlapping(shn.src_addr as *const u8, current_load_ptr, shn.src_size);
            shn.dest_addr = current_load_ptr as usize;
            test_log!("Loaded location:{} from {:#X} to {:#X} of size: {}", _idx, shn.src_addr, current_load_ptr as usize, shn.src_size);
            
            current_load_ptr = current_load_ptr.add(shn.src_size);
            current_load_ptr = current_load_ptr.add(current_load_ptr.align_offset(aux_alignment));
        }
    }

    // Remove the symtab and dynsymtab from the reloc list once we're done with the common load logic
    // The order of the tables mentioned in this list matter
    for region in [dynstr, dynsymtab, symstr, symtab] {
        if let Some(sym) = region {
            sym.dest_addr = reloc_sections.last().unwrap().dest_addr;
            reloc_sections.pop();
        }
    }
}

fn apply_relocation(load_base: usize, kernel_size: usize, reloc_sections: &Vec<MapRegion>, dyn_tab: &Option<MapRegion>) {
    // Necessary, since it could be zero after removing symbol table from list
    if reloc_sections.len() == 0 {
        return;
    }

    let info = |bitmap: u64| {
        (bitmap & 0xffffffff) as u32
    };
    
    assert_eq!(reloc_sections[0].dest_size, size_of::<Elf64Rela>(), "Relocation section entry size not matching!!");
    let mut rel_relocations = 0;
    let mut abs_relocations = 0;
    let mut jmp_relocations = 0;
    let mut glob_relocations = 0;
    for shn in reloc_sections {
        let num_entries = shn.src_size / shn.dest_size;
        let entries = unsafe {
            core::slice::from_raw_parts(shn.dest_addr as *const Elf64Rela, num_entries)
        };

        // Here, we are assuming that linker assigned base address of elf as 0
        for entry in entries {
            let address = load_base + entry.r_offset as usize;
            match info(entry.r_info) {
                R_X86_64_RELATIVE => {
                    let value = load_base as i64 + entry.r_addend;
                    assert!(address < load_base + kernel_size);
                    unsafe {
                        *(address as *mut u64) = value as u64;
                    }

                    rel_relocations += 1;
                },
                R_X86_64_64 => {
                    abs_relocations += 1;
                },
                R_GLOB_DAT => {
                    glob_relocations += 1;
                },
                R_JUMP_SLOT => {
                    assert!(dyn_tab.is_some());
                    let dyn_entries = unsafe {
                        let tab = dyn_tab.as_ref().unwrap();
                        core::slice::from_raw_parts(tab.dest_addr as *const Elf64Sym, tab.src_size / tab.dest_size)
                    };

                    let sym_idx = (entry.r_info >> 32) as usize;

                    let value = load_base + dyn_entries[sym_idx].st_value as usize;

                    unsafe {
                        *(address as *mut u64) = value as u64;
                    }
                    jmp_relocations += 1;
                },
                _=> {}
            }
        }
    }
    
    debug!("Relative relocations = {}, absolute relocations = {}, dynamic relocations = {}, global relocations = {}", rel_relocations, abs_relocations, jmp_relocations, glob_relocations);
}

#[cfg(debug_assertions)]
pub fn print_exported_symbols(dynsym: &Option<ArrayTable>, dynstr: &Option<MemoryRegion>) {
    if dynsym.is_none() {
        return;
    }

    let tab = dynsym.as_ref().unwrap();
    let str_tab = dynstr.as_ref().unwrap();

    let stringizer = |str_idx: usize| {
        use core::ffi::CStr;

        let str_base = unsafe {
            (str_tab.base_address as *const u8).add(str_idx)
        };

        unsafe {
            CStr::from_ptr(str_base as *const i8).to_str().unwrap()
        }
    };

    let entries = unsafe {
        core::slice::from_raw_parts(tab.start as *const Elf64Sym, tab.size / tab.entry_size)
    };

    debug!("====Printing kernel exported symbols====");
    for entry in entries {
        let name = stringizer(entry.st_name as usize);
        if !name.trim().is_empty() {
            debug!("Address={:#X}->{}", entry.st_value, name);
        }
    }
}  


#[cfg(target_arch="x86_64")]
pub fn load_kernel_arch(kernel_base: *const u8, hdr: &Elf64Ehdr) -> ModuleInfo {
    assert_eq!(hdr.e_ident[4], ELFCLASS64, "x86_64 arch requires kernel elf file to be of 64 bit type!");
    debug!("Found 64 bit kernel elf header");

    assert_eq!(hdr.e_phentsize, size_of::<Elf64Phdr>() as u16);
    assert_eq!(hdr.e_shentsize, size_of::<Elf64Shdr>() as u16);

    let prog_base = unsafe {
        kernel_base.add(hdr.e_phoff as usize)
    } as *const Elf64Phdr;
    
    let shn_base = unsafe {
        kernel_base.add(hdr.e_shoff as usize)
    } as *const Elf64Shdr;

    let prog_hdrs = unsafe {
        core::slice::from_raw_parts(prog_base, hdr.e_phnum as usize)
    };

    let shn_hdrs = unsafe {
        core::slice::from_raw_parts(shn_base, hdr.e_shnum as usize)
    };

    assert!(prog_hdrs.len() != 0 && shn_hdrs.len() != 0, "No program or section header found in kernel elf file");
    assert!((hdr.e_shstrndx < hdr.e_shnum) && (shn_hdrs[hdr.e_shstrndx as usize].sh_type == SHT_STRTAB), "No string table in elf file!");
    let mut map_regions_list = Vec::new();

    let mut loadable_segments = 0;
    let mut dyn_shn = None;
    let mut max_alignment: usize = 0;

    test_log!("Printing loadable segment descriptors");

    // Get information on all loadable segments (.text, .rodata etc)
    for prog_hdr in prog_hdrs.iter().filter(|entry| {
        entry.p_type == PT_LOAD || entry.p_type == PT_DYNAMIC
    }) {
            map_regions_list.push(MapRegion {src_addr: unsafe {
                kernel_base.add(prog_hdr.p_offset as usize) as usize
            },
            dest_addr: prog_hdr.p_vaddr as usize, src_size: prog_hdr.p_filesz as usize, dest_size: prog_hdr.p_memsz as usize 
            });

            test_log!("src: {:#X}, dest: {:#X}, src-size: {}, dest-size: {}, aligment: {}", map_regions_list.last().unwrap().src_addr,
            map_regions_list.last().unwrap().dest_addr, map_regions_list.last().unwrap().src_size, 
            map_regions_list.last().unwrap().dest_size, prog_hdr.p_align); 

            if prog_hdr.p_align != 0 && prog_hdr.p_align != 1 {
                max_alignment = max_alignment.max(prog_hdr.p_align as usize);
            }

#[cfg(test)]
            assert!(map_regions_list.last().unwrap().dest_addr % prog_hdr.p_align as usize == 0, "Provided virtual address does not satisfy alignment constraint");

            loadable_segments += 1;
            if prog_hdr.p_type == PT_DYNAMIC {
                dyn_shn = Some(map_regions_list.last().unwrap().clone());
            }
    }

    // For symbol table and reloc section, dest_size is reinterpreted as per-entry size
    let mut symtab = None;
    let mut symstr = None;
    let mut reloc_sections = Vec::new();
    let mut dynsymtab = None;
    let mut dynstr = None;
    let mut aux_alignment: usize = 0;
    
    // Check if symbol and relocation tables are present and load it to memory
    shn_hdrs.iter().filter(|entry| {
        entry.sh_type == SHT_SYMTAB || entry.sh_type == SHT_RELA || entry.sh_type == SHT_DYNSYM
    }).for_each(|entry| {
            let reg = MapRegion::create_array_rgn(kernel_base, entry.sh_offset, entry.sh_size, entry.sh_entsize);
            let str_shn = &shn_hdrs[entry.sh_link as usize];
            
            match entry.sh_type {
                SHT_SYMTAB => {
                    assert_eq!(str_shn.sh_type, SHT_STRTAB);
                    symtab = Some(reg);
                    symstr = Some(MapRegion::create_map_rgn(kernel_base, str_shn.sh_offset, str_shn.sh_size));
                },
                SHT_RELA => {
                    reloc_sections.push(reg);
                },
                SHT_DYNSYM => {
                    assert_eq!(str_shn.sh_type, SHT_STRTAB);
                    dynsymtab = Some(reg);
                    dynstr = Some(MapRegion::create_map_rgn(kernel_base, str_shn.sh_offset, str_shn.sh_size));
                },
                SHT_DYNAMIC => {
                    assert_eq!(str_shn.sh_type, SHT_STRTAB);
                    dynstr = Some(MapRegion::create_map_rgn(kernel_base, str_shn.sh_offset, str_shn.sh_size));
                },
                _ => {}
            }

            if entry.sh_addralign != 0 && entry.sh_addralign != 1 {
                aux_alignment = aux_alignment.max(entry.sh_addralign as usize);
            }
    });
    
    assert!(!(dynsymtab.is_some() ^ (reloc_sections.len() > 0)));

    debug!("Loadable segments: {}, Dynamic segment present: {}, Symbol table present: {}, max_alignment: {}, reloc sections:{}, aux_alignment: {}", loadable_segments, 
    dyn_shn.is_some(), symtab.is_some(), max_alignment, reloc_sections.len(), aux_alignment);

    // Need the kernel code + data regions to be in sorted order of their dest addr as upcoming logic depends on it
    map_regions_list.sort_by(|a, b| {
        a.dest_addr.cmp(&b.dest_addr)
    });

    let num_reloc_shns = reloc_sections.len();
    // Push the symbol and dynamic symbol tables onto the relocation list 
    // Since we're going to apply same operations to both symtab and reloc sections, we're adding symtab into reloc array
    // to reduce code repetition    
    if let Some(sym) = &symtab {
        reloc_sections.push(sym.clone());
        reloc_sections.push(symstr.as_ref().unwrap().clone());
    }
    if let Some(dyntab) = &dynsymtab {
        reloc_sections.push(dyntab.clone());
        reloc_sections.push(dynstr.as_ref().unwrap().clone());
    }
    
    // Next, we will calculate the size required to store these auxiliary tables
    let mut aux_size = 0;
    for shn in reloc_sections.iter() {
        aux_size += shn.src_size;

        // Add padding to ensure alignment
        aux_size += (aux_size as *const u8).align_offset(aux_alignment);
    }

    // Layout info
    // 1st we load all the binary code + data regions
    // 2nd we load the auxiliary tables (reloc shn, symtab, dynsymtab, string shn etc)
    // 3rd we load an array of descriptors which mentions the locations of the reloc sections within the loaded address space

    let last_entry =  map_regions_list.last().unwrap();
    let main_shn_size = last_entry.dest_addr + last_entry.dest_size;
    let aux_padding = (main_shn_size as *const u8).align_offset(aux_alignment);
    let aux_shn_end = aux_padding + main_shn_size + aux_size; 
    let reloc_desc_alignment = core::mem::align_of::<MemoryRegion>();  
    let reloc_desc_padding = (aux_shn_end as *const u8).align_offset(reloc_desc_alignment);
    let total_module_size = aux_shn_end + reloc_desc_padding + num_reloc_shns * core::mem::size_of::<MemoryRegion>(); 
    
    let mut layout = Layout::from_size_align(total_module_size, max_alignment.max(aux_alignment).max(reloc_desc_alignment)).unwrap();
    let load_base = unsafe {
        loader_alloc(layout)
    };

    debug!("Loading kernel regions at load_base: {:#X}", load_base as usize);
    //Now, map all loadable regions to appropriate locations
    for (idx, entry) in map_regions_list.iter().enumerate() {
        unsafe {
            let current_load_ptr = load_base.add(entry.dest_addr);

            // First, zero fill the memory region (Some regions have dest_size > src_size, so remaining part (dest_size - src_size) must be zeroed)
            current_load_ptr.write_bytes(0, entry.dest_size);
            
            copy_nonoverlapping(entry.src_addr as *const u8, current_load_ptr, entry.src_size);
            debug!("Loaded location:{} from {:#X} of va:{:#X} to {:#X}", idx, entry.src_addr, entry.dest_addr, current_load_ptr as usize);
        }
    }
    
    if reloc_sections.len() > 0 {
        load_aux_tables(&mut reloc_sections, &mut symtab, &mut symstr, &mut dynsymtab, &mut dynstr, load_base as usize + main_shn_size + aux_padding, aux_alignment);
        apply_relocation(load_base as usize, main_shn_size, &reloc_sections, &dynsymtab);
    }

    // Fill up all output information
    let (sym_tab_out, sym_tab_str) = if let Some(sym) = symtab {
        (Some(ArrayTable {start: sym.dest_addr, size: sym.src_size, entry_size: sym.dest_size}), Some(MemoryRegion{base_address: symstr.as_ref().unwrap().dest_addr, size: symstr.as_ref().unwrap().src_size}))
    }
    else {
        (None, None)
    };
    
    let dyn_sym_tab_out = dynsymtab.map(|sym| 
        ArrayTable {start: sym.dest_addr, size: sym.src_size, entry_size: sym.dest_size}
    );
    
    let dyn_str_out = dynstr.map(|sym| 
        MemoryRegion {base_address: sym.dest_addr, size: sym.src_size}
    );

    let dyn_shn_out = dyn_shn.map(|sym| 
        ArrayTable {start: load_base as usize + sym.dest_addr, size: sym.src_size, entry_size: size_of::<ElfDyn>()}
    );

    let reloc_shn_out = if reloc_sections.len() > 0 {
        layout = Layout::array::<MemoryRegion>(reloc_sections.len()).unwrap();
        let reloc_base = load_base as usize + aux_shn_end + reloc_desc_padding;
        let reloc_shns = unsafe {
            core::slice::from_raw_parts_mut(reloc_base as *mut MemoryRegion, reloc_sections.len())
        };

        for (idx, shn) in reloc_sections.iter().enumerate() {
            reloc_shns[idx] = MemoryRegion {
                base_address: shn.dest_addr,
                size: shn.src_size
            }
        }

        Some(ArrayTable {start: reloc_base as usize, size: layout.size(), entry_size: size_of::<MemoryRegion>()})
    }
    else {
        None
    };

#[cfg(debug_assertions)]
    print_exported_symbols(&dyn_sym_tab_out, &dyn_str_out);

    ModuleInfo {
        entry: hdr.e_entry as usize + load_base as usize,
        base: load_base as usize,
        size: main_shn_size,
        total_size: total_module_size, 
        sym_tab: sym_tab_out,
        sym_str: sym_tab_str,
        dyn_tab: dyn_sym_tab_out,
        dyn_str: dyn_str_out,
        rlc_shn: reloc_shn_out,
        dyn_shn: dyn_shn_out
    }

}