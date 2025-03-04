use core::fmt::Debug;
use core::sync::atomic::{ AtomicI32, AtomicU32, Ordering };
use core::mem::forget;
use core::ops::{Drop, Deref, DerefMut};
use std::cmp::min;
use std::sync::RwLock;
use super::worker::*;
use crate::scheduler::Task as TaskTrait;
use crate::scheduler::TaskObject as TaskObjectTrait;
use crate::scheduler::LoopArguments as LoopArgumentsTrait;

pub const ATOMICS_SIZE: usize = 64;
pub struct Task (*mut TaskObject<()>);

impl TaskTrait for Task {
  type Workers<'b> = Workers<'b>;
  type TaskObject<T: Send + Sync> = TaskObject<T>;
  type LoopArguments<'c> = LoopArguments<'c>;

  fn new_dataparallel<T: Send + Sync>(
    work: for <'a, 'b, 'c> fn(workers: &'a Self::Workers<'b>, data: *const Self::TaskObject<T>, loop_arguments: Self::LoopArguments<'c>) -> (),
    finish: for <'a, 'b> fn(workers: &'a Self::Workers<'b>, data: *mut Self::TaskObject<T>) -> (),
    data: T,
    work_size: u32
  ) -> Task {
    Task::new_dataparallel(work, finish, data, work_size)
  }

  fn new_single<T: Send + Sync>(
    function: for <'a, 'b> fn(workers: &'a Self::Workers<'b>, data: *mut Self::TaskObject<T>) -> (),
    data: T
  ) -> Task {
    Task::new_single(function, data)
  }

  fn work_loop<'a, F: FnMut(u32)>(
      loop_arguments: Self::LoopArguments<'a>,
      mut work: F,
    ) {
      // Claim work
    let mut chunk_idx;
    let mut loop_arguments = loop_arguments;

    let atomics_length = loop_arguments.work_size.len();

    for _ in 0..atomics_length {
      let current_index = loop_arguments.work_indexes_index.fetch_add(1, Ordering::Relaxed) as usize % atomics_length;

      chunk_idx = loop_arguments.work_indexes.read().unwrap()[current_index].fetch_add(1, Ordering::Relaxed);
      while chunk_idx < loop_arguments.work_size[current_index] {
      //   println!("workassisting_loop: {:?}", loop_arguments);

      //   println!("Doing work: {:?}", chunk_idx);

        // Copy chunk_index to an immutable variable, such that a user of this macro cannot mutate it.
        let chunk_index = chunk_idx;
        work(chunk_index);

        chunk_idx = loop_arguments.work_indexes.read().unwrap()[current_index].fetch_add(1, Ordering::Relaxed);
      }
    }
    loop_arguments.empty_signal.task_empty();
  }
}

#[repr(C)]
pub struct TaskObject<T> {
  // 'work' borrows the TaskObject
  pub(super) work: Option<fn(workers: &Workers, this: *const TaskObject<T>, loop_arguments: LoopArguments) -> ()>,
  // 'finish' takes ownership of the TaskObject
  pub(super) finish: fn(workers: &Workers, this: *mut TaskObject<T>) -> (),
  // The number of active_threads, offset by the tag in the activities array.
  // If this task is present in activities, then:
  //   - active_threads contains - (the number of finished threads), thus non-positive.
  //   - the tag in activities (in AtomicTaggedPtr) contains the number of threads that have started working on this task
  // When a thread removes this task from activities, it will assure that:
  //   - active_threads contains the number of active threads, thus is non-negative
  // When active_threads becomes zero after a decrement:
  //   - the task is not present in activities.
  //   - no thread is still working on this task.
  // Hence we can run the finish function and deallocate the task.
  pub(super) active_threads: AtomicI32,
  pub(super) work_indexes: RwLock<Vec<AtomicU32>>,
  pub(super) work_indexes_index: AtomicU32,
  pub(super) work_size: Vec<u32>,
  pub data: T,
}

impl<T: Send + Sync> TaskObjectTrait<T> for TaskObject<T> {
  unsafe fn get_data<'a>(task: *const TaskObject<T>) -> &'a T {
    TaskObject::get_data(task)
  }

  unsafe fn take_data<'a>(task: *mut TaskObject<T>) -> T {
    TaskObject::take_data(task)
  }
}

