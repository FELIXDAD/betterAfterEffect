//! Runtime registries: id → item maps that app modules and plugins write into.
//!
//! Design goals:
//!
//! * **Lock-free reads.** Readers (render thread, UI, MCP server) load an immutable
//!   snapshot through [`arc_swap::ArcSwap`]; a lookup never blocks and never waits
//!   for a writer. Writes are rare (startup, plugin load/unload) and copy-on-write.
//! * **Registration at any time.** Registries are internally synchronized, so the
//!   plugin host can add or remove entries long after startup from any thread.
//! * **Ownership.** Every entry records the [`Owner`] that registered it so that
//!   unloading a plugin removes exactly its entries ([`Registry::unregister_owner`]).
//! * **Safe unloading.** Items are handed out as `Arc<T>`; an in-flight user keeps an
//!   item alive after it was unregistered. The plugin host must wait until the entries
//!   it removed are no longer shared ([`Entry::is_unique`]) before unloading the code
//!   that implements them.
//! * **Change notification.** [`Registry::subscribe`] lets the UI rebuild menus,
//!   panels or tool lists when something is registered or removed.

use std::any::{Any, type_name};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arc_swap::ArcSwap;
use indexmap::IndexMap;
use parking_lot::Mutex;

/// Who registered an entry.
///
/// "App module" (internal, compiled in) and "plugin" (user-facing, loaded at
/// runtime) are deliberately distinct variants.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    /// The application shell itself, or code running outside any module.
    Core,
    /// An internal app module (see [`crate::AppModule`]), by module name.
    Module(Arc<str>),
    /// A user-facing plugin loaded by the plugin host, by plugin id.
    Plugin(Arc<str>),
}

impl Owner {
    /// An internal app module owner.
    pub fn module(name: impl Into<Arc<str>>) -> Self {
        Self::Module(name.into())
    }

    /// A plugin owner.
    pub fn plugin(id: impl Into<Arc<str>>) -> Self {
        Self::Plugin(id.into())
    }

    /// Returns `true` for [`Owner::Plugin`].
    pub fn is_plugin(&self) -> bool {
        matches!(self, Self::Plugin(_))
    }
}

impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core => f.write_str("core"),
            Self::Module(name) => write!(f, "module:{name}"),
            Self::Plugin(id) => write!(f, "plugin:{id}"),
        }
    }
}

/// Errors returned by registry writes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// An entry with this id already exists. Use [`Registry::replace`] to override.
    #[error("`{id}` is already registered in {registry} by {existing_owner}")]
    Duplicate {
        /// Registry item type name.
        registry: &'static str,
        /// The conflicting id.
        id: Arc<str>,
        /// Owner of the existing entry.
        existing_owner: Owner,
    },
    /// The id is empty or contains whitespace / control characters.
    #[error("invalid registry id {id:?}: {reason}")]
    InvalidId {
        /// The rejected id.
        id: Arc<str>,
        /// Why it was rejected.
        reason: &'static str,
    },
}

/// One registered item.
pub struct Entry<T: ?Sized> {
    id: Arc<str>,
    owner: Owner,
    item: Arc<T>,
}

impl<T: ?Sized> Entry<T> {
    /// The entry id (conventionally namespaced, e.g. `bae.motion.precompose`).
    pub fn id(&self) -> &Arc<str> {
        &self.id
    }

    /// Who registered the entry.
    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    /// The registered item.
    pub fn item(&self) -> &Arc<T> {
        &self.item
    }

    /// `true` when no one else (other snapshots, callers) holds the item any more.
    /// The plugin host polls this for removed entries before unloading their code.
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.item) == 1
    }
}

impl<T: ?Sized> Clone for Entry<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            owner: self.owner.clone(),
            item: self.item.clone(),
        }
    }
}

impl<T: ?Sized> fmt::Debug for Entry<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("id", &self.id)
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

