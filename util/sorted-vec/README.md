# `sorted_vec`

[![Latest Version]][crates.io] [![Rust Version]][Rust 1.85] [![License]][license-file] [![Documentation]][docs] [![Build Status]][pipelines]

[Latest Version]: https://img.shields.io/crates/v/sorted-vec.svg
[crates.io]: https://crates.io/crates/sorted-vec
[Rust Version]: https://img.shields.io/crates/msrv/sorted-vec.svg
[Rust 1.85]: https://blog.rust-lang.org/2025/03/18/Rust-1.85.1/
[License]: https://img.shields.io/crates/l/sorted-vec.svg
[license-file]: https://gitlab.com/spearman/sorted-vec/-/blob/master/LICENSE
[Documentation]: https://docs.rs/sorted-vec/badge.svg
[docs]: https://docs.rs/sorted-vec
[Build Status]: https://gitlab.com/spearman/sorted-vec/badges/master/pipeline.svg
[pipelines]: https://gitlab.com/spearman/sorted-vec/-/pipelines

> Create and maintain collections of sorted elements.

[Documentation](https://docs.rs/sorted-vec)

```rust
let mut v = SortedVec::new();
assert_eq!(v.insert (5), 0);
assert_eq!(v.insert (3), 0);
assert_eq!(v.insert (4), 1);
assert_eq!(v.insert (4), 1);
assert_eq!(v.len(), 4);
v.dedup();
assert_eq!(v.len(), 3);
assert_eq!(v.binary_search (&3), Ok (0));
assert_eq!(*SortedVec::from_unsorted (
  vec![5, -10, 99, -11, 2, 17, 10]),
  vec![-11, -10, 2, 5, 10, 17, 99]);
```

Also provides sorted set containers only containing unique elements.

## `serde` support

`serde` de/serialization is an optional feature.

By default, deserializing an unsorted container is an error.

To sort on deserialization, tag the field with
`#[serde(deserialize_with = "SortedVec::deserialize_unsorted")]`:
```rust
#[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Foo {
  #[serde(deserialize_with = "SortedVec::deserialize_unsorted")]
  pub v : SortedVec <u64>
}
```
