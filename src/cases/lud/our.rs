// Implementation of LU decomposition.
// This implementation is mainly based on the OpenMP implementation from Rodinia.
// Source: /rodinia_3.1/openmp/lud/omp/lud.c
// Licence: /rodinia_3.1/LICENSE

use crate::utils::matrix::SquareMatrix;
use core::sync::atomic::Ordering;
use std::sync::atomic::AtomicU64;
use crate::scheduler::*;
use super::*;

// The workload of LU decomposition is parallelised as follows.
// Compared to the sequential implementation, we perform OUTER_BLOCK_SIZE of the outer loop at the same time.
// First we perform the LU decomposition of the OUTER_BLOCK_SIZE * OUTER_BLOCK_SIZE elements at the left top of the matrix.
// This happens in diagonal_tile, as is sequential.
// Then we propagate this information over the left border and the top border,
// i.e. the first OUTER_BLOCK_SIZE columns and OUTER_BLOCK_SIZE rows.
// This happens in task_border_go and is parallel.
// Then we work in the remaining part of the matrix, the interior. As soon as the work for the top left of the interior is completed,
// diagonal_tile is already executed for the next iteration.
// When all interior parts are handled, the next iteration contines (with the border and then the interior).
// This repeats until the entire matrix is handled.

// The outer loop is split in tiles of OUTER_BLOCK_SIZE * OUTER_BLOCK_SIZE elements.
const OUTER_BLOCK_SIZE: usize = 32;
// The left border and top border are handled in chunks of BORDER_BLOCK_SIZE elements.
const BORDER_BLOCK_SIZE: usize = 32;
// The inner part of the array is handled in tiles of the following size.
pub const INNER_BLOCK_SIZE_ROWS: usize = 32;
pub const INNER_BLOCK_SIZE_COLUMNS: usize = 32;

// The matrix size should be a multiple of OUTER_BLOCK_SIZE.
// OUTER_BLOCK_SIZE should be a multiple of BORDER_BLOCK_SIZE, INNER_BLOCK_SIZE_ROWS and INNER_BLOCK_SIZE_COLUMNS.

pub fn create_task<S, T>(matrices: &[(SquareMatrix, AtomicU64, AtomicU64)], pending: &AtomicU64) -> T 
  where 
    S: Scheduler<Task=T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); (TILE_SIZE / S::CHUNK_SIZE) * (TILE_SIZE / S::CHUNK_SIZE)]: ,
    [(); max(INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE, INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE)]:
{ 
  pending.store(matrices.len() as u64, Ordering::Relaxed);
  T::new_dataparallel::<Init>(task_init_go::<S, T>, task_init_finish::<T>, Init{ matrices, pending }, matrices.len() as u32)
}

struct Init<'a> {
  matrices: &'a[(SquareMatrix, AtomicU64, AtomicU64)], // Only the first AtomicU64 is used
  pending: &'a AtomicU64
}

fn task_init_go<'a, 'b, 'c, S, T>(workers: &'a T::Workers<'b>, task: *const T::TaskObject<Init>, loop_arguments: T::LoopArguments<'c>) 
  where
    S: Scheduler<Task=T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); (TILE_SIZE / S::CHUNK_SIZE) * (TILE_SIZE / S::CHUNK_SIZE)]: ,
    [(); max(INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE, INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE)]:
{
  let data = unsafe { T::TaskObject::get_data(task) };

  T::work_loop(loop_arguments, |index| {
    let (matrix, synchronisation_var, _) = &data.matrices[index as usize];
    diagonal_tile::<{S::CHUNK_SIZE}>(0, matrix);
    start_iteration::<S, T>(workers, 0, matrix, synchronisation_var, data.pending)
  });
}

fn task_init_finish<'a, 'b, T:Task>(_workers: &'a T::Workers<'b>, task: *mut T::TaskObject<Init>) {
  unsafe {
    // Assure that the task gets dropped
    drop(Box::from_raw(task));
  }
}

struct Data<'a> {
  matrix: &'a SquareMatrix,
  offset: usize,
  synchronisation_var: &'a AtomicU64,
  pending: &'a AtomicU64,
}