/// A single change applied to a registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryChange {
    /// A new id was registered.
    Registered {
        /// The id.
        id: Arc<str>,
        /// Its owner.
        owner: Owner,
    },
    /// An existing id was replaced by a new item.
    Replaced {
        /// The id.
        id: Arc<str>,
        /// Owner of the new item.
        owner: Owner,
        /// Owner of the replaced item.
        previous_owner: Owner,
    },
    /// An id was removed.
    Unregistered {
        /// The id.
        id: Arc<str>,
        /// Owner of the removed item.
        owner: Owner,
    },
}

/// A committed batch of changes, delivered to subscribers.
#[derive(Debug, Clone)]
pub struct RegistryEvent {
    /// Item type name of the registry that changed.
    pub registry: &'static str,
    /// Generation of the registry after the change. Generations strictly increase,
    /// but events from concurrent writers may be delivered out of order: compare
    /// generations, or simply re-read the latest [`Registry::snapshot`].
    pub generation: u64,
    /// The changes, in application order.
    pub changes: Vec<RegistryChange>,
}

/// Handle returned by [`Registry::subscribe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

type Listener = Arc<dyn Fn(&RegistryEvent) + Send + Sync>;

struct State<T: ?Sized> {
    generation: u64,
    entries: IndexMap<Arc<str>, Entry<T>>,
}

/// An immutable, cheap-to-clone view of a registry at one generation.
pub struct RegistrySnapshot<T: ?Sized> {
    state: Arc<State<T>>,
}

impl<T: ?Sized> Clone for RegistrySnapshot<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<T: ?Sized> RegistrySnapshot<T> {
    /// Generation this snapshot was taken at.
    pub fn generation(&self) -> u64 {
        self.state.generation
    }

    /// Looks up an item by id.
    pub fn get(&self, id: &str) -> Option<&Arc<T>> {
        self.state.entries.get(id).map(|entry| &entry.item)
    }

    /// Looks up an entry (item + owner) by id.
    pub fn entry(&self, id: &str) -> Option<&Entry<T>> {
        self.state.entries.get(id)
    }

    /// Returns `true` if `id` is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.state.entries.contains_key(id)
    }

    /// Entries in registration order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Entry<T>> + '_ {
        self.state.entries.values()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.state.entries.len()
    }

    /// Returns `true` if the registry was empty at this generation.
    pub fn is_empty(&self) -> bool {
        self.state.entries.is_empty()
    }
}

/// A thread-safe, runtime-mutable map from string ids to items of type `T`.
///
/// `T` may be unsized (`Registry<dyn Importer>`); items are stored as `Arc<T>`.
pub struct Registry<T: ?Sized + Send + Sync + 'static> {
    name: &'static str,
    state: ArcSwap<State<T>>,
    write_lock: Mutex<()>,
    listeners: Mutex<Vec<(SubscriptionId, Listener)>>,
    next_listener: AtomicU64,
}

impl<T: ?Sized + Send + Sync + 'static> Default for Registry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized + Send + Sync + 'static> Registry<T> {
    /// Creates an empty registry named after the item type.
    pub fn new() -> Self {
        Self {
            name: type_name::<T>(),
            state: ArcSwap::from_pointee(State {
                generation: 0,
                entries: IndexMap::new(),
            }),
            write_lock: Mutex::new(()),
            listeners: Mutex::new(Vec::new()),
            next_listener: AtomicU64::new(1),
        }
    }

    /// Item type name (diagnostics).
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Current generation; increases by one with every committed write.
    pub fn generation(&self) -> u64 {
        self.state.load().generation
    }

    /// A consistent snapshot of all entries. Lock-free.
    pub fn snapshot(&self) -> RegistrySnapshot<T> {
        RegistrySnapshot {
            state: self.state.load_full(),
        }
    }

    /// Looks up an item by id. Lock-free.
    pub fn get(&self, id: &str) -> Option<Arc<T>> {
        self.state
            .load()
            .entries
            .get(id)
            .map(|entry| entry.item.clone())
    }

