use core::sync::atomic::{Ordering, AtomicU64};
use rayon::prelude::*;
use crate::scheduler::*;
use crate::for_each_scheduler_with_arg;
use crate::utils::benchmark::{benchmark, ChartStyle, Nesting, ChartLineStyle, Benchmarker};
use crate::utils::thread_pinning::AFFINITY_MAPPING;
use num_format::{Locale, ToFormattedString};

mod deque;
mod our;

pub const BLOCK_SIZE: usize = 2048;

pub const START: u64 = 1024 * 1024 * 1024;

pub fn run(open_mp_enabled: bool) {
  for count in [1024 * 1024 * 32 + 1234, 1024 * 1024 * 64 + 1234] {
    let name = "Sum array (n = ".to_owned() + &(count).to_formatted_string(&Locale::en) + ")";
    let array: Vec<u64> = (START .. START + count).map(|number| crate::cases::sum_function::random(number) as u64).collect();

    let mut benchmark = benchmark(
      ChartStyle::WithKey,
      16,
      &name,
      || reference_sequential_single(&array)
    )
      .rayon(None, || reference_parallel(&array))
      .static_parallel(|thread_count, pinned| static_parallel(&array, thread_count, pinned))
      .work_stealing(|thread_count| {
        deque::sum(&array, thread_count)
      })
      .open_mp(open_mp_enabled, "OpenMP (static)", ChartLineStyle::OmpStatic, "sum-array-static", Nesting::Flat, count as usize, None)
      .open_mp(open_mp_enabled, "OpenMP (dynamic)", ChartLineStyle::OmpDynamic, "sum-array-dynamic", Nesting::Flat, count as usize, None)
      .open_mp(open_mp_enabled, "OpenMP (taskloop)", ChartLineStyle::OmpTask, "sum-array-taskloop", Nesting::Flat, count as usize, None);

    for_each_scheduler_with_arg!(benchmark_our, benchmark, &array);

      fn benchmark_our<S>(
        benchmark: Benchmarker<u64>,
        array: &Vec<u64>
      ) -> Benchmarker<u64>
      where
        S: Scheduler
      {
        return benchmark.our(|thread_count| {
          let counter = AtomicU64::new(0);
          let task = our::create_task(&counter, array);
          S::Workers::run(thread_count, task);
          counter.load(Ordering::Acquire)
        })
      }
  }
}

pub fn reference_sequential_single(array: &[u64]) -> u64 {
  let mut counter = 0;
  for number in array {
    counter += *number;
  }
  counter
}

pub fn reference_parallel(array: &[u64]) -> u64 {
  array.into_par_iter().sum()
}

pub fn static_parallel(array: &[u64], thread_count: usize, pinned: bool) -> u64 {
  let result = AtomicU64::new(0);
  let full = affinity::get_thread_affinity().unwrap();
  std::thread::scope(|s| {
    let result_ref = &result;
    for thread_index in 0 .. thread_count {
      if pinned {
        affinity::set_thread_affinity([AFFINITY_MAPPING[thread_index]]).unwrap();
      }
      s.spawn(move || {
        let start = thread_index * array.len() / thread_count;
        let end = (thread_index + 1) * array.len() / thread_count;
        let mut sum = 0;
        for idx in start .. end {
          sum += array[idx];
        }
        result_ref.fetch_add(sum, Ordering::Relaxed);
      });
    }
    affinity::set_thread_affinity(full).unwrap();
  });
  result.load(Ordering::Relaxed)
}

