//! Fixture-free checks using the pinned codec's allocation code and observer.
// These unchanged upstream modules have additional entry points used by the full decoder.
#[allow(dead_code)]
#[cfg(test)]
#[path = "../codec/src/allocation.rs"]
mod allocation;
#[cfg(test)]
#[path = "../codec/src/av1/tile_decode/post_filter_state/indexed_merge.rs"]
mod indexed_merge;
#[allow(dead_code)]
#[cfg(test)]
#[path = "../codec/src/test_allocation_observer.rs"]
mod test_allocation_observer;

#[cfg(test)]
#[derive(Debug)]
pub enum DecoderError {
    InvalidParam(String),
}

#[cfg(test)]
mod tests {
    // Domain
    use super::DecoderError;
    // Services
    use super::allocation::{AllocationClass, AllocationLedger, AllocationTicket};
    use super::{indexed_merge, test_allocation_observer as observer};

    struct Ledger {
        live: usize,
        peak: usize,
        limit: usize,
    }

    impl AllocationLedger for Ledger {
        type Checkpoint = usize;

        fn checkpoint(&self) -> usize {
            self.live
        }

        fn restore(&mut self, checkpoint: usize) {
            observer::record_restore();
            self.live = checkpoint;
        }

        fn check(&self, _: AllocationClass, bytes: usize, _: &str) -> Result<(), DecoderError> {
            if self.live.checked_add(bytes).is_some_and(|sum| sum <= self.limit) {
                Ok(())
            } else {
                Err(DecoderError::InvalidParam("live allocation limit".into()))
            }
        }

        fn charge(&mut self, class: AllocationClass, bytes: usize) -> Result<(), DecoderError> {
            self.check(class, bytes, "charge")?;
            self.live += bytes;
            self.peak = self.peak.max(self.live);
            Ok(())
        }

