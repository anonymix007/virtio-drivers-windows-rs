//! Sorted vectors of types implementing `PartialOrd`.
//!
//! It is a runtime panic if an incomparable element is compared.

use core;
use core::hash::{Hash, Hasher};


/// Forward sorted vector
#[derive(Clone, Debug, PartialEq, PartialOrd)]
#[expect(clippy::derive_partial_eq_without_eq)]
pub struct SortedVec <T : PartialOrd> {
  vec : Vec <T>
}

/// Forward sorted set
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct SortedSet <T : PartialOrd> {
  set : SortedVec <T>
}

/// Reverse sorted vector
#[derive(Clone, Debug, PartialEq, PartialOrd)]
#[expect(clippy::derive_partial_eq_without_eq)]
pub struct ReverseSortedVec <T : PartialOrd> {
  vec : Vec <T>
}

/// Reverse sorted set
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct ReverseSortedSet <T : PartialOrd> {
  set : ReverseSortedVec <T>
}

/// Unwraps a `partial_cmp`
fn partial_compare <T : PartialOrd> (lhs : &T, rhs : &T) -> std::cmp::Ordering {
  lhs.partial_cmp (rhs).unwrap()
}

//
//  impl SortedVec
//

impl <T : PartialOrd> SortedVec <T> {
  #[inline]
  pub const fn new() -> Self {
    SortedVec { vec: Vec::new() }
  }
  #[inline]
  pub fn with_capacity (capacity : usize) -> Self {
    SortedVec { vec: Vec::with_capacity (capacity) }
  }
  /// Uses `sort_unstable_by()` to sort in place.
  #[inline]
  pub fn from_unsorted (mut vec : Vec <T>) -> Self {
    vec.sort_unstable_by (partial_compare);
    SortedVec { vec }
  }
  /// Insert an element into sorted position, returning the order index at which
  /// it was placed.
  ///
  /// Partial order comparison panics if items are not comparable.
  pub fn insert (&mut self, element : T) -> usize {
    let insert_at = match self.binary_search (&element) {
      Ok (insert_at) | Err (insert_at) => insert_at
    };
    self.vec.insert (insert_at, element);
    insert_at
  }
  /// Find the element and return the index with `Ok`, otherwise insert the
  /// element and return the new element index with `Err`.
  ///
  /// Partial order comparison panics if items are not comparable.
  #[inline]
  pub fn find_or_insert (&mut self, element : T) -> Result <usize, usize> {
    self.binary_search (&element)
      .inspect_err (|&insert_at| self.vec.insert (insert_at, element))
  }
  #[inline]
  pub fn remove_item (&mut self, item : &T) -> Option <T> {
    match self.vec.binary_search_by (
      |other_item| partial_compare (other_item, item)
    ) {
      Ok  (remove_at) => Some (self.vec.remove (remove_at)),
      Err (_)         => None
    }
  }
  /// Panics if index is out of bounds
  #[inline]
  pub fn remove_index (&mut self, index : usize) -> T {
    self.vec.remove (index)
  }
  #[inline]
  pub fn binary_search (&self, x : &T) -> Result <usize, usize> {
    self.vec.binary_search_by (|y| partial_compare (y, x))
  }
  #[inline]
  pub fn pop (&mut self) -> Option <T> {
    self.vec.pop()
  }
  #[inline]
  pub fn clear (&mut self) {
    self.vec.clear()
  }
  #[inline]
  pub fn dedup (&mut self) {
    self.vec.dedup();
  }
  #[inline]
  pub fn dedup_by_key <F, K> (&mut self, key : F) where
    F : FnMut (&mut T) -> K,
    K : PartialEq <K>
  {
    self.vec.dedup_by_key (key);
  }
  #[inline]
  #[expect(mismatched_lifetime_syntaxes)]
  pub fn drain <R> (&mut self, range : R) -> std::vec::Drain <T> where
    R : std::ops::RangeBounds <usize>
  {
    self.vec.drain (range)
  }
  #[inline]
  pub fn retain <F> (&mut self, f : F) where F : FnMut (&T) -> bool {
    self.vec.retain (f)
  }
  /// NOTE: `to_vec()` is a slice method that is accessible through deref,
  /// use this instead to avoid cloning
  #[inline]
  pub fn into_vec (self) -> Vec <T> {
    self.vec
  }
  /// Apply a closure mutating the sorted vector and use `sort_unstable_by()` to
  /// re-sort the mutated vector
  pub fn mutate_vec <F, O> (&mut self, f : F) -> O where
    F : FnOnce (&mut Vec <T>) -> O
  {
    let res = f (&mut self.vec);
    self.vec.sort_unstable_by (partial_compare);
    res
  }
  /// The caller must ensure that the provided vector is already sorted.
  ///
  /// # Safety
  ///
  /// There is a debug assertion that the input is sorted.
  ///
  /// ```should_panic
  /// use sorted_vec::partial::SortedVec;
  /// let v = vec![4.0, 3.0, 2.0];
  /// let _s = unsafe { SortedVec::from_sorted(v) };  // panic!
  /// ```
  #[inline]
  pub unsafe fn from_sorted(vec : Vec<T>) -> Self {
    debug_assert!(vec.is_sorted());
    SortedVec { vec }
  }
}
impl <T : PartialOrd> Default for SortedVec <T> {
  fn default() -> Self {
    Self::new()
  }
}
impl <T : PartialOrd> From <Vec <T>> for SortedVec <T> {
  fn from (unsorted : Vec <T>) -> Self {
    Self::from_unsorted (unsorted)
  }
}
impl <T : PartialOrd> std::ops::Deref for SortedVec <T> {
  type Target = Vec <T>;
  fn deref (&self) -> &Vec <T> {
    &self.vec
  }
}
impl <T : PartialOrd> Extend <T> for SortedVec <T> {
  fn extend <I : IntoIterator <Item = T>> (&mut self, iter : I) {
    for t in iter {
      let _ = self.insert (t);
    }
  }
}
impl <T : PartialOrd> FromIterator <T> for SortedVec <T> {
  fn from_iter <I> (iter : I) -> Self where I : IntoIterator <Item=T> {
    let mut s = SortedVec::new();
    s.extend (iter);
    s
  }
}
impl <T : PartialOrd> IntoIterator for SortedVec <T> {
  type Item = T;
  type IntoIter = std::vec::IntoIter<T>;
  fn into_iter (self) -> Self::IntoIter {
    self.vec.into_iter()
  }
}
impl<'a, T : PartialOrd> IntoIterator for &'a SortedVec<T> {
  type Item = &'a T;
  type IntoIter = std::slice::Iter<'a, T>;
  fn into_iter (self) -> Self::IntoIter {
    self.vec.iter()
  }
}
impl <T : PartialOrd + Hash> Hash for SortedVec <T> {
  fn hash <H : Hasher> (&self, state : &mut H) {
    let v : &Vec <T> = self.as_ref();
    v.hash (state);
  }
}

