use core::mem;
use core::fmt;
use core::cell::UnsafeCell;
use core::ops::{IndexMut, Index, Mul};

#[repr(C)]
#[repr(align(64))]
#[derive(Clone, Copy)]
pub struct F32xN<const N: usize>(pub [f32; N]);

// Square matrix with interior mutability
pub struct SquareMatrix<const N: usize> {
  size: usize,
  data: Box<[UnsafeCell<F32xN<N>>]>
}
unsafe impl<const N: usize> Sync for SquareMatrix<N> {}

impl<const N: usize> SquareMatrix<N> {
  pub fn new(size: usize) -> SquareMatrix<N> {
    let data: Vec<F32xN<N>> = vec![F32xN([0.0; N]); size * size / N];
    SquareMatrix{
      size,
      // Safety: f32 and UnsafeCell<f32> have the same representation in memory
      data: unsafe { mem::transmute(data.into_boxed_slice()) }
    }
  }

  #[inline(always)]
  fn data_f32(&self) -> &[UnsafeCell<f32>] {
    unsafe {
      std::slice::from_raw_parts(self.data.as_ptr() as *const UnsafeCell<f32>, self.data.len() * N)
    }
  }

  #[inline(always)]
  pub fn size(&self) -> usize {
    self.size
  }

  #[inline(always)]
  pub fn write(&self, index: (usize, usize), value: f32) {
    unsafe {
      *self.get_unsafe_cell(index).get() = value;
    }
  }

  // Row major
  #[inline(always)]
  fn linear_index(&self, (row, column): (usize, usize)) -> usize {
    assert!(row < self.size);
    assert!(column < self.size);
    row * self.size + column
  }

  #[inline(always)]
  pub fn get_unsafe_cell(&self, index: (usize, usize)) -> &UnsafeCell<f32> {
    unsafe {
      &self.data_f32().get_unchecked(self.linear_index(index))
    }
  }

  #[inline(always)]
  pub fn slice(&self, row: usize, column_start: usize, column_count: usize) -> &[UnsafeCell<f32>] {
    let index = row * self.size + column_start;
    &self.data_f32()[index .. index + column_count]
  }

  #[inline(always)]
  pub fn slice32(&self, row: usize, column_start: usize) -> &UnsafeCell<F32xN<N>> {
    let index = row * self.size + column_start;
    unsafe { self.data.get_unchecked(index / N) }
  }

  pub fn upper_triangle_with_diagonal(&self) -> SquareMatrix<N> {
    let mut output = SquareMatrix::new(self.size);

    for row in 0 .. self.size {
      for column in row .. self.size {
        output[(row, column)] = self[(row, column)];
      }
    }

    output
  }

  pub fn lower_triangle_with_1_diagonal(&self) -> SquareMatrix<N> {
    let mut output = SquareMatrix::new(self.size);

    for row in 0 .. self.size {
      for column in 0 .. row {
        output[(row, column)] = self[(row, column)];
      }
    }

    for i in 0 .. self.size {
      output[(i, i)] = 1.0;
    }

    output
  }

  pub fn copy_to(&self, other: &SquareMatrix<N>) {
    assert_eq!(self.data.len(), other.data.len());
    for i in 0 .. self.data.len() {
      unsafe {
        *(other.data[i].get()) = *self.data[i].get();
      }
    }
  }
}

impl<const N: usize> Clone for SquareMatrix<N> {
  fn clone(&self) -> Self {
    let mut data: Vec<F32xN<N>> = vec![F32xN([0.0; N]); self.data.len()];
    for i in 0 .. data.len() {
      data[i] = unsafe { *self.data[i].get() };
    }
    SquareMatrix { size: self.size, data: unsafe { mem::transmute(data.into_boxed_slice()) } }
  }
}

impl<const N: usize> Index<(usize, usize)> for SquareMatrix<N> {
  type Output = f32;

  #[inline(always)]
  fn index(&self, index: (usize, usize)) -> &Self::Output {
    unsafe { &*self.get_unsafe_cell(index).get() }
  }
}

impl<const N: usize> IndexMut<(usize, usize)> for SquareMatrix<N> {
  #[inline(always)]
  fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
    unsafe { &mut *self.get_unsafe_cell(index).get() }
  }
}

impl<const N: usize> Mul for &SquareMatrix<N> {
  type Output = SquareMatrix<N>;

  fn mul(self, other: &SquareMatrix<N>) -> Self::Output {
    assert_eq!(self.size, other.size);
    let mut output = SquareMatrix::new(self.size);
    for row in 0 .. self.size {
      for column in 0 .. self.size {
        let mut sum = 0.0;
        for k in 0 .. self.size {
          sum += self[(row, k)] * other[(k, column)];
        }
        output[(row, column)] = sum;
      }
    }
    output
  }
}

impl<const N: usize> fmt::Debug for SquareMatrix<N> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "SquareMatrix ({}x{}) {{", self.size, self.size)?;
    for row in 0 .. self.size {
      write!(f, "\n ")?;
      for column in 0 .. self.size {
        write!(f, " {}", self[(row, column)])?;
      }
    }
    write!(f, "\n}}")
  }
}
