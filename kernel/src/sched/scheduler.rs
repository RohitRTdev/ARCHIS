use alloc::sync::Arc;
use crate::cpu::{MAX_CPUS, PerCpu, Stack, get_panic_base, set_panic_base, get_total_cores, get_worker_stack};
use crate::hal::{self, IPIRequestType, create_kernel_context, disable_scheduler_timer, enable_scheduler_timer, fetch_context, switch_context};
use crate::mem::{PoolAllocatorGlobal, VCB, get_kernel_addr_space, set_address_space};
use crate::{ds::*, sched};
use crate::sync::{KSem, KSemInnerType, Spinlock};
use super::{KProcess, ProcessStatus, get_current_process, get_process_info, KTimerInnerType};
use core::sync::atomic::{AtomicU8,AtomicUsize, Ordering};
use core::ptr::NonNull;
use core::mem::take;
use alloc::collections::BTreeMap;
use kernel_intf::{KError, debug, info};

// This is in milliseconds
pub const QUANTUM: usize = 10;
const INIT_QUANTA: usize = 10;

pub type KThread = Arc<Spinlock<Task>, PoolAllocatorGlobal>;

static TASK_ID: AtomicUsize = AtomicUsize::new(0);
static TASK_CPU: AtomicU8 = AtomicU8::new(0);
static TASKS: Spinlock<BTreeMap<usize, KThread>> = Spinlock::new(BTreeMap::new());

const _: () = {
    assert!(u8::MAX as usize + 1 >= MAX_CPUS);
};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TaskStatus {
    RUNNING,
    ACTIVE,
    WAITING,
    TERMINATED
}

pub struct Task {
    id: usize,
    core: usize,
    stack: Option<Stack>,
    status: TaskStatus,
    context: *const u8,
    quanta: usize,
    panic_base: usize,
    wait_semaphores: DynList<KSemInnerType>,
    term_notify: KSem,
    process: Option<KProcess>,
    vcb: Option<VCB>
}

impl Task {
    fn new(alloc_stack: bool, core: usize) -> Result<KThread, KError> {
        let stack  = if alloc_stack {
            Some(Stack::new()?)
        } else {
            None
        };
        let id = TASK_ID.fetch_add(1, Ordering::Relaxed);  

        if alloc_stack {
            debug!("Creating task with ID:{} and stack_addr={:#X} on core {}", id, stack.as_ref().unwrap().get_stack_base(), core);
        } 
        else {
            debug!("Creating task with ID:{}", id);
        }

        let task = Arc::new_in(Spinlock::new(Task {
            id,
            core, 
            stack,
            status: TaskStatus::ACTIVE,
            context: core::ptr::null(),
            quanta: INIT_QUANTA,
            panic_base: 0,
            wait_semaphores: List::new(),
            term_notify: KSem::new(0, 1),
            process: None,
            vcb: None
        }), PoolAllocatorGlobal);

        Ok(task)
    }

    pub fn get_id(&self) -> usize {
        self.id
    }
    
    pub fn get_status(&self) -> TaskStatus {
        self.status
    }
    
    pub fn get_core(&self) -> usize {
        self.core
    }

    pub fn get_process(&self) -> Option<KProcess> {
        if let Some(proc) = &self.process {
            Some(Arc::clone(proc))
        }
        else {
            None
        }
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        info!("Dropping task:{}", self.id);
        assert!(self.wait_semaphores.get_nodes() == 0);
    }
}

unsafe impl Send for Task {}

pub struct TaskQueue {
    active_tasks: DynList<KThread>,
    waiting_tasks: DynList<KThread>,
    terminated_tasks: DynList<KThread>,
    notifier_list: DynList<KSem>,
    timer_list: DynList<KTimerInnerType>,
    running_task: Option<NonNull<ListNode<KThread>>>,
    idle_task_stack: NonNull<u8>,
    leftover_stack: DynList<Stack>,
    flip_flop: bool,
    preemption_count: usize
}

unsafe impl Send for TaskQueue{}