//
//  impl SortedSet
//

impl <T : PartialOrd> SortedSet <T> {
  #[inline]
  pub const fn new() -> Self {
    SortedSet { set: SortedVec::new() }
  }
  #[inline]
  pub fn with_capacity (capacity : usize) -> Self {
    SortedSet { set: SortedVec::with_capacity (capacity) }
  }
  /// Uses `sort_unstable()` to sort in place and `dedup()` to remove
  /// duplicates.
  #[inline]
  pub fn from_unsorted (vec : Vec <T>) -> Self {
    let mut set = SortedVec::from_unsorted (vec);
    set.dedup();
    SortedSet { set }
  }
  /// Insert an element into sorted position, returning the order index at which
  /// it was placed.
  #[inline]
  pub fn insert (&mut self, element : T) -> usize {
    let _ = self.remove_item (&element);
    self.set.insert (element)
  }
  /// Find the element and return the index with `Ok`, otherwise insert the
  /// element and return the new element index with `Err`.
  #[inline]
  pub fn find_or_insert (&mut self, element : T) -> Result <usize, usize> {
    self.set.find_or_insert (element)
  }
  #[inline]
  pub fn remove_item (&mut self, item : &T) -> Option <T> {
    self.set.remove_item (item)
  }
  /// Panics if index is out of bounds
  #[inline]
  pub fn remove_index (&mut self, index : usize) -> T {
    self.set.remove_index (index)
  }
  #[inline]
  pub fn pop (&mut self) -> Option <T> {
    self.set.pop()
  }
  #[inline]
  pub fn clear (&mut self) {
    self.set.clear()
  }
  #[inline]
  #[expect(mismatched_lifetime_syntaxes)]
  pub fn drain <R> (&mut self, range : R) -> std::vec::Drain <T> where
    R : std::ops::RangeBounds <usize>
  {
    self.set.drain (range)
  }
  #[inline]
  pub fn retain <F> (&mut self, f : F) where F : FnMut (&T) -> bool {
    self.set.retain (f)
  }
  /// NOTE: `to_vec()` is a slice method that is accessible through deref, use
  /// this instead to avoid cloning
  #[inline]
  pub fn into_vec (self) -> Vec <T> {
    self.set.into_vec()
  }
  /// Apply a closure mutating the sorted vector and use `sort_unstable()`
  /// to re-sort the mutated vector and `dedup()` to remove any duplicate
  /// values
  pub fn mutate_vec <F, O> (&mut self, f : F) -> O where
    F : FnOnce (&mut Vec <T>) -> O
  {
    let res = self.set.mutate_vec (f);
    self.set.dedup();
    res
  }
  /// The caller must ensure that the provided vector is already sorted and deduped.
  ///
  /// ```
  /// use sorted_vec::partial::SortedSet;
  /// let v = vec![1.0, 2.0, 3.0, 4.0];
  /// let _s = unsafe { SortedSet::from_sorted(v) };
  /// ```
  ///
  /// # Safety
  ///
  /// Not safe.
  ///
  /// # Panics
  ///
  /// There will be debug assertions if the input is not sorted or deduped.
  ///
  /// ```should_panic
  /// use sorted_vec::partial::SortedSet;
  /// let v = vec![4.0, 3.0, 2.0];
  /// let _s = unsafe { SortedSet::from_sorted(v) };  // panic!
  /// ```
  ///
  /// ```should_panic
  /// use sorted_vec::partial::SortedSet;
  /// let v = vec![1.0, 2.0, 3.0, 3.0, 4.0];
  /// let _s = unsafe { SortedSet::from_sorted(v) };  // panic!
  /// ```
  #[inline]
  pub unsafe fn from_sorted(vec : Vec<T>) -> Self {
    let set = unsafe { SortedVec::from_sorted(vec) };
    if cfg!(debug_assertions) {
      for i in 0..set.len()-1 {
        #[expect(clippy::manual_assert)]   // T is not Debug, can't use assert
        if set[i] == set[i+1] {
          panic!("input contains duplicates")
        }
      }
    }
    SortedSet { set }
  }
}
impl <T : PartialOrd> Default for SortedSet <T> {
  fn default() -> Self {
    Self::new()
  }
}
impl <T : PartialOrd> From <Vec <T>> for SortedSet <T> {
  fn from (unsorted : Vec <T>) -> Self {
    Self::from_unsorted (unsorted)
  }
}
impl <T : PartialOrd> std::ops::Deref for SortedSet <T> {
  type Target = SortedVec <T>;
  fn deref (&self) -> &SortedVec <T> {
    &self.set
  }
}
impl <T : PartialOrd> Extend <T> for SortedSet <T> {
  fn extend <I : IntoIterator <Item = T>> (&mut self, iter : I) {
    for t in iter {
      let _ = self.insert (t);
    }
  }
}
impl <T : PartialOrd> FromIterator <T> for SortedSet <T> {
  fn from_iter <I> (iter : I) -> Self where I : IntoIterator <Item=T> {
    let mut s = SortedSet::new();
    s.extend (iter);
    s
  }
}
impl<T : PartialOrd> IntoIterator for SortedSet<T> {
  type Item = T;
  type IntoIter = std::vec::IntoIter<T>;
  fn into_iter (self) -> Self::IntoIter {
    self.set.vec.into_iter()
  }
}
impl<'a, T : PartialOrd> IntoIterator for &'a SortedSet<T> {
  type Item = &'a T;
  type IntoIter = std::slice::Iter<'a, T>;
  fn into_iter (self) -> Self::IntoIter {
    self.set.iter()
  }
}
impl <T : PartialOrd + Hash> Hash for SortedSet <T> {
  fn hash <H : Hasher> (&self, state : &mut H) {
    let v : &Vec <T> = self.as_ref();
    v.hash (state);
  }
}

