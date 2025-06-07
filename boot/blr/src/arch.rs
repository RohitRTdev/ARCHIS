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

extern "Rust" {
    fn loader_alloc(layout: Layout) -> *mut u8;
}

fn load_aux_tables(reloc_sections: &mut Vec<MapRegion>, symtab: &mut Option<MapRegion>, aux_alignment: usize) {
    // Next, we will calculate the size required to store these auxiliary tables
    let mut aux_size = 0;
    for shn in reloc_sections.iter() {
        aux_size += shn.src_size;

        // Add padding to ensure alignment
        aux_size += (aux_size as *const u8).align_offset(aux_alignment);
    }

    let layout = Layout::from_size_align(aux_size, aux_alignment).unwrap();
    let aux_base = unsafe {
        loader_alloc(layout)
    };

    // Now map the symbol table and relocation sections
    test_log!("Loading reloc sections and symbol table");
    let mut current_load_ptr = aux_base;
    for shn in reloc_sections.iter_mut().enumerate() {
        unsafe {
            copy_nonoverlapping(shn.1.src_addr as *const u8, current_load_ptr, shn.1.src_size);
            shn.1.dest_addr = current_load_ptr as usize;
            test_log!("Loaded location:{} from {:#X} to {:#X} of size: {}", shn.0, shn.1.src_addr, current_load_ptr as usize, shn.1.src_size);
            
            current_load_ptr = current_load_ptr.add(shn.1.src_size);
            current_load_ptr = current_load_ptr.add(current_load_ptr.align_offset(aux_alignment));
        }
    }

    if let Some(sym) = symtab {
        sym.dest_addr = reloc_sections.last().unwrap().dest_addr;
        reloc_sections.pop();
    }

}

fn apply_relocation(load_base: usize, reloc_sections: &Vec<MapRegion>) {
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
            match info(entry.r_info) {
                R_X86_64_RELATIVE => {
                    let address = load_base + entry.r_offset as usize;
                    let value = load_base as i64 + entry.r_addend;
                    rel_relocations += 1;
                    unsafe {
                        *(address as *mut u64) = value as u64;
                    }
                },
                R_X86_64_64 => {
                    abs_relocations += 1;
                },
                R_GLOB_DAT => {
                    glob_relocations += 1;
                },
                R_JUMP_SLOT => {
                    jmp_relocations += 1;
                },
                _=> {}
            }
        }
    }
    
    debug!("Relative relocations = {}, absolute relocations = {}, dynamic relocations = {}, global relocations = {}", rel_relocations, abs_relocations, jmp_relocations, glob_relocations);
}


