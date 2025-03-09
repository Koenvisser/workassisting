use crate::utils::benchmark::ChartLineStyle;

pub trait Scheduler {
  type Workers<'a>: Workers<'a, Task = Self::Task> where Self: Sized;
  type Task: Task where Self: Sized;
  fn get_name() -> String;
  fn get_chart_line_style() -> ChartLineStyle;
  const CHUNK_SIZE: usize;
}

pub trait Workers<'a> {
  type Task: Task<Workers<'a> = Self> where Self: Sized;

  fn run(worker_count: usize, initial_task: Self::Task) where Self: Sized;

  fn run_on(affinities: &[usize], initial_task: Self::Task) where Self: Sized;

  fn finish(&self);

  fn push_task(&self, task: Self::Task) where Self: Sized;
}

pub trait TaskObject<T: Send + Sync> {
  unsafe fn get_data<'a>(task: *const Self) -> &'a T;
  unsafe fn take_data(task: *mut Self) -> T;
}

pub trait LoopArguments<'a> {}

pub trait Task {
  type Workers<'a>: Workers<'a, Task = Self>;
  type TaskObject<T: Send + Sync>: TaskObject<T>;
  type LoopArguments<'b>: LoopArguments<'b>;

  fn new_dataparallel<T: Send + Sync>(
    work: for <'a, 'b, 'c> fn(workers: &'a Self::Workers<'b>, data: *const Self::TaskObject<T>, loop_arguments: Self::LoopArguments<'c>) -> (),
    finish: for <'a, 'b> fn(workers: &'a Self::Workers<'b>, data: *mut Self::TaskObject<T>) -> (),
    data: T,
    work_size: u32
  ) -> Self where Self: Sized;

  fn new_single<T: Send + Sync>(
    function: for <'a, 'b> fn(workers: &'a Self::Workers<'b>, data: *mut Self::TaskObject<T>) -> (),
    data: T
  ) -> Self where Self: Sized;

  fn work_loop<'a, F: FnMut(u32)>(
    loop_arguments: Self::LoopArguments<'a>,
    work: F,
  ); 
}
