use core::alloc::Layout;
use core::marker::PhantomData;
use alloc::sync::Arc;
use common::{MemoryRegion, PAGE_SIZE};
use kernel_intf::KError;
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
            PageDescriptor::VIRTUAL
        ).expect("Filebuffer memory deallocation failed!");
    }
}

pub struct FileInst {
    file_name: &'static str,
    offset: usize,
    total_size: usize
}


impl FileInst {
    pub fn read(&mut self, buffer: &FileBuffer) -> usize {
        let init_fs = INIT_FS.get().unwrap();

        let entry=  init_fs.get(self.file_name)
        .expect("Critical error! File not found in init fs!");

        let start = unsafe {
            entry.as_ptr().add(self.offset)
        };

        let len = (entry.len() - self.offset).min(buffer.len());
        buffer.write(start.addr(), len, 0);
        self.offset += len;
    
        len
    }

    pub fn write(&mut self, _: FileBuffer) {
        panic!("write() not supported right now!");
    }

    pub fn len(&self) -> usize {
        let init_fs = INIT_FS.get().unwrap();

        let entry=  init_fs.get(self.file_name)
        .expect("Critical error! File not found in init fs!");

        entry.len()
    }

    pub fn get_offset(&self) -> usize {
        self.offset
    }
}

pub fn open(file_name: &'static str) -> Result<FileInstance, KError> {
    let init_fs = INIT_FS.get().unwrap();

    let entry=  init_fs.get(file_name)
    .ok_or(KError::InvalidArgument)?;
    
    let file_desc = FileInst {
        file_name,
        offset: 0,
        total_size: entry.len()
    };
    
    let file_instance = Arc::new_in(
        Spinlock::new(
            file_desc
        ),
        PoolAllocatorGlobal
    );

    add_new_handle(FileHandle(file_instance.clone()));

    Ok(file_instance)
}