// A fully parallel implementation of quicksort.
// - Not inplace, one additional array is used
// - Parallel partition with data parallelism
// - Two sections are sorted in parallel with task parallelism

use core::sync::atomic::{Ordering, AtomicU32, AtomicU64};
use crate::scheduler::*;
use crate::cases::quicksort::count_recursive_calls;
use crate::cases::quicksort::parallel_partition_chunk;

use crate::cases::quicksort::{SEQUENTIAL_CUTOFF, DATAPAR_CUTOFF, BLOCK_SIZE};
use crate::cases::quicksort::sequential;
use crate::cases::quicksort::task_parallel;

struct Data<'a> {
  pending_tasks: &'a AtomicU64,
  input: &'a [AtomicU32],
  output: &'a [AtomicU32],
  input_output_flipped: bool,
  // 32 least significant bits are used for the number of items on the left side,
  // 32 most significat bits are used for the number of items on the right side
  counters: AtomicU64, //crossbeam::utils::CachePadded<AtomicU64>
}

pub fn create_task<'a, S, T>(pending_tasks: &'a AtomicU64, input: &'a [AtomicU32], output: &'a [AtomicU32], input_output_flipped: bool) -> Option<T> 
  where
    S: Scheduler<Task = T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); BLOCK_SIZE / S::CHUNK_SIZE]:
{
  assert_eq!(input.len(), output.len());

  if input.len() == 0 {
    return None
  } else if input.len() == 1 {
    if !input_output_flipped {
      output[0].store(input[0].load(Ordering::Relaxed), Ordering::Relaxed);
    }
    return None;
  }

  if input.len() < SEQUENTIAL_CUTOFF {
    return Some(sequential::create_task(pending_tasks, input, if input_output_flipped { None } else { Some(output) }));
  }

  if input.len() < DATAPAR_CUTOFF {
    if input_output_flipped {
      let data = task_parallel::Sort{
        pending_tasks,
        array: input
      };
      return Some(T::new_single(task_parallel::run, data));
    } else {
      let data = task_parallel::SortWithCopy{
        pending_tasks,
        input,
        output
      };
      return Some(T::new_single(task_parallel::run_with_copy, data));
    }
  }

  let data = Data{
    pending_tasks,
    input,
    output,
    input_output_flipped,
    counters: AtomicU64::new(0)
  };

  Some(T::new_dataparallel(partition_run::<S, T>, partition_finish, data, ((input.len() - 1 + {BLOCK_SIZE / S::CHUNK_SIZE} - 1) / {BLOCK_SIZE / S::CHUNK_SIZE}) as u32))
}

fn partition_run<'a, 'b, 'c, S, T>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) 
  where
    S: Scheduler<Task = T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); BLOCK_SIZE / S::CHUNK_SIZE]:
{
  let data = unsafe { T::TaskObject::get_data(task) };

  let pivot = data.input[0].load(Ordering::Relaxed);

  let input = data.input;
  let output = data.output;
  let counters = &data.counters;
  T::work_loop(loop_arguments, |chunk_index| {
    parallel_partition_chunk::<{S::CHUNK_SIZE}>(input, output, pivot, counters, chunk_index as usize);
  });
}

fn partition_finish<'a, 'b, S, T>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Data>) 
  where
    S: Scheduler<Task = T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); BLOCK_SIZE / S::CHUNK_SIZE]:
{
  let data = unsafe { T::TaskObject::take_data(task) };

  let counters = data.counters.load(Ordering::Relaxed);
  let count_left = counters & 0xFFFFFFFF;
  let count_right = counters >> 32;
  assert_eq!(count_left + count_right + 1, data.input.len() as u64);

  let pivot = data.input[0].load(Ordering::Relaxed);
  (if data.input_output_flipped { data.input } else { data.output })
    [count_left as usize].store(pivot, Ordering::Relaxed);

  match count_recursive_calls(data.input.len(), count_left as usize) {
    2 => {
      data.pending_tasks.fetch_add(1, Ordering::Relaxed);
    },
    0 => {
      if data.pending_tasks.fetch_sub(1, Ordering::Relaxed) == 1 {
        workers.finish();
      }
    },
    _ => {} // No work to be done if there is one recursive call,
    // As the number of pending tasks doesn't change.
  }

  for (from, to) in [(0, count_left as usize), (count_left as usize + 1, data.input.len())] {
    if let Some(task) = create_task(data.pending_tasks, &data.output[from .. to], &data.input[from .. to], !data.input_output_flipped) {
      workers.push_task(task);
    }
  }
}
