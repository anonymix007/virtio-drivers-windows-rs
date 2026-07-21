//! Sorted vectors.
//!
//! [Repository](https://gitlab.com/spearman/sorted-vec)
//!
//! - `SortedVec` -- sorted from least to greatest, may contain duplicates
//! - `SortedSet` -- sorted from least to greatest, unique elements
//! - `ReverseSortedVec` -- sorted from greatest to least, may contain
//!   duplicates
//! - `ReverseSortedSet` -- sorted from greatest to least, unique elements
//!
//! The `partial` module provides sorted vectors of types that only implement
//! `PartialOrd` where comparison of incomparable elements results in runtime
//! panic.

#[cfg(feature = "serde")]
#[macro_use] extern crate serde;

use std::hash::{Hash, Hasher};

pub mod partial;

/// Forward sorted vector
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(all(feature = "serde", not(feature = "serde-nontransparent")),
  serde(transparent))]
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SortedVec <T : Ord> {
  #[cfg_attr(feature = "serde", serde(deserialize_with = "SortedVec::parse_vec"))]
  #[cfg_attr(feature = "serde",
    serde(bound(deserialize = "T : serde::Deserialize <'de>")))]
  vec : Vec <T>
}

/// Forward sorted set
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(all(feature = "serde", not(feature = "serde-nontransparent")),
  serde(transparent))]
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SortedSet <T : Ord> {
  #[cfg_attr(feature = "serde", serde(deserialize_with = "SortedSet::parse_vec"))]
  #[cfg_attr(feature = "serde",
    serde(bound(deserialize = "T : serde::Deserialize <'de>")))]
  set : SortedVec <T>
}

/// Value returned when `find_or_insert` is used.
#[derive(PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
pub enum FindOrInsert {
  /// Contains a found index
  Found(usize),

  /// Contains an inserted index
  Inserted(usize),
}

/// Converts from the `binary_search` result type into the `FindOrInsert` type
impl From <Result <usize, usize>> for FindOrInsert {
  fn from (result : Result<usize, usize>) -> Self {
    match result {
      Ok  (value) => FindOrInsert::Found    (value),
      Err (value) => FindOrInsert::Inserted (value),
    }
  }
}

impl FindOrInsert {
  /// Get the index of the element that was either found or inserted.
  pub const fn index (&self) -> usize {
    match self {
      FindOrInsert::Found(value) | FindOrInsert::Inserted(value) => *value
    }
  }

  /// If an equivalent element was found in the container, get the value of
  /// its index. Otherwise get None.
  pub const fn found (&self) -> Option<usize> {
    match self {
      FindOrInsert::Found(value) => Some(*value),
      FindOrInsert::Inserted(_) => None
    }
  }

  /// If the provided element was inserted into the container, get the value
  /// of its index. Otherwise get None.
  pub const fn inserted (&self) -> Option<usize> {
    match self {
      FindOrInsert::Found(_) => None,
      FindOrInsert::Inserted(value) => Some(*value)
    }
  }

  /// Returns true if the element was found.
  pub const fn is_found (&self) -> bool {
    matches!(self, FindOrInsert::Found(_))
  }

  /// Returns true if the element was inserted.
  pub const fn is_inserted (&self) -> bool {
    matches!(self, FindOrInsert::Inserted(_))
  }
}

//
//  impl SortedVec
//

