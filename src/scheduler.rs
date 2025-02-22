pub trait Scheduler {
  type Workers<'a>: Workers<'a> where Self: Sized;
  type Task: Task where Self: Sized;
  fn get_name(&self) -> &'static str;
  fn run(&self, worker_count: usize, initial_task: Self::Task) where Self: Sized;
}


pub trait Workers<'a, T: Task> {
  fn run(worker_count: usize, initial_task: T) where Self: Sized;

  fn run_on(affinities: &[usize], initial_task: T) where Self: Sized;

  fn finish(&self);

  fn push_task(&self, task: T) where Self: Sized;
}

pub trait TaskObject<T: Send + Sync> {}

pub trait LoopArguments<'a> {}

pub trait Task<'w, W: Workers<'w, Self>, T: TaskObject<T> + std::marker::Sync + std::marker::Send, L: LoopArguments<'w>> {
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
}