fn start_iteration<'a, S, T>(workers: &T::Workers<'a>, offset: usize, matrix: &SquareMatrix, synchronisation_var: &AtomicU64, pending: &AtomicU64) 
  where
    S: Scheduler<Task=T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); (TILE_SIZE / S::CHUNK_SIZE) * (TILE_SIZE / S::CHUNK_SIZE)]: ,
    [(); max(INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE, INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE)]:
{
  let i_end = offset + (OUTER_BLOCK_SIZE / S::CHUNK_SIZE);

  if offset + (OUTER_BLOCK_SIZE / S::CHUNK_SIZE) >= matrix.size() {
    // Work for this matrix is finished. Check if this was the last matrix.
    let old = pending.fetch_sub(1, Ordering::Relaxed);
    if old == 1 {
      workers.finish();
    }
  } else {
    // Continue with remaining part of the matrix
    let remaining = matrix.size() - i_end;

    synchronisation_var.store(0, Ordering::Relaxed);

    workers.push_task(
      T::new_dataparallel::<Data>(
        task_border_left_go::<S, T>,
        task_border_finish::<S, T>,
        Data{ matrix, offset, synchronisation_var, pending },
        ((remaining + (BORDER_BLOCK_SIZE / S::CHUNK_SIZE) - 1) / (BORDER_BLOCK_SIZE / S::CHUNK_SIZE)) as u32
      )
    );
    workers.push_task(
      T::new_dataparallel::<Data>(
        task_border_top_go::<S, T>,
        task_border_finish::<S, T>,
        Data{ matrix, offset, synchronisation_var, pending },
        ((remaining + (BORDER_BLOCK_SIZE / S::CHUNK_SIZE) - 1) / (BORDER_BLOCK_SIZE / S::CHUNK_SIZE)) as u32
      )
    );
  }
}

fn task_border_left_go<'a, 'b, 'c, S, T>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) 
  where
    S: Scheduler<Task=T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); (TILE_SIZE / S::CHUNK_SIZE) * (TILE_SIZE / S::CHUNK_SIZE)]:
{
  let data = unsafe { T::TaskObject::get_data(task) };

  let mut temp = Align([0.0; (OUTER_BLOCK_SIZE / S::CHUNK_SIZE) * (OUTER_BLOCK_SIZE / S::CHUNK_SIZE)]);
  border_init(data.offset, data.matrix, &mut temp);

  T::work_loop(loop_arguments, |chunk_index| {
    border_left_chunk::<BORDER_BLOCK_SIZE, {S::CHUNK_SIZE}>(data.offset, data.matrix, &temp, chunk_index as usize);
  });
}

fn task_border_top_go<'a, 'b, S, T>(_workers: &T::Workers<'a>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'b>) 
  where
    S: Scheduler<Task=T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); (TILE_SIZE / S::CHUNK_SIZE) * (TILE_SIZE / S::CHUNK_SIZE)]:
{
  let data = unsafe { T::TaskObject::get_data(task) };

  let mut temp = Align([0.0; (OUTER_BLOCK_SIZE / S::CHUNK_SIZE) * (OUTER_BLOCK_SIZE / S::CHUNK_SIZE)]);
  border_init::<{S::CHUNK_SIZE}>(data.offset, data.matrix, &mut temp);

  T::work_loop(loop_arguments, |chunk_index| {
    border_top_chunk::<BORDER_BLOCK_SIZE, {S::CHUNK_SIZE}>(data.offset, data.matrix, &temp, chunk_index as usize);
  });
}

