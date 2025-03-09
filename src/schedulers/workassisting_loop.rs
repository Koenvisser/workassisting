#[macro_export]
macro_rules! for_each_scheduler {
  ($body: ident $(, $arg: expr)*) => {
    $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 1, 1>>($($arg),*);
    $body::<crate::schedulers::multi_atomics::worker::Scheduler<32, 1, 1>>($($arg),*);
    $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 10, 1>>($($arg),*);
    $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 1, 4>>($($arg),*);
    $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 10, 4>>($($arg),*);
    $body::<crate::schedulers::workassisting::worker::Scheduler>($($arg),*);
  };
}

#[macro_export]
macro_rules! for_each_scheduler_with_arg {
  ($body: ident, $arg1: expr $(, $arg: expr)*) => {
    $arg1 = $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 1, 1>>($arg1 $(, $arg)*);
    $arg1 = $body::<crate::schedulers::multi_atomics::worker::Scheduler<32, 1, 1>>($arg1 $(, $arg)*);
    $arg1 = $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 10, 1>>($arg1 $(, $arg)*);
    $arg1 = $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 1, 4>>($arg1 $(, $arg)*);
    $arg1 = $body::<crate::schedulers::multi_atomics::worker::Scheduler<64, 10, 4>>($arg1 $(, $arg)*);
    $arg1 = $body::<crate::schedulers::workassisting::worker::Scheduler>($arg1 $(, $arg)*);
  };
}