    /// Returns `true` if `id` is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.state.load().entries.contains_key(id)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.state.load().entries.len()
    }

    /// Returns `true` if nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.state.load().entries.is_empty()
    }

    /// Registers `item` under `id`. Fails if the id is taken.
    pub fn register(
        &self,
        owner: Owner,
        id: impl Into<Arc<str>>,
        item: T,
    ) -> Result<(), RegistryError>
    where
        T: Sized,
    {
        self.register_arc(owner, id, Arc::new(item))
    }

    /// Registers an already shared (possibly unsized) item under `id`.
    pub fn register_arc(
        &self,
        owner: Owner,
        id: impl Into<Arc<str>>,
        item: Arc<T>,
    ) -> Result<(), RegistryError> {
        let id = id.into();
        self.batch(|tx| tx.register_arc(owner, id, item))
    }

    /// Registers `item` under `id`, replacing (and returning) any previous entry.
    pub fn replace(
        &self,
        owner: Owner,
        id: impl Into<Arc<str>>,
        item: Arc<T>,
    ) -> Result<Option<Entry<T>>, RegistryError> {
        let id = id.into();
        self.batch(|tx| tx.replace(owner, id, item))
    }

    /// Removes the entry registered under `id`.
    pub fn unregister(&self, id: &str) -> Option<Entry<T>> {
        self.batch(|tx| tx.unregister(id))
    }

    /// Removes every entry registered by `owner` (e.g. when a plugin unloads) and
    /// returns them, so the caller can wait until they are no longer in use.
    pub fn unregister_owner(&self, owner: &Owner) -> Vec<Entry<T>> {
        self.batch(|tx| tx.unregister_owner(owner))
    }

    /// Applies several writes atomically: readers observe either none or all of
    /// them, and subscribers receive one [`RegistryEvent`]. Copies the map once,
    /// so prefer this over many single writes during startup.
    pub fn batch<R>(&self, f: impl FnOnce(&mut RegistryTransaction<'_, T>) -> R) -> R {
        let (result, event) = {
            let _guard = self.write_lock.lock();
            let current = self.state.load_full();
            let mut tx = RegistryTransaction {
                registry: self.name,
                entries: None,
                base: &current.entries,
                changes: Vec::new(),
            };
            let result = f(&mut tx);
            if tx.changes.is_empty() {
                (result, None)
            } else {
                let generation = current.generation + 1;
                let entries = tx.entries.take().unwrap_or_else(|| current.entries.clone());
                self.state.store(Arc::new(State {
                    generation,
                    entries,
                }));
                let event = RegistryEvent {
                    registry: self.name,
                    generation,
                    changes: std::mem::take(&mut tx.changes),
                };
                (result, Some(event))
            }
        };
        if let Some(event) = event {
            self.notify(&event);
        }
        result
    }

    /// Calls `listener` after every committed change. Listeners run on the writing
    /// thread after the write lock is released, so they may read or write this
    /// registry. Keep them cheap (mark something dirty, send a message).
    pub fn subscribe(
        &self,
        listener: impl Fn(&RegistryEvent) + Send + Sync + 'static,
    ) -> SubscriptionId {
        let id = SubscriptionId(self.next_listener.fetch_add(1, Ordering::Relaxed));
        self.listeners.lock().push((id, Arc::new(listener)));
        id
    }

    /// Removes a listener. Returns `false` if it was not subscribed.
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut listeners = self.listeners.lock();
        let before = listeners.len();
        listeners.retain(|(existing, _)| *existing != id);
        listeners.len() != before
    }

    fn notify(&self, event: &RegistryEvent) {
        let listeners: Vec<Listener> = self
            .listeners
            .lock()
            .iter()
            .map(|(_, listener)| listener.clone())
            .collect();
        for listener in listeners {
            listener(event);
        }
    }
}