//
//  impl ReverseSortedVec
//

impl <T : PartialOrd> ReverseSortedVec <T> {
  #[inline]
  pub const fn new() -> Self {
    ReverseSortedVec { vec: Vec::new() }
  }
  #[inline]
  pub fn with_capacity (capacity : usize) -> Self {
    ReverseSortedVec { vec: Vec::with_capacity (capacity) }
  }
  /// Uses `sort_unstable_by()` to sort in place.
  #[inline]
  pub fn from_unsorted (mut vec : Vec <T>) -> Self {
    vec.sort_unstable_by (|x,y| partial_compare (x,y).reverse());
    ReverseSortedVec { vec }
  }
  /// Insert an element into (reverse) sorted position, returning the order
  /// index at which it was placed.
  ///
  /// Partial order comparison panics if items are not comparable.
  pub fn insert (&mut self, element : T) -> usize {
    let insert_at = match self.binary_search (&element) {
      Ok (insert_at) | Err (insert_at) => insert_at
    };
    self.vec.insert (insert_at, element);
    insert_at
  }
  /// Find the element and return the index with `Ok`, otherwise insert the
  /// element and return the new element index with `Err`.
  ///
  /// Partial order comparison panics if items are not comparable.
  #[inline]
  pub fn find_or_insert (&mut self, element : T) -> Result <usize, usize> {
    self.binary_search (&element)
      .inspect_err (|&insert_at| self.vec.insert (insert_at, element))
  }
  #[inline]
  pub fn remove_item (&mut self, item : &T) -> Option <T> {
    match self.vec.binary_search_by (
      |other_item| partial_compare (other_item, item).reverse()
    ) {
      Ok  (remove_at) => Some (self.vec.remove (remove_at)),
      Err (_)         => None
    }
  }
  /// Panics if index is out of bounds
  #[inline]
  pub fn remove_index (&mut self, index : usize) -> T {
    self.vec.remove (index)
  }
  #[inline]
  pub fn binary_search (&self, x : &T) -> Result <usize, usize> {
    self.vec.binary_search_by (|y| partial_compare (y, x).reverse())
  }
  #[inline]
  pub fn pop (&mut self) -> Option <T> {
    self.vec.pop()
  }
  #[inline]
  pub fn clear (&mut self) {
    self.vec.clear()
  }
  pub fn dedup (&mut self) {
    self.vec.dedup();
  }
  #[inline]
  pub fn dedup_by_key <F, K> (&mut self, key : F) where
    F : FnMut (&mut T) -> K,
    K : PartialEq <K>
  {
    self.vec.dedup_by_key (key);
  }
  #[inline]
  #[expect(mismatched_lifetime_syntaxes)]
  pub fn drain <R> (&mut self, range : R) -> std::vec::Drain <T> where
    R : std::ops::RangeBounds <usize>
  {
    self.vec.drain (range)
  }
  #[inline]
  pub fn retain <F> (&mut self, f : F) where F : FnMut (&T) -> bool {
    self.vec.retain (f)
  }
  /// NOTE: `to_vec()` is a slice method that is accessible through deref,
  /// use this instead to avoid cloning
  #[inline]
  pub fn into_vec (self) -> Vec <T> {
    self.vec
  }
  /// Apply a closure mutating the reverse-sorted vector and use
  /// `sort_unstable_by()` to re-sort the mutated vector
  pub fn mutate_vec <F, O> (&mut self, f : F) -> O where
    F : FnOnce (&mut Vec <T>) -> O
  {
    let res = f (&mut self.vec);
    self.vec.sort_unstable_by (|x,y| partial_compare (x,y).reverse());
    res
  }
}
impl <T : PartialOrd> Default for ReverseSortedVec <T> {
  fn default() -> Self {
    Self::new()
  }
}
impl <T : PartialOrd> From <Vec <T>> for ReverseSortedVec <T> {
  fn from (unsorted : Vec <T>) -> Self {
    Self::from_unsorted (unsorted)
  }
}
impl <T : PartialOrd> std::ops::Deref for ReverseSortedVec <T> {
  type Target = Vec <T>;
  fn deref (&self) -> &Vec <T> {
    &self.vec
  }
}
impl <T : PartialOrd> Extend <T> for ReverseSortedVec <T> {
  fn extend <I : IntoIterator <Item = T>> (&mut self, iter : I) {
    for t in iter {
      let _ = self.insert (t);
    }
  }
}
impl <T : PartialOrd> FromIterator <T> for ReverseSortedVec <T> {
  fn from_iter <I> (iter : I) -> Self where I : IntoIterator <Item=T> {
    let mut s = ReverseSortedVec::new();
    s.extend (iter);
    s
  }
}
impl <T : PartialOrd + Hash> Hash for ReverseSortedVec <T> {
  fn hash <H : Hasher> (&self, state : &mut H) {
    let v : &Vec <T> = self.as_ref();
    v.hash (state);
  }
}