impl TaskQueue {
    const fn new() -> Self {
        TaskQueue {
            active_tasks: List::new(),
            waiting_tasks: List::new(),
            terminated_tasks: List::new(),
            notifier_list: List::new(),
            timer_list: List::new(),
            running_task: None,
            idle_task_stack: NonNull::dangling(),
            leftover_stack: List::new(),
            flip_flop: false,
            preemption_count: 0
        }
    }
}

static SCHEDULER_CON_BLK: PerCpu<Spinlock<TaskQueue>> = PerCpu::new_with(
    [const {Spinlock::new(TaskQueue::new())}; MAX_CPUS]
);

pub fn get_task_info(task_id: usize) -> Option<KThread> {
    let task_map = TASKS.lock();

    task_map.get(&task_id).map(|item| {
        Arc::clone(item)
    })
}

// None indicates that idle task is currently running
// Be careful while calling these functions as they increment the strong count
// Once done with retreiving the info you want, it's important that you drop them
pub fn get_current_task() -> Option<KThread> {
    let cb = SCHEDULER_CON_BLK.local().lock().running_task;
    if cb.is_none() {
        return None;
    }
    
    
    Some(Arc::clone(unsafe { &**cb.unwrap().as_ptr() }))
}

// Use this, if you just want the id
pub fn get_current_task_id() -> Option<usize> {
    Some(get_current_task()?.lock().get_id())
}

pub fn yield_cpu() {
    // Remove all remaining run time
    get_current_task()
    .expect("yield_cpu() called from idle task!").lock().quanta = 0;

    hal::yield_cpu();
}

pub fn init() {
    let init_task = Task::new(false, 0)
    .expect("Init task creation failed!!");
    
    let init_proc = get_process_info(0).expect("Unable to locate init process!");
    
    TASKS.lock().insert(0, Arc::clone(&init_task));

    init_task.lock().status = TaskStatus::RUNNING;
    init_task.lock().panic_base = get_panic_base();
    init_task.lock().process = Some(init_proc);
    init_task.lock().vcb = Some(get_kernel_addr_space());

    {
        let mut sched_cb = SCHEDULER_CON_BLK.local().lock();
        sched_cb.active_tasks.add_node(init_task).expect("Init task creation failed!");

        let task = NonNull::from(sched_cb.active_tasks.first().unwrap());

        unsafe {
            let guard = sched_cb.active_tasks.remove_node(task);
            sched_cb.running_task = Some(ListNode::into_inner(guard));
        }
    }

    // Now init idle task stack for all cpus
    for core in 0..get_total_cores() {
        let stack_base = get_worker_stack(core);
        let mut sched_cb = unsafe {
            SCHEDULER_CON_BLK.get(core).lock()
        };

        // We need to create separate stack for idle task on cpu 0, since the current stack is used by init task
        if core == 0 {
            let stack = Stack::into_inner(&mut Stack::new().expect("Could not create worker stack for cpu 0"));
            sched_cb.idle_task_stack = stack; 
        }
        else {
            sched_cb.idle_task_stack = NonNull::new(stack_base as *mut u8).unwrap();
        }
    }
    
    info!("Created init task 0");
    enable_scheduler_timer();
}

// Set task to waiting and add the timer atomically
pub fn add_cur_task_to_wait_queue_with_timer(wait_semaphore: KSemInnerType, timer: KTimerInnerType) -> bool {
    let mut sched_cb = SCHEDULER_CON_BLK.local().lock();
    let cb = sched_cb.running_task;
    if cb.is_none() {
        panic!("add_cur_task_to_wait_queue_with_timer() called from idle task!!");
    }
    
    let cur_task = unsafe { &**cb.unwrap().as_ptr() };
    let mut task = cur_task.lock();
    
    // TERMINATED > WAITING, don't do anything
    if task.status == TaskStatus::TERMINATED {
        return false;
    }
    
    let res = sched_cb.timer_list.add_node(timer);
    if res.is_err() {
        return false;
    }

    let res = task.wait_semaphores.add_node(wait_semaphore);
    if res.is_err() {
        sched_cb.timer_list.pop_node();
        return false;
    }

    task.status = TaskStatus::WAITING;
    true
}

