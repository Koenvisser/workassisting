use core::fmt::Debug;
use core::sync::atomic::{ AtomicI32, AtomicU32, Ordering };
use core::mem::forget;
use core::ops::{Drop, Deref, DerefMut};
use std::cmp::{max, min};
use crossbeam::utils::CachePadded;

use super::worker::*;
use crate::scheduler::Task as TaskTrait;
use crate::scheduler::TaskObject as TaskObjectTrait;
use crate::scheduler::LoopArguments as LoopArgumentsTrait;

pub struct Task<const ATOMICS: usize, const MIN_CHUNKS: usize> (*mut TaskObject<(), ATOMICS, MIN_CHUNKS>);

impl<const ATOMICS: usize, const MIN_CHUNKS: usize> TaskTrait for Task<ATOMICS, MIN_CHUNKS> {
  type Workers<'b> = Workers<'b, ATOMICS, MIN_CHUNKS>;
  type TaskObject<T: Send + Sync> = TaskObject<T, ATOMICS, MIN_CHUNKS>;
  type LoopArguments<'c> = LoopArguments<'c, ATOMICS, MIN_CHUNKS>;

  fn new_dataparallel<T: Send + Sync>(
    work: for <'a, 'b, 'c> fn(workers: &'a Self::Workers<'b>, data: *const Self::TaskObject<T>, loop_arguments: Self::LoopArguments<'c>) -> (),
    finish: for <'a, 'b> fn(workers: &'a Self::Workers<'b>, data: *mut Self::TaskObject<T>) -> (),
    data: T,
    work_size: u32
  ) -> Task<ATOMICS, MIN_CHUNKS> {
    Task::new_dataparallel(work, finish, data, work_size)
  }

  fn new_single<T: Send + Sync>(
    function: for <'a, 'b> fn(workers: &'a Self::Workers<'b>, data: *mut Self::TaskObject<T>) -> (),
    data: T
  ) -> Task<ATOMICS, MIN_CHUNKS> {
    Task::new_single(function, data)
  }
  
  #[inline(always)]
  fn work_loop<'a, F: FnMut(u32)>(
      loop_arguments: Self::LoopArguments<'a>,
      mut work: F,
    ) {
      // Claim work
    let mut chunk_idx;
    let mut loop_arguments = loop_arguments;

    let atomics_length = loop_arguments.work_size.len();
    
    // To make sure all atomics are finished, we need to loop over all atomics.
    for _ in 0..atomics_length {
      // Next atomic to work on
      let current_index = loop_arguments.work_indexes_index.fetch_add(1, Ordering::Relaxed) as usize % atomics_length;

      // Get the chunk index from the atomic
      chunk_idx = loop_arguments.work_indexes[current_index].fetch_add(1, Ordering::Relaxed);
      // Keep working on the atomic until all chunks are done
      while chunk_idx < loop_arguments.work_size[current_index] {
        let chunk_index = chunk_idx;
        work(chunk_index);

        chunk_idx = loop_arguments.work_indexes[current_index].fetch_add(1, Ordering::Relaxed);
      }
    }
    
    loop_arguments.empty_signal.task_empty();
  }
}

#[repr(C)]
pub struct TaskObject<T, const ATOMICS: usize, const MIN_CHUNKS: usize> {
  // 'work' borrows the TaskObject
  pub(super) work: Option<fn(workers: &Workers<ATOMICS, MIN_CHUNKS>, this: *const TaskObject<T, ATOMICS, MIN_CHUNKS>, loop_arguments: LoopArguments<ATOMICS, MIN_CHUNKS>) -> ()>,
  // 'finish' takes ownership of the TaskObject
  pub(super) finish: fn(workers: &Workers<ATOMICS, MIN_CHUNKS>, this: *mut TaskObject<T, ATOMICS, MIN_CHUNKS>) -> (),
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
  pub(super) work_indexes: Vec<CachePadded<AtomicU32>>,
  pub(super) work_indexes_index: CachePadded<AtomicU32>,
  pub(super) work_size: Vec<u32>,
  pub data: T,
}

impl<T: Send + Sync, const ATOMICS: usize, const MIN_CHUNKS: usize> TaskObjectTrait<T> for TaskObject<T, ATOMICS, MIN_CHUNKS> {
  unsafe fn get_data<'a>(task: *const TaskObject<T, ATOMICS, MIN_CHUNKS>) -> &'a T {
    TaskObject::get_data(task)
  }

  unsafe fn take_data<'a>(task: *mut TaskObject<T, ATOMICS, MIN_CHUNKS>) -> T {
    TaskObject::take_data(task)
  }
}

impl<const ATOMICS: usize, const MIN_CHUNKS: usize> Debug for Task<ATOMICS, MIN_CHUNKS> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    let obj = unsafe { &*self.0 };
    obj.fmt(f)
  }
}