//
//  impl ReverseSortedSet
//

impl <T : PartialOrd> ReverseSortedSet <T> {
  #[inline]
  pub const fn new() -> Self {
    ReverseSortedSet { set: ReverseSortedVec::new() }
  }
  #[inline]
  pub fn with_capacity (capacity : usize) -> Self {
    ReverseSortedSet { set: ReverseSortedVec::with_capacity (capacity) }
  }
  /// Uses `sort_unstable()` to sort in place and `dedup()` to remove
  /// duplicates.
  #[inline]
  pub fn from_unsorted (vec : Vec <T>) -> Self {
    let mut set = ReverseSortedVec::from_unsorted (vec);
    set.dedup();
    ReverseSortedSet { set }
  }
  /// Insert an element into sorted position, returning the order index at which
  /// it was placed.
  #[inline]
  pub fn insert (&mut self, element : T) -> usize {
    let _ = self.remove_item (&element);
    self.set.insert (element)
  }
  /// Find the element and return the index with `Ok`, otherwise insert the
  /// element and return the new element index with `Err`.
  #[inline]
  pub fn find_or_insert (&mut self, element : T) -> Result <usize, usize> {
    self.set.find_or_insert (element)
  }
  #[inline]
  pub fn remove_item (&mut self, item : &T) -> Option <T> {
    self.set.remove_item (item)
  }
  /// Panics if index is out of bounds
  #[inline]
  pub fn remove_index (&mut self, index : usize) -> T {
    self.set.remove_index (index)
  }
  #[inline]
  pub fn pop (&mut self) -> Option <T> {
    self.set.pop()
  }
  #[inline]
  pub fn clear (&mut self) {
    self.set.clear()
  }
  #[inline]
  #[expect(mismatched_lifetime_syntaxes)]
  pub fn drain <R> (&mut self, range : R) -> std::vec::Drain <T> where
    R : std::ops::RangeBounds <usize>
  {
    self.set.drain (range)
  }
  #[inline]
  pub fn retain <F> (&mut self, f : F) where F : FnMut (&T) -> bool {
    self.set.retain (f)
  }
  /// NOTE: `to_vec()` is a slice method that is accessible through deref, use
  /// this instead to avoid cloning
  #[inline]
  pub fn into_vec (self) -> Vec <T> {
    self.set.into_vec()
  }
  /// Apply a closure mutating the sorted vector and use `sort_unstable()`
  /// to re-sort the mutated vector and `dedup()` to remove any duplicate
  /// values
  pub fn mutate_vec <F, O> (&mut self, f : F) -> O where
    F : FnOnce (&mut Vec <T>) -> O
  {
    let res = self.set.mutate_vec (f);
    self.set.dedup();
    res
  }
}
impl <T : PartialOrd> Default for ReverseSortedSet <T> {
  fn default() -> Self {
    Self::new()
  }
}
impl <T : PartialOrd> From <Vec <T>> for ReverseSortedSet <T> {
  fn from (unsorted : Vec <T>) -> Self {
    Self::from_unsorted (unsorted)
  }
}
impl <T : PartialOrd> std::ops::Deref for ReverseSortedSet <T> {
  type Target = ReverseSortedVec <T>;
  fn deref (&self) -> &ReverseSortedVec <T> {
    &self.set
  }
}
impl <T : PartialOrd> Extend <T> for ReverseSortedSet <T> {
  fn extend <I : IntoIterator <Item = T>> (&mut self, iter : I) {
    for t in iter {
      let _ = self.insert (t);
    }
  }
}
impl <T : PartialOrd> FromIterator <T> for ReverseSortedSet <T> {
  fn from_iter <I> (iter : I) -> Self where I : IntoIterator <Item=T> {
    let mut s = ReverseSortedSet::new();
    s.extend (iter);
    s
  }
}
impl <T : PartialOrd + Hash> Hash for ReverseSortedSet <T> {
  fn hash <H : Hasher> (&self, state : &mut H) {
    let v : &Vec <T> = self.as_ref();
    v.hash (state);
  }
}