impl <T : Ord> SortedVec <T> {
  #[inline]
  pub const fn new() -> Self {
    SortedVec { vec: Vec::new() }
  }
  #[inline]
  pub fn with_capacity (capacity : usize) -> Self {
    SortedVec { vec: Vec::with_capacity (capacity) }
  }
  /// Uses `sort_unstable()` to sort in place.
  #[inline]
  pub fn from_unsorted (mut vec : Vec <T>) -> Self {
    vec.sort_unstable();
    SortedVec { vec }
  }
  /// Insert an element into sorted position, returning the order index at which
  /// it was placed.
  pub fn insert (&mut self, element : T) -> usize {
    let insert_at = match self.binary_search (&element) {
      Ok (insert_at) | Err (insert_at) => insert_at
    };
    self.vec.insert (insert_at, element);
    insert_at
  }
  /// Find the element and return the index with `Ok`, otherwise insert the
  /// element and return the new element index with `Err`.
  pub fn find_or_insert (&mut self, element : T) -> FindOrInsert {
    self.binary_search (&element)
      .inspect_err (|&insert_at| self.vec.insert (insert_at, element))
      .into()
  }
  /// Same as insert, except performance is O(1) when the element belongs at the
  /// back of the container. This avoids an O(log(N)) search for inserting
  /// elements at the back.
  #[inline]
  pub fn push(&mut self, element: T) -> usize {
    if let Some(last) = self.vec.last() {
      let cmp = element.cmp(last);
      if cmp == std::cmp::Ordering::Greater || cmp == std::cmp::Ordering::Equal {
        // The new element is greater than or equal to the current last element,
        // so we can simply push it onto the vec.
        self.vec.push(element);
        self.vec.len() - 1
      } else {
        // The new element is less than the last element in the container, so we
        // cannot simply push. We will fall back on the normal insert behavior.
        self.insert(element)
      }
    } else {
      // If there is no last element then the container must be empty, so we
      // can simply push the element and return its index, which must be 0.
      self.vec.push(element);
      0
    }
  }
  /// Reserves additional capacity in the underlying vector.
  /// See `std::vec::Vec::reserve`.
  #[inline]
  pub fn reserve(&mut self, additional: usize) {
    self.vec.reserve(additional);
  }
  /// Same as `find_or_insert`, except performance is O(1) when the element
  /// belongs at the back of the container.
  pub fn find_or_push(&mut self, element: T) -> FindOrInsert {
    if let Some(last) = self.vec.last() {
      let cmp = element.cmp(last);
      if cmp == std::cmp::Ordering::Equal {
        FindOrInsert::Found(self.vec.len() - 1)
      } else if cmp == std::cmp::Ordering::Greater {
        self.vec.push(element);
        FindOrInsert::Inserted(self.vec.len() - 1)
      } else {
        // The new element is less than the last element in the container, so we
        // need to fall back on the regular find_or_insert
        self.find_or_insert(element)
      }
    } else {
      // If there is no last element then the container must be empty, so we can
      // simply push the element and return that it was inserted.
      self.vec.push(element);
      FindOrInsert::Inserted(0)
    }
  }
  #[inline]
  pub fn remove_item (&mut self, item : &T) -> Option <T> {
    match self.vec.binary_search (item) {
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
  /// NOTE: `to_vec()` is a slice method that is accessible through deref, use
  /// this instead to avoid cloning
  #[inline]
  pub fn into_vec (self) -> Vec <T> {
    self.vec
  }
  /// Apply a closure mutating the sorted vector and use `sort_unstable()`
  /// to re-sort the mutated vector
  pub fn mutate_vec <F, O> (&mut self, f : F) -> O where
    F : FnOnce (&mut Vec <T>) -> O
  {
    let res = f (&mut self.vec);
    self.vec.sort_unstable();
    res
  }
  /// The caller must ensure that the provided vector is already sorted.
  ///
  /// # Safety
  ///
  /// There is a debug assertion that the input is sorted.
  ///
  /// ```should_panic
  /// use sorted_vec::SortedVec;
  /// let v = vec![4, 3, 2];
  /// let _s = unsafe { SortedVec::from_sorted(v) };  // panic!
  /// ```
  #[inline]
  pub unsafe fn from_sorted(vec : Vec<T>) -> Self {
    debug_assert!(vec.is_sorted());
    SortedVec { vec }
  }
  /// Unsafe access to the underlying vector. The caller must ensure that any
  /// changes to the values in the vector do not impact the ordering of the
  /// elements inside, or else this container will misbehave.
  ///
  /// # Safety
  ///
  /// Not safe.
  pub const unsafe fn get_unchecked_mut_vec(&mut self) -> &mut Vec<T> {
    &mut self.vec
  }

  /// Perform sorting on the input sequence when deserializing with `serde`.
  ///
  /// Use with `#[serde(deserialize_with = "SortedVec::deserialize_unsorted")]`:
  /// ```text
  /// #[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
  /// pub struct Foo {
  ///   #[serde(deserialize_with = "SortedVec::deserialize_unsorted")]
  ///   pub v : SortedVec <u64>
  /// }
  /// ```
  #[cfg(feature = "serde")]
  pub fn deserialize_unsorted <'de, D> (deserializer : D)
    -> Result <Self, D::Error>
  where
    D : serde::Deserializer <'de>,
    T : serde::Deserialize <'de>
  {
    use serde::Deserialize;
    let v = Vec::deserialize (deserializer)?;
    Ok (SortedVec::from_unsorted (v))
  }

  #[cfg(feature = "serde")]
  fn parse_vec <'de, D> (deserializer : D) -> Result <Vec <T>, D::Error> where
    D : serde::Deserializer <'de>,
    T : serde::Deserialize <'de>
  {
    use serde::Deserialize;
    use serde::de::Error;
    let v = Vec::deserialize (deserializer)?;
    if !v.is_sorted() {
      Err (D::Error::custom ("input sequence is not sorted"))
    } else {
      Ok (v)
    }
  }
}
impl <T : Ord> Default for SortedVec <T> {
  fn default() -> Self {
    Self::new()
  }
}
impl <T : Ord> From <Vec <T>> for SortedVec <T> {
  fn from (unsorted : Vec <T>) -> Self {
    Self::from_unsorted (unsorted)
  }
}
impl <T : Ord> std::ops::Deref for SortedVec <T> {
  type Target = Vec <T>;
  fn deref (&self) -> &Vec <T> {
    &self.vec
  }
}
impl <T : Ord> Extend <T> for SortedVec <T> {
  fn extend <I : IntoIterator <Item = T>> (&mut self, iter : I) {
    for t in iter {
      let _ = self.insert (t);
    }
  }
}
impl <T : Ord> FromIterator <T> for SortedVec <T> {
  fn from_iter <I> (iter : I) -> Self where I : IntoIterator <Item=T> {
    let mut s = SortedVec::new();
    s.extend (iter);
    s
  }
}
impl <T : Ord> IntoIterator for SortedVec <T> {
  type Item = T;
  type IntoIter = std::vec::IntoIter <T>;
  fn into_iter (self) -> Self::IntoIter {
    self.vec.into_iter()
  }
}
impl <'a, T : Ord> IntoIterator for &'a SortedVec <T> {
  type Item = &'a T;
  type IntoIter = std::slice::Iter <'a, T>;
  fn into_iter (self) -> Self::IntoIter {
    self.vec.iter()
  }
}
impl <T : Ord + Hash> Hash for SortedVec <T> {
  fn hash <H : Hasher> (&self, state : &mut H) {
    let v : &Vec <T> = self.as_ref();
    v.hash (state);
  }
}

//
//  impl SortedSet
//

impl <T : Ord> SortedSet <T> {
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
  /// it was placed. If an existing item was found it will be returned.
  #[inline]
  pub fn replace (&mut self, mut element : T) -> (usize, Option <T>) {
    match self.set.binary_search(&element) {
      Ok (existing_index) => {
        unsafe {
          // If binary_search worked correctly, then this must be the index of a
          // valid element to get from the vector.
          std::mem::swap (&mut element,
            self.set.vec.get_unchecked_mut(existing_index))
        }
        (existing_index, Some (element))
      },
      Err (insert_index) => {
        self.set.vec.insert(insert_index, element);
        (insert_index, None)
      }
    }
  }
  /// Find the element and return the index with `Ok`, otherwise insert the
  /// element and return the new element index with `Err`.
  #[inline]
  pub fn find_or_insert (&mut self, element : T) -> FindOrInsert {
    self.set.find_or_insert (element)
  }
  /// Same as replace, except performance is O(1) when the element belongs at
  /// the back of the container. This avoids an O(log(N)) search for inserting
  /// elements at the back.
  #[inline]
  pub fn push(&mut self, element: T) -> (usize, Option<T>) {
    if let Some(last) = self.vec.last() {
      let cmp = element.cmp(last);
      if cmp == std::cmp::Ordering::Greater {
        // The new element is greater than the current last element, so we can
        // simply push it onto the vec.
        self.set.vec.push(element);
        (self.vec.len() - 1, None)
      } else if cmp == std::cmp::Ordering::Equal {
        // The new element is equal to the last element, so we can simply return
        // the last index in the vec and the value that is being replaced.
        let original = self.set.vec.pop();
        self.set.vec.push(element);
        (self.vec.len() - 1, original)
      } else {
        // The new element is less than the last element, so we need to fall
        // back on the regular insert function.
        self.replace(element)
      }
    } else {
      // If there is no last element then the container must be empty, so we can
      // simply push the element and return its index, which must be 0.
      self.set.vec.push(element);
      (0, None)
    }
  }
  /// Reserves additional capacity in the underlying vector.
  /// See `std::vec::Vec::reserve`.
  #[inline]
  pub fn reserve(&mut self, additional: usize) {
    self.set.reserve(additional);
  }
  /// Same as `find_or_insert`, except performance is O(1) when the element
  /// belongs at the back of the container.
  pub fn find_or_push(&mut self, element: T) -> FindOrInsert {
    self.set.find_or_insert(element)
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
  /// # Safety
  ///
  /// There will be debug assertions if the input is not sorted or deduped.
  ///
  /// ```should_panic
  /// use sorted_vec::SortedSet;
  /// let v = vec![4, 3, 2];
  /// let _s = unsafe { SortedSet::from_sorted(v) };  // panic!
  /// ```
  ///
  /// ```should_panic
  /// use sorted_vec::SortedSet;
  /// let v = vec![1, 2, 3, 3, 4];
  /// let _s = unsafe { SortedSet::from_sorted(v) };  // panic!
  /// ```
  #[inline]
  pub unsafe fn from_sorted(vec : Vec<T>) -> Self {
    #[expect(clippy::debug_assert_with_mut_call)]
    if cfg!(debug_assertions) {
      let mut unique = std::collections::BTreeSet::new();
      debug_assert!(vec.iter().all(|x| unique.insert(x)));
    }
    let set = unsafe { SortedVec::from_sorted(vec) };
    SortedSet { set }
  }
  /// Unsafe access to the underlying vector. The caller must ensure that any
  /// changes to the values in the vector do not impact the ordering of the
  /// elements inside, or else this container will misbehave.
  ///
  /// # Safety
  ///
  /// Not safe.
  pub const unsafe fn get_unchecked_mut_vec(&mut self) -> &mut Vec<T> {
    unsafe { self.set.get_unchecked_mut_vec() }
  }

  /// Perform deduplication and sorting on the input sequence when deserializing
  /// with `serde`.
  ///
  /// Use with
  /// `#[serde(deserialize_with = "SortedSet::deserialize_dedup_unsorted")]`:
  /// ```text
  /// #[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
  /// pub struct Foo {
  ///   #[serde(deserialize_with = "SortedSet::deserialize_dedup_unsorted")]
  ///   pub s : SortedSet <u64>
  /// }
  /// ```
  #[cfg(feature = "serde")]
  pub fn deserialize_dedup_unsorted <'de, D> (deserializer : D)
    -> Result <Self, D::Error>
  where
    D : serde::Deserializer <'de>,
    T : serde::Deserialize <'de>
  {
    use serde::Deserialize;
    let v = Vec::deserialize (deserializer)?;
    Ok (SortedSet::from_unsorted (v))
  }

  #[cfg(feature = "serde")]
  fn parse_vec <'de, D> (deserializer : D)
    -> Result <SortedVec <T>, D::Error>
  where
    D : serde::Deserializer <'de>,
    T : serde::Deserialize <'de>
  {
    use serde::Deserialize;
    use serde::de::Error;
    let mut vec = Vec::deserialize (deserializer)?;
    let input_len = vec.len();
    vec.dedup();
    if vec.len() != input_len {
      Err (D::Error::custom ("input set contains duplicate values"))
    } else if !vec.is_sorted() {
      Err (D::Error::custom ("input set is not sorted"))
    } else {
      Ok (SortedVec { vec })
    }
  }
}
impl <T : Ord> Default for SortedSet <T> {
  fn default() -> Self {
    Self::new()
  }
}
impl <T : Ord> From <Vec <T>> for SortedSet <T> {
  fn from (unsorted : Vec <T>) -> Self {
    Self::from_unsorted (unsorted)
  }
}
impl <T : Ord> std::ops::Deref for SortedSet <T> {
  type Target = SortedVec <T>;
  fn deref (&self) -> &SortedVec <T> {
    &self.set
  }
}
impl <T : Ord> Extend <T> for SortedSet <T> {
  fn extend <I : IntoIterator <Item = T>> (&mut self, iter : I) {
    for t in iter {
      let _ = self.find_or_insert (t);
    }
  }
}
impl <T : Ord> FromIterator <T> for SortedSet <T> {
  fn from_iter <I> (iter : I) -> Self where I : IntoIterator <Item=T> {
    let mut s = SortedSet::new();
    s.extend (iter);
    s
  }
}
impl <T : Ord> IntoIterator for SortedSet <T> {
  type Item = T;
  type IntoIter = std::vec::IntoIter <T>;
  fn into_iter (self) -> Self::IntoIter {
    self.set.into_iter()
  }
}
impl <'a, T : Ord> IntoIterator for &'a SortedSet <T> {
  type Item = &'a T;
  type IntoIter = std::slice::Iter <'a, T>;
  fn into_iter (self) -> Self::IntoIter {
    self.set.iter()
  }
}
impl <T : Ord + Hash> Hash for SortedSet <T> {
  fn hash <H : Hasher> (&self, state : &mut H) {
    let v : &Vec <T> = self.as_ref();
    v.hash (state);
  }
}

/// Reverse-sorted Containers.
///
/// Use these containers to have the vector sorted in the reverse order of its
/// usual comparison.
///
/// Note that objects going into the reverse container needs to be wrapped in
/// `std::cmp::Reverse`.
///
/// # Examples
///
/// ```
/// use std::cmp::Reverse;
/// use sorted_vec::ReverseSortedVec;
///
/// let mut vec = ReverseSortedVec::<u64>::new();
/// vec.insert(Reverse(10));
/// vec.insert(Reverse(15));
/// assert_eq!(vec.last().unwrap().0, 10);
/// ```
pub type ReverseSortedVec<T> = SortedVec<std::cmp::Reverse<T>>;
pub type ReverseSortedSet<T> = SortedSet<std::cmp::Reverse<T>>;

#[cfg(test)]
mod tests {
  // NOTE: some tests may break in future version of Rust: according to the
  // documentation of binary_search, if there are multiple matches the index is
  // chosen deterministically but is subject to change in future versions of
  // Rust
  use super::*;
  use std::cmp::Reverse;

  #[test]
  fn sorted_vec() {
    let mut v = SortedVec::new();
    assert_eq!(v.insert (5), 0);
    assert_eq!(v.insert (3), 0);
    assert_eq!(v.insert (4), 1);
    assert_eq!(v.insert (4), 1);
    assert_eq!(v.find_or_insert (4), FindOrInsert::Found (2));
    assert_eq!(v.find_or_insert (4).index(), 2);
    assert_eq!(v.len(), 4);
    v.dedup();
    assert_eq!(v.len(), 3);
    assert_eq!(v.binary_search (&3), Ok (0));
    assert_eq!(*SortedVec::from_unsorted (
      vec![5, -10, 99, -11, 2, 17, 10]),
      vec![-11, -10, 2, 5, 10, 17, 99]);
    assert_eq!(SortedVec::from_unsorted (
      vec![5, -10, 99, -11, 2, 17, 10]),
      vec![5, -10, 99, -11, 2, 17, 10].into());
    let mut v = SortedVec::new();
    v.extend([5, -10, 99, -11, 2, 17, 10]);
    assert_eq!(*v, vec![-11, -10, 2, 5, 10, 17, 99]);
    let () = v.mutate_vec (|v|{
      v[0] = 11;
      v[3] = 1;
    });
    assert_eq!(
      v.drain(..).collect::<Vec <i32>>(),
      vec![-10, 1, 2, 10, 11, 17, 99]);
    let v = SortedVec::from_iter ([5, -10, 99, -11, 2, 17, 10]);
    assert_eq!(**v, [-11, -10, 2, 5, 10, 17, 99]);
  }

  #[test]
  fn sorted_vec_push() {
    let mut v = SortedVec::new();
    assert_eq!(v.push (5), 0);
    assert_eq!(v.push (3), 0);
    assert_eq!(v.push (4), 1);
    assert_eq!(v.push (4), 1);
    assert_eq!(v.find_or_push (4), FindOrInsert::Found (2));
    assert_eq!(v.find_or_push (4).index(), 2);
    assert_eq!(v.len(), 4);
    v.dedup();
    assert_eq!(v.len(), 3);
    assert_eq!(v.binary_search (&3), Ok (0));
    assert_eq!(*SortedVec::from_unsorted (
      vec![5, -10, 99, -11, 2, 17, 10]),
      vec![-11, -10, 2, 5, 10, 17, 99]);
    assert_eq!(SortedVec::from_unsorted (
      vec![5, -10, 99, -11, 2, 17, 10]),
      vec![5, -10, 99, -11, 2, 17, 10].into());
    let mut v = SortedVec::new();
    v.extend([5, -10, 99, -11, 2, 17, 10]);
    assert_eq!(*v, vec![-11, -10, 2, 5, 10, 17, 99]);
    let () = v.mutate_vec (|v|{
      v[0] = 11;
      v[3] = 1;
    });
    assert_eq!(
      v.drain(..).collect::<Vec <i32>>(),
      vec![-10, 1, 2, 10, 11, 17, 99]);
  }

  #[test]
  fn sorted_set() {
    let mut s = SortedSet::new();
    assert_eq!(s.replace (5), (0, None));
    assert_eq!(s.replace (3), (0, None));
    assert_eq!(s.replace (4), (1, None));
    assert_eq!(s.replace (4), (1, Some (4)));
    assert_eq!(s.find_or_insert (4), FindOrInsert::Found (1));
    assert_eq!(s.find_or_insert (4).index(), 1);
    assert_eq!(s.len(), 3);
    assert_eq!(s.binary_search (&3), Ok (0));
    assert_eq!(**SortedSet::from_unsorted (
      vec![5, -10, 99, -10, -11, 10, 2, 17, 10]),
      vec![-11, -10, 2, 5, 10, 17, 99]);
    assert_eq!(SortedSet::from_unsorted (
      vec![5, -10, 99, -10, -11, 10, 2, 17, 10]),
      vec![5, -10, 99, -10, -11, 10, 2, 17, 10].into());
    let mut s = SortedSet::new();
    s.extend([5, -11, -10, 99, -11, 2, 17, 2, 10]);
    assert_eq!(**s, vec![-11, -10, 2, 5, 10, 17, 99]);
    let () = s.mutate_vec (|s|{
      s[0] = 5;
      s[3] = 1;
    });
    assert_eq!(
      s.drain(..).collect::<Vec <i32>>(),
      vec![-10, 1, 2, 5, 10, 17, 99]);
  }

  #[test]
  fn sorted_set_push() {
    let mut s = SortedSet::new();
    assert_eq!(s.push (5), (0, None));
    assert_eq!(s.push (3), (0, None));
    assert_eq!(s.push (4), (1, None));
    assert_eq!(s.push (4), (1, Some (4)));
    assert_eq!(s.find_or_push (4), FindOrInsert::Found (1));
    assert_eq!(s.find_or_push (4).index(), 1);
    assert_eq!(s.len(), 3);
    assert_eq!(s.binary_search (&3), Ok (0));
    assert_eq!(**SortedSet::from_unsorted (
      vec![5, -10, 99, -10, -11, 10, 2, 17, 10]),
      vec![-11, -10, 2, 5, 10, 17, 99]);
    assert_eq!(SortedSet::from_unsorted (
      vec![5, -10, 99, -10, -11, 10, 2, 17, 10]),
      vec![5, -10, 99, -10, -11, 10, 2, 17, 10].into());
    let mut s = SortedSet::new();
    s.extend([5, -11, -10, 99, -11, 2, 17, 2, 10]);
    assert_eq!(**s, vec![-11, -10, 2, 5, 10, 17, 99]);
    let () = s.mutate_vec (|s|{
      s[0] = 5;
      s[3] = 1;
    });
    assert_eq!(
      s.drain(..).collect::<Vec <i32>>(),
      vec![-10, 1, 2, 5, 10, 17, 99]);
  }

  #[test]
  fn reverse_sorted_vec() {
    let mut v = ReverseSortedVec::new();
    assert_eq!(v.insert (Reverse(5)), 0);
    assert_eq!(v.insert (Reverse(3)), 1);
    assert_eq!(v.insert (Reverse(4)), 1);
    assert_eq!(v.find_or_insert (Reverse(6)), FindOrInsert::Inserted (0));
    assert_eq!(v.insert (Reverse(4)), 2);
    assert_eq!(v.find_or_insert (Reverse(4)), FindOrInsert::Found (3));
    assert_eq!(v.len(), 5);
    v.dedup();
    assert_eq!(v.len(), 4);
    assert_eq!(*ReverseSortedVec::from_unsorted (
      Vec::from_iter ([5, -10, 99, -11, 2, 17, 10].map (Reverse))),
      Vec::from_iter ([99, 17, 10, 5, 2, -10, -11].map (Reverse)));
    assert_eq!(ReverseSortedVec::from_unsorted (
      Vec::from_iter ([5, -10, 99, -11, 2, 17, 10].map (Reverse))),
      Vec::from_iter ([5, -10, 99, -11, 2, 17, 10].map (Reverse)).into());
    let mut v = ReverseSortedVec::new();
    v.extend([5, -10, 99, -11, 2, 17, 10].map (Reverse));
    assert_eq!(v.as_slice(), [99, 17, 10, 5, 2, -10, -11].map (Reverse));
    let () = v.mutate_vec (|v|{
      v[6] = Reverse(11);
      v[3] = Reverse(1);
    });
    assert_eq!(
      v.drain(..).collect::<Vec <Reverse<i32>>>(),
      Vec::from_iter ([99, 17, 11, 10, 2, 1, -10].map (Reverse)));
  }

  #[test]
  fn reverse_sorted_set() {
    let mut s = ReverseSortedSet::new();
    assert_eq!(s.replace (Reverse(5)), (0, None));
    assert_eq!(s.replace (Reverse(3)), (1, None));
    assert_eq!(s.replace (Reverse(4)), (1, None));
    assert_eq!(s.find_or_insert (Reverse(6)), FindOrInsert::Inserted (0));
    assert_eq!(s.replace (Reverse(4)), (2, Some (Reverse(4))));
    assert_eq!(s.find_or_insert (Reverse(4)), FindOrInsert::Found (2));
    assert_eq!(s.len(), 4);
    assert_eq!(s.binary_search (&Reverse(3)), Ok (3));
    assert_eq!(**ReverseSortedSet::from_unsorted (
      Vec::from_iter ([5, -10, 99, -11, 2, 99, 17, 10, -10].map (Reverse))),
      Vec::from_iter ([99, 17, 10, 5, 2, -10, -11].map (Reverse)));
    assert_eq!(ReverseSortedSet::from_unsorted (
      Vec::from_iter ([5, -10, 99, -11, 2, 99, 17, 10, -10].map (Reverse))),
      Vec::from_iter ([5, -10, 99, -11, 2, 99, 17, 10, -10].map (Reverse)).into());
    let mut s = ReverseSortedSet::new();
    s.extend([5, -10, 2, 99, -11, -11, 2, 17, 10].map (Reverse));
    assert_eq!(s.as_slice(), [99, 17, 10, 5, 2, -10, -11].map (Reverse));
    let () = s.mutate_vec (|s|{
      s[6] = Reverse(17);
      s[3] = Reverse(1);
    });
    assert_eq!(
      s.drain(..).collect::<Vec <Reverse<i32>>>(),
      Vec::from_iter ([99, 17, 10, 2, 1, -10].map (Reverse)));
    let s = ReverseSortedSet::from_iter (
      [5, -10, 2, 99, -11, -11, 2, 17, 10].map (Reverse));
    assert_eq!(**s, [99, 17, 10, 5, 2, -10, -11].map (Reverse));
  }
  #[cfg(feature = "serde-nontransparent")]
  #[test]
  fn deserialize() {
    let s = r#"{"vec":[-11,-10,2,5,10,17,99]}"#;
    let _ = serde_json::from_str::<SortedVec <i32>> (s).unwrap();
  }
  #[cfg(all(feature = "serde", not(feature = "serde-nontransparent")))]
  #[test]
  fn deserialize() {
    let s = "[-11,-10,2,5,10,17,99]";
    let _ = serde_json::from_str::<SortedVec <i32>> (s).unwrap();
  }
  #[cfg(feature = "serde-nontransparent")]
  #[test]
  #[should_panic]
  fn deserialize_unsorted() {
    let s = r#"{"vec":[99,-11,-10,2,5,10,17]}"#;
    let _ = serde_json::from_str::<SortedVec <i32>> (s).unwrap();
  }
  #[cfg(all(feature = "serde", not(feature = "serde-nontransparent")))]
  #[test]
  #[should_panic]
  fn deserialize_unsorted() {
    let s = "[99,-11,-10,2,5,10,17]";
    let _ = serde_json::from_str::<SortedVec <i32>> (s).unwrap();
  }
  #[cfg(feature = "serde-nontransparent")]
  #[test]
  fn deserialize_reverse() {
    let s = r#"{"vec":[99,17,10,5,2,-10,-11]}"#;
    let _ = serde_json::from_str::<ReverseSortedVec <i32>> (s).unwrap();
  }
  #[cfg(all(feature = "serde", not(feature = "serde-nontransparent")))]
  #[test]
  fn deserialize_reverse() {
    let s = "[99,17,10,5,2,-10,-11]";
    let _ = serde_json::from_str::<ReverseSortedVec <i32>> (s).unwrap();
  }
  #[cfg(feature = "serde-nontransparent")]
  #[test]
  #[should_panic]
  fn deserialize_reverse_unsorted() {
    let s = r#"{"vec":[99,-11,-10,2,5,10,17]}"#;
    let _ = serde_json::from_str::<ReverseSortedVec <i32>> (s).unwrap();
  }
  #[cfg(all(feature = "serde", not(feature = "serde-nontransparent")))]
  #[test]
  #[should_panic]
  fn deserialize_reverse_unsorted() {
    let s = "[99,-11,-10,2,5,10,17]";
    let _ = serde_json::from_str::<ReverseSortedVec <i32>> (s).unwrap();
  }
}
