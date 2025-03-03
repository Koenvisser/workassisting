#[macro_export]
macro_rules! workassisting_loop {
  ($loop_arguments_expr: expr, |$chunk_index: ident| $body: block, $scheduler: ident) => {
    $scheduler.workassisting_loop($loop_arguments_expr, |$chunk_index| $body);
  };
}
pub(crate) use workassisting_loop;


#[macro_export]
macro_rules! for_each_scheduler {
  ($body: expr $(, $arg: expr)*) => {
    use crate::schedulers::multi_atomics::worker::Scheduler;
    $body(Scheduler $(, $arg)*);
    $body(Scheduler $(, $arg)*);
  };
}
pub(crate) use for_each_scheduler;

#[macro_export]
macro_rules! for_each_scheduler_with_arg {
  ($body: expr, $arg1: expr $(, $arg: expr)*) => {
    use crate::schedulers::multi_atomics::worker::Scheduler;
    $arg1 = $body(Scheduler, $arg1 $(, $arg)*);
    $body(Scheduler, $arg1 $(, $arg)*);
  };
}
pub(crate) use for_each_scheduler_with_arg;