pub fn add_cur_task_to_wait_queue(wait_semaphore: KSemInnerType) -> bool {
    let sched_cb = SCHEDULER_CON_BLK.local().lock();
    let cb = sched_cb.running_task;
    if cb.is_none() {
        panic!("add_cur_task_to_wait_queue() called from idle task!!");
    }
    
    let cur_task = unsafe { &**cb.unwrap().as_ptr() };

    let mut task = cur_task.lock();
    
    // TERMINATED > WAITING, don't do anything
    if task.status == TaskStatus::TERMINATED {
        return false;
    }

    let res = task.wait_semaphores.add_node(wait_semaphore);
    if res.is_err() {
        return false;
    }

    task.status = TaskStatus::WAITING;
    true
}

fn remove_wait_semaphore(task: &mut Task, wait_semaphore: KSemInnerType) {
    let mut sem = None;
    let sem_val = (&*wait_semaphore) as *const _;

    for semaphore in task.wait_semaphores.iter() {
        let val = (&***semaphore) as *const _;

        if val == sem_val {
            sem = Some(NonNull::from(semaphore));
            break;
        } 
    }

    if sem.is_some() {
        unsafe {
            task.wait_semaphores.remove_node(sem.unwrap());
        }
    }
}


pub fn signal_waiting_task(task_id: usize, wait_semaphore: KSemInnerType) {
    let this_task = get_task_info(task_id);

    // It could be that this task has been killed
    if this_task.is_none() {
        return;
    }

    let this_task = this_task.unwrap();
    let core = this_task.lock().core;
    let mut skip_notify = false;
    disable_preemption();
    {
        let mut sched_cb = unsafe {
            SCHEDULER_CON_BLK.get(core).lock() 
        };

        let status = this_task.lock().status;
        match status {
            TaskStatus::WAITING => {
                let mut waiting_task = None;
                for task in sched_cb.waiting_tasks.iter() {
                    if task.lock().get_id() == task_id {
                        waiting_task = Some(NonNull::from(task));
                        break;
                    }
                }
                
                let mut task = this_task.lock();
                // This happens when signal task is called even before the waiting task gets a chance to be put into the wait queue
                if waiting_task.is_none() {
                    // Let task run again with high priority
                    task.status = TaskStatus::RUNNING;
                    task.quanta = INIT_QUANTA;
                }
                else {
                    let signal_task = unsafe {
                        ListNode::into_inner(sched_cb.waiting_tasks.remove_node(waiting_task.unwrap()))
                    };
                    
                    sched_cb.active_tasks.insert_node_at_head(signal_task);
                    task.status = TaskStatus::ACTIVE;
                }

                remove_wait_semaphore(&mut *task, wait_semaphore);
            },

            TaskStatus::TERMINATED => {
                skip_notify = true;
            },

            TaskStatus::ACTIVE | TaskStatus::RUNNING => {
                panic!("Signalled task {} which was in ACTIVE/RUNNING state??", task_id);
            }
        }
    }
    
    if !skip_notify {
        notify_other_cpu(core);
    }

    enable_preemption();
}


