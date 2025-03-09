use core::sync::atomic::{Ordering, AtomicU64, AtomicUsize};
use crate::cases::compact::{compact_sequential, count_sequential};
use crate::scheduler::*;

pub const BLOCK_SIZE: u64 = 1024 * 4;

#[derive(Copy, Clone)]
struct InitialData<'a> {
  mask: u64,
  inputs: &'a [Box<[u64]>],
  temps: &'a [Box<[BlockInfo]>],
  outputs: &'a [Box<[AtomicU64]>],
  pending: &'a AtomicUsize
}

pub fn create_initial_task<S, T>(mask: u64, inputs: &[Box<[u64]>], temps: &[Box<[BlockInfo]>], outputs: &[Box<[AtomicU64]>], pending: &AtomicUsize) -> T 
  where 
    S: Scheduler<Task=T>,
    T: Task
{
  if inputs.len() == 1 {
    pending.store(1, Ordering::Relaxed);
    create_task::<S, T>(mask, &inputs[0], &temps[0], &outputs[0], pending)
  } else {
    T::new_dataparallel::<InitialData>(initial_run::<S, T>, initial_finish, InitialData{ mask, inputs, temps, outputs, pending }, inputs.len() as u32)
  }
}

fn initial_run<'a, 'b, 'c, S, T>(workers: &'a T::Workers<'b>, task: *const T::TaskObject<InitialData>, loop_arguments: T::LoopArguments<'c>) 
  where 
    S: Scheduler<Task=T>,
    T: Task
{
  let data = unsafe { T::TaskObject::get_data(task) };
  T::work_loop(loop_arguments, |i| {
    workers.push_task(create_task::<S, T>(data.mask, &data.inputs[i as usize], &data.temps[i as usize], &data.outputs[i as usize], data.pending));
  });
}
fn initial_finish<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<InitialData>) {
  let data = unsafe { T::TaskObject::take_data(task) };
  if data.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
    workers.finish();
  }
}

#[derive(Copy, Clone)]
struct Data<'a> {
  mask: u64,
  input: &'a [u64],
  temp: &'a [BlockInfo],
  output: &'a [AtomicU64],
  pending: &'a AtomicUsize
}

pub struct BlockInfo {
  pub state: AtomicU64,
  pub aggregate: AtomicUsize,
  pub prefix: AtomicUsize
}

pub const STATE_INITIALIZED: u64 = 0;
pub const STATE_AGGREGATE_AVAILABLE: u64 = 1;
pub const STATE_PREFIX_AVAILABLE: u64 = 2;

pub fn create_temp(size: usize) -> Box<[BlockInfo]> {
  (0 .. (size as u64 + BLOCK_SIZE - 1) / BLOCK_SIZE).map(|_| BlockInfo{
    state: AtomicU64::new(STATE_INITIALIZED), aggregate: AtomicUsize::new(0), prefix: AtomicUsize::new(0)
  }).collect()
}

pub fn create_temp_scheduler<S:Scheduler>(size: usize) -> Box<[BlockInfo]> {
  let block_size: u64 = const { BLOCK_SIZE / S::CHUNK_SIZE as u64 };
  (0 .. (size as u64 + block_size - 1) / block_size).map(|_| BlockInfo{
    state: AtomicU64::new(STATE_INITIALIZED), aggregate: AtomicUsize::new(0), prefix: AtomicUsize::new(0)
  }).collect()
}

pub fn reset(temp: &[BlockInfo]) {
  for i in 0 .. temp.len() {
    temp[i].state.store(STATE_INITIALIZED, Ordering::Relaxed);
    temp[i].aggregate.store(0, Ordering::Relaxed);
    temp[i].prefix.store(0, Ordering::Relaxed);
  }
}

pub fn create_task<S, T>(mask: u64, input: &[u64], temp: &[BlockInfo], output: &[AtomicU64], pending: &AtomicUsize) -> T 
  where 
    S: Scheduler<Task=T>,
    T: Task
{
  let block_size = const { BLOCK_SIZE / S::CHUNK_SIZE as u64 };
  reset(temp);
  T::new_dataparallel::<Data>(run::<S, T>, finish, Data{ mask, input, temp, output, pending }, ((input.len() as u64 + block_size - 1) / block_size) as u32)
}

fn run<'a, 'b, 'c, S, T>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) 
  where 
    S: Scheduler<Task=T>,
    T: Task
{
  let data = unsafe { T::TaskObject::get_data(task) };
  let mut sequential = true;
  let block_size = const { BLOCK_SIZE / S::CHUNK_SIZE as u64 };
  T::work_loop(loop_arguments, |block_index| {
    // Local scan
    // reduce-then-scan
    let start = block_index as usize * block_size as usize;
    let end = ((block_index as usize + 1) * block_size as usize).min(data.input.len());

    // Check if we already have an aggregate of the previous block.
    // If that is the case, then we can perform the scan directly.
    // Otherwise we perform a reduce-then-scan over this block.
    let aggregate_start = if !sequential {
      None // Don't switch back from parallel mode to sequential mode
    } else if block_index ==  0 {
      Some(0)
    } else {
      let previous = block_index - 1;
      let previous_state = data.temp[previous as usize].state.load(Ordering::Acquire);
      if previous_state == STATE_PREFIX_AVAILABLE {
        Some(data.temp[previous as usize].prefix.load(Ordering::Acquire))
      } else {
        None
      }
    };

    if let Some(aggregate) = aggregate_start {
      let local = compact_sequential(data.mask, &data.input[start .. end], data.output, aggregate);
      data.temp[block_index as usize].prefix.store(local, Ordering::Relaxed);
      data.temp[block_index as usize].state.store(STATE_PREFIX_AVAILABLE, Ordering::Release);
    } else {
      sequential = false;
      let local = count_sequential(data.mask, &data.input[start .. end]);
      // Share own local value
      data.temp[block_index as usize].aggregate.store(local, Ordering::Relaxed);
      data.temp[block_index as usize].state.store(STATE_AGGREGATE_AVAILABLE, Ordering::Release);

      // Find aggregate
      let mut aggregate = 0;
      let mut previous = block_index - 1;

      loop {
        let previous_state = data.temp[previous as usize].state.load(Ordering::Acquire);
        if previous_state == STATE_PREFIX_AVAILABLE {
          aggregate = data.temp[previous as usize].prefix.load(Ordering::Acquire) + aggregate;
          break;
        } else if previous_state == STATE_AGGREGATE_AVAILABLE {
          aggregate = data.temp[previous as usize].aggregate.load(Ordering::Acquire) + aggregate;
          previous = previous - 1;
        } else {
          // Continue looping until the state of previous block changes.
        }
      }

      // Make aggregate available
      data.temp[block_index as usize].prefix.store(aggregate + local, Ordering::Relaxed);
      data.temp[block_index as usize].state.store(STATE_PREFIX_AVAILABLE, Ordering::Release);
      compact_sequential(data.mask, &data.input[start .. end], data.output, aggregate);
    }
  });
}

fn finish<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Data>) {
  let data = unsafe { TaskObject::take_data(task) };
  if data.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
    workers.finish();
  }
}