impl<T: ?Sized + Send + Sync + 'static> fmt::Debug for Registry<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state.load();
        f.debug_struct("Registry")
            .field("item", &self.name)
            .field("generation", &state.generation)
            .field("ids", &state.entries.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Pending writes inside [`Registry::batch`].
pub struct RegistryTransaction<'a, T: ?Sized> {
    registry: &'static str,
    base: &'a IndexMap<Arc<str>, Entry<T>>,
    /// Copy-on-first-write of `base`.
    entries: Option<IndexMap<Arc<str>, Entry<T>>>,
    changes: Vec<RegistryChange>,
}

impl<T: ?Sized> RegistryTransaction<'_, T> {
    fn view(&self) -> &IndexMap<Arc<str>, Entry<T>> {
        self.entries.as_ref().unwrap_or(self.base)
    }

    fn view_mut(&mut self) -> &mut IndexMap<Arc<str>, Entry<T>> {
        let base = self.base;
        self.entries.get_or_insert_with(|| base.clone())
    }

    /// Looks up an item, including writes made earlier in this transaction.
    pub fn get(&self, id: &str) -> Option<&Arc<T>> {
        self.view().get(id).map(|entry| &entry.item)
    }

    /// Returns `true` if `id` is registered (including pending writes).
    pub fn contains(&self, id: &str) -> bool {
        self.view().contains_key(id)
    }

    /// Registers `item` under `id`. Fails if the id is taken.
    pub fn register(
        &mut self,
        owner: Owner,
        id: impl Into<Arc<str>>,
        item: T,
    ) -> Result<(), RegistryError>
    where
        T: Sized,
    {
        self.register_arc(owner, id, Arc::new(item))
    }

    /// Registers a shared item under `id`. Fails if the id is taken.
    pub fn register_arc(
        &mut self,
        owner: Owner,
        id: impl Into<Arc<str>>,
        item: Arc<T>,
    ) -> Result<(), RegistryError> {
        let id = id.into();
        validate_id(&id)?;
        if let Some(existing) = self.view().get(&*id) {
            return Err(RegistryError::Duplicate {
                registry: self.registry,
                id,
                existing_owner: existing.owner.clone(),
            });
        }
        self.changes.push(RegistryChange::Registered {
            id: id.clone(),
            owner: owner.clone(),
        });
        self.view_mut()
            .insert(id.clone(), Entry { id, owner, item });
        Ok(())
    }

    /// Registers `item`, replacing and returning any previous entry with that id.
    /// The replaced entry keeps its position in registration order.
    pub fn replace(
        &mut self,
        owner: Owner,
        id: impl Into<Arc<str>>,
        item: Arc<T>,
    ) -> Result<Option<Entry<T>>, RegistryError> {
        let id = id.into();
        validate_id(&id)?;
        let entry = Entry {
            id: id.clone(),
            owner: owner.clone(),
            item,
        };
        let previous = self.view_mut().insert(id.clone(), entry);
        self.changes.push(match &previous {
            Some(previous) => RegistryChange::Replaced {
                id,
                owner,
                previous_owner: previous.owner.clone(),
            },
            None => RegistryChange::Registered { id, owner },
        });
        Ok(previous)
    }

    /// Removes the entry registered under `id`.
    pub fn unregister(&mut self, id: &str) -> Option<Entry<T>> {
        if !self.view().contains_key(id) {
            return None;
        }
        let removed = self.view_mut().shift_remove(id)?;
        self.changes.push(RegistryChange::Unregistered {
            id: removed.id.clone(),
            owner: removed.owner.clone(),
        });
        Some(removed)
    }

    /// Removes every entry registered by `owner`.
    pub fn unregister_owner(&mut self, owner: &Owner) -> Vec<Entry<T>> {
        if !self.view().values().any(|entry| &entry.owner == owner) {
            return Vec::new();
        }
        let mut removed = Vec::new();
        self.view_mut().retain(|_, entry| {
            if &entry.owner == owner {
                removed.push(entry.clone());
                false
            } else {
                true
            }
        });
        self.changes
            .extend(removed.iter().map(|entry| RegistryChange::Unregistered {
                id: entry.id.clone(),
                owner: entry.owner.clone(),
            }));
        removed
    }
}