// Killing a thread is an unsafe process in general
// This procedure must be called in a coordinated manner, otherwise it simply
// destroys a task/process asynchronously. Most of the time it's fine. However this
// could lead to memory leaks. A task could have a heap reference Arc pointer on it's stack.
// If task is killed at this point, the destructor is never run and the memory is leaked
// Note that there is no stack unwinding destructor calls to avoid this problem within the kernel
// Doing stack unwinding for every process/task destruction is not practical and can cause lot
// of bookkeeping and performance issues
pub fn kill_thread(task_id: usize) {
    let mut yield_flag = false;
    let mut drop_task  = false;
    let mut skip_notify  = false;
    let this_task = get_task_info(task_id);

    if this_task.is_none() {
        return;
    }

    let this_task  = this_task.unwrap();
    let core = this_task.lock().core;
    
    disable_preemption();

    assert!(task_id != 0, "Attempted to kill init task!!!");
    {
        let mut sched_cb = unsafe {
            SCHEDULER_CON_BLK.get(core).lock()
        };

        let status = {
            let mut task_locked= this_task.lock();
            let status = task_locked.status;
            task_locked.status = TaskStatus::TERMINATED;
            status 
        };

        // Remove task from active list and add to terminated list
        match status {
            TaskStatus::ACTIVE => {
                let mut task_l = None;
                for active_task in sched_cb.active_tasks.iter() {
                    if active_task.lock().id == task_id {
                        task_l = Some(NonNull::from(active_task));
                        break;
                    }
                }

                debug!("Killing active task..");
                assert!(task_l.is_some());
                let task_node = unsafe {
                    ListNode::into_inner(sched_cb.active_tasks.remove_node(task_l.unwrap()))
                };

                sched_cb.terminated_tasks.insert_node_at_tail(task_node);
            },

            TaskStatus::WAITING => {
                let mut task_l = None;
                for waiting_task in sched_cb.waiting_tasks.iter() {
                    if waiting_task.lock().id == task_id {
                        task_l = Some(NonNull::from(waiting_task));
                        break;
                    }
                }
                
                debug!("Killing waiting task..");

                if task_l.is_some() {
                    let task_node = unsafe {
                        ListNode::into_inner(sched_cb.waiting_tasks.remove_node(task_l.unwrap()))
                    };

                    sched_cb.terminated_tasks.insert_node_at_tail(task_node);

                }
                else {
                    // Task might not have been scheduled out
                    // In this case, let scheduler take care of it

                    // We might be here due to interrupt in same cpu / kill task issued by different cpu
                }

                drop_task = true;
            },

            TaskStatus::RUNNING => {
                // The current task is killing itself (exit)
                
                // Since the task is currently running, we can't immediately drop it as it is using this stack
                // So, delay the stack destruction
                
                // Stack is guaranteed to be present. Only init task has None value here
                let stack = take(this_task.lock().stack.as_mut().unwrap());
                
                info!("Killing running task");
                sched_cb.leftover_stack.add_node(stack).expect("Unable to add stack node to leftover_stack list!");
                sched_cb.flip_flop = true;
                
                // Only yield if the current task is killing itself (i.e It's not just that a task from another cpu is killing the 
                // current running task of this cpu)
                yield_flag = hal::get_core() == core;
                if yield_flag {
                    debug!("Self yielding");
                }
            },

            TaskStatus::TERMINATED => {
                debug!("Task {} already terminated..", task_id);
                skip_notify = true;
            }
        }
    }

    if !skip_notify {
        notify_other_cpu(core);
    }

    // Inform semaphore that this task is about to be killed, remove it from the blocked list
    if drop_task {
        while this_task.lock().wait_semaphores.get_nodes() != 0 {
            // We do it in this weird fashion since we don't want the task to be locked during call to drop_task
            let (sem_wrap, sem_inner) = {
                let task = this_task.lock();
                let sem = task.wait_semaphores.first();
                (Arc::clone(&**sem.unwrap()), NonNull::from(sem.unwrap()))
            };

            KSem::drop_task(sem_wrap, task_id);

            unsafe {
                this_task.lock().wait_semaphores.remove_node(sem_inner);
            }
        } 
    }

    if yield_flag {
        // Drop it explicitly since we won't return from here and rust thinks that 
        // this stack frame here is preserved, which means that this reference gets leaked
        drop(this_task);
    }

    enable_preemption();

    // The current running task is killed, yield remaining context
    if yield_flag {
        info!("Yielding task {}", task_id);
        yield_cpu();
    }
}

pub fn exit_thread() -> ! {
    let thread_id = get_current_task_id().expect("Attempted to kill idle task!!");

    kill_thread(thread_id);

    panic!("exit_thread() unreachable reached!!");
}

