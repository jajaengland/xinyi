// Task context for x86
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Context {
    pub edi: u32,
    pub esi: u32,
    pub ebx: u32,
    pub ebp: u32,
    pub eip: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum TaskState {
    Running,
    Ready,
    Sleeping,
    Zombie,
}

pub struct Task {
    pub id: u32,
    pub state: TaskState,
    pub page_directory: u32,
    pub kernel_stack: u32,
    pub user_stack: u32,
    pub esp: u32,
    pub context: Context,
    _address_space: crate::paging::AddressSpace,
    program: crate::loader::LoadedProgram,
    _kernel_frames: crate::loader::Frames,
}

const MAX_TASKS: usize = 16;

static mut TASKS: [Option<Task>; MAX_TASKS] = [const { None }; MAX_TASKS];
static mut CURRENT_TASK: i32 = -1;
static mut NEXT_TASK_ID: u32 = 1;

pub fn init() {
    unsafe {
        for task in TASKS.iter_mut().take(MAX_TASKS) {
            *task = None;
        }
        CURRENT_TASK = -1;
        NEXT_TASK_ID = 1;
    }
}

pub fn create_task(program: crate::loader::LoadedProgram) -> Option<u32> {
    if task_count() == MAX_TASKS {
        return None;
    }
    let task_id = unsafe { NEXT_TASK_ID };
    let next_id = task_id.checked_add(1)?;

    // Allocate kernel stack from physical memory
    let kernel_frames = crate::loader::Frames::new(crate::loader::KERNEL_STACK_PAGES)?;
    let kernel_stack = kernel_frames.top();
    let address_space = crate::paging::AddressSpace::new(&program)?;
    let entry_point = crate::paging::USER_BASE;
    let user_stack = crate::paging::USER_STACK_TOP - 4;

    let task = Task {
        id: task_id,
        state: TaskState::Ready,
        page_directory: address_space.directory(),
        kernel_stack,
        user_stack,
        esp: kernel_stack,
        context: Context {
            edi: 0,
            esi: 0,
            ebx: 0,
            ebp: 0,
            eip: entry_point,
        },
        _address_space: address_space,
        program,
        _kernel_frames: kernel_frames,
    };

    unsafe {
        for (i, slot) in TASKS.iter_mut().enumerate().take(MAX_TASKS) {
            if slot.is_none() {
                *slot = Some(task);
                NEXT_TASK_ID = next_id;
                if CURRENT_TASK == -1 {
                    CURRENT_TASK = i as i32;
                    if let Some(t) = TASKS[i].as_mut() {
                        t.state = TaskState::Running;
                    }
                }
                return Some(task_id);
            }
        }
    }

    None
}

pub fn describe(id: u32) -> Option<(u32, u32, u32, u32, u32)> {
    unsafe {
        TASKS
            .iter()
            .flatten()
            .find(|task| task.id == id)
            .map(|task| {
                (
                    task.program.image.base(),
                    task.program.size,
                    task.program.stack.top() - 4,
                    task.kernel_stack,
                    task.page_directory,
                )
            })
    }
}
pub fn execute(id: u32) -> bool {
    let task = unsafe {
        let slots = &mut *core::ptr::addr_of_mut!(TASKS);
        let Some(index) = slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|task| task.id == id))
        else {
            return false;
        };
        let task = slots[index].take().unwrap();
        if CURRENT_TASK == index as i32 {
            CURRENT_TASK = slots
                .iter()
                .position(Option::is_some)
                .map_or(-1, |index| index as i32);
        }
        task
    };
    let Task {
        program,
        _address_space,
        _kernel_frames,
        ..
    } = task;
    crate::usermode::execute(program, _address_space, _kernel_frames, id, 0);
    true
}

// Only paused records remain in the table; execution consumes a record.
pub fn remove(id: u32) -> bool {
    unsafe {
        for index in 0..MAX_TASKS {
            if TASKS[index].as_ref().is_some_and(|task| task.id == id) {
                if TASKS[index].as_ref().unwrap().page_directory
                    == crate::paging::status().page_directory
                {
                    return false;
                }
                TASKS[index] = None;
                if CURRENT_TASK == index as i32 {
                    CURRENT_TASK = TASKS
                        .iter()
                        .position(Option::is_some)
                        .map_or(-1, |i| i as i32);
                    if CURRENT_TASK >= 0 {
                        TASKS[CURRENT_TASK as usize].as_mut().unwrap().state = TaskState::Running;
                    }
                }
                return true;
            }
        }
    }
    false
}

pub fn current_task_id() -> Option<u32> {
    unsafe {
        if CURRENT_TASK >= 0 {
            TASKS[CURRENT_TASK as usize].as_ref().map(|t| t.id)
        } else {
            None
        }
    }
}

pub fn task_count() -> usize {
    unsafe { TASKS.iter().filter(|t| t.is_some()).count() }
}

pub fn schedule() {
    unsafe {
        if CURRENT_TASK < 0 {
            return;
        }
        let current = CURRENT_TASK as usize;
        // Find next ready task
        let mut next_idx = (current + 1) % MAX_TASKS;
        loop {
            if next_idx == current {
                break;
            }
            if let Some(ref task) = TASKS[next_idx] {
                if matches!(task.state, TaskState::Ready) {
                    // Switch tasks
                    if let Some(ref mut cur) = TASKS[current] {
                        if matches!(cur.state, TaskState::Running) {
                            cur.state = TaskState::Ready;
                        }
                    }
                    TASKS[next_idx].as_mut().unwrap().state = TaskState::Running;
                    CURRENT_TASK = next_idx as i32;
                    break;
                }
            }
            next_idx = (next_idx + 1) % MAX_TASKS;
        }
    }
}

pub(crate) fn reserve_id()->Option<u32> {unsafe {let id=NEXT_TASK_ID;NEXT_TASK_ID=id.checked_add(1)?;Some(id)}}
