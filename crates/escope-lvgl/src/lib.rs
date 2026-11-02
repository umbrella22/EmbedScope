//! UI-thread bridge for an existing LVGL instance.
#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

use core::marker::PhantomData;
use escope_protocol::{decode_payload, encode_payload, Status, TreeQuery, UiNode};

#[cfg(feature = "native")]
pub mod native;

/// Read on the LVGL thread, between layout/reconcile operations. A nonzero
/// revision changes whenever nodes, properties or layout in this tree change.
pub trait TreeSource {
    fn revision(&self) -> u32;
    fn read_node(&mut self, cursor: u32) -> Result<UiNode<'_>, Status>;
}

/// This bridge is neither Send nor Sync. Platforms queue requests to the UI
/// thread and return the encoded response to their transport task.
pub struct UiBridge<T> {
    source: T,
    _thread: PhantomData<*mut ()>,
}

impl<T: TreeSource> UiBridge<T> {
    pub fn new(source: T) -> Self {
        Self {
            source,
            _thread: PhantomData,
        }
    }
    pub fn source_mut(&mut self) -> &mut T {
        &mut self.source
    }
    pub fn tree(&mut self, payload: &[u8], out: &mut [u8]) -> Result<usize, Status> {
        let query: TreeQuery = decode_payload(payload).map_err(|_| Status::InvalidArgument)?;
        let revision = self.source.revision();
        if revision == 0 || (query.revision != 0 && query.revision != revision) {
            return Err(Status::Stale);
        }
        let node = self.source.read_node(query.cursor)?;
        if node.revision != revision || node.id == 0 {
            return Err(Status::Stale);
        }
        encode_payload(&node, out).map_err(|_| Status::Capacity)
    }
}

#[derive(Clone, Copy)]
struct Slot {
    token: usize,
    generation: u16,
    retired: bool,
}

/// Session-local identities for a host's object registry. Deletion hooks must
/// call remove before the native pointer can be reused. Slots retire before a
/// generation wrap, so an old ID cannot become valid again in this registry.
pub struct NodeRegistry<const N: usize> {
    slots: [Slot; N],
    _thread: PhantomData<*mut ()>,
}

impl<const N: usize> Default for NodeRegistry<N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<const N: usize> NodeRegistry<N> {
    pub const fn new() -> Self {
        assert!(N <= u16::MAX as usize);
        Self {
            slots: [Slot {
                token: 0,
                generation: 1,
                retired: false,
            }; N],
            _thread: PhantomData,
        }
    }
    pub fn insert(&mut self, token: usize) -> Result<u32, Status> {
        if token == 0 {
            return Err(Status::InvalidArgument);
        }
        if let Some(index) = self.slots.iter().position(|s| s.token == token) {
            return Ok(self.id(index));
        }
        let index = self
            .slots
            .iter()
            .position(|s| s.token == 0 && !s.retired)
            .ok_or(Status::Capacity)?;
        self.slots[index].token = token;
        Ok(self.id(index))
    }
    fn id(&self, index: usize) -> u32 {
        ((self.slots[index].generation as u32) << 16) | (index as u32 + 1)
    }
    pub fn resolve(&self, id: u32) -> Result<usize, Status> {
        let index = (id & 0xffff).checked_sub(1).ok_or(Status::Stale)? as usize;
        let slot = self.slots.get(index).ok_or(Status::Stale)?;
        if slot.token == 0 || slot.retired || self.id(index) != id {
            return Err(Status::Stale);
        }
        Ok(slot.token)
    }
    pub fn remove(&mut self, token: usize) {
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|s| s.token == token && token != 0)
        {
            slot.token = 0;
            if slot.generation == u16::MAX {
                slot.retired = true;
            } else {
                slot.generation += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deleted_pointer_reuse_does_not_resolve_old_id() {
        let mut registry = NodeRegistry::<2>::new();
        let first = registry.insert(0x1000).unwrap();
        registry.remove(0x1000);
        let second = registry.insert(0x1000).unwrap();
        assert_ne!(first, second);
        assert_eq!(registry.resolve(first), Err(Status::Stale));
        assert_eq!(registry.resolve(second), Ok(0x1000));
    }
    #[test]
    fn generation_exhaustion_retires_slot() {
        let mut registry = NodeRegistry::<1>::new();
        for _ in 0..u16::MAX {
            registry.insert(1).unwrap();
            registry.remove(1);
        }
        assert_eq!(registry.insert(1), Err(Status::Capacity));
    }
}