// We do all this moving out of stuff and into other stuff drama in order to avoid holding any lock during signal operation
fn reap_tasks(sched_cb: &mut TaskQueue) {
    while sched_cb.terminated_tasks.get_nodes() != 0 {
        let task = NonNull::from(sched_cb.terminated_tasks.first().unwrap());
        let task_inner = unsafe {
            &*task.as_ptr()
        };
        
        let id = task_inner.lock().get_id();

        sched_cb.notifier_list.add_node(task_inner.lock().term_notify.clone()).expect("Failed to add semaphore to notifier list!");

        // Extract the pointer, release the lock and then call remove_thread
        // Otherwise, we run the risk of deadlock
        let process_ref = task_inner.lock().process.as_ref().unwrap().clone();
        process_ref.lock().remove_thread(id);

        info!("Removing task {} on core {}", id, hal::get_core());
        unsafe {
            sched_cb.terminated_tasks.remove_node(task);
        }
        
        TASKS.lock().remove(&id);
    }   
}

fn update_timers(sched_cb: &mut TaskQueue) {
    let mut idx = 0;
    let list_size = sched_cb.timer_list.get_nodes();

    while idx < list_size {
        let timer = sched_cb.timer_list.first().unwrap();

        let is_done = timer.lock().update_timer_count(QUANTUM);

        if is_done {
            let sem = timer.lock().get_semaphore();
            sched_cb.notifier_list.add_node(sem).expect("Unable to add timer node semaphore into notifier list!");

            unsafe {
                sched_cb.timer_list.remove_node(NonNull::from(timer))
            };
        }
        else {
            let timer_ref = unsafe {
                ListNode::into_inner(sched_cb.timer_list.remove_node(NonNull::from(timer)))
            };
            
            sched_cb.timer_list.insert_node_at_tail(timer_ref);
        }

        idx += 1;
    }
}


fn notify_watchers(notifier_list: &DynList<KSem>) {
    for sem in notifier_list.iter() {
        sem.signal();
    }
}

pub fn disable_preemption() {
    let mut sched_cb = SCHEDULER_CON_BLK.local().lock();
    sched_cb.preemption_count += 1;
}

pub fn enable_preemption() {
    let mut sched_cb = SCHEDULER_CON_BLK.local().lock();
    sched_cb.preemption_count = sched_cb.preemption_count.saturating_sub(1);
}

fn can_sleep(sched_cb: &mut TaskQueue) -> bool {
    sched_cb.flip_flop == false && sched_cb.timer_list.get_nodes() == 0
}

#[inline]
fn switch_address_space(old_vcb: VCB, new_vcb: VCB) {
    if old_vcb != new_vcb {
        unsafe {
            set_address_space(new_vcb);
        }
    }
}

#[inline]
fn switch_address_space_for_idle(old_vcb: VCB) {
    let new_vcb = get_kernel_addr_space();

    switch_address_space(old_vcb, new_vcb);
}

#[inline]
fn switch_address_space_from_idle(new_vcb: VCB) {
    let old_vcb = get_kernel_addr_space();

    switch_address_space(old_vcb, new_vcb);
}