#[cfg(test)]
mod tests {
  // NOTE: some tests may break in future version of Rust: according to the
  // documentation of binary_search, if there are multiple matches the index is
  // chosen deterministically but is subject to change in future versions of
  // Rust
  use super::*;

  #[test]
  fn sorted_vec() {
    let mut v = SortedVec::new();
    assert_eq!(v.insert (5.0), 0);
    assert_eq!(v.insert (3.0), 0);
    assert_eq!(v.insert (4.0), 1);
    assert_eq!(v.insert (4.0), 1);
    assert_eq!(v.find_or_insert (4.0), Ok (2));
    assert_eq!(v.len(), 4);
    v.dedup();
    assert_eq!(v.len(), 3);
    assert_eq!(v.binary_search (&3.0), Ok (0));
    assert_eq!(*SortedVec::from_unsorted (
      vec![  5.0, -10.0, 99.0, -11.0,  2.0, 17.0, 10.0]),
      vec![-11.0, -10.0,  2.0,   5.0, 10.0, 17.0, 99.0]);
    assert_eq!(SortedVec::from_unsorted (
      vec![  5.0, -10.0, 99.0, -11.0,  2.0, 17.0, 10.0]),
      vec![  5.0, -10.0, 99.0, -11.0,  2.0, 17.0, 10.0].into());
    let mut v = SortedVec::new();
    v.extend([5.0, -10.0, 99.0, -11.0, 2.0, 17.0, 10.0]);
    assert_eq!(
      v.drain(..).collect::<Vec <f32>>(),
      vec![-11.0, -10.0, 2.0, 5.0, 10.0, 17.0, 99.0]);
    let v = SortedVec::from_iter (
      [5.0, -10.0, 99.0, -11.0, 2.0, 17.0, 10.0]);
    assert_eq!(**v, [-11.0, -10.0, 2.0, 5.0, 10.0, 17.0, 99.0]);
  }

