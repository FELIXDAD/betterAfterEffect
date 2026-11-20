//! [`AppModule`], [`AppBuilder`] and the finished [`App`].

use std::any::{Any, TypeId, type_name};
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::registry::{AnyRegistry, Owner, Registry, RegistryError};
use crate::resources::Resources;

/// A unit of application functionality, contributed by one crate.
///
/// `build` runs once while the application is assembled. It may insert resources,
/// register items into registries (commands, panels, workspaces, CLI subcommands,
/// importers, exporters, effects …) and add the modules it depends on — adding a
/// module twice is a no-op, so dependencies can be declared freely.
pub trait AppModule: Send + Sync + 'static {
    /// Human-readable module name, used as the [`Owner`] of its registrations.
    /// Defaults to the Rust type name; override with a stable short name such as
    /// `"bae-gpu"`.
    fn name(&self) -> &str {
        type_name::<Self>()
    }

    /// Registers the module's resources and registry entries.
    fn build(&self, app: &mut AppBuilder);

    /// Runs after **every** module's `build`, in the same order. Use it for work
    /// that needs other modules' registrations (e.g. building menus from the
    /// command registry).
    fn finish(&self, app: &mut AppBuilder) {
        let _ = app;
    }
}

/// Metadata about an added module.
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    /// [`AppModule::name`].
    pub name: Arc<str>,
    /// Rust type name of the module.
    pub type_name: &'static str,
}

struct ModuleSlot {
    info: ModuleInfo,
    module: Arc<dyn AppModule>,
}

type RegistryMap = IndexMap<TypeId, Arc<dyn AnyRegistry>>;

/// Assembles an [`App`] from [`AppModule`]s.
pub struct AppBuilder {
    resources: Resources,
    registries: RegistryMap,
    module_types: HashSet<TypeId>,
    modules: Vec<ModuleSlot>,
    pending_finish: Vec<usize>,
    owner_stack: Vec<Owner>,
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    /// Creates an empty builder.
    pub fn new() -> Self {
        Self {
            resources: Resources::new(),
            registries: IndexMap::new(),
            module_types: HashSet::new(),
            modules: Vec::new(),
            pending_finish: Vec::new(),
            owner_stack: Vec::new(),
        }
    }

    /// Adds `module` and runs its [`AppModule::build`] immediately. Adding the same
    /// module type twice is a no-op (the second instance is dropped).
    pub fn add_module<M: AppModule>(&mut self, module: M) -> &mut Self {
        if !self.module_types.insert(TypeId::of::<M>()) {
            return self;
        }
        let info = ModuleInfo {
            name: Arc::from(module.name()),
            type_name: type_name::<M>(),
        };
        self.owner_stack.push(Owner::Module(info.name.clone()));
        module.build(self);
        self.owner_stack.pop();
        self.pending_finish.push(self.modules.len());
        self.modules.push(ModuleSlot {
            info,
            module: Arc::new(module),
        });
        self
    }

    /// Returns `true` if a module of type `M` was added.
    pub fn has_module<M: AppModule>(&self) -> bool {
        self.module_types.contains(&TypeId::of::<M>())
    }

