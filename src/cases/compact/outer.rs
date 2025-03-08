// Only exploit the outer parallelism.
// It performs the different compactions in parallel, but the compactions themself are sequential.
use core::sync::atomic::AtomicU64;
use crate::cases::compact::compact_sequential;
use crate::scheduler::*;

#[derive(Copy, Clone)]
struct Data<'a> {
  mask: u64,
  inputs: &'a [Box<[u64]>],
  outputs: &'a [Box<[AtomicU64]>]
}

pub fn create_task<S, T>(mask: u64, inputs: &[Box<[u64]>], outputs: &[Box<[AtomicU64]>]) -> T 
  where 
    S: Scheduler<Task=T>,
    T: Task
{
  T::new_dataparallel::<Data>(run, finish, Data{ mask, inputs, outputs }, inputs.len() as u32)
}

fn run<'a, 'b, 'c, T:Task>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) {
  let data = unsafe { TaskObject::get_data(task) };
  T::work_loop(loop_arguments, |i| {
    compact_sequential(data.mask, &data.inputs[i as usize], &data.outputs[i as usize], 0);
  });
}
fn finish<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Data>) {
  let _data = unsafe { T::TaskObject::take_data(task) };
  workers.finish();
}
