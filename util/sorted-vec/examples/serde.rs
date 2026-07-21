use sorted_vec::*;
use serde_json;

fn main() {
  let v = SortedVec::from_unsorted (vec![5, -10, 99, -11, 2, 17, 10]);
  println!("{}", serde_json::to_string (&v).unwrap());
  println!("{:?}",
    serde_json::from_str::<SortedVec <i32>> ("[-11,-10,2,5,10,17,99]").unwrap());
}