// Main scheduler loop
pub fn schedule() {
    let notifier_list = {
        let mut sched_cb = SCHEDULER_CON_BLK.local().lock();
        update_timers(&mut sched_cb);
        
        if sched_cb.preemption_count > 0 {
            return;
        }

        if sched_cb.running_task.is_some() {
            let current_task = sched_cb.running_task.unwrap(); 

            let mut task_info = unsafe {
                current_task.as_ref().lock()
            };

            task_info.quanta = task_info.quanta.saturating_sub(1);
            let old_vcb = task_info.vcb.expect("VCB is none");

            // Switch to new task
            if task_info.status == TaskStatus::WAITING || task_info.status == TaskStatus::TERMINATED ||
            task_info.quanta == 0 {
                // First choose new task
                // We create NonNull here so that the node can later be removed
                let head_task = sched_cb.active_tasks.first().and_then(|item| {
                    Some(NonNull::from(item))
                });

                if head_task.is_some() {
                    let mut head_task_info = unsafe {
                        head_task.unwrap().as_ref().lock()
                    };
                    
                    assert!(head_task_info.status == TaskStatus::ACTIVE); 
                    head_task_info.status = TaskStatus::RUNNING;
                    head_task_info.quanta = INIT_QUANTA;
                    let new_context = head_task_info.context;
                    let new_vcb = head_task_info.vcb.expect("VCB is none");

                    let prev_context = fetch_context();
                    task_info.context = prev_context;
                    if task_info.status == TaskStatus::RUNNING {
                        task_info.status = TaskStatus::ACTIVE; 
                    }

                    // This ensures that list doesn't delete the node. It simply removes it from the list 
                    let head_task = unsafe {
                        ListNode::into_inner(sched_cb.active_tasks.remove_node(head_task.unwrap()))
                    };

                    if task_info.status == TaskStatus::WAITING {
                        sched_cb.waiting_tasks.insert_node_at_tail(current_task);
                    }
                    else if task_info.status == TaskStatus::TERMINATED {
                        sched_cb.terminated_tasks.insert_node_at_tail(current_task);
                    }
                    else {
                        sched_cb.active_tasks.insert_node_at_tail(current_task);
                    }

                    sched_cb.running_task = Some(head_task);

                    switch_address_space(old_vcb, new_vcb);
                    set_panic_base(head_task_info.panic_base);
                    switch_context(new_context);
                }
                else {
                    if task_info.status != TaskStatus::RUNNING {
                        let prev_context = fetch_context();
                        task_info.context = prev_context;

                        if task_info.status == TaskStatus::WAITING {
                            sched_cb.waiting_tasks.insert_node_at_tail(current_task);
                        }
                        else if task_info.status == TaskStatus::TERMINATED {
                            debug!("Adding task {} to terminated list", task_info.id);
                            sched_cb.terminated_tasks.insert_node_at_tail(current_task);
                        }

                        prep_idle_task(&mut sched_cb, old_vcb);
                    }
                    else {
                        // No other task to run. Continue with this task
                        task_info.quanta = INIT_QUANTA;
                    }
                }
            }
        }
        else {
            // This means we're in idle task. Check and run any active tasks
            let head_task = sched_cb.active_tasks.first().and_then(|item| {
                Some(NonNull::from(item))
            });

            if head_task.is_some() {
                let mut head_task_info = unsafe {
                    head_task.unwrap().as_ref().lock()
                };

                assert!(head_task_info.status == TaskStatus::ACTIVE); 
                head_task_info.status = TaskStatus::RUNNING;
                head_task_info.quanta = INIT_QUANTA;
                let new_context = head_task_info.context;
                
                let head_task = unsafe {
                    ListNode::into_inner(sched_cb.active_tasks.remove_node(head_task.unwrap()))
                };
                sched_cb.running_task = Some(head_task);
                
                // Idle task uses the default kernel virtual address space
                let new_vcb = head_task_info.vcb.unwrap();

                switch_address_space_from_idle(new_vcb);
                set_panic_base(head_task_info.panic_base);
                switch_context(new_context);
            }
            else {
                // If stack deletions / timers are pending, don't go into idle task yet
                if can_sleep(&mut sched_cb) {
                    debug!("Disabling timer on core {}", hal::get_core());
                    disable_scheduler_timer(); 
                }
            }
        }

        // We do the flip flop technique since we are still using the same terminated task stack at this point
        // But we won't be using it from the next schedule on
        if sched_cb.flip_flop {
            sched_cb.flip_flop = false;
        }
        else {
            // Any delayed stack deletions can be done here, since we have switched from that stack
            // Debug-safety check: ensure we are not freeing a stack which currently contains
            // the saved CPU context. This guards against use-after-free of active contexts.
            #[cfg(debug_assertions)]
            {
                let cur_ctx = crate::hal::fetch_context() as usize;
                for st in sched_cb.leftover_stack.iter() {
                    let alloc_base = st.get_alloc_base();
                    let top = st.get_stack_base();
                    assert!(cur_ctx < alloc_base || cur_ctx >= top, "Attempting to destroy a stack that contains the current CPU context!");
                }
            }
            sched_cb.leftover_stack.clear();
        }

        reap_tasks(&mut sched_cb);
        take(&mut sched_cb.notifier_list)
    };

    notify_watchers(&notifier_list);
}