fn validate_id(id: &Arc<str>) -> Result<(), RegistryError> {
    let reason = if id.is_empty() {
        Some("id must not be empty")
    } else if id.chars().any(|c| c.is_whitespace() || c.is_control()) {
        Some("id must not contain whitespace or control characters")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(RegistryError::InvalidId {
            id: id.clone(),
            reason,
        }),
        None => Ok(()),
    }
}

/// Type-erased view of a registry, used to clean up after an owner across all
/// registries without knowing their item types (plugin unload).
pub trait AnyRegistry: Send + Sync + 'static {
    /// Item type name.
    fn item_type_name(&self) -> &'static str;
    /// Number of entries.
    fn entry_count(&self) -> usize;
    /// Current generation.
    fn current_generation(&self) -> u64;
    /// Ids registered by `owner`, in registration order.
    fn ids_owned_by(&self, owner: &Owner) -> Vec<Arc<str>>;
    /// Removes all entries of `owner`; returns how many were removed.
    fn remove_owner(&self, owner: &Owner) -> usize;
    /// `true` if at least one entry is registered by `owner`.
    fn has_owner(&self, owner: &Owner) -> bool;
    /// Upcast for typed downcasting.
    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

impl<T: ?Sized + Send + Sync + 'static> AnyRegistry for Registry<T> {
    fn item_type_name(&self) -> &'static str {
        self.name
    }

    fn entry_count(&self) -> usize {
        self.len()
    }

    fn current_generation(&self) -> u64 {
        self.generation()
    }

    fn ids_owned_by(&self, owner: &Owner) -> Vec<Arc<str>> {
        self.state
            .load()
            .entries
            .values()
            .filter(|entry| &entry.owner == owner)
            .map(|entry| entry.id.clone())
            .collect()
    }

    fn remove_owner(&self, owner: &Owner) -> usize {
        self.unregister_owner(owner).len()
    }

    fn has_owner(&self, owner: &Owner) -> bool {
        self.state
            .load()
            .entries
            .values()
            .any(|entry| &entry.owner == owner)
    }

    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    trait Greeter: Send + Sync {
        fn greet(&self) -> String;
    }
    struct Hello;
    impl Greeter for Hello {
        fn greet(&self) -> String {
            "hello".into()
        }
    }

    #[test]
    fn register_get_and_duplicates() {
        let reg = Registry::<u32>::new();
        reg.register(Owner::Core, "a", 1).unwrap();
        reg.register(Owner::module("m"), "b", 2).unwrap();
        assert_eq!(reg.get("a").as_deref(), Some(&1));
        assert_eq!(reg.len(), 2);
        let err = reg.register(Owner::plugin("p"), "a", 3).unwrap_err();
        assert!(
            matches!(err, RegistryError::Duplicate { ref existing_owner, .. } if *existing_owner == Owner::Core)
        );
        assert_eq!(
            reg.generation(),
            2,
            "failed writes do not bump the generation"
        );
    }

    #[test]
    fn invalid_ids_are_rejected() {
        let reg = Registry::<u32>::new();
        assert!(matches!(
            reg.register(Owner::Core, "", 1),
            Err(RegistryError::InvalidId { .. })
        ));
        assert!(matches!(
            reg.register(Owner::Core, "a b", 1),
            Err(RegistryError::InvalidId { .. })
        ));
        assert!(reg.is_empty());
    }

    #[test]
    fn unsized_items() {
        let reg = Registry::<dyn Greeter>::new();
        reg.register_arc(Owner::Core, "hello", Arc::new(Hello))
            .unwrap();
        assert_eq!(reg.get("hello").unwrap().greet(), "hello");
    }

    #[test]
    fn snapshots_are_stable_and_ordered() {
        let reg = Registry::<&'static str>::new();
        for id in ["z", "a", "m"] {
            reg.register(Owner::Core, id, id).unwrap();
        }
        let snap = reg.snapshot();
        reg.unregister("a");
        assert_eq!(snap.len(), 3, "old snapshot is unaffected");
        let ids: Vec<_> = reg.snapshot().iter().map(|e| e.id().to_string()).collect();
        assert_eq!(ids, ["z", "m"], "registration order is preserved");
    }

    #[test]
    fn unregister_owner_removes_only_that_owner() {
        let reg = Registry::<u32>::new();
        let plugin = Owner::plugin("com.example.glow");
        reg.batch(|tx| {
            tx.register(Owner::Core, "core.blur", 1).unwrap();
            tx.register(plugin.clone(), "glow.effect", 2).unwrap();
            tx.register(plugin.clone(), "glow.preset", 3).unwrap();
        });
        assert_eq!(reg.generation(), 1, "a batch is one generation");
        let removed = reg.unregister_owner(&plugin);
        assert_eq!(removed.len(), 2);
        assert!(reg.contains("core.blur"));
        assert!(!reg.contains("glow.effect"));
        assert!(removed.iter().all(Entry::is_unique));
    }

    #[test]
    fn in_flight_items_outlive_unregistration() {
        let reg = Registry::<String>::new();
        reg.register(Owner::plugin("p"), "x", "payload".to_string())
            .unwrap();
        let in_use = reg.get("x").unwrap();
        let removed = reg.unregister_owner(&Owner::plugin("p"));
        assert!(!removed[0].is_unique(), "a caller still holds the item");
        assert_eq!(*in_use, "payload");
        drop(in_use);
        assert!(removed[0].is_unique());
    }

    #[test]
    fn replace_keeps_position_and_reports() {
        let reg = Registry::<u32>::new();
        reg.register(Owner::Core, "a", 1).unwrap();
        reg.register(Owner::Core, "b", 2).unwrap();
        let previous = reg.replace(Owner::plugin("p"), "a", Arc::new(10)).unwrap();
        assert_eq!(previous.map(|e| *e.item().as_ref()), Some(1));
        let snap = reg.snapshot();
        let first = snap.iter().next().unwrap();
        assert_eq!((first.id().as_ref(), **first.item()), ("a", 10));
        assert_eq!(first.owner(), &Owner::plugin("p"));
    }

    #[test]
    fn listeners_receive_batched_events_and_may_reenter() {
        let reg = Arc::new(Registry::<u32>::new());
        let seen = Arc::new(AtomicUsize::new(0));
        let reg2 = reg.clone();
        let seen2 = seen.clone();
        let sub = reg.subscribe(move |event| {
            seen2.fetch_add(event.changes.len(), Ordering::SeqCst);
            // Re-entrant read and write from a listener must not deadlock.
            if !reg2.contains("echo") {
                reg2.register(Owner::Core, "echo", 0).unwrap();
            }
        });
        reg.batch(|tx| {
            tx.register(Owner::Core, "a", 1).unwrap();
            tx.register(Owner::Core, "b", 2).unwrap();
        });
        // 2 changes from the batch + 1 from the echo registration.
        assert_eq!(seen.load(Ordering::SeqCst), 3);
        assert!(reg.unsubscribe(sub));
        assert!(!reg.unsubscribe(sub));
        reg.register(Owner::Core, "c", 3).unwrap();
        assert_eq!(seen.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn concurrent_writers_and_readers() {
        let reg = Arc::new(Registry::<usize>::new());
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let reg = reg.clone();
                std::thread::spawn(move || {
                    for i in 0..100 {
                        reg.register(Owner::module(format!("m{t}")), format!("{t}.{i}"), i)
                            .unwrap();
                        let _ = reg.get(&format!("{t}.{i}")).unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(reg.len(), 800);
        assert_eq!(reg.generation(), 800);
        let erased: Arc<dyn AnyRegistry> = reg.clone();
        assert_eq!(erased.ids_owned_by(&Owner::module("m3")).len(), 100);
        assert_eq!(erased.remove_owner(&Owner::module("m3")), 100);
        assert!(!erased.has_owner(&Owner::module("m3")));
        assert_eq!(erased.entry_count(), 700);
    }
}
