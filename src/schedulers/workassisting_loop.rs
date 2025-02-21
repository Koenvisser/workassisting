use crate::scheduler::Scheduler;

#[macro_export]
macro_rules! workassisting_loop {
  ($loop_arguments_expr: expr, |$chunk_index: ident| $body: block, scheduler: Scheduler) => {
    scheduler.workassisting_loop($loop_arguments_expr, |$chunk_index| $body);
  };
}
pub(crate) use workassisting_loop;
