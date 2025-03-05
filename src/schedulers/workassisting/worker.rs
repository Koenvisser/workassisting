use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use crossbeam::deque;
use crossbeam::deque::Steal;
use super::task::*;
use crate::utils;
use crate::utils::ptr::AtomicTaggedPtr;
use crate::utils::ptr::TaggedPtr;
use crate::utils::thread_pinning::AFFINITY_MAPPING;
use crate::scheduler::Workers as WorkersTrait;

pub struct Workers<'a> {
  is_finished: &'a AtomicBool,
  worker_count: usize,
  worker: deque::Worker<Task>,
  stealers: &'a [deque::Stealer<Task>],
  activities: &'a [AtomicTaggedPtr<TaskObject<()>>]
}

impl<'a> WorkersTrait<'a> for Workers<'a> {
  type Task = Task;

  fn run(worker_count: usize, initial_task: Task) {
    Workers::run(worker_count, initial_task);
  }

  fn run_on(affinities: &[usize], initial_task: Task) {
    Workers::run_on(affinities, initial_task);
  }

  fn finish(&self) {
    self.finish();
  }

  fn push_task(&self, task: Task) {
    self.push_task(task);
  }
}

impl<'a> Workers<'a> {
  pub fn run(worker_count: usize, initial_task: Task) {
    Workers::run_on(&AFFINITY_MAPPING[0 .. worker_count], initial_task);
  }

  pub fn run_on(affinities: &[usize], initial_task: Task) {
    let worker_count = affinities.len();
    let workers: Vec<deque::Worker<Task>> = (0 .. worker_count).into_iter().map(|_| deque::Worker::new_lifo()).collect();
    let stealers: Box<[deque::Stealer<Task>]> = workers.iter().map(|w| w.stealer()).collect();

    workers[0].push(initial_task);

    let activities: Box<[AtomicTaggedPtr<TaskObject<()>>]> = unsafe {
      std::mem::transmute(vec![0 as usize; worker_count].into_boxed_slice())
    };

    let is_finished = AtomicBool::new(false);

    /* let full = affinity::get_thread_affinity().unwrap();
    std::thread::scope(|s| {
      for (thread_index, worker) in workers.into_iter().enumerate() {
        affinity::set_thread_affinity([affinities[thread_index]]).unwrap();
        let workers = Workers{
          is_finished: &is_finished,
          worker_count,
          worker,
          stealers: &stealers,
          activities: &activities
        };
        s.spawn(move || {
          workers.do_work(thread_index);
        });
      }
      affinity::set_thread_affinity(full).unwrap();
    }); */
    let threads: Vec<libc::pthread_t> = workers.into_iter().enumerate().map(|(thread_index, worker)| {
      let workers = Workers{
        is_finished: &is_finished,
        worker_count,
        worker,
        stealers: &stealers,
        activities: &activities
      };
      unsafe {
        utils::thread::unsafe_spawn_on(affinities[thread_index], Box::new(move || {
          workers.do_work(thread_index);
        })).unwrap()
      }
    }).collect();

    for thread in threads {
      let mut value = std::ptr::null_mut();
      unsafe { libc::pthread_join(thread, &mut value); }
    }
  }

  pub fn finish(&self) {
    self.is_finished.store(true, Ordering::Release);
  }

  pub fn push_task(&self, task: Task) {
    self.worker.push(task);
  }

  fn do_work(&self, thread_index: usize) {
    let backoff = crossbeam::utils::Backoff::new();
    loop {
      if self.is_finished.load(Ordering::Relaxed) {
        return;
      }

      // First try work stealing of tasks, to exploit task parallelism.
      match self.claim_task(thread_index) {
        Steal::Success(task) => {
          self.start_task(task, thread_index);
          backoff.reset();
        },
        Steal::Retry => {
          backoff.spin();
        },
        Steal::Empty => {
          // There is not enough task parallelism.
          // We try to perform work assisting on data parallel workloads.
          if self.try_assist(thread_index) {
            backoff.reset();
          } else {
            backoff.snooze();
          }
        }
      }
    }
  }

  fn claim_task(&self, thread_index: usize) -> Steal<Task> {
    // First we try to claim a task from our own deque.
    if let Some(item) = self.worker.pop() {
      return Steal::Success(item)
    }
    // If we didn't have tasks on our own deque, we try to steal a task from another thread.
    let mut other_index = thread_index;
    let increment = if thread_index % 2 == 0 { 1 } else { self.worker_count - 1 };
    let mut retry = false;
    loop {
      other_index = (other_index + increment) % self.worker_count;
      if other_index == thread_index {
        break;
      }
      match self.stealers[other_index].steal() {
        Steal::Success(item) => {
          return Steal::Success(item);
        },
        Steal::Retry => {
          retry = true;
        },
        Steal::Empty => {
        }
      }
    }
    if retry { Steal::Retry } else { Steal::Empty }
  }

