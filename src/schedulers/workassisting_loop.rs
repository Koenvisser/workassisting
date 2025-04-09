#[macro_export]
macro_rules! for_each_scheduler {
  ($body: ident $(, $arg: expr)*) => {
    $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 1, 4>>($($arg),*);
  };
}

#[macro_export]
macro_rules! for_each_scheduler_with_arg {
  ($body: ident, $arg1: expr $(, $arg: expr)*) => {
    $arg1 = $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 1, 4>>($arg1 $(, $arg)*);
    $arg1 = $body::<crate::schedulers::multi_atomics_2::worker::Scheduler<64, 1, 4>>($arg1 $(, $arg)*);
  };
}
