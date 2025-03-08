use core::sync::atomic::{Ordering, AtomicU32};
use crate::scheduler::*;
use crate::loop_fixed_size;
use crate::cases::prime;

struct Data<'a> {
  counter: &'a AtomicU32,
  first: u64,
  length: u64
}

pub fn create_task<T:Task>(counter: &AtomicU32, first: u64, length: u64) -> T {
  T::new_dataparallel::<Data>(go, finish, Data{ counter, first, length }, ((length + prime::BLOCK_SIZE - 1) / prime::BLOCK_SIZE) as u32)
}

fn go<'a, 'b, 'c, T:Task>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) {
  let data = unsafe { T::TaskObject::get_data(task) };

  let mut local_count = 0;

  T::work_loop(loop_arguments, |chunk_index| {
    let mut local_local_count = 0;
    loop_fixed_size!(number in
      data.first + chunk_index as u64 * prime::BLOCK_SIZE,
      data.first + (chunk_index as u64 + 1) * prime::BLOCK_SIZE,
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
