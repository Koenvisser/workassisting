use core::sync::atomic::{Ordering, AtomicU32, AtomicU64};
use num_format::{Locale, ToFormattedString};
use crate::scheduler::*;
use crate::schedulers::workassisting::worker::Workers as WorkAssisting;
use crate::for_each_scheduler_with_arg;
use crate::specialize_if;
use crate::utils::array::alloc_undef_u32_array;
use crate::utils::benchmark::{Benchmarker, ChartLineStyle};
use crate::utils::benchmark::{benchmark, ChartStyle, Nesting};

pub mod our;
pub mod deque_parallel_partition;
pub mod sequential;
pub mod task_parallel;

pub const BLOCK_SIZE: usize = 4096;

pub const DATAPAR_CUTOFF: usize = 1024 * 32;
pub const SEQUENTIAL_CUTOFF: usize = 1024 * 8;

pub fn run(open_mp_enabled: bool) {
  run_on(open_mp_enabled, 1024 * 256);
  run_on(open_mp_enabled, 1024 * 1024);
}

fn run_on(open_mp_enabled: bool, size: usize) {
  let array1 = unsafe { alloc_undef_u32_array(size) };
  let array2 = unsafe { alloc_undef_u32_array(size) };
  let name = "Sort (n = ".to_owned() + &size.to_formatted_string(&Locale::en) + ")";
  let mut benchmark = benchmark(
    if size == 1024 * 1024 { ChartStyle::WithoutKey } else { ChartStyle::WithKey },
    16,
    &name,
    || reference_sequential_single(&array1)
  )
  .parallel("Sequential partition", ChartLineStyle::SequentialPartition, |thread_count| {
    let pending_tasks = AtomicU64::new(1);
    WorkAssisting::run(thread_count, create_task_reset::<crate::schedulers::workassisting::worker::Scheduler<1>, crate::schedulers::workassisting::task::Task>(&array1, &pending_tasks, Kind::OnlyTaskParallel));
    assert_eq!(pending_tasks.load(Ordering::Relaxed), 0);
    output(&array1)
  })
  .work_stealing(|thread_count| {
    deque_parallel_partition::reset_and_sort(&array1, &array2, thread_count);
    output(&array2)
  })
  .open_mp(open_mp_enabled, "OpenMP (nested loops)", ChartLineStyle::OmpDynamic, "quicksort", Nesting::Nested, size, None)
  .open_mp(open_mp_enabled, "OpenMP (tasks)", ChartLineStyle::OmpTask, "quicksort-taskloop", Nesting::Flat, size, None);

  for_each_scheduler_with_arg!(benchmark_our, benchmark, &array1, &array2);

  fn benchmark_our<S>(
    benchmark: Benchmarker<u64>,
    array1: &Box<[AtomicU32]>,
    array2: &Box<[AtomicU32]>
  ) -> Benchmarker<u64>
  where
    S: Scheduler,
    [(); S::CHUNK_SIZE]: ,
    [(); BLOCK_SIZE / S::CHUNK_SIZE]:
  {
    return benchmark.parallel(&S::get_name(), S::get_chart_line_style(), |thread_count| {
      let pending_tasks = AtomicU64::new(1);
      S::Workers::run(thread_count, create_task_reset::<S, S::Task>(array1, &pending_tasks, Kind::DataParallel(array2)));
      output(array2)
    });
  }
}

pub fn random(mut seed: u64) -> u32 {
  seed += 876998787696;
  seed = seed.wrapping_mul(35334534876231);
  seed ^= seed << 19;
  seed ^= seed >> 23;
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  (seed & 0xFFFFFFFF) as u32
}

fn create_task_reset<S, T>(array: &[AtomicU32], pending_tasks: &AtomicU64, kind: Kind) -> T 
  where
    S: Scheduler<Task = T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); BLOCK_SIZE / S::CHUNK_SIZE]:
{
  let data = Reset{ array, pending_tasks, kind };
  T::new_dataparallel(reset_run, reset_finish, data, ((array.len() + {BLOCK_SIZE / S::CHUNK_SIZE} - 1) / {BLOCK_SIZE / S::CHUNK_SIZE}) as u32)
}