fn task_border_finish<'a, S, T>(workers: &T::Workers<'a>, task: *mut T::TaskObject<Data>) 
  where 
    S: Scheduler<Task=T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); (TILE_SIZE / S::CHUNK_SIZE) * (TILE_SIZE / S::CHUNK_SIZE)]: ,
    [(); max(INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE, INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE)]:
{
  let data = unsafe { T::TaskObject::take_data(task) };

  // The algorithm continues when both the left and the top parts have finished.
  // This function handles the finish phase of both tasks.
  // The first task to finish sets the synchronisation var to 1 (the CAS succeeds).
  // The CAS fails in the second task, which signals that the algorithm can continue.
  let cas = data.synchronisation_var.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
  if cas.is_ok() { return; }

  let remaining = data.matrix.size() - data.offset - (OUTER_BLOCK_SIZE / S::CHUNK_SIZE);
  let rows = (remaining + (INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE) - 1) / (INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE);
  let columns = (remaining + (INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE) - 1) / (INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE);

  let inner_tiles_per_outer_tile = ((OUTER_BLOCK_SIZE / S::CHUNK_SIZE) / INNER_BLOCK_SIZE_COLUMNS) * ((OUTER_BLOCK_SIZE / S::CHUNK_SIZE) / (INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE));
  data.synchronisation_var.store(inner_tiles_per_outer_tile as u64, Ordering::Relaxed);

  workers.push_task(
    Task::new_dataparallel::<Data>(
      task_inner_go,
      task_inner_finish::<S, T>,
      Data{ matrix: data.matrix, offset: data.offset, synchronisation_var: data.synchronisation_var, pending: data.pending },
      (rows * columns) as u32
    )
  );
}

fn task_inner_go<'a, 'b, 'c, S, T>(_workers: &'a T::Workers<'b>, task: *const T::TaskObject<Data>, loop_arguments: T::LoopArguments<'c>) 
  where 
    S: Scheduler<Task=T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); (INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE) * (OUTER_BLOCK_SIZE / S::CHUNK_SIZE)]: ,
    [(); max(INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE, INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE)]:
{
  let data = unsafe { TaskObject::get_data(task) };

  let remaining = data.matrix.size() - data.offset - (OUTER_BLOCK_SIZE / S::CHUNK_SIZE);
  let rows = (remaining + (INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE) - 1) / (INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE);

  let mut temp_top = Align([0.0; (INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE) * (OUTER_BLOCK_SIZE / S::CHUNK_SIZE)]);
  let mut sum = Align([0.0; max(INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE, INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE)]);
  let mut temp_index = 0;

  T::work_loop(loop_arguments, |chunk_index| {
    interior_chunk::<INNER_BLOCK_SIZE_ROWS, INNER_BLOCK_SIZE_COLUMNS, {S::CHUNK_SIZE}>
      (data.offset, rows, data.matrix, &mut temp_index, &mut temp_top.0, &mut sum.0, chunk_index as usize);
    let i_global = data.offset + (OUTER_BLOCK_SIZE / S::CHUNK_SIZE) + (INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE) * (chunk_index as usize % rows);
    let j_global = data.offset + (OUTER_BLOCK_SIZE / S::CHUNK_SIZE) + (INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE) * (chunk_index as usize / rows);

    if i_global < data.offset + 2 * (OUTER_BLOCK_SIZE / S::CHUNK_SIZE) && j_global < data.offset + 2 * (OUTER_BLOCK_SIZE / S::CHUNK_SIZE) {
      let old_remaining = data.synchronisation_var.fetch_sub(1, Ordering::AcqRel);
      if old_remaining == 1 {
        // All inner chunks of the first chunk (in terms of outer chunk sizes) are finished.
        // Start working on the diagonal chunk of the next iteration already.
        diagonal_tile::<{S::CHUNK_SIZE}>(data.offset + (OUTER_BLOCK_SIZE / S::CHUNK_SIZE), data.matrix);
      }
    }
  });
}

fn task_inner_finish<'a, S, T>(workers: &T::Workers<'a>, task: *mut T::TaskObject<Data>) 
  where 
    S: Scheduler<Task=T>,
    T: Task,
    [(); S::CHUNK_SIZE]: ,
    [(); (TILE_SIZE / S::CHUNK_SIZE) * (TILE_SIZE / S::CHUNK_SIZE)]: ,
    [(); max(INNER_BLOCK_SIZE_COLUMNS / S::CHUNK_SIZE, INNER_BLOCK_SIZE_ROWS / S::CHUNK_SIZE)]:
{
  let data = unsafe { TaskObject::take_data(task) };
  start_iteration::<S, T>(workers, data.offset + OUTER_BLOCK_SIZE, data.matrix, data.synchronisation_var, data.pending);
}

// https://stackoverflow.com/questions/53619695/calculating-maximum-value-of-a-set-of-constant-expressions-at-compile-time
pub const fn max(a: usize, b: usize) -> usize {
  [a, b][(a < b) as usize]
}
