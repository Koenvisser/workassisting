use core::sync::atomic::{Ordering, AtomicU32};
use crate::scheduler::*;
use crate::loop_fixed_size;
use crate::cases::prime;

struct Data<'a> {
  counter: &'a AtomicU32,
  first: u64,
  length: u64
}

pub fn create_task<S, T>(counter: &AtomicU32, first: u64, length: u64) -> T 
  where 
    S: Scheduler<Task=T>,
    T: Task
{
  let block_size = const { prime::BLOCK_SIZE / S::CHUNK_SIZE as u64};
  T::new_dataparallel::<Data>(go::<S, T>, finish, Data{ counter, first, length }, ((length + block_size - 1) / block_size) as u32)
}

fn go<'a, 'b, 'c, S, T>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) 
  where 
    S: Scheduler<Task=T>,
    T: Task
{
  let data = unsafe { T::TaskObject::get_data(task) };

  let mut local_count = 0;
  let block_size = const { prime::BLOCK_SIZE / S::CHUNK_SIZE as u64};

  T::work_loop(loop_arguments, |chunk_index| {
    let mut local_local_count = 0;
    loop_fixed_size!(number in
      data.first + chunk_index as u64 * block_size,
      data.first + (chunk_index as u64 + 1) * block_size,
      data.first + data.length,
      {
        if prime::is_prime(number) {
          local_local_count += 1;
        }
      }
    );
    local_count += local_local_count;
  });
  data.counter.fetch_add(local_count, Ordering::Relaxed);
}

fn finish<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Data>) {
  let _ = unsafe { T::TaskObject::take_data(task) };
  workers.finish();
}