        fn release(&mut self, class: AllocationClass, bytes: usize) -> Result<(), DecoderError> {
            observer::record_token_release(&AllocationTicket {
                class,
                charged_capacity_bytes: bytes,
            });
            self.live = self.live.checked_sub(bytes).expect("charge must still be live");
            Ok(())
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Record {
        key: (usize, usize, usize, usize),
        value: usize,
    }

    fn reference(destination: &mut Vec<Record>, source: Vec<Record>) {
        for incoming in source {
            if let Some(existing) = destination.iter_mut().find(|value| value.key == incoming.key) {
                *existing = incoming;
            } else {
                destination.push(incoming);
            }
        }
    }

    fn records(seed: &mut u64, count: usize) -> Vec<Record> {
        (0..count)
            .map(|index| {
                *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                Record {
                    key: (
                        (*seed >> 20) as usize % 2,
                        (*seed >> 24) as usize % 8,
                        (*seed >> 28) as usize % 8,
                        (*seed >> 32) as usize % 3,
                    ),
                    value: index + (*seed >> 40) as usize,
                }
            })
            .collect()
    }

    #[test]
    fn indexed_merge_matches_linear_reference_with_duplicates_and_all_key_fields() {
        let mut seed = 37;
        for case in 0..600 {
            let mut destination = records(&mut seed, case % 64);
            let source = records(&mut seed, case % 97);
            let mut expected = destination.clone();
            reference(&mut expected, source.clone());
            destination.reserve(source.len());
            let mut scratch = Vec::with_capacity(destination.len() + source.len());
            indexed_merge::merge(&mut destination, source, &mut scratch, |value| value.key);
            assert_eq!(destination, expected, "case {case}");
        }
    }

    #[test]
    fn duplicate_first_match_last_write_and_original_order_are_preserved() {
        let record = |key, value| Record { key: (key, 0, 0, 0), value };
        let mut destination = vec![record(8, 1), record(3, 2), record(8, 3)];
        let source = vec![record(4, 4), record(8, 5), record(4, 6), record(1, 7), record(3, 8)];
        destination.reserve(source.len());
        let mut scratch = Vec::with_capacity(destination.len() + source.len());
        indexed_merge::merge(&mut destination, source, &mut scratch, |value| value.key);
        assert_eq!(
            destination,
            vec![record(8, 5), record(3, 8), record(8, 3), record(4, 6), record(1, 7)]
        );
    }

    #[test]
    fn merge_uses_no_additional_allocation_and_reuses_scratch() {
        let mut seed = 99;
        let mut scratch = Vec::with_capacity(512);
        let scratch_pointer = scratch.as_ptr();
        for _ in 0..5 {
            let mut destination = records(&mut seed, 128);
            let source = records(&mut seed, 128);
            destination.reserve(source.len());
            let (_, requests) = observer::count_allocation_requests(|| {
                indexed_merge::merge(&mut destination, source, &mut scratch, |value| value.key);
            });
            assert_eq!(requests, 0);
            assert_eq!(scratch.as_ptr(), scratch_pointer);
        }
    }

    #[test]
    fn scratch_is_dropped_before_release_on_success_and_operation_error() {
        for fail in [false, true] {
            let mut ledger = Ledger { live: 64, peak: 64, limit: 4096 };
            let observation = observer::Observation::begin(1, false);
            let _tracking = observer::track_all_candidates();
            let result = indexed_merge::with_scratch(&mut ledger, 128, |scratch| {
                assert!(scratch.capacity() >= 128);
                if fail {
                    Err(DecoderError::InvalidParam("operation refused".into()))
                } else {
                    Ok(())
                }
            });
            assert_eq!(result.is_err(), fail);
            assert_eq!(ledger.live, 64);
            assert!(ledger.peak >= 64 + 128 * size_of::<indexed_merge::Entry>());
            assert_eq!(observation.release_count(), 1);
            assert_eq!(observation.release_drop_snapshots()[0], 1);
        }
    }

    #[test]
    fn requested_budget_refusal_precedes_allocation_and_operation() {
        let count = 1024;
        let bytes = count * size_of::<indexed_merge::Entry>();
        let mut ledger = Ledger { live: 64, peak: 64, limit: bytes };
        let observation = observer::Observation::begin(bytes, false);
        let result =
            indexed_merge::with_scratch(&mut ledger, count, |_| -> Result<(), DecoderError> {
                panic!("operation must not run");
            });
        assert!(result.is_err());
        assert_eq!(observation.requests(), 0);
        assert_eq!(ledger.live, 64);
    }

    #[test]
    fn scratch_size_overflow_is_refused_before_operation() {
        let mut ledger = Ledger { live: 64, peak: 64, limit: usize::MAX };
        let result =
            indexed_merge::with_scratch(&mut ledger, usize::MAX, |_| -> Result<(), DecoderError> {
                panic!("operation must not run");
            });
        assert!(result.is_err());
        assert_eq!(ledger.live, 64);
    }

    #[test]
    fn allocator_refusal_returns_error_and_restores_ledger() {
        let count = 1024;
        let bytes = count * size_of::<indexed_merge::Entry>();
        let mut ledger = Ledger { live: 64, peak: 64, limit: bytes + 64 };
        let observation = observer::Observation::begin(bytes, true);
        let result =
            indexed_merge::with_scratch(&mut ledger, count, |_| -> Result<(), DecoderError> {
                panic!("operation must not run");
            });
        assert!(result.is_err());
        assert_eq!(observation.requests(), 1);
        assert_eq!(observation.restore_count(), 1);
        assert_eq!(ledger.live, 64);
    }

    #[test]
    fn excessive_actual_capacity_is_dropped_before_budget_rollback() {
        let count = 128;
        let bytes = count * size_of::<indexed_merge::Entry>();
        let mut ledger = Ledger { live: 64, peak: 64, limit: bytes + 64 };
        let observation = observer::Observation::begin(1, false);
        let _extra = observer::force_fresh_capacity_extra("post-filter merge index", 1 << 20);
        let result =
            indexed_merge::with_scratch(&mut ledger, count, |_| -> Result<(), DecoderError> {
                panic!("operation must not run");
            });
        assert!(result.is_err());
        assert_eq!(ledger.live, 64);
        assert_eq!(observation.restore_count(), 1);
        assert_eq!(observation.restore_drop_snapshots()[0], 1);
    }
}
