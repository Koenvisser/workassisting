use core::sync::atomic::{Ordering, AtomicU64};
use crate::cases::scan::{scan_sequential, fold_sequential};
use crate::scheduler::*;
use super::our::{BLOCK_SIZE, BlockInfo, reset, STATE_AGGREGATE_AVAILABLE, STATE_PREFIX_AVAILABLE};

#[derive(Copy, Clone)]
struct Data<'a> {
  inputs: &'a [Box<[u64]>],
  temps: &'a [Box<[BlockInfo]>],
  outputs: &'a [Box<[AtomicU64]>]
}

pub fn create_task<T:Task>(inputs: &[Box<[u64]>], temps: &[Box<[BlockInfo]>], outputs: &[Box<[AtomicU64]>]) -> T {
  assert!(inputs.len() != 0);
  reset(&temps[0]);
  T::new_dataparallel::<Data>(run, finish, Data{ inputs, temps, outputs }, ((inputs[0].len() as u64 + BLOCK_SIZE - 1) / BLOCK_SIZE) as u32)
}

fn run<'a, 'b, 'c, T:Task>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) {
  let data = unsafe { TaskObject::get_data(task) };
  let mut sequential = true;
  T::work_loop(loop_arguments, |block_index| {
    let input = &data.inputs[0];
    let temp = &data.temps[0];
    let output = &data.outputs[0];
    // Local scan
    // reduce-then-scan
    let start = block_index as usize * BLOCK_SIZE as usize;
    let end = ((block_index as usize + 1) * BLOCK_SIZE as usize).min(input.len());

    // Check if we already have an aggregate of the previous block.
    // If that is the case, then we can perform the scan directly.
    // Otherwise we perform a reduce-then-scan over this block.
    let aggregate_start = if !sequential {
      None // Don't switch back from parallel mode to sequential mode
    } else if block_index ==  0 {
      Some(0)
    } else {
      let previous = block_index - 1;
      let previous_state = temp[previous as usize].state.load(Ordering::Acquire);
      if previous_state == STATE_PREFIX_AVAILABLE {
        Some(temp[previous as usize].prefix.load(Ordering::Acquire))
      } else {
        None
      }
    };

    if let Some(aggregate) = aggregate_start {
      let local = scan_sequential(&input[start .. end], aggregate, &output[start .. end]);
      temp[block_index as usize].prefix.store(local, Ordering::Relaxed);
      temp[block_index as usize].state.store(STATE_PREFIX_AVAILABLE, Ordering::Release);
    } else {
      sequential = false;
      let local = fold_sequential(&input[start .. end]);
      // Share own local value
      temp[block_index as usize].aggregate.store(local, Ordering::Relaxed);
      temp[block_index as usize].state.store(STATE_AGGREGATE_AVAILABLE, Ordering::Release);

      // Find aggregate
      let mut aggregate = 0;
      let mut previous = block_index - 1;

      loop {
        let previous_state = temp[previous as usize].state.load(Ordering::Acquire);
        if previous_state == STATE_PREFIX_AVAILABLE {
          aggregate = temp[previous as usize].prefix.load(Ordering::Acquire) + aggregate;
          break;
        } else if previous_state == STATE_AGGREGATE_AVAILABLE {
          aggregate = temp[previous as usize].aggregate.load(Ordering::Acquire) + aggregate;
          previous = previous - 1;
        } else {
          // Continue looping until the state of previous block changes.
        }
      }

      // Make aggregate available
      temp[block_index as usize].prefix.store(aggregate + local, Ordering::Relaxed);
      temp[block_index as usize].state.store(STATE_PREFIX_AVAILABLE, Ordering::Release);
      scan_sequential(&input[start .. end], aggregate, &output[start .. end]);
    }
  });
}

fn finish<'a, 'b, T:Task>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Data>) {
  let data = unsafe { T::TaskObject::take_data(task) };
  if data.inputs.len() == 1 {
    workers.finish();
  } else {
    workers.push_task(create_task(&data.inputs[1..], &data.temps[1..], &data.outputs[1..]));
  }
}
