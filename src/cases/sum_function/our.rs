use core::sync::atomic::{Ordering, AtomicU64};
use crate::scheduler::*;
use crate::utils::loops::*;
use crate::cases::sum_function;

struct Data<'a> {
  counter: &'a AtomicU64,
  first: u64,
  length: u64
}

pub fn create_task<S, T>(counter: &AtomicU64, first: u64, length: u64) -> T 
  where 
    S: Scheduler<Task=T>,
    T: Task
{
  let block_size = const { sum_function::BLOCK_SIZE / S::CHUNK_SIZE as u64 };
  T::new_dataparallel::<Data>(work::<S, T>, finish, Data{ counter, first, length }, ((length + block_size - 1) / block_size) as u32)
}

fn work<'a, 'b, 'c, S, T>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) 
  where 
    S: Scheduler<Task=T>,
    T: Task
{
  let data = unsafe { T::TaskObject::get_data(task) };

  let mut local_count = 0;
  let block_size = const { sum_function::BLOCK_SIZE / S::CHUNK_SIZE as u64 };
  
  let first = data.first;
  let length = data.length;
  let counter = data.counter;
  T::work_loop(loop_arguments, |chunk_index| {
    let from = first + chunk_index as u64 * block_size;
    let to = from + block_size;

    let mut local_local_count = 0;
    loop_fixed_size!(number in from, to, first + length, {
      local_local_count += sum_function::random(number) as u64;
    });
    local_count += local_local_count;
  });
  counter.fetch_add(local_count, Ordering::Relaxed);
}

fn finish<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Data>) {
  unsafe {
    drop(Box::from_raw(task));
  }
  workers.finish();
}