#[cfg(target_arch="x86_64")]
pub fn load_kernel_arch(kernel_base: *const u8, hdr: &Elf64Ehdr) -> KernelInfo {
    assert_eq!(hdr.e_ident[4], ELFCLASS64, "x86_64 arch requires kernel elf file to be of 64 bit type!");
    debug!("Found 64 bit kernel elf header");

    assert_eq!(hdr.e_phentsize, size_of::<Elf64Phdr>() as u16);
    assert_eq!(hdr.e_shentsize, size_of::<Elf64Shdr>() as u16);

    let stringizer = |hdr: &Elf64Ehdr, shn_hdrs: &[Elf64Shdr], str_idx: usize| {
        use core::ffi::CStr;

        let str_base = unsafe {
            kernel_base.add(shn_hdrs[hdr.e_shstrndx as usize].sh_offset as usize).add(str_idx)
        };

        unsafe {
            CStr::from_ptr(str_base as *const i8)
        }
    };


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
    let mut aux_alignment: usize = 0;
    
    // Check if symbol table is present and load it to memory
    shn_hdrs.iter().filter(|entry| {
        entry.sh_type == SHT_SYMTAB
    }).for_each(|entry| {
        symtab = Some(MapRegion {src_addr: unsafe {
            kernel_base.add(entry.sh_offset as usize) as usize
        },
        dest_addr: 0, src_size: entry.sh_size as usize, dest_size: entry.sh_entsize as usize 
        });

        if entry.sh_addralign != 0 && entry.sh_addralign != 1 {
            aux_alignment = entry.sh_addralign as usize;
        }
    });
    
    // Fetch information on all relocation sections
    let mut reloc_sections = Vec::new();
    shn_hdrs.iter().filter(|entry| {
        entry.sh_type == SHT_RELA
    }).for_each(|entry| {
        reloc_sections.push(MapRegion {
            src_addr: unsafe {
                kernel_base.add(entry.sh_offset as usize) as usize
            },
        dest_addr: 0, src_size: entry.sh_size as usize, dest_size: entry.sh_entsize as usize 
        });
        
        if entry.sh_addralign != 0 && entry.sh_addralign != 1 {
            aux_alignment = aux_alignment.max(entry.sh_addralign as usize);
        }
    });

    debug!("Loadable segments: {}, Dynamic segment present: {}, Symbol table present: {}, max_alignment: {}, reloc sections:{}, aux_alignment: {}", loadable_segments, 
    dyn_shn.is_some(), symtab.is_some(), max_alignment, reloc_sections.len(), aux_alignment);

    // This is not really needed, but it's here just to make sure
    map_regions_list.sort_by(|a, b| {
        a.dest_addr.cmp(&b.dest_addr)
    });

    let last_entry =  map_regions_list.last().unwrap();
    let layout = Layout::from_size_align(last_entry.dest_addr + last_entry.dest_size, max_alignment).unwrap();
    let load_base = unsafe {
        loader_alloc(layout)
    };

    let mut current_load_ptr = load_base;
    let mut last_load_addr = map_regions_list[0].dest_addr;
    test_log!("Loading kernel regions at load_base: {:#X}", load_base as usize);
    //Now, map all loadable regions to appropriate locations
    for entry in map_regions_list.iter().enumerate() {
        unsafe {
            current_load_ptr = current_load_ptr.add(entry.1.dest_addr - last_load_addr);

            // First, zero fill the memory region (Some regions have dest_size > src_size, so remaining part (dest_size - src_size) must be zeroed)
            current_load_ptr.write_bytes(0, entry.1.dest_size);
            
            copy_nonoverlapping(entry.1.src_addr as *const u8, current_load_ptr, entry.1.src_size);
            test_log!("Loaded location:{} from {:#X} to {:#X}", entry.0, entry.1.src_addr, current_load_ptr as usize);
        }
        last_load_addr = entry.1.dest_addr;
    }
    
    // Since we're going to apply same operations to both symtab and reloc sections, we're adding symtab into reloc array
    // to reduce code repetition
    if let Some(sym) = &symtab {
        reloc_sections.push(sym.clone());
    }

    if reloc_sections.len() > 0 {
        load_aux_tables(&mut reloc_sections, &mut symtab, aux_alignment);
        apply_relocation(load_base as usize, &reloc_sections);
    }

    let sym_tab_out = if let Some(sym) = &symtab {
        Some(SymTable {start: sym.dest_addr, size: sym.src_size, entry_size: sym.dest_size})
    }
    else {
        None
    };

    let dyn_shn_out = if let Some(dyn_tab) = &dyn_shn {
        Some(SymTable {start: load_base as usize + dyn_tab.dest_addr, size: dyn_tab.src_size, entry_size: size_of::<ElfDyn>()})
    }
    else {
        None
    };

    let reloc_shn_out = if reloc_sections.len() > 0 {
        Some(SymTable {start: reloc_sections[0].dest_addr, size: reloc_sections[0].src_size, entry_size: reloc_sections[0].dest_size})
    }
    else {
        None
    };

    KernelInfo {
        entry: hdr.e_entry as usize + load_base as usize,
        base: load_base as usize,
        size: last_entry.dest_addr + last_entry.dest_size,
        sym_tab: sym_tab_out,
        reloc_section: reloc_shn_out,
        dynamic_section: dyn_shn_out
    }

}