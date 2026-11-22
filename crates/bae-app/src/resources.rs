//! A type map of application-wide resources.

use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::fmt;

/// Type-keyed storage for application resources (one value per type).
///
/// Resources are `Send + Sync` so a finished [`crate::App`] can be shared with
/// worker threads (headless rendering, the MCP server, the plugin host).
#[derive(Default)]
pub struct Resources {
    map: HashMap<TypeId, Slot>,
}

struct Slot {
    name: &'static str,
    value: Box<dyn Any + Send + Sync>,
}

impl Resources {
    /// Creates an empty resource map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts `value`, returning the previous value of the same type, if any.
    pub fn insert<R: Send + Sync + 'static>(&mut self, value: R) -> Option<R> {
        let previous = self.map.insert(
            TypeId::of::<R>(),
            Slot {
                name: type_name::<R>(),
                value: Box::new(value),
            },
        );
        previous.map(|slot| *downcast_box::<R>(slot.value))
    }

    /// Returns a reference to the resource of type `R`.
    pub fn get<R: Send + Sync + 'static>(&self) -> Option<&R> {
        self.map
            .get(&TypeId::of::<R>())
            .and_then(|slot| slot.value.downcast_ref::<R>())
    }

    /// Returns a mutable reference to the resource of type `R`.
    pub fn get_mut<R: Send + Sync + 'static>(&mut self) -> Option<&mut R> {
        self.map
            .get_mut(&TypeId::of::<R>())
            .and_then(|slot| slot.value.downcast_mut::<R>())
    }

    /// Returns the resource of type `R`, inserting `init()` first if it is missing.
    pub fn get_or_insert_with<R: Send + Sync + 'static>(
        &mut self,
        init: impl FnOnce() -> R,
    ) -> &mut R {
        let slot = self.map.entry(TypeId::of::<R>()).or_insert_with(|| Slot {
            name: type_name::<R>(),
            value: Box::new(init()),
        });
        slot.value
            .downcast_mut::<R>()
            .expect("resource slot keyed by TypeId always holds that type")
    }

    /// Removes and returns the resource of type `R`.
    pub fn remove<R: Send + Sync + 'static>(&mut self) -> Option<R> {
        self.map
            .remove(&TypeId::of::<R>())
            .map(|slot| *downcast_box::<R>(slot.value))
    }

    /// Returns `true` if a resource of type `R` is present.
    pub fn contains<R: Send + Sync + 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<R>())
    }

    /// Number of stored resources.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if no resources are stored.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Type names of all stored resources (diagnostics only; unordered).
    pub fn type_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.map.values().map(|slot| slot.name)
    }
}

fn downcast_box<R: 'static>(value: Box<dyn Any + Send + Sync>) -> Box<R> {
    match value.downcast::<R>() {
        Ok(value) => value,
        Err(_) => unreachable!("resource slot keyed by TypeId always holds that type"),
    }
}

impl fmt::Debug for Resources {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut names: Vec<_> = self.type_names().collect();
        names.sort_unstable();
        f.debug_struct("Resources").field("types", &names).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Default)]
    struct Counter(u32);

    #[test]
    fn insert_get_remove() {
        let mut res = Resources::new();
        assert!(res.insert(Counter(1)).is_none());
        assert_eq!(res.insert(Counter(2)), Some(Counter(1)));
        assert_eq!(res.get::<Counter>(), Some(&Counter(2)));
        res.get_mut::<Counter>().unwrap().0 += 1;
        assert_eq!(res.get::<Counter>(), Some(&Counter(3)));
        assert!(res.contains::<Counter>());
        assert_eq!(res.remove::<Counter>(), Some(Counter(3)));
        assert!(res.is_empty());
    }

    #[test]
    fn get_or_insert_with_only_initializes_once() {
        let mut res = Resources::new();
        res.get_or_insert_with(Counter::default).0 += 1;
        res.get_or_insert_with(|| Counter(100)).0 += 1;
        assert_eq!(res.get::<Counter>(), Some(&Counter(2)));
        assert!(format!("{res:?}").contains("Counter"));
    }
}