  #[test]
  fn sorted_set() {
    let mut s = SortedSet::new();
    assert_eq!(s.insert (5.0), 0);
    assert_eq!(s.insert (3.0), 0);
    assert_eq!(s.insert (4.0), 1);
    assert_eq!(s.insert (4.0), 1);
    assert_eq!(s.find_or_insert (4.0), Ok (1));
    assert_eq!(s.len(), 3);
    assert_eq!(s.binary_search (&3.0), Ok (0));
    assert_eq!(**SortedSet::from_unsorted (
      vec![  5.0, -10.0, 99.0, -10.0, -11.0,  10.0, 2.0, 17.0, 10.0]),
      vec![-11.0, -10.0,  2.0,   5.0, 10.0, 17.0, 99.0]);
    assert_eq!(SortedSet::from_unsorted (
      vec![  5.0, -10.0, 99.0, -10.0, -11.0,  10.0, 2.0, 17.0, 10.0]),
      vec![  5.0, -10.0, 99.0, -10.0, -11.0,  10.0, 2.0, 17.0, 10.0].into());
    let mut s = SortedSet::new();
    s.extend(
      vec![5.0, -11.0, -10.0, 99.0, -11.0, 2.0, 17.0, 2.0, 10.0]);
    assert_eq!(**s, vec![-11.0, -10.0, 2.0, 5.0, 10.0, 17.0, 99.0]);
    let () = s.mutate_vec (|s|{
      s[0] = 5.0;
      s[3] = 11.0;
    });
    assert_eq!(
      s.drain(..).collect::<Vec <f32>>(),
      vec![-10.0, 2.0, 5.0, 10.0, 11.0, 17.0, 99.0]);
    let s = SortedSet::from_iter (
      [5.0, -11.0, -10.0, 99.0, -11.0, 2.0, 17.0, 2.0, 10.0]);
    assert_eq!(***s, [-11.0, -10.0, 2.0, 5.0, 10.0, 17.0, 99.0]);
  }

