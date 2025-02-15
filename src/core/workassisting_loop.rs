#[macro_export]
macro_rules! workassisting_loop {
  ($loop_arguments_expr: expr, |$chunk_index: ident| $body: block) => {
    use rand::Rng;

    let mut loop_arguments: LoopArguments = $loop_arguments_expr;
    // Claim work
    let mut chunk_idx = loop_arguments.first_index;


    while chunk_idx < loop_arguments.work_size[loop_arguments.current_index] {
      println!("workassisting_loop: {:?}", loop_arguments);
      if chunk_idx == loop_arguments.work_size[loop_arguments.current_index] - 1 {
        // All work in this atomic integer is claimed.
        println!("All work in this atomic integer is claimed");
        loop_arguments.work_indexes.write().unwrap().remove(loop_arguments.current_index);
        let new_length = loop_arguments.work_indexes.read().unwrap().len();

        if new_length == 0 {
          // All work is claimed.
          println!("All work is claimed");
          loop_arguments.empty_signal.task_empty();
        }

        else {
          // Claim work from another atomic integer.
          loop_arguments.current_index = rand::rng().random_range(0..new_length);
        }
      }

      println!("Doing work: {:?}", chunk_idx);

      // Copy chunk_index to an immutable variable, such that a user of this macro cannot mutate it.
      let $chunk_index = chunk_idx;
      $body

      chunk_idx = loop_arguments.work_indexes.read().unwrap()[loop_arguments.current_index].fetch_add(1, Ordering::Relaxed);
    }
    loop_arguments.empty_signal.task_empty();
  };
}
pub(crate) use workassisting_loop;
