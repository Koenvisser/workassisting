use core::fmt::Debug;
use std::time;
use std::fs::File;
use std::io::{prelude::*, BufWriter};
use crate::utils;
use crate::utils::thread_pinning::AFFINITY_MAPPING;

const THREAD_COUNTS: [usize; 14] = [1, 2, 3, 4, 6, 8, 10, 12, 14, 16, 20, 24, 28, 32];

pub struct Benchmarker<T> {
  chart_style: ChartStyle,
  max_y: u64,
  name: String,
  title: String,
  reference_time: u64,
  expected: T,
  output: Vec<(String, ChartLineStyle, Vec<f32>)>
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum ChartStyle {
  WithKey,
  WithoutKey,
  Small,
  SmallWithKey
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum ChartLineStyle {
  WorkAssisting(usize),
  WorkStealing,
  OmpStatic,
  OmpDynamic,
  OmpTask,
  Static,
  StaticPinned,
  Rayon,
  SequentialPartition,
  Dynamic(usize, usize, usize),
}

fn chart_line_style_to_str(style: ChartLineStyle) -> String {
  match style {
    ChartLineStyle::WorkAssisting(ref chunk_size)
      => {
        let color = format!("#{:02X}0A35", chunk_size * 50 % 256);
        format!("pointsize 0.4 lw 2 pt 7 linecolor rgb \"{}\"", color)
      },
    ChartLineStyle::WorkStealing
      => "pointsize 0.7 lw 1 pt 6 linecolor rgb \"#5B2182\"".to_string(),
    ChartLineStyle::OmpStatic
      => "pointsize 0.7 lw 1 pt 5 linecolor rgb \"#FFCD00\"".to_string(),
    ChartLineStyle::OmpDynamic
      => "pointsize 0.7 lw 1 pt 4 linecolor rgb \"#001240\"".to_string(),
    ChartLineStyle::OmpTask
      => "pointsize 0.7 lw 1 pt 12 linecolor rgb \"#F3965E\"".to_string(),
    ChartLineStyle::Static
      => "pointsize 0.7 lw 1 pt 2 linecolor rgb \"#5287C6\"".to_string(),
    ChartLineStyle::StaticPinned
      => "pointsize 0.7 lw 1 pt 3 linecolor rgb \"#24A793\"".to_string(),
    ChartLineStyle::Rayon
      => "pointsize 0.7 lw 1 pt 1 linecolor rgb \"#6E3B23\"".to_string(),
    ChartLineStyle::SequentialPartition
      => "pointsize 0.7 lw 1 pt 1 linecolor rgb \"#24A793\"".to_string(),
    ChartLineStyle::Dynamic(ref atomics, ref min_chunks, ref chunk_size)
      => {
        let color = format!("#{:02X}{:02X}{:02X}", atomics * 10 % 256, min_chunks * 20 % 256, chunk_size * 30 % 256);
        format!("pointsize 0.7 lw 2 pt 1 linecolor rgb \"{}\"", color)
      },
  }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Nesting {
  Nested,
  Flat
}

pub fn benchmark<T: Debug + Eq, Ref: FnMut() -> T>(chart_style: ChartStyle, max_y: u64, name: &str, reference: Ref) -> Benchmarker<T> {
  benchmark_with_title(chart_style, max_y, name, name, reference)
}
pub fn benchmark_with_title<T: Debug + Eq, Ref: FnMut() -> T>(chart_style: ChartStyle, max_y: u64, name: &str, title: &str, reference: Ref) -> Benchmarker<T> {
  println!("");
  println!("Benchmark {}", name);
  let (expected, reference_time) = time(100, reference);
  println!("Sequential   {} ms", reference_time / 1000);
  Benchmarker{ chart_style, max_y, name: name.to_owned(), title: title.to_owned(), reference_time, expected, output: vec![] }
}

impl<T: Copy + Debug + Eq + Send> Benchmarker<T> {
  pub fn rayon<Par: FnMut() -> T + Sync + Send>(self, label: Option<&str>, mut parallel: Par) -> Self {
    let string;
    let name = if let Some(l) = label {
      string = "Rayon (".to_owned() + l + ")";
      &string
    } else {
      "Rayon"
    };
    self.parallel(name, ChartLineStyle::Rayon, |thread_count|
      utils::rayon::run_in_pool(thread_count, || { parallel() })
    )
  }

  pub fn static_parallel<Par: Fn(usize, bool) -> T>(self, parallel: Par) -> Self {
    self.parallel("Static", ChartLineStyle::Static, |thread_count| parallel(thread_count, false))
      .parallel("Static (pinned)", ChartLineStyle::StaticPinned, |thread_count| parallel(thread_count, true))
  }

  pub fn work_stealing<Par: FnMut(usize) -> T>(self, parallel: Par) -> Self {
    self.parallel("Work stealing", ChartLineStyle::WorkStealing, parallel)
  }

  pub fn sequential<Seq: FnMut() -> T>(self, name: &str, sequential: Seq) -> Self {
    let (value, time) = time(100, sequential);
    assert_eq!(self.expected, value);

    let relative = self.reference_time as f32 / time as f32;
    if name.len() <= 12 {
      println!("{:12} {} ms ({:.2}x)", name, time / 1000, relative);
    } else {
      println!("{}", name);
      println!("{:12} {} ms ({:.2}x)", "", time / 1000, relative);
    }
    self
  }

  pub fn parallel<Par: FnMut(usize) -> T>(mut self, name: &str, chart_line_style: ChartLineStyle, mut parallel: Par) -> Self {
    println!("{}", name);
    let mut results = vec![];
    for thread_count in THREAD_COUNTS {
      if thread_count > AFFINITY_MAPPING.len() { break; }
      let (value, time) = time(100, || parallel(thread_count));
      assert_eq!(self.expected, value);
      let relative = self.reference_time as f32 / time as f32;
      results.push(relative);
      println!("  {:02} threads {} ms ({:.2}x)", thread_count, time / 1000, relative);
    }
    self.output.push((name.to_owned(), chart_line_style, results));
    self
  }

  pub fn open_mp(mut self, openmp_enabled: bool, name: &str, chart_line_style: ChartLineStyle, cpp_name: &str, nesting: Nesting, size1: usize, size2: Option<usize>) -> Self {
    if !openmp_enabled { return self; }

    println!("{}", name);
    let mut results = vec![];
    for thread_count in THREAD_COUNTS {
      if thread_count > AFFINITY_MAPPING.len() { break; }
      let affinity = (0 .. thread_count).map(|i| 1 << AFFINITY_MAPPING[i]).fold(0, |a, b| a | b);

      let mut command = std::process::Command::new("taskset");
      command
        .env("OMP_NUM_THREADS", thread_count.to_string())
        .arg(format!("{:X}", affinity))
        .arg("./reference-openmp/build/main")
        .arg(cpp_name)
        .arg(size1.to_string());

      if let Some(s) = size2 {
        command.arg(s.to_string());
      }

      if nesting != Nesting::Flat {
        command.env("OMP_NESTED", "True");
      }

      let child = command
        .output()
        .expect("Reference sequential C++ implementation failed");

      let time_str = String::from_utf8_lossy(&child.stdout);
      let time: u64 = time_str.trim().parse().expect(&("Unexpected output from reference C++ program: ".to_owned() + &time_str));
      let relative = self.reference_time as f32 / time as f32;
      results.push(relative);
      println!("  {:02} threads {} ms ({:.2}x)", thread_count, time / 1000, relative);
    }
    self.output.push((name.to_owned(), chart_line_style, results));

    self
  }

  pub fn open_mp_lud(mut self, openmp_enabled: bool, name: &str, taskloops: bool, chart_line_style: ChartLineStyle, filename: &str, m: usize) -> Self {
    if !openmp_enabled { return self; }

    println!("{}", name);
    let mut results = vec![];
    for thread_count in THREAD_COUNTS {
      if thread_count > AFFINITY_MAPPING.len() { break; }
      let affinity = (0 .. thread_count).map(|i| 1 << AFFINITY_MAPPING[i]).fold(0, |a, b| a | b);

      let mut total_time = 0.0;
      let runs = 100;
      let path = if taskloops {
        "./rodinia_3.1/openmp-taskloops/lud/omp/lud_omp"
      } else {
        "./rodinia_3.1/openmp/lud/omp/lud_omp"
      };
      for _ in 0 .. runs {
        let mut command = std::process::Command::new("taskset");
        command
          .arg(format!("{:X}", affinity))
          .arg(path)
          .arg("-i")
          .arg(filename)
          .arg("-m")
          .arg(m.to_string())
          .arg("-n")
          .arg(thread_count.to_string());

        let child = command
          .output()
          .expect("Reference sequential C++ implementation failed");

        let output = String::from_utf8_lossy(&child.stdout);
        let time_str = output.split(' ').filter(|s| s.trim() != "").last().expect("Empty output from OpenMP program");
        let time_f: f32 = time_str.trim().parse().expect(&("Unexpected output from reference C++ program: ".to_owned() + &time_str));
        total_time += time_f * 1000.0;
      }
      let time = (total_time / runs as f32) as u64;
      let relative = self.reference_time as f32 / time as f32;
      results.push(relative);
      println!("  {:02} threads {} ms ({:.2}x)", thread_count, time / 1000, relative);
    }
    self.output.push((name.to_owned(), chart_line_style, results));

    self
  }
}

impl<T> Drop for Benchmarker<T> {
  fn drop(&mut self) {
    std::fs::create_dir_all("./results").unwrap();
    let filename = "./results/".to_owned() + &self.name.replace(' ', "_").replace('(', "").replace(')', "").replace('=', "_");

    let file_gnuplot = File::create(filename.clone() + ".gnuplot").unwrap();
    let mut gnuplot = BufWriter::new(&file_gnuplot);
    let small = self.chart_style == ChartStyle::Small || self.chart_style == ChartStyle::SmallWithKey;
    writeln!(&mut gnuplot, "set title \"{}\"", self.title).unwrap();
    if small {
      writeln!(&mut gnuplot, "set terminal pdf size 2.2,2.0").unwrap();
    } else {
      writeln!(&mut gnuplot, "set terminal pdf size 3.2,2.9").unwrap();
    }
    writeln!(&mut gnuplot, "set output \"{}\"", filename.clone() + ".pdf").unwrap();
    if self.chart_style == ChartStyle::WithKey || self.chart_style == ChartStyle::SmallWithKey {
      writeln!(&mut gnuplot, "set key on").unwrap();
      writeln!(&mut gnuplot, "set key top left Left reverse").unwrap();
      if small {
        writeln!(&mut gnuplot, "set key samplen 1.4").unwrap();
      } else {
        writeln!(&mut gnuplot, "set key samplen 2.5").unwrap();
      }
    } else {
      writeln!(&mut gnuplot, "set key off").unwrap();
    }
    let max_threads = 32.min(AFFINITY_MAPPING.len());
    writeln!(&mut gnuplot, "set xrange [1:{}]", max_threads).unwrap();
    write!(&mut gnuplot, "set xtics (1").unwrap();
    for i in (4 ..= max_threads).step_by(4) {
      write!(&mut gnuplot, ", {}", i).unwrap();
    }
    writeln!(&mut gnuplot, ")").unwrap();
    writeln!(&mut gnuplot, "set xlabel \"Number of threads\"").unwrap();
    writeln!(&mut gnuplot, "set yrange [0:{}]", self.max_y).unwrap();
    if self.max_y == 9 {
      writeln!(&mut gnuplot, "set ytics (0, 2, 4, 6, 8)").unwrap();
    } else if self.max_y == 5 {
      writeln!(&mut gnuplot, "set ytics (0, 1, 2, 3, 4)").unwrap();
    }
    writeln!(&mut gnuplot, "set ylabel \"Speedup\"").unwrap();

    write!(&mut gnuplot, "plot ").unwrap();
    for (idx, result) in self.output.iter().enumerate() {
      if idx != 0 {
        write!(&mut gnuplot, ", \\\n  ").unwrap();
      }
      write!(&mut gnuplot, "'{}.dat' using 1:{} title \"{}\" {} with linespoints", filename, idx+2, result.0, chart_line_style_to_str(result.1)).unwrap();
    }
    writeln!(&mut gnuplot, "").unwrap();

    let file_data = File::create(filename.clone() + ".dat").unwrap();
    let mut writer = BufWriter::new(&file_data);
    write!(&mut writer, "# Benchmark {}\n", self.name).unwrap();
    write!(&mut writer, "# Speedup compared to a sequential implementation.\n").unwrap();
    
    // Header
    write!(&mut writer, "# NCPU").unwrap();
    for result in &self.output {
      write!(&mut writer, "\t{}", result.0).unwrap();
    }
    write!(&mut writer, "\n").unwrap();

    for (idx, thread_count) in THREAD_COUNTS.iter().enumerate() {
      if *thread_count > AFFINITY_MAPPING.len() { break; }
      write!(&mut writer, "{}", thread_count).unwrap();
      for result in &self.output {
        if idx < result.2.len() {
          write!(&mut writer, "\t{}", result.2[idx]).unwrap();
        } else {
          write!(&mut writer, "\t").unwrap();
        }
      }
      write!(&mut writer, "\n").unwrap();
    }

    std::process::Command::new("gnuplot")
      .arg(filename + ".gnuplot")
      .spawn()
      .expect("gnuplot failed");
  }
}

pub fn time<T: Debug + Eq, F: FnMut() -> T>(runs: usize, mut f: F) -> (T, u64) {
  let first = f();
  
  let timer = time::Instant::now();
  for _ in 0 .. runs {
    assert_eq!(first, f());
  }

  (first, (timer.elapsed().as_micros() / runs as u128) as u64)
}