    /// Modules in the order they were added.
    pub fn modules(&self) -> impl ExactSizeIterator<Item = &ModuleInfo> + '_ {
        self.modules.iter().map(|slot| &slot.info)
    }

    /// The owner that registrations made right now are attributed to: the module
    /// whose `build`/`finish` is running, or [`Owner::Core`].
    pub fn current_owner(&self) -> Owner {
        self.owner_stack.last().cloned().unwrap_or(Owner::Core)
    }

    // ---- resources -------------------------------------------------------------

    /// Inserts a resource, returning the previous value of that type.
    pub fn insert_resource<R: Send + Sync + 'static>(&mut self, value: R) -> Option<R> {
        self.resources.insert(value)
    }

    /// Returns the resource of type `R`, inserting `R::default()` if missing.
    pub fn init_resource<R: Default + Send + Sync + 'static>(&mut self) -> &mut R {
        self.resources.get_or_insert_with(R::default)
    }

    /// Returns the resource of type `R`.
    pub fn resource<R: Send + Sync + 'static>(&self) -> Option<&R> {
        self.resources.get()
    }

    /// Returns the resource of type `R` mutably.
    pub fn resource_mut<R: Send + Sync + 'static>(&mut self) -> Option<&mut R> {
        self.resources.get_mut()
    }

    /// Removes the resource of type `R`.
    pub fn remove_resource<R: Send + Sync + 'static>(&mut self) -> Option<R> {
        self.resources.remove()
    }

    /// All resources.
    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    /// All resources, mutably.
    pub fn resources_mut(&mut self) -> &mut Resources {
        &mut self.resources
    }

    // ---- registries ------------------------------------------------------------

    /// The registry for items of type `T`, created on first use. The returned `Arc`
    /// stays valid for the lifetime of the app and may be kept by long-lived
    /// services (plugin host, UI) to register or look up items at runtime.
    pub fn registry<T: ?Sized + Send + Sync + 'static>(&mut self) -> Arc<Registry<T>> {
        registry_in(&mut self.registries)
    }

    /// Registers `item` under `id` in `Registry<T>`, owned by [`Self::current_owner`].
    pub fn register<T: Send + Sync + 'static>(
        &mut self,
        id: impl Into<Arc<str>>,
        item: T,
    ) -> Result<(), RegistryError> {
        let owner = self.current_owner();
        self.registry::<T>().register(owner, id, item)
    }

    /// Registers a shared (possibly unsized) item, owned by [`Self::current_owner`].
    pub fn register_arc<T: ?Sized + Send + Sync + 'static>(
        &mut self,
        id: impl Into<Arc<str>>,
        item: Arc<T>,
    ) -> Result<(), RegistryError> {
        let owner = self.current_owner();
        self.registry::<T>().register_arc(owner, id, item)
    }

    /// Runs every module's [`AppModule::finish`] and returns the finished [`App`].
    pub fn build(mut self) -> App {
        // `finish` may add more modules; their `build` runs immediately and their
        // `finish` is queued, so loop until nothing is pending.
        while !self.pending_finish.is_empty() {
            let pending = std::mem::take(&mut self.pending_finish);
            for index in pending {
                let module = self.modules[index].module.clone();
                let name = self.modules[index].info.name.clone();
                self.owner_stack.push(Owner::Module(name));
                module.finish(&mut self);
                self.owner_stack.pop();
            }
        }
        App {
            resources: self.resources,
            registries: self.registries,
            modules: self.modules.into_iter().map(|slot| slot.info).collect(),
        }
    }
}

impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppBuilder")
            .field(
                "modules",
                &self.modules().map(|m| &m.name).collect::<Vec<_>>(),
            )
            .field("resources", &self.resources)
            .field("registries", &registry_names(&self.registries))
            .finish()
    }
}

/// The assembled application: resources and registries, shared by the shell, the
/// headless runner, the MCP server and the plugin host.
pub struct App {
    resources: Resources,
    registries: RegistryMap,
    modules: Vec<ModuleInfo>,
}

impl App {
    /// Modules in the order they were added.
    pub fn modules(&self) -> &[ModuleInfo] {
        &self.modules
    }

    /// Inserts a resource, returning the previous value of that type.
    pub fn insert_resource<R: Send + Sync + 'static>(&mut self, value: R) -> Option<R> {
        self.resources.insert(value)
    }

    /// Returns the resource of type `R`.
    pub fn resource<R: Send + Sync + 'static>(&self) -> Option<&R> {
        self.resources.get()
    }

    /// Returns the resource of type `R` mutably.
    pub fn resource_mut<R: Send + Sync + 'static>(&mut self) -> Option<&mut R> {
        self.resources.get_mut()
    }

    /// Removes the resource of type `R`.
    pub fn remove_resource<R: Send + Sync + 'static>(&mut self) -> Option<R> {
        self.resources.remove()
    }

    /// All resources.
    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    /// All resources, mutably.
    pub fn resources_mut(&mut self) -> &mut Resources {
        &mut self.resources
    }

    /// The registry for `T`, created if no module touched it yet.
    pub fn registry<T: ?Sized + Send + Sync + 'static>(&mut self) -> Arc<Registry<T>> {
        registry_in(&mut self.registries)
    }

    /// The registry for `T`, if it exists.
    pub fn get_registry<T: ?Sized + Send + Sync + 'static>(&self) -> Option<Arc<Registry<T>>> {
        self.registries
            .get(&TypeId::of::<Registry<T>>())
            .map(|erased| downcast_registry::<T>(erased.clone()))
    }

    /// Every registry, type-erased, in creation order.
    pub fn registries(&self) -> impl ExactSizeIterator<Item = &Arc<dyn AnyRegistry>> + '_ {
        self.registries.values()
    }

    /// Removes everything `owner` registered, in every registry (plugin unload).
    /// Returns the number of removed entries.
    pub fn unregister_owner(&self, owner: &Owner) -> usize {
        self.registries
            .values()
            .map(|registry| registry.remove_owner(owner))
            .sum()
    }
}

impl fmt::Debug for App {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("App")
            .field(
                "modules",
                &self.modules.iter().map(|m| &m.name).collect::<Vec<_>>(),
            )
            .field("resources", &self.resources)
            .field("registries", &registry_names(&self.registries))
            .finish()
    }
}

