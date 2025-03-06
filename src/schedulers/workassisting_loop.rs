#[macro_export]
macro_rules! for_each_scheduler {
  ($body: ident $(, $arg: expr)*) => {
    $body::<crate::schedulers::multi_atomics::worker::Scheduler>($($arg),*);
    $body::<crate::schedulers::workassisting::worker::Scheduler>($($arg),*);
  };
}
pub(crate) use for_each_scheduler;

#[macro_export]
macro_rules! for_each_scheduler_with_arg {
  ($body: expr, $arg1: expr $(, $arg: expr)*) => {
    $arg1 = $body(crate::schedulers::multi_atomics::worker::Scheduler, $arg1 $(, $arg)*);
    $arg1 = $body(crate::schedulers::workassisting::worker::Scheduler, $arg1 $(, $arg)*);
  };
}
pub(crate) use for_each_scheduler_with_arg;