impl<T, const ATOMICS: usize, const MIN_CHUNKS: usize> Debug for TaskObject<T, ATOMICS, MIN_CHUNKS> {
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

impl<const ATOMICS: usize, const MIN_CHUNKS: usize> Task<ATOMICS, MIN_CHUNKS> {
  pub fn new_dataparallel<T: Send + Sync>(
    work: fn(workers: &Workers<ATOMICS, MIN_CHUNKS>, data: *const TaskObject<T, ATOMICS, MIN_CHUNKS>, loop_arguments: LoopArguments<ATOMICS, MIN_CHUNKS>) -> (),
    finish: fn(workers: &Workers<ATOMICS, MIN_CHUNKS>, data: *mut TaskObject<T, ATOMICS, MIN_CHUNKS>) -> (),
    data: T,
    work_size: u32
  ) -> Task<ATOMICS, MIN_CHUNKS> {
    let atomics = max(1, min(ATOMICS, (work_size / MIN_CHUNKS as u32) as usize));
    let mut work_size = distribute(work_size, atomics);

    let mut index = 0;
    // Distribute the work over the atomics, each atomic starts at the end of the previous atomic.
    let work_indexes: Vec<CachePadded<AtomicU32>> = (0..atomics).map(|i| {
      let result = AtomicU32::new(index).into();
      index += work_size[i];
      work_size[i] = index;
      result
    }).collect();

    let task_box: Box<TaskObject<T, ATOMICS, MIN_CHUNKS>> = Box::new(TaskObject{
      work: Some(work),
      finish,
      work_size,
      active_threads: AtomicI32::new(0),
      work_indexes,
      work_indexes_index: AtomicU32::new(0).into(),
      data
    });
    Task(Box::into_raw(task_box) as *mut TaskObject<(), ATOMICS, MIN_CHUNKS>)
  }

  pub fn new_single<T: Send + Sync>(
    function: fn(workers: &Workers<ATOMICS, MIN_CHUNKS>, data: *mut TaskObject<T, ATOMICS, MIN_CHUNKS>) -> (),
    data: T
  ) -> Task<ATOMICS, MIN_CHUNKS> {
    // The work_size is empty, as there is no work to distribute.
    let task_box: Box<TaskObject<T, ATOMICS, MIN_CHUNKS>> = Box::new(TaskObject{
      work: None,
      finish: function,
      work_size: vec![],
      active_threads: AtomicI32::new(0),
      work_indexes: vec![],
      work_indexes_index: AtomicU32::new(0).into(),
      data
    });
    Task(Box::into_raw(task_box) as *mut TaskObject<(), ATOMICS, MIN_CHUNKS>)
  }

  // The caller should assure that the object is properly deallocated.
  // This can be done by calling Task::from_raw.
  pub fn into_raw(self) -> *mut TaskObject<(), ATOMICS, MIN_CHUNKS> {
    let ptr = self.0;
    forget(self); // Don't run drop() on self, as that would deallocate the TaskObject
    ptr
  }
}

unsafe impl<const ATOMICS: usize, const MIN_CHUNKS: usize> Send for Task<ATOMICS, MIN_CHUNKS> {}
unsafe impl<const ATOMICS: usize, const MIN_CHUNKS: usize> Sync for Task<ATOMICS, MIN_CHUNKS> {}

impl<const ATOMICS: usize, const MIN_CHUNKS: usize> Drop for Task<ATOMICS, MIN_CHUNKS> {
  fn drop(&mut self) {
    // We cannot drop the TaskObject<T> here, as we don't know the type argument T here.
    // We assume that the TaskObject is passed to Workers; that will handle the deallocation of the TaskObject.
    println!("Warning: TaskObject not cleared. Make sure that all constructed Tasks are also passed to Workers.");
  }
}

impl<const ATOMICS: usize, const MIN_CHUNKS: usize> Deref for Task<ATOMICS, MIN_CHUNKS> {
  type Target = TaskObject<(), ATOMICS, MIN_CHUNKS>;

  fn deref(&self) -> &Self::Target {
    unsafe { &*self.0 }
  }
}

impl<const ATOMICS: usize, const MIN_CHUNKS: usize> DerefMut for Task<ATOMICS, MIN_CHUNKS> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { &mut *self.0 }
  }
}

impl<T, const ATOMICS: usize, const MIN_CHUNKS: usize> TaskObject<T, ATOMICS, MIN_CHUNKS> {
  // Safety: caller should guarantee that the TaskObject outlives lifetime 'a.
  pub unsafe fn get_data<'a>(task: *const TaskObject<T, ATOMICS, MIN_CHUNKS>) -> &'a T {
    unsafe { &(*task).data }
  }

  pub unsafe fn take_data<'a>(task: *mut TaskObject<T, ATOMICS, MIN_CHUNKS>) -> T {
    unsafe { Box::from_raw(task) }.data
  }
}

pub struct LoopArguments<'a, const ATOMICS: usize, const MIN_CHUNKS: usize> {
  pub work_size: &'a Vec<u32>,
  pub work_indexes: &'a Vec<CachePadded<AtomicU32>>,
  pub work_indexes_index: &'a CachePadded<AtomicU32>,
  pub empty_signal: EmptySignal<'a, ATOMICS, MIN_CHUNKS>,
}

impl<const ATOMICS: usize, const MIN_CHUNKS: usize> Debug for LoopArguments<'_, ATOMICS, MIN_CHUNKS> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    write!(f, "LoopArguments:\n  work_size {:?}\n  work_indexes {:?}", self.work_size, self.work_indexes)
  }
}

impl<'a, const ATOMICS: usize, const MIN_CHUNKS: usize> LoopArgumentsTrait<'a> for LoopArguments<'a, ATOMICS, MIN_CHUNKS> {}
