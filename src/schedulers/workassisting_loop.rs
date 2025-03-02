#[macro_export]
macro_rules! workassisting_loop {
  ($loop_arguments_expr: expr, |$chunk_index: ident| $body: block, $scheduler: ident) => {
    $scheduler.workassisting_loop($loop_arguments_expr, |$chunk_index| $body);
  };
}
pub(crate) use workassisting_loop;


#[macro_export]
macro_rules! for_each_scheduler {
  ($body: expr, $($arg: expr),*) => {
    use crate::schedulers::multi_atomics::worker::Scheduler;
    $body(Scheduler, $($arg),*);
    $body(Scheduler, $($arg),*);
  };
  ($body: expr) => {
    use crate::schedulers::multi_atomics::worker::Scheduler;
    $body(Scheduler);
    $body(Scheduler);
  };
}
pub(crate) use for_each_scheduler;
