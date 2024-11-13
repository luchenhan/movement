use crate::garbage::Duration;
use std::collections::BTreeMap;

pub struct GcCounter {
	/// The number of milliseconds a value is valid for.
	value_ttl_ms: Duration,
	/// The duration of a garbage collection slot in milliseconds.
	/// This is used to bin values into slots for O(value_ttl_ms/gc_slot_duration_ms * log value_ttl_ms/gc_slot_duration_ms) garbage collection.
	gc_slot_duration_ms: Duration,
	/// The value lifetimes, indexed by slot.
	value_lifetimes: BTreeMap<u64, u64>,
}

impl GcCounter {
	/// Creates a new GcCounter with a specified garbage collection slot duration.
	pub fn new(value_ttl_ms: Duration, gc_slot_duration_ms: Duration) -> Self {
		GcCounter { value_ttl_ms, gc_slot_duration_ms, value_lifetimes: BTreeMap::new() }
	}

	/// Decrements from the first slot that has a value thereby decrementing the overall count
	pub fn decrement(&mut self) {
		// check each slot for the key
		for lifetime in self.value_lifetimes.values_mut() {
			if lifetime > &mut 0 {
				*lifetime -= 1;
				break;
			}
		}
	}

	/// Sets the value for an key.
	pub fn increment(&mut self, current_time_ms: u64) {
		// compute the slot for the new lifetime and add accordingly
		let slot = current_time_ms / self.gc_slot_duration_ms.get();

		// increment the slot
		match self.value_lifetimes.get_mut(&slot) {
			Some(lifetime) => {
				*lifetime += 1;
			}
			None => {
				self.value_lifetimes.insert(slot, 1);
			}
		}
	}

	/// Gets the current count
	pub fn get_count(&self) -> u64 {
		// sum up all the slots
		self.value_lifetimes.values().sum()
	}

	/// Garbage collects values that have expired.
	/// This should be called periodically.
	pub fn gc(&mut self, current_time_ms: u64) {
		let gc_slot = current_time_ms / self.gc_slot_duration_ms.get();

		// remove all slots that are too old
		let slot_cutoff = gc_slot - self.value_ttl_ms.get() / self.gc_slot_duration_ms.get();
		let slots_to_remove: Vec<u64> = self
			.value_lifetimes
			.keys()
			.take_while(|slot| **slot <= slot_cutoff)
			.cloned()
			.collect();
		for slot in slots_to_remove {
			self.value_lifetimes.remove(&slot);
		}
	}
}

#[cfg(test)]
pub mod tests {
	use super::*;

	#[test]
	fn test_gc_counter() -> Result<(), anyhow::Error> {
		let value_ttl_ms = Duration::try_new(100)?;
		let gc_slot_duration_ms = Duration::try_new(10)?;
		let mut gc_counter = GcCounter::new(value_ttl_ms, gc_slot_duration_ms);

		let current_time_ms = 0;

		// add three
		gc_counter.increment(current_time_ms);
		gc_counter.increment(current_time_ms);
		gc_counter.increment(current_time_ms);
		assert_eq!(gc_counter.get_count(), 3);

		// decrement one
		gc_counter.decrement();
		assert_eq!(gc_counter.get_count(), 2);

		// add one garbage collect the rest
		gc_counter.increment(current_time_ms + 10);
		gc_counter.gc(current_time_ms + 100);

		// check that the count is 1
		assert_eq!(gc_counter.get_count(), 1);

		Ok(())
	}
}