fn registry_in<T: ?Sized + Send + Sync + 'static>(map: &mut RegistryMap) -> Arc<Registry<T>> {
    let erased = map
        .entry(TypeId::of::<Registry<T>>())
        .or_insert_with(|| Arc::new(Registry::<T>::new()) as Arc<dyn AnyRegistry>)
        .clone();
    downcast_registry::<T>(erased)
}

fn downcast_registry<T: ?Sized + Send + Sync + 'static>(
    erased: Arc<dyn AnyRegistry>,
) -> Arc<Registry<T>> {
    let any: Arc<dyn Any + Send + Sync> = erased.into_any_arc();
    match any.downcast::<Registry<T>>() {
        Ok(registry) => registry,
        Err(_) => unreachable!("registry map is keyed by TypeId of Registry<T>"),
    }
}

fn registry_names(map: &RegistryMap) -> Vec<&'static str> {
    map.values().map(|r| r.item_type_name()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct Command {
        title: &'static str,
    }

    #[derive(Default)]
    struct Log(Mutex<Vec<String>>);

    struct Commands;
    impl AppModule for Commands {
        fn name(&self) -> &str {
            "commands"
        }
        fn build(&self, app: &mut AppBuilder) {
            app.init_resource::<Log>()
                .0
                .lock()
                .unwrap()
                .push("commands.build".into());
            app.register("file.new", Command { title: "New" }).unwrap();
        }
        fn finish(&self, app: &mut AppBuilder) {
            let count = app.registry::<Command>().len();
            app.resource::<Log>()
                .unwrap()
                .0
                .lock()
                .unwrap()
                .push(format!("commands.finish saw {count}"));
        }
    }

    struct Menus;
    impl AppModule for Menus {
        fn name(&self) -> &str {
            "menus"
        }
        fn build(&self, app: &mut AppBuilder) {
            app.add_module(Commands); // dependency; no-op if already added
            app.register("edit.undo", Command { title: "Undo" })
                .unwrap();
        }
    }

    struct Late;
    impl AppModule for Late {
        fn build(&self, app: &mut AppBuilder) {
            app.register("late.cmd", Command { title: "Late" }).unwrap();
        }
    }

    struct AddsLateInFinish;
    impl AppModule for AddsLateInFinish {
        fn build(&self, _app: &mut AppBuilder) {}
        fn finish(&self, app: &mut AppBuilder) {
            app.add_module(Late);
        }
    }

    #[test]
    fn modules_register_with_owners_and_dedupe() {
        let mut builder = AppBuilder::new();
        builder.add_module(Menus).add_module(Commands);
        assert!(builder.has_module::<Commands>());
        let names: Vec<_> = builder.modules().map(|m| m.name.to_string()).collect();
        assert_eq!(names, ["commands", "menus"], "dependency built first, once");
        let mut app = builder.build();
        let commands = app.registry::<Command>();
        let snap = commands.snapshot();
        assert_eq!(
            snap.entry("file.new").unwrap().owner(),
            &Owner::module("commands")
        );
        assert_eq!(
            snap.entry("edit.undo").unwrap().owner(),
            &Owner::module("menus")
        );
        assert_eq!(snap.get("edit.undo").unwrap().title, "Undo");
        let log = app.resource::<Log>().unwrap().0.lock().unwrap().clone();
        assert_eq!(log, ["commands.build", "commands.finish saw 2"]);
    }

    #[test]
    fn finish_may_add_modules() {
        let mut builder = AppBuilder::new();
        builder.add_module(AddsLateInFinish);
        let app = builder.build();
        assert_eq!(app.modules().len(), 2);
        assert!(app.get_registry::<Command>().unwrap().contains("late.cmd"));
    }

    #[test]
    fn runtime_registration_and_plugin_unload() {
        let mut app = AppBuilder::new().build();
        assert!(app.get_registry::<Command>().is_none());
        let commands = app.registry::<Command>();
        let plugin = Owner::plugin("com.example.tools");
        commands
            .register(plugin.clone(), "tools.align", Command { title: "Align" })
            .unwrap();
        app.registry::<u64>()
            .register(plugin.clone(), "tools.counter", 7)
            .unwrap();
        app.registry::<u64>()
            .register(Owner::Core, "core.counter", 1)
            .unwrap();
        assert_eq!(app.unregister_owner(&plugin), 2);
        assert!(commands.is_empty());
        assert_eq!(app.get_registry::<u64>().unwrap().len(), 1);
        assert_eq!(app.registries().len(), 2);
    }

    #[test]
    fn registrations_outside_modules_are_core() {
        let mut builder = AppBuilder::new();
        builder.register("x", 1u8).unwrap();
        assert_eq!(builder.current_owner(), Owner::Core);
        let app = builder.build();
        let reg = app.get_registry::<u8>().unwrap();
        assert_eq!(reg.snapshot().entry("x").unwrap().owner(), &Owner::Core);
    }
}
