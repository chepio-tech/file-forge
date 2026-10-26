// Core
use std::panic::{AssertUnwindSafe, catch_unwind};
// Domain
use super::*;

#[test]
fn exact_capacity_and_aggregate_one_under_limit() {
    let budget = Budget::new(12);
    let first = budget.zeroed(8).expect("first");
    let second = budget.zeroed(4).expect("exact total");
    assert_eq!(budget.live(), 12);
    assert!(budget.empty(1).is_err());
    drop(first);
    assert_eq!(budget.live(), 4);
    let recovered = budget.zeroed(8).expect("released space");
    drop((second, recovered));
    assert_eq!(budget.live(), 0);
    assert_eq!(budget.peak(), 12);
    assert!(Budget::new(11).zeroed(12).is_err());
}

#[test]
fn checks_requested_size_before_calling_allocator() {
    let budget = Budget::new(7);
    let called = Cell::new(false);
    assert!(
        budget
            .allocate_with(8, |_| {
                called.set(true);
                Ok(Vec::new())
            })
            .is_err()
    );
    assert!(!called.get());
    assert_eq!(budget.live(), 0);
}

#[test]
fn charges_actual_capacity_and_rolls_back_oversized_candidate() {
    let larger = Vec::with_capacity(9);
    let capacity = larger.capacity();
    let budget = Budget::new(capacity);
    let bytes = budget.allocate_with(4, |_| Ok(larger)).expect("larger allocation admitted");
    assert_eq!(bytes.capacity(), capacity);
    assert_eq!(budget.live(), capacity);
    drop(bytes);
    assert_eq!(budget.live(), 0);

    let budget = Budget::new(capacity - 1);
    let first = budget.empty(1).expect("existing owner");
    assert!(budget.allocate_with(4, |_| Ok(Vec::with_capacity(capacity))).is_err());
    assert_eq!(budget.live(), 1);
    drop(first);
    assert!(budget.zeroed(capacity - 1).is_ok());
    assert_eq!(budget.live(), 0);
}

#[test]
fn allocation_errors_overflow_and_unwind_release_reservations() {
    let budget = Budget::new(16);
    assert!(budget.allocate_with(8, |_| Err("allocation refused".into())).is_err());
    assert_eq!(budget.live(), 0);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = budget.allocate_with(8, |_| panic!("test allocation failure"));
        }))
        .is_err()
    );
    assert_eq!(budget.live(), 0);
    assert!(budget.zeroed(16).is_ok());

    let huge = Budget::new(usize::MAX);
    assert!(huge.empty(usize::MAX).is_err());
    assert_eq!(huge.live(), 0);
    let _held = huge.reserve(usize::MAX).expect("ledger only, no allocation");
    assert!(huge.reserve(1).is_err());
    assert_eq!(huge.live(), usize::MAX);
}

#[test]
fn owner_is_destroyed_before_its_charge_on_drop_and_unwind() {
    struct Owner {
        budget: Budget,
        observed: Rc<Cell<usize>>,
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            self.observed.set(self.budget.live());
        }
    }
    for unwind in [false, true] {
        let budget = Budget::new(8);
        let observed = Rc::new(Cell::new(0));
        let owner = Allocation {
            value: Owner { budget: budget.clone(), observed: observed.clone() },
            charge: budget.reserve(8).expect("charge"),
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _owner = owner;
            if unwind {
                panic!("test owner failure");
            }
        }));
        assert_eq!(result.is_err(), unwind);
        assert_eq!(observed.get(), 8);
        assert_eq!(budget.live(), 0);
    }
}

#[test]
fn growth_accounts_for_both_owners_and_preserves_bytes_on_refusal() {
    let budget = Budget::new(11);
    let mut bytes = budget.empty(4).expect("initial");
    bytes.append(&[1, 2, 3, 4]).expect("fill");
    assert!(bytes.append(&[5]).is_err());
    assert!(bytes.grow(8).is_err());
    assert_eq!(&*bytes, &[1, 2, 3, 4]);
    assert_eq!(budget.live(), 4);
    bytes.grow(7).expect("exact overlap");
    assert_eq!(&*bytes, &[1, 2, 3, 4]);
    assert_eq!(budget.live(), 7);
    assert_eq!(budget.peak(), 11);
    drop(bytes);
    assert_eq!(budget.live(), 0);
}
