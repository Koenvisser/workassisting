use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use num_format::{Locale, ToFormattedString};
use our::BlockInfo;

use crate::{for_each_scheduler, for_each_scheduler_with_arg};
use crate::scheduler::Scheduler as SchedulerTrait;
use crate::scheduler::Workers;
use crate::utils::benchmark::{benchmark_with_title, Benchmarker, ChartLineStyle, ChartStyle, Nesting};
use crate::utils;

mod our;
mod outer;
mod inner;

const SIZE: usize = 1024 * 1024 * 4;

pub fn find_affinities() -> Box<[usize]> {
  let cores = affinity::get_core_num().min(32);
  affinity::set_thread_affinity([0]).unwrap();

  let inputs = vec![create_input(SIZE)];
  let temps = vec![our::create_temp(SIZE)];
  let outputs = vec![unsafe { utils::array::alloc_undef_u64_array(SIZE) }];

  let ratio = 2;
  let mask = ratio - 1; // Assumes ratio is a power of two

  return utils::thread_pinning::find_best_affinity_mapping(cores, |affinities| {
    for_each_scheduler!(run_our, mask, &inputs, &temps, &outputs, 1, affinities);
  });

  fn run_our<S>(
    scheduler: S, 
    mask: u64, 
    inputs: &[Box<[u64]>], 
    temps: &Vec<Box<[BlockInfo]>>,
    outputs: &[Box<[AtomicU64]>],
    array_count: usize,
    affities: &[usize])
      -> () 
    where S: SchedulerTrait {
      let pending = AtomicUsize::new(array_count + 1);
      let task = our::create_initial_task::<S, S::Task>(mask, inputs, temps, outputs, &pending);
      S::Workers::run_on(affities, task);
  }

}

pub fn run(open_mp_enabled: bool) {
  run_on(open_mp_enabled, 1, SIZE);
  run_on(open_mp_enabled, 2, SIZE);
  run_on(open_mp_enabled, 4, SIZE);
  run_on(open_mp_enabled, 8, SIZE);
  run_on(open_mp_enabled, 16, SIZE);
  run_on(open_mp_enabled, 32, SIZE);
}

fn run_on(open_mp_enabled: bool, array_count: usize, size: usize) {
  utils::affinity_first();
  
  let mut inputs = vec![];
  let mut temps = vec![];
  let mut outputs = vec![];

  let ratio = 2;
  let mask = ratio - 1; // Assumes ratio is a power of two

  for _ in 0 .. array_count {
    inputs.push(create_input(size));
    temps.push(our::create_temp(size));
    outputs.push(unsafe { utils::array::alloc_undef_u64_array(size) });
  }

  utils::affinity_full();

  let name = "Compactions (n = ".to_owned() + &size.to_formatted_string(&Locale::en) + ", m = " + &array_count.to_formatted_string(&Locale::en) + ")";
  let title = "m = ".to_owned() + &array_count.to_formatted_string(&Locale::en);
  let mut benchmark = benchmark_with_title(if array_count == 1 { ChartStyle::SmallWithKey } else { ChartStyle::Small }, 5, &name, &title, || {
    reference_sequential(mask, &inputs, &outputs);
  }).open_mp(open_mp_enabled, "OpenMP", ChartLineStyle::OmpDynamic, "compact", Nesting::Nested, array_count, Some(size));

  for_each_scheduler_with_arg!(benchmark_outer_parallelism, benchmark, mask, &inputs, &outputs);
  for_each_scheduler_with_arg!(benchmark_inner_parallelism, benchmark, mask, &inputs, &temps, &outputs);
  for_each_scheduler_with_arg!(benchmark_our, benchmark, mask, &inputs, &temps, &outputs, array_count);

  fn benchmark_outer_parallelism<S>(
    scheduler: S, 
    benchmark: Benchmarker<()>, 
    mask: u64, 
    inputs: &[Box<[u64]>], 
    outputs: &[Box<[AtomicU64]>])
      -> Benchmarker<()> 
    where S: SchedulerTrait {
      return benchmark.parallel("Outer parallelism", ChartLineStyle::SequentialPartition, |thread_count| {
        let task = outer::create_task::<S, S::Task>(mask, inputs, outputs);
        scheduler.run(thread_count, task);
      });
  }

  fn benchmark_inner_parallelism<S>(
    scheduler: S, 
    benchmark: Benchmarker<()>, 
    mask: u64, 
    inputs: &[Box<[u64]>], 
    temps: &Vec<Box<[BlockInfo]>>,
    outputs: &[Box<[AtomicU64]>])
      -> Benchmarker<()> 
    where S: SchedulerTrait {
      return benchmark.parallel("Inner parallelism", ChartLineStyle::Static, |thread_count| {
        let task = inner::create_task::<S, S::Task>(mask, inputs, temps, outputs);
        scheduler.run(thread_count, task);
      });
  }

  fn benchmark_our<S>(
    scheduler: S, 
    benchmark: Benchmarker<()>, 
    mask: u64, 
    inputs: &[Box<[u64]>], 
    temps: &Vec<Box<[BlockInfo]>>,
    outputs: &[Box<[AtomicU64]>],
    array_count: usize)
      -> Benchmarker<()> 
    where S: SchedulerTrait {
      return benchmark.our(|thread_count| {
        let pending = AtomicUsize::new(array_count + 1);
        let task = our::create_initial_task::<S, S::Task>(mask, inputs, temps, outputs, &pending);
        scheduler.run(thread_count, task);
      });
  }
}

pub fn reference_sequential(mask: u64, inputs: &[Box<[u64]>], outputs: &[Box<[AtomicU64]>]) -> () {
  for (input, output) in inputs.iter().zip(outputs) {
    compact_sequential(mask, input, output, 0);
  }
  ()
}

pub fn compact_sequential(mask: u64, input: &[u64], output: &[AtomicU64], mut output_index: usize) -> usize {
  for &value in input {
    if predicate(mask, value) {
      unsafe { output.get_unchecked(output_index) }.store(value, Ordering::Relaxed);
      output_index += 1;
    }
  }

  output_index
}

pub fn count_sequential(mask: u64, input: &[u64]) -> usize {
  let mut count = 0;
  for &value in input {
    if predicate(mask, value) {
      count += 1;
    }
  }
  count
}

fn create_input(size: usize) -> Box<[u64]> {
  (0..size).map(|x| random(x as u64) as u64).collect()
}

fn random(mut seed: u64) -> u32 {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  seed as u32
}

pub fn predicate(mask: u64, mut value: u64) -> bool {
  value ^= value >> 11;
  value ^= value << 7;
  value ^= value >> 5;
  (value & mask) == mask
}
