// Only exploit the outer parallelism.
// It performs the different scans in parallel, but the scans themself are sequential.
use core::sync::atomic::{Ordering, AtomicU64};
use crate::cases::scan::scan_sequential;
use crate::scheduler::*;

#[derive(Copy, Clone)]
struct Data<'a> {
  inputs: &'a [Box<[u64]>],
  outputs: &'a [Box<[AtomicU64]>]
}

pub fn create_task<T:Task>(inputs: &[Box<[u64]>], outputs: &[Box<[AtomicU64]>]) -> T {
  T::new_dataparallel::<Data>(run, finish, Data{ inputs, outputs }, inputs.len() as u32)
}

fn run<'a, 'b, 'c, T:Task>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) {
  let data = unsafe { TaskObject::get_data(task) };
  T::work_loop(loop_arguments, |i| {
    scan_sequential(&data.inputs[i as usize], 0, &data.outputs[i as usize]);
  });
}
fn finish<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Data>) {
  let _data = unsafe { T::TaskObject::take_data(task) };
  workers.finish();
}
