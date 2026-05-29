use core::alloc::Layout;
use core::marker::PhantomData;
use alloc::sync::Arc;
use alloc::string::String;
use alloc::borrow::ToOwned;
use common::{MemoryRegion, PAGE_SIZE};
use kernel_intf::{KError, info};
use crate::INIT_FS;
use crate::hal::copy_user_memory;
use crate::mem::{PageDescriptor, PoolAllocatorGlobal, allocate_memory, deallocate_memory};
use crate::sched::{add_new_handle, Handle::FileHandle};
use crate::sync::Spinlock;

pub type FileInstance = Arc<Spinlock<FileInst>, PoolAllocatorGlobal>;

pub struct FileBuffer {
    region: MemoryRegion,
    is_user: bool,
    own: bool,

    // Send trait is unsafe here, because if the buffer refers 
    // to user memory, it is only valid in the current process context
    _nosend: PhantomData<*const ()> 
}

impl FileBuffer {
    pub fn into_inner(buf: &mut Self) {
        buf.own = false;
    }

    pub fn new(size: usize, is_user: bool) -> Result<Self, KError> {
        let base_address = allocate_memory(
            Layout::from_size_align(size, PAGE_SIZE).unwrap(),
        PageDescriptor::VIRTUAL | (if is_user {PageDescriptor::USER} else {0})
        )?.addr();

        Ok(
            Self {
                region: MemoryRegion {
                    base_address,
                    size
                },
                is_user,
                own: true,
                _nosend: PhantomData
            }
        )
    }

    pub fn from(base_address: usize, size: usize, is_user: bool) -> Self {
        Self {
            region: MemoryRegion { 
                base_address, 
                size 
            },
            is_user,
            own: false,
            _nosend: PhantomData
        }
    }

    // dest pointer here must be kernel memory
    pub fn read(&self, to: usize, len: usize, offset: usize) {
        assert!(len + offset <= self.region.size);
        if len == 0 {
            return;
        }
        
        if self.is_user {
            unsafe {
                copy_user_memory(
                    to as *mut u8, 
                    (self.region.base_address as *mut u8).add(offset),
                    len
                );
            }
        }
        else {
            unsafe {
                core::ptr::copy(
                    (self.region.base_address as *const u8).add(offset),
                    to as *mut u8,
                    len
                )
            }
        }
    }
    
    // src pointer here must be kernel memory
    pub fn write(&self, from: usize, len: usize, offset: usize) {
        assert!(len + offset <= self.region.size);
        if len == 0 {
            return;
        }

        if self.is_user {
            unsafe {
                copy_user_memory(
                    (self.region.base_address as *mut u8).add(offset),
                    from as *const u8, 
                    len
                );
            }
        }
        else {
            unsafe {
                core::ptr::copy(
                    from as *const u8,
                    (self.region.base_address as *mut u8).add(offset),
                    len
                )
            }
        }
    }

    pub fn len(&self) -> usize {
        self.region.size
    }

    pub fn as_slice<'a>(&'a self) -> &'a [u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.region.base_address as *const u8,
                self.len()
            )
        }
    }
}

impl Drop for FileBuffer {
    fn drop(&mut self) {
        if !self.own {
            return;
        }

        deallocate_memory(
            self.region.base_address as *mut u8,
            Layout::from_size_align(
                self.region.size,
                PAGE_SIZE
            ).unwrap(),
            PageDescriptor::VIRTUAL | (if self.is_user {PageDescriptor::USER} else {0})
        ).expect("Filebuffer memory deallocation failed!");
    }
}

pub struct FileInst {
    file_name: String,
    offset: usize,
    total_size: usize
}

impl FileInst {
    pub fn read(&mut self, buffer: &FileBuffer) -> usize {
        let remaining = self.total_size.saturating_sub(self.offset);
        let len = remaining.min(buffer.len());
        if len == 0 {
            return 0;
        }

        let filename = resolve_symlink(self.file_name.as_str());
        let init_fs = INIT_FS.get().unwrap();
        let entry = init_fs.fs.get(filename)
        .expect("Critical error! File not found in init fs!");

        let start = unsafe {
            entry.as_ptr().add(self.offset)
        };

        buffer.write(start.addr(), len, 0);
        self.offset += len;

        len
    }

    pub fn write(&mut self, _: FileBuffer) {
        panic!("write() not supported right now!");
    }

    pub fn len(&self) -> usize {
        self.total_size
    }

    pub fn get_offset(&self) -> usize {
        self.offset
    }

    pub fn get_path(&self) -> &str {
        self.file_name.as_str()
    }
}

impl Drop for FileInst {
    fn drop(&mut self) {
        info!("Dropped file instance: {}", self.file_name);
    }
}

pub fn open(file_name: &str) -> Result<FileInstance, KError> {
    let init_fs = INIT_FS.get().unwrap();
    let filename = resolve_symlink(file_name);

    let entry = init_fs.fs.get(filename)
    .ok_or(KError::InvalidArgument).or_else(|e| {
        info!("Failed to open file {}", file_name);
        Err(e)
    })?;
    
    let file_desc = FileInst {
        file_name: filename.to_owned(),
        offset: 0,
        total_size: entry.len()
    };
    
    let file_instance = Arc::new_in(
        Spinlock::new(
            file_desc
        ),
        PoolAllocatorGlobal
    );

    Ok(file_instance)
}

pub fn resolve_symlink(name: &str) -> &str {
    let init_fs = INIT_FS.get().unwrap();
    init_fs.symlinks.get(name).copied().unwrap_or(name)
}