  #[test]
  fn reverse_sorted_vec() {
    let mut v = ReverseSortedVec::new();
    assert_eq!(v.insert (5.0), 0);
    assert_eq!(v.insert (3.0), 1);
    assert_eq!(v.insert (4.0), 1);
    assert_eq!(v.find_or_insert (6.0), Err (0));
    assert_eq!(v.insert (4.0), 2);
    assert_eq!(v.find_or_insert (4.0), Ok (3));
    assert_eq!(v.len(), 5);
    v.dedup();
    assert_eq!(v.len(), 4);
    assert_eq!(v.binary_search (&3.0), Ok (3));
    assert_eq!(*ReverseSortedVec::from_unsorted (
      vec![5.0, -10.0, 99.0, -11.0, 2.0,  17.0,  10.0]),
      vec![99.0, 17.0, 10.0,   5.0, 2.0, -10.0, -11.0]);
    assert_eq!(ReverseSortedVec::from_unsorted (
      vec![5.0, -10.0, 99.0, -11.0, 2.0,  17.0,  10.0]),
      vec![5.0, -10.0, 99.0, -11.0, 2.0,  17.0,  10.0].into());
    let mut v = ReverseSortedVec::new();
    v.extend([5.0, -10.0, 99.0, -11.0, 2.0, 17.0, 10.0]);
    assert_eq!(
      v.drain(..).collect::<Vec <f32>>(),
      vec![99.0, 17.0, 10.0, 5.0, 2.0, -10.0, -11.0]);
    let v = ReverseSortedVec::from_iter (
      [5.0, -10.0, 99.0, -11.0, 2.0, 17.0, 10.0]);
    assert_eq!(**v, [99.0, 17.0, 10.0, 5.0, 2.0, -10.0, -11.0]);
  }

  #[test]
  fn reverse_sorted_set() {
    let mut s = ReverseSortedSet::new();
    assert_eq!(s.insert (5.0), 0);
    assert_eq!(s.insert (3.0), 1);
    assert_eq!(s.insert (4.0), 1);
    assert_eq!(s.find_or_insert (6.0), Err (0));
    assert_eq!(s.insert (4.0), 2);
    assert_eq!(s.find_or_insert (4.0), Ok (2));
    assert_eq!(s.len(), 4);
    assert_eq!(s.binary_search (&3.0), Ok (3));
    assert_eq!(**ReverseSortedSet::from_unsorted (
      vec![5.0, -10.0, 99.0, -11.0, 2.0,  17.0,  10.0, -10.0]),
      vec![99.0, 17.0, 10.0,   5.0, 2.0, -10.0, -11.0]);
    assert_eq!(ReverseSortedSet::from_unsorted (
      vec![5.0, -10.0, 99.0, -11.0, 2.0,  17.0,  10.0, -10.0]),
      vec![5.0, -10.0, 99.0, -11.0, 2.0,  17.0,  10.0, -10.0].into());
    let mut s = ReverseSortedSet::new();
    s.extend(vec![5.0, -10.0, 2.0, 99.0, -11.0, -11.0, 2.0, 17.0, 10.0]);
    assert_eq!(**s, vec![99.0, 17.0, 10.0, 5.0, 2.0, -10.0, -11.0]);
    let () = s.mutate_vec (|s|{
      s[6] = 17.0;
      s[3] = 1.0;
    });
    assert_eq!(
      s.drain(..).collect::<Vec <f32>>(),
      vec![99.0, 17.0, 10.0, 2.0, 1.0, -10.0]);
    let s = ReverseSortedSet::from_iter(
      [5.0, -10.0, 2.0, 99.0, -11.0, -11.0, 2.0, 17.0, 10.0]);
    assert_eq!(***s, [99.0, 17.0, 10.0, 5.0, 2.0, -10.0, -11.0]);
  }
}