fn prep_idle_task(sched_cb: &mut TaskQueue, old_vcb: VCB) {
    sched_cb.running_task = None;
    let context = create_kernel_context(idle_task, sched_cb.idle_task_stack.as_ptr() as *mut u8);
    
    switch_address_space_for_idle(old_vcb);
    set_panic_base(sched_cb.idle_task_stack.as_ptr() as usize);
    switch_context(context);
    
    if can_sleep(sched_cb) {
        disable_scheduler_timer(); 
    }
}

fn idle_task() -> ! {
    hal::sleep();
}

fn notify_other_cpu(target_core: usize) {
    if hal::get_core() == target_core {
        enable_scheduler_timer();
        return;
    }

    let _ = hal::notify_core(IPIRequestType::SchedChange, target_core);
}

fn create_thread_common(handler: fn() -> !) -> Result<(KThread, usize), KError> {
    // We will use simple round robin to determine the cpu which gets this task
    let core = TASK_CPU.fetch_add(1, Ordering::Relaxed) as usize % get_total_cores();   
    let task = Task::new(true, core)?;
    
    {
        let mut task = task.lock();
        let stack_base = task.stack.as_ref().unwrap().get_stack_base();

        // Setup the initial context
        let context = create_kernel_context(handler, stack_base as *mut u8);
        task.context = context;  
        task.panic_base = stack_base;
    }

    Ok((task, core))
}

// Internal API: Do not call this
pub fn create_init_thread(handler: fn() -> !, process: KProcess) -> Result<KThread, KError> {
    let (thread, core) = create_thread_common(handler)?;
    let thread_id = thread.lock().get_id();
    let proc_id = process.lock().get_id();
    let proc_addr_space = process.lock().get_vcb();
    debug!("Created init thread {} on process {} on core {}", thread_id, proc_id, core);
    
    thread.lock().process = Some(process);
    thread.lock().vcb = Some(proc_addr_space);
    Ok(thread)
}

pub fn start_task(thread: &KThread, core: usize, process: &KProcess, registry: &Spinlock<BTreeMap<usize, KProcess>>) -> Result<(), KError> {
    {
        let mut sched_cb = unsafe {
            SCHEDULER_CON_BLK.get(core).lock()
        };
        
        let thread_id = thread.lock().get_id();
        // Add to ready queue
        sched_cb.active_tasks.add_node(Arc::clone(&thread))?;

        let mut process_inner = process.lock();
        let proc_id = process_inner.get_id();
        process_inner.attach_thread_to_current_process(thread_id)?;
        registry.lock().insert(proc_id, Arc::clone(&process));

        TASKS.lock().insert(thread_id, Arc::clone(&thread));
    }
    notify_other_cpu(core);

    Ok(())
}

// Must be called from valid process context 
pub fn create_thread(handler: fn() -> !) -> Result<KThread, KError> {
    disable_preemption();
    let (thread, core) = create_thread_common(handler)?;
    let thread_id = thread.lock().get_id();
    let cur_process = get_current_process();

    // Lock order => Scheduler -> Process -> Task
    {
        let mut sched_cb = unsafe {
            SCHEDULER_CON_BLK.get(core).lock()
        };

        if let Some(process) = cur_process {
            let process_ref = Arc::clone(&process);
            let mut guard = process.lock();
            let proc_addr_space = guard.get_vcb();
            if guard.get_status() == ProcessStatus::Terminated {
                return Err(KError::ProcessTerminated);
            }

            debug!("Creating thread id {} under process id {} on core {}", thread_id, guard.get_id(), core);

            guard.attach_thread_to_current_process(thread_id)?;
            thread.lock().process = Some(process_ref);
            thread.lock().vcb = Some(proc_addr_space);
        }
        else {
            panic!("create_thread() called from idle task!!");
        }
        
        sched_cb.active_tasks.add_node(Arc::clone(&thread))?;
        TASKS.lock().insert(thread_id, Arc::clone(&thread));
    }

    notify_other_cpu(core);
    enable_preemption();

    Ok(thread)
}

impl Spinlock<Task> {
    pub fn wait(&self) -> Result<(), KError> {
        let sem = {
            let task = self.lock();
            task.term_notify.clone() 
        };

        sem.wait()
    }
}