use crate::garbage::Duration;
use std::collections::{BTreeMap, HashSet};
use std::hash::Hash;

pub struct GcSet<V>
where
	V: Eq + Hash,
{
	/// The number of milliseconds a value is valid for.
	value_ttl_ms: Duration,
	/// The duration of a garbage collection slot in milliseconds.
	/// This is used to bin values into slots for O(value_ttl_ms/gc_slot_duration_ms * log value_ttl_ms/gc_slot_duration_ms) garbage collection.
	gc_slot_duration_ms: Duration,
	/// The value lifetimes, indexed by slot.
	value_lifetimes: BTreeMap<u64, HashSet<V>>,
}

impl<V> GcSet<V>
where
	V: Eq + Hash,
{
	/// Creates a new GcSet with a specified garbage collection slot duration.
	pub fn new(value_ttl_ms: Duration, gc_slot_duration_ms: Duration) -> Self {
		GcSet { value_ttl_ms, gc_slot_duration_ms, value_lifetimes: BTreeMap::new() }
	}

	/// Removes the value for an key.
	pub fn remove_value(&mut self, value: &V) {
		// check each slot for the key
		for lifetimes in self.value_lifetimes.values_mut().rev() {
			if lifetimes.remove(value) {
				break;
			}
		}
	}

	/// Sets the value for an key.
	pub fn insert(&mut self, value: V, current_time_ms: u64) {
		// remove the old value
		self.remove_value(&value);

		// compute the slot for the new lifetime and add accordingly
		let slot = current_time_ms / self.gc_slot_duration_ms.get();

		// add the new value
		self.value_lifetimes.entry(slot).or_insert_with(HashSet::new).insert(value);
	}

	/// Checks if the value is in the set.
	pub fn contains(&self, value: &V) -> bool {
		self.value_lifetimes.values().any(|lifetimes| lifetimes.contains(value))
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
pub mod test {

	use super::*;

	#[derive(Debug, Eq, PartialEq, Hash)]
	pub struct Value(u64);

	#[test]
	fn test_gc_set() -> Result<(), anyhow::Error> {
		let value_ttl_ms = Duration::try_new(100)?;
		let gc_slot_duration_ms = Duration::try_new(10)?;
		let mut gc_set = GcSet::new(value_ttl_ms, gc_slot_duration_ms);

		let current_time_ms = 0;

		// set the value for key 1
		gc_set.insert(Value(1), current_time_ms);
		assert_eq!(gc_set.contains(&Value(1)), true);

		// write the value for key 1 again at later time
		gc_set.insert(Value(1), current_time_ms + 100);
		assert_eq!(gc_set.contains(&Value(1)), true);

		// add another value back at the original time
		gc_set.insert(Value(2), current_time_ms);

		// garbage collect
		gc_set.gc(current_time_ms + 100);

		// assert the value 1 is still there
		assert_eq!(gc_set.contains(&Value(1)), true);

		// assert the value 2 is gone
		assert_eq!(gc_set.contains(&Value(2)), false);

		Ok(())
	}
}
