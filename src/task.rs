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

#[derive(Clone, Copy)]
pub struct Task {
    pub id: u32,
    pub state: TaskState,
    pub page_directory: u32,
    pub kernel_stack: u32,
    pub user_stack: u32,
    pub esp: u32,
    pub context: Context,
}

const MAX_TASKS: usize = 16;

static mut TASKS: [Option<Task>; MAX_TASKS] = [None; MAX_TASKS];
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

pub fn create_task(entry_point: u32, user_stack: u32) -> Option<u32> {
    let task_id = unsafe { NEXT_TASK_ID };
    unsafe { NEXT_TASK_ID += 1; }

    // Allocate kernel stack from physical memory
    let kernel_stack_phys = crate::phys::alloc_frame()? as u32;
    let kernel_stack = kernel_stack_phys + 4096; // top of stack

    let task = Task {
        id: task_id,
        state: TaskState::Ready,
        page_directory: crate::paging::status().page_directory,
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
    };

    unsafe {
        for (i, slot) in TASKS.iter_mut().enumerate().take(MAX_TASKS) {
            if slot.is_none() {
                *slot = Some(task);
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
    unsafe {
        TASKS.iter().filter(|t| t.is_some()).count()
    }
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
