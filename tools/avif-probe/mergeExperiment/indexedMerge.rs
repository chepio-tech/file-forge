//! Temporary index for the isolated bounded filter-state merge experiment.
#![forbid(unsafe_code)]

// Domain
use crate::DecoderError;
// Services
use crate::allocation::{AllocationClass, AllocationLedger, admit_fresh_with, fresh_replacement};

pub(crate) type Entry = (usize, usize);

pub(crate) fn with_scratch<L: AllocationLedger, R>(
    ledger: &mut L,
    count: usize,
    operation: impl FnOnce(&mut Vec<Entry>) -> Result<R, DecoderError>,
) -> Result<R, DecoderError> {
    let (allocation, ticket) = admit_fresh_with(
        ledger,
        count,
        AllocationClass::Frame,
        "post-filter merge index",
        fresh_replacement::<Entry>,
    )?;
    let mut scratch = allocation.into_vec();
    let result = operation(&mut scratch);
    // The allocation must die before its live-memory charge is released.
    drop(scratch);
    ledger.release(ticket.class, ticket.charged_capacity_bytes)?;
    result
}

/// Preserve the original first-match/last-write policy and insertion order,
/// including duplicate keys already present in either input. Only the index is
/// sorted. The caller has already reserved the destination and scratch buffers.
pub(crate) fn merge<T, K: Ord>(
    destination: &mut Vec<T>,
    source: Vec<T>,
    scratch: &mut Vec<Entry>,
    key: impl Fn(&T) -> K,
) {
    if source.is_empty() {
        return;
    }
    let original_len = destination.len();
    let count = original_len.checked_add(source.len()).expect("merge size was admitted");
    assert!(destination.capacity() >= count && scratch.capacity() >= count);
    scratch.clear();
    scratch.extend((0..count).map(|index| (index, index)));
    {
        let value = |index| {
            if index < original_len { &destination[index] } else { &source[index - original_len] }
        };
        scratch.sort_unstable_by(|a, b| key(value(a.0)).cmp(&key(value(b.0))).then(a.0.cmp(&b.0)));
        let mut first = 0;
        for index in 0..count {
            if key(value(scratch[index].0)) != key(value(scratch[first].0)) {
                first = index;
            }
            scratch[index].1 = scratch[first].0;
        }
    }
    scratch.sort_unstable_by_key(|entry| entry.0);
    for (index, value) in source.into_iter().enumerate() {
        let origin = original_len + index;
        let first = scratch[origin].1;
        if first == origin {
            scratch[origin].1 = destination.len();
            destination.push(value);
        } else {
            let target = if first < original_len { first } else { scratch[first].1 };
            destination[target] = value;
        }
    }
}
