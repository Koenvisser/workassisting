// A partially parallel implementation of quicksort.
// - Inplace
// - Sequential partition
// - Two sections are sorted in parallel with task parallelism

use core::sync::atomic::{Ordering, AtomicU32, AtomicU64};
use crate::scheduler::*;
use crate::cases::quicksort::{SEQUENTIAL_CUTOFF, count_recursive_calls};
use crate::cases::quicksort::sequential;

pub fn create_task<'a, T:Task>(pending_tasks: &'a AtomicU64, array: &'a [AtomicU32]) -> Option<T> {
  if array.len() <= 1 {
    return None
  }

  if array.len() < SEQUENTIAL_CUTOFF {
    return Some(sequential::create_task(pending_tasks, array, None));
  }

  let data = Sort{
    pending_tasks,
    array
  };

  Some(T::new_single(run, data))
}

pub struct Sort<'a> {
  pub pending_tasks: &'a AtomicU64,
  pub array: &'a [AtomicU32],
}

pub fn run<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Sort>) {
  let data = unsafe { T::TaskObject::take_data(task) };
  run_go::<T>(workers, data.pending_tasks, data.array);
}

fn run_go<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, pending_tasks: &AtomicU64, array: &[AtomicU32]) {
  let array = array;
  assert!(array.len() > 1);

  let pivot = array[0].load(Ordering::Relaxed);

  let mut left = 1;
  let mut right = array.len() - 1;
  loop {
    while left < array.len() && array[left].load(Ordering::Relaxed) < pivot { left += 1; }
    while right > 0 && array[right].load(Ordering::Relaxed) >= pivot { right -= 1; }
    if left >= right { break; }
    let left_value = array[left].load(Ordering::Relaxed);
    array[left].store(array[right].load(Ordering::Relaxed), Ordering::Relaxed);
    array[right].store(left_value, Ordering::Relaxed);
    left += 1;
    right -= 1;
  }

  assert_eq!(left - 1, right);

  // Pivot should be placed at index 'right'.
  array[0].store(array[right].load(Ordering::Relaxed), Ordering::Relaxed);
  array[right].store(pivot, Ordering::Relaxed);

  match count_recursive_calls(array.len(), right as usize) {
    2 => {
      pending_tasks.fetch_add(1, Ordering::Relaxed);
    },
    0 => {
      if pending_tasks.fetch_sub(1, Ordering::Relaxed) == 1 {
        workers.finish();
      }
    },
    _ => {} // No work to be done if there is one recursive call,
    // As the number of pending tasks doesn't change.
  }

  for (start, end) in [(0, right), (right + 1, array.len())] {
    if let Some(task) = create_task(pending_tasks, &array[start .. end]) {
      workers.push_task(task);
    }
  }
}

pub struct SortWithCopy<'a> {
  pub pending_tasks: &'a AtomicU64,
  pub input: &'a [AtomicU32],
  pub output: &'a [AtomicU32]
}

pub fn run_with_copy<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<SortWithCopy>) {
  let data = unsafe { T::TaskObject::take_data(task) };
  for i in 0 .. data.output.len() {
    data.output[i].store(data.input[i].load(Ordering::Relaxed), Ordering::Relaxed);
  }
  run_go::<T>(workers, data.pending_tasks, data.output);
}