struct Reset<'a> {
  array: &'a [AtomicU32],

  // Info for next task
  pending_tasks: &'a AtomicU64,
  kind: Kind<'a>
}
enum Kind<'a> {
  OnlyTaskParallel,
  DataParallel(&'a [AtomicU32])
}

fn reset_run<'a, 'b, 'c, T:Task>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Reset>, loop_arguments: T::LoopArguments<'c>) {
  let data = unsafe { TaskObject::get_data(task) };

  T::work_loop(loop_arguments, |chunk_index| {
    for index in chunk_index as usize * BLOCK_SIZE .. ((chunk_index as usize + 1) * BLOCK_SIZE).min(data.array.len()) {
      data.array[index as usize].store(random(index as u64), Ordering::Relaxed);
    }
  });
}

fn reset_finish<'a, 'b, S, T>(workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Reset>) 
  where
    S: Scheduler<Task = T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); BLOCK_SIZE / S::CHUNK_SIZE]:
{
  let data = unsafe { T::TaskObject::take_data(task) };

  match data.kind {
    Kind::OnlyTaskParallel => {
      workers.push_task(task_parallel::create_task(data.pending_tasks, data.array).unwrap());
    },
    Kind::DataParallel(output) => {
      workers.push_task(our::create_task(data.pending_tasks, data.array, output, false).unwrap());
    }
  }
}

fn output(array: &[AtomicU32]) -> u64 {
  array[0].load(Ordering::Relaxed) as u64
    + array[478].load(Ordering::Relaxed) as u64
    + array[array.len() / 2].load(Ordering::Relaxed) as u64
    + array[array.len() - 324].load(Ordering::Relaxed) as u64
    + array[array.len() - 1].load(Ordering::Relaxed) as u64
}

fn reference_sequential_single(array: &[AtomicU32]) -> u64 {
  for i in 0 .. array.len() {
    array[i].store(random(i as u64), Ordering::Relaxed);
  }
  sequential::sort(array);
  output(array)
}

#[repr(C)]
#[repr(align(64))]
pub struct Align<T>(T);

#[inline(always)]
pub fn parallel_partition_chunk<const CHUNK_DIV: usize>(input: &[AtomicU32], output: &[AtomicU32], pivot: u32, counters: &AtomicU64, chunk_index: usize) 
  where
  [(); BLOCK_SIZE / CHUNK_DIV]:
{
  // Loop starts at 1, as element 0 is the pivot.
  let start = 1 + chunk_index as usize * {BLOCK_SIZE / CHUNK_DIV};
  assert_eq!(input.len(), output.len());

  // Treat the input as an immutable array. This thread, nor any other thread, will modify this part of the input
  // at this moment.
  let input1: &[u32] = unsafe { std::mem::transmute(input) };

  specialize_if!(start + {BLOCK_SIZE / CHUNK_DIV} <= input.len(), {BLOCK_SIZE / CHUNK_DIV}, input.len() - start, |end| {
    let mut values = Align([0; BLOCK_SIZE / CHUNK_DIV]);
    let mut left_count = 0;
    for (i, value) in input1[start .. start + end].iter().copied().enumerate() {
      let destination;
      if value < pivot {
        destination = left_count;
        left_count += 1;
      } else {
        destination = end as u64 - (i as u64 - left_count) - 1;
      }
      values.0[destination as usize] = value;
    }
    let right_count = end as u64 - left_count;
    let counters_value = counters.fetch_add((right_count << 32) | left_count, Ordering::SeqCst);
    let left_offset = (counters_value & 0xFFFFFFFF) as usize;
    let right_offset = input.len() - right_count as usize - (counters_value >> 32) as usize;
    if left_count != 0 {
      unsafe {
        std::ptr::copy_nonoverlapping(
          &values.0[0],
          output[left_offset].as_ptr(),
          left_count as usize);
      }
    }
    if right_count != 0 {
      unsafe {
        std::ptr::copy_nonoverlapping(
          &values.0[left_count as usize],
          output[right_offset].as_ptr(),
          right_count as usize);
      }
    }
  });
}
pub fn count_recursive_calls(len: usize, pivot: usize) -> usize {
  let mut count = 0;
  if pivot > 1 {
    // Left segment is non-trivial
    count += 1;
  }
  if len - pivot > 2 {
    count += 1;
  }
  count
}
