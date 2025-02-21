#[macro_export]
macro_rules! workassisting_loop2 {
  ($loop_arguments_expr: expr, |$chunk_index: ident| $body: block) => {
    let mut loop_arguments: LoopArguments = $loop_arguments_expr;
    // Claim work
    let mut chunk_idx;

    let atomics_length = loop_arguments.work_size.len();

    for _ in 0..atomics_length {
      let current_index = loop_arguments.work_indexes_index.fetch_add(1, Ordering::Relaxed) as usize % atomics_length;

      chunk_idx = loop_arguments.work_indexes.read().unwrap()[current_index].fetch_add(1, Ordering::Relaxed);
      while chunk_idx < loop_arguments.work_size[current_index] {
      //   println!("workassisting_loop: {:?}", loop_arguments);

      //   println!("Doing work: {:?}", chunk_idx);

        // Copy chunk_index to an immutable variable, such that a user of this macro cannot mutate it.
        let $chunk_index = chunk_idx;
        $body

        chunk_idx = loop_arguments.work_indexes.read().unwrap()[current_index].fetch_add(1, Ordering::Relaxed);
      }
    }
    loop_arguments.empty_signal.task_empty();
  };
}
pub(crate) use workassisting_loop2;