  fn try_assist(&self, thread_index: usize) -> bool {
    let mut other_index = thread_index;
    let increment = if thread_index % 2 == 0 { 1 } else { self.worker_count - 1 };

    loop {
      other_index = (other_index + increment) % self.worker_count;
      if other_index == thread_index {
        return false;
      }

      let check = self.activities[other_index].load(Ordering::Relaxed);
      if check.ptr().is_null() { continue; }

      // Increment reference count (in tag).
      // Reading in 'check' and in 'activity' may be interleaved, but that is
      // not an issue as we again check whether the point is null.
      // The additional test with 'check' is required, as we could otherwise
      // repeatedly increment the tag of a null pointer, and the tag could then
      // overflow into the bits of the pointer.
      let activity = self.activities[other_index].fetch_add_tag(1, Ordering::Acquire);
      if activity.ptr().is_null() {
        // The before mentioned interleaving happened.
        // We now incremented the tag of a null pointer. We don't have to revert
        // this; as mentioned before this can only happen once per thread so
        // the tag cannot overflow.
        continue;
      }

      // We can assist this thread
      let task = unsafe { &*activity.ptr() };
      let mut signal = EmptySignal{ pointer: &self.activities[other_index], task, state: EmptySignalState::Assist };

      // Claim the first chunk
      let current_index = task.work_index.fetch_add(1, Ordering::Acquire);

      // Early out.
      if current_index >= task.work_size {
        signal.task_empty();
        self.end_task(task);
        return true;
      }
      self.call_task(task, signal, current_index);
      return true;
    }
  }

  fn start_task(&self, task: Task, thread_index: usize) {
    if task.work_size == 0 {
      // This task doesn't have data parallelism.
      // Hence task.work doesn't need to be called,
      // only task.finish.
      // No other threads will work on this task,
      // as it is never pushed to the 'activities' list.
      // Hence we can take unique ownership of this task here,
      // and pass it to finish.
      let task_ref: *const TaskObject<()> = &*task;
      let finish = task.finish;
      // task.finish will drop the object. Hence we shouldn't do that here.
      std::mem::forget(task);
      (finish)(self, task_ref as *mut TaskObject<()>);
      return;
    }

    let task_ptr = task.into_raw();
    let task_ref = unsafe { &*task_ptr };

    // Since this thread previously had no activity (i.e., a null pointer),
    // we don't have to keep track of the reference count that was previously
    // stored in the AtomicTaggedPtr.
    self.activities[thread_index].store(TaggedPtr::new(task_ptr, 0), Ordering::Release);

    let signal = EmptySignal{ pointer: &self.activities[thread_index], task: task_ref, state: EmptySignalState::Main };
    self.call_task(unsafe { &*task_ptr }, signal, 0);
  }

  // Calls the work function of a task, and calls end_task afterwards
  fn call_task(&self, task: *const TaskObject<()>, signal: EmptySignal, first_index: u32) {
    let task_ref = unsafe { &*task };
    (task_ref.work.unwrap())(self, task, LoopArguments{ work_size: task_ref.work_size, work_index: &task_ref.work_index, empty_signal: signal, first_index });
    self.end_task(task);
  }

  fn end_task(&self, task: *const TaskObject<()>) {
    let task_ref = unsafe { &*task };
    // Check whether there is no pending work (that is claimed, but not finished yet).
    let remaining = task_ref.active_threads.fetch_sub(1, Ordering::AcqRel) - 1;
    if remaining == 0 {
      // Only one thread will decrement active_threads to zero.
      // That thread will call the finish of the task.
      // As documented in TaskObject.active_threads,
      // this task is not present anymore in activities at this point
      // and other threads are not working on this task any more.
      // Hence we can take unique ownership of this task now.
      let finish = task_ref.finish;
      // task.finish will drop the object. Hence we shouldn't do that here.
      (finish)(self, task as *mut TaskObject<()>);
    }
  }
}

pub struct EmptySignal<'a> {
  pointer: &'a AtomicTaggedPtr<TaskObject<()>>,
  task: &'a TaskObject<()>,
  state: EmptySignalState
}

enum EmptySignalState {
  Main,
  Assist,
  DidSignal
}

impl<'a> EmptySignal<'a> {
  pub fn task_empty(&mut self) {
    match self.state {
      EmptySignalState::DidSignal => {},
      EmptySignalState::Main => {
        let old = self.pointer.swap(TaggedPtr::new(std::ptr::null(), 0), Ordering::Relaxed);
        // Encorporate the tag of the AtomicTaggedPtr in the reference count of the tag.
        if old.ptr() == self.task {
          self.task.active_threads.fetch_add(old.tag() as i32 + 1, Ordering::Relaxed);
          // Note that this fetch-and-add won't decrement the reference count to zero yet,
          // as this thread is working on the task and thus present in the reference count.
          // The reference count will be at least one now. It can only become zero in call_task.
        }
      },
      EmptySignalState::Assist => {
        let mut value = self.pointer.load(Ordering::Relaxed);
        // Update 'pointer' using a CAS-loop.
        // We must update pointer to not point at 'self.task'.
        // This requires a compare-and-swap (compare-exchange), as we should
        // only update it when it currently points to self.task. It could be
        // that the main thread has progressed to a new task, and overwriting
        // that would prevent any other thread from assisting.
        while value.ptr() == self.task {
          let result = self.pointer.compare_exchange_weak(value, TaggedPtr::new(std::ptr::null(), 0), Ordering::Relaxed, Ordering::Relaxed);
          match result {
            Ok(_) => {
              // Encorporate the tag of the AtomicTaggedPtr in the reference count of the task.
              self.task.active_threads.fetch_add(value.tag() as i32 + 1, Ordering::Relaxed);
            },
            Err(new) => {
              // compare-exchange failed. This is caused by either:
              // - Another thread updated the pointer to point to null or a different task. The loop will stop.
              // - Another thread just joined the (already finished) computation. The loop will continue.
              value = new;
            }
          }
        }
      }
    }
    self.state = EmptySignalState::DidSignal;
  }
}
