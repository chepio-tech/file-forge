//! Admission for adapter-owned byte vectors, separate from the native decoder's ledger.
// Core
use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

#[derive(Clone)]
pub struct Budget(Rc<State>);

struct State {
    limit: usize,
    live: Cell<usize>,
    peak: Cell<usize>,
}

struct Charge {
    budget: Budget,
    bytes: usize,
}

// Fields drop in declaration order: destroy the owner before releasing its charge.
struct Allocation<T> {
    value: T,
    charge: Charge,
}

pub struct Bytes(Allocation<Vec<u8>>);

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Bytes,
}

impl Budget {
    pub fn new(limit: usize) -> Self {
        Self(Rc::new(State { limit, live: Cell::new(0), peak: Cell::new(0) }))
    }

    pub fn live(&self) -> usize {
        self.0.live.get()
    }

    pub fn peak(&self) -> usize {
        self.0.peak.get()
    }

    fn reserve(&self, bytes: usize) -> Result<Charge, String> {
        let mut charge = Charge { budget: self.clone(), bytes: 0 };
        charge.increase(bytes)?;
        Ok(charge)
    }

    fn allocate_with(
        &self,
        requested: usize,
        allocate: impl FnOnce(usize) -> Result<Vec<u8>, String>,
    ) -> Result<Bytes, String> {
        let charge = self.reserve(requested)?;
        let value = allocate(requested)?;
        let mut allocation = Allocation { value, charge };
        let capacity = allocation.value.capacity();
        if !allocation.value.is_empty() || capacity < requested {
            return Err("invalid byte allocation".into());
        }
        // The allocator may return more than requested. Reject before admitting the buffer;
        // this cannot prevent that temporary allocation or bound allocator overhead/RSS.
        allocation.charge.increase(capacity - requested)?;
        Ok(Bytes(allocation))
    }

    pub fn empty(&self, capacity: usize) -> Result<Bytes, String> {
        self.allocate_with(capacity, |capacity| {
            let mut value = Vec::new();
            value.try_reserve_exact(capacity).map_err(|error| error.to_string())?;
            Ok(value)
        })
    }

    pub fn zeroed(&self, length: usize) -> Result<Bytes, String> {
        let mut bytes = self.empty(length)?;
        bytes.0.value.resize(length, 0);
        Ok(bytes)
    }
}

impl Charge {
    fn increase(&mut self, extra: usize) -> Result<(), String> {
        let state = &self.budget.0;
        let live = state
            .live
            .get()
            .checked_add(extra)
            .filter(|n| *n <= state.limit)
            .ok_or("adapter buffer allocation budget exceeded")?;
        self.bytes += extra;
        state.live.set(live);
        state.peak.set(state.peak.get().max(live));
        Ok(())
    }
}

impl Drop for Charge {
    fn drop(&mut self) {
        self.budget.0.live.set(self.budget.live() - self.bytes);
    }
}

impl Bytes {
    pub fn capacity(&self) -> usize {
        self.0.value.capacity()
    }

    pub fn zeroed_sibling(&self, length: usize) -> Result<Self, String> {
        self.0.charge.budget.zeroed(length)
    }

    pub fn append(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.len().checked_add(bytes.len()).is_none_or(|n| n > self.capacity()) {
            return Err("byte append would exceed admitted capacity".into());
        }
        self.0.value.extend_from_slice(bytes);
        Ok(())
    }

    pub fn grow(&mut self, capacity: usize) -> Result<(), String> {
        if capacity <= self.capacity() {
            return Ok(());
        }
        // Keep both owners charged during allocation and copying; never reallocate in place.
        let mut replacement = self.0.charge.budget.empty(capacity)?;
        replacement.append(self)?;
        *self = replacement;
        Ok(())
    }
}

impl Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0.value
    }
}

impl DerefMut for Bytes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.value
    }
}

#[cfg(test)]
mod tests;
