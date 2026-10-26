// Core
use std::io::{Cursor, Error};
// Domain
use super::*;

struct Chunks(Cursor<Vec<u8>>);

impl Read for Chunks {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let length = output.len().min(2);
        self.0.read(&mut output[..length])
    }
}

#[test]
fn stale_length_hint_grows_with_accounted_overlap() {
    let budget = Budget::new(10);
    let result = read(Chunks(Cursor::new(vec![1, 2, 3, 4, 5, 6])), 1, 6, &budget).expect("growth");
    assert_eq!(&*result, &[1, 2, 3, 4, 5, 6]);
    assert_eq!(budget.live(), 6);
    assert_eq!(budget.peak(), 10);
    drop(result);
    assert_eq!(budget.live(), 0);
    let budget = Budget::new(9);
    assert!(read(Chunks(Cursor::new(vec![0; 6])), 1, 6, &budget).is_err());
    assert_eq!(budget.live(), 0);
    assert!(read(Cursor::new([1; 6]), 6, 6, &budget).is_ok());
}

#[test]
fn growing_input_reads_only_one_byte_beyond_limit_and_releases_storage() {
    let budget = Budget::new(10);
    let mut input = Cursor::new([0; 100]);
    assert!(read(&mut input, 5, 5, &budget).is_err());
    assert_eq!(input.position(), 6);
    assert_eq!(budget.live(), 0);
    assert!(read(Cursor::new([1; 5]), 5, 5, &budget).is_ok());
}

#[test]
fn exact_size_shrink_and_empty_inputs_obey_capacity_not_length() {
    let budget = Budget::new(5);
    let bytes = read(Cursor::new([1; 5]), 5, 5, &budget).expect("exact");
    assert_eq!(budget.live(), 5);
    drop(bytes);
    assert!(read(Cursor::new([1; 5]), 5, 5, &Budget::new(4)).is_err());
    let bytes = read(Cursor::new([1; 2]), 5, 5, &budget).expect("shrunk file");
    assert_eq!(bytes.len(), 2);
    assert_eq!(budget.live(), 5);
    drop(bytes);
    let empty = read(std::io::empty(), 0, 0, &Budget::new(0)).expect("empty");
    assert!(empty.is_empty());
    assert!(read(std::io::empty(), 6, 5, &budget).is_err());
    assert_eq!(budget.live(), 0);
}

#[test]
fn interrupted_reads_retry_and_io_failure_releases_capacity() {
    struct Failing(u8);
    impl Read for Failing {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            self.0 += 1;
            match self.0 {
                1 => Err(Error::from(ErrorKind::Interrupted)),
                2 => {
                    output[0] = 1;
                    Ok(1)
                }
                _ => Err(Error::other("read failed")),
            }
        }
    }
    let budget = Budget::new(5);
    let mut reader = Failing(0);
    assert!(read(&mut reader, 5, 5, &budget).is_err());
    assert_eq!(reader.0, 3);
    assert_eq!(budget.live(), 0);
    assert!(read(Cursor::new([1; 5]), 5, 5, &budget).is_ok());
}