impl Debug for Task {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    let obj = unsafe { &*self.0 };
    obj.fmt(f)
  }
}

impl<T> Debug for TaskObject<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    let work = self.work.map(|f| f as *const ());
    write!(f, "Task:\n  work {:?}\n  finish {:?}\n size {:?}\n index {:?}\n active threads {:?}", work, self.finish as *const (), self.work_size, self.work_indexes, self.active_threads)
  }
}


// Distribute x over n elements, such that the sum of the elements is x.
fn distribute(x: u32, n: usize) -> Vec<u32> {
  let mut result = vec![x / n as u32; n];
  let remainder = (x % n as u32) as usize;

  for i in 0..remainder {
      result[i] += 1;
  }
  result
}

impl Task {
  pub fn new_dataparallel<T: Send + Sync>(
    work: fn(workers: &Workers, data: *const TaskObject<T>, loop_arguments: LoopArguments) -> (),
    finish: fn(workers: &Workers, data: *mut TaskObject<T>) -> (),
    data: T,
    work_size: u32
  ) -> Task {
    let atomics = min(ATOMICS_SIZE, work_size as usize);
    let mut work_size = distribute(work_size, atomics);

    let mut index = 0;
    let work_indexes: Vec<AtomicU32> = (0..atomics).map(|i| {
      let result = AtomicU32::new(index);
      index += work_size[i];
      work_size[i] = index;
      result
    }).collect();

    let task_box: Box<TaskObject<T>> = Box::new(TaskObject{
      work: Some(work),
      finish,
      work_size,
      active_threads: AtomicI32::new(0),
      work_indexes: RwLock::new(work_indexes),
      work_indexes_index: AtomicU32::new(0),
      data
    });
    Task(Box::into_raw(task_box) as *mut TaskObject<()>)
  }

  pub fn new_single<T: Send + Sync>(
    function: fn(workers: &Workers, data: *mut TaskObject<T>) -> (),
    data: T
  ) -> Task {
    let task_box: Box<TaskObject<T>> = Box::new(TaskObject{
      work: None,
      finish: function,
      work_size: vec![],
      active_threads: AtomicI32::new(0),
      work_indexes: RwLock::new(vec![]),
      work_indexes_index: AtomicU32::new(0),
      data
    });
    Task(Box::into_raw(task_box) as *mut TaskObject<()>)
  }

  // The caller should assure that the object is properly deallocated.
  // This can be done by calling Task::from_raw.
  pub fn into_raw(self) -> *mut TaskObject<()> {
    let ptr = self.0;
    forget(self); // Don't run drop() on self, as that would deallocate the TaskObject
    ptr
  }
}

unsafe impl Send for Task {}
unsafe impl Sync for Task {}

impl Drop for Task {
  fn drop(&mut self) {
    // We cannot drop the TaskObject<T> here, as we don't know the type argument T here.
    // We assume that the TaskObject is passed to Workers; that will handle the deallocation of the TaskObject.
    println!("Warning: TaskObject not cleared. Make sure that all constructed Tasks are also passed to Workers.");
  }
}

impl Deref for Task {
  type Target = TaskObject<()>;

  fn deref(&self) -> &Self::Target {
    unsafe { &*self.0 }
  }
}

impl DerefMut for Task {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { &mut *self.0 }
  }
}

impl<T> TaskObject<T> {
  // Safety: caller should guarantee that the TaskObject outlives lifetime 'a.
  pub unsafe fn get_data<'a>(task: *const TaskObject<T>) -> &'a T {
    unsafe { &(*task).data }
  }

  pub unsafe fn take_data<'a>(task: *mut TaskObject<T>) -> T {
    unsafe { Box::from_raw(task) }.data
  }
}

pub struct LoopArguments<'a> {
  pub work_size: &'a Vec<u32>,
  pub work_indexes: &'a RwLock<Vec<AtomicU32>>,
  pub work_indexes_index: &'a AtomicU32,
  pub empty_signal: EmptySignal<'a>,
}

impl Debug for LoopArguments<'_> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    write!(f, "LoopArguments:\n  work_size {:?}\n  work_indexes {:?}", self.work_size, self.work_indexes)
  }
}

impl<'a> LoopArgumentsTrait<'a> for LoopArguments<'a> {}
