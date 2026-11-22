//! Zero-copy display of render output in the UI.
//!
//! The renderer publishes the texture it rendered a composition into under a
//! [`ViewportTextureId`]; the desktop shell (apps/bae) registers each published
//! view with egui-wgpu once and records the resulting egui texture id as the
//! entry's *display handle*; UI panels (composition viewer, footage viewer,
//! thumbnails) draw that handle. The pixels never leave the GPU, and the
//! registry has no UI dependency, so renderer crates can publish without
//! knowing about egui.
//!
//! ```text
//! bae-render ──publish(id, texture)──► ViewportTextureRegistry ◄──sync each frame── apps/bae shell
//!                                            │ display_handle(id)                      (egui-wgpu)
//!                                            ▼
//!                               UI panel: egui::Image(TextureId::User(handle))
//! ```
//!
//! Rules for published textures: 2D, `TEXTURE_BINDING` usage, a filterable color
//! format (`Rgba8Unorm`, `Rgba16Float`, …), premultiplied alpha, already in the
//! display encoding expected by the UI (the color workstream's display
//! transform runs before publishing). Republish (same id) whenever the texture
//! object changes (resize, new allocation); rendering new pixels into the same
//! texture needs no republish.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;

/// Identifies a viewport texture slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ViewportTextureId(u64);

impl ViewportTextureId {
    /// The raw id.
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ViewportTextureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "viewport#{}", self.0)
    }
}

/// A published texture view.
#[derive(Debug, Clone)]
pub struct ViewportTexture {
    /// The view the UI samples.
    pub view: wgpu::TextureView,
    /// Size in pixels.
    pub size: [u32; 2],
    /// Format of the view.
    pub format: wgpu::TextureFormat,
    /// Sampling filter for display (nearest for pixel-exact zoom).
    pub filter: wgpu::FilterMode,
    /// Bumped on every publish; consumers re-register when it changes.
    pub generation: u64,
}

/// Errors from [`ViewportTextureRegistry::publish`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ViewportError {
    /// The id was never created or was removed.
    #[error("unknown viewport texture {0}")]
    UnknownId(ViewportTextureId),
    /// The texture cannot be displayed.
    #[error("texture cannot be displayed: {0}")]
    Unsupported(String),
}

#[derive(Debug)]
struct Slot {
    label: Arc<str>,
    texture: Option<ViewportTexture>,
    display_handle: Option<u64>,
}

#[derive(Debug, Default)]
struct State {
    next_id: u64,
    slots: BTreeMap<ViewportTextureId, Slot>,
}

/// Thread-safe map of viewport ids to texture views. Shared as an
/// `Arc<ViewportTextureRegistry>` app resource.
#[derive(Debug, Default)]
pub struct ViewportTextureRegistry {
    state: Mutex<State>,
    generation: AtomicU64,
}

/// One entry of [`ViewportTextureRegistry::snapshot`].
#[derive(Debug, Clone)]
pub struct ViewportEntry {
    /// The id.
    pub id: ViewportTextureId,
    /// Debug label.
    pub label: Arc<str>,
    /// The published texture, if any.
    pub texture: Option<ViewportTexture>,
    /// The UI's handle for it, if the shell registered it.
    pub display_handle: Option<u64>,
}

impl ViewportTextureRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a slot (no texture yet).
    pub fn create(&self, label: impl Into<Arc<str>>) -> ViewportTextureId {
        let mut state = self.state.lock();
        state.next_id += 1;
        let id = ViewportTextureId(state.next_id);
        state.slots.insert(
            id,
            Slot {
                label: label.into(),
                texture: None,
                display_handle: None,
            },
        );
        drop(state);
        self.bump();
        id
    }

    /// Publishes `texture` (its full mip 0) under `id`.
    pub fn publish(
        &self,
        id: ViewportTextureId,
        texture: &wgpu::Texture,
        filter: wgpu::FilterMode,
    ) -> Result<(), ViewportError> {
        if !texture
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(ViewportError::Unsupported(
                "needs TEXTURE_BINDING usage".into(),
            ));
        }
        if texture.dimension() != wgpu::TextureDimension::D2 {
            return Err(ViewportError::Unsupported("must be a 2D texture".into()));
        }
        let format = texture.format();
        if !matches!(
            format.sample_type(None, None),
            Some(wgpu::TextureSampleType::Float { filterable: true })
        ) {
            return Err(ViewportError::Unsupported(format!(
                "{format:?} is not a filterable color format"
            )));
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("bae.viewport"),
            base_mip_level: 0,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: Some(1),
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });
        self.publish_view(
            id,
            view,
            [texture.width(), texture.height()],
            format,
            filter,
        )
    }

    /// Publishes an existing view (2D, filterable, from a `TEXTURE_BINDING` texture).
    pub fn publish_view(
        &self,
        id: ViewportTextureId,
        view: wgpu::TextureView,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        filter: wgpu::FilterMode,
    ) -> Result<(), ViewportError> {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let mut state = self.state.lock();
        let slot = state
            .slots
            .get_mut(&id)
            .ok_or(ViewportError::UnknownId(id))?;
        slot.texture = Some(ViewportTexture {
            view,
            size,
            format,
            filter,
            generation,
        });
        Ok(())
    }

    /// Removes a slot. The shell frees its UI registration on the next sync.
    pub fn remove(&self, id: ViewportTextureId) -> bool {
        let removed = self.state.lock().slots.remove(&id).is_some();
        if removed {
            self.bump();
        }
        removed
    }

    /// The published texture of `id`.
    pub fn get(&self, id: ViewportTextureId) -> Option<ViewportTexture> {
        self.state.lock().slots.get(&id)?.texture.clone()
    }

    /// Changes whenever a slot is created, published or removed. Consumers compare
    /// it with the value they last synced to skip work on unchanged frames.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// All slots, in id order.
    pub fn snapshot(&self) -> Vec<ViewportEntry> {
        self.state
            .lock()
            .slots
            .iter()
            .map(|(&id, slot)| ViewportEntry {
                id,
                label: slot.label.clone(),
                texture: slot.texture.clone(),
                display_handle: slot.display_handle,
            })
            .collect()
    }

    /// Records the UI's handle for `id` (egui: the `u64` of `TextureId::User`).
    /// Set by the shell only. Does not bump the generation.
    pub fn set_display_handle(&self, id: ViewportTextureId, handle: Option<u64>) {
        if let Some(slot) = self.state.lock().slots.get_mut(&id) {
            slot.display_handle = handle;
        }
    }

    /// The UI's handle for `id`, once the shell registered the texture. In the
    /// desktop shell this is egui's `TextureId::User(handle)`.
    pub fn display_handle(&self, id: ViewportTextureId) -> Option<u64> {
        self.state.lock().slots.get(&id)?.display_handle
    }

    fn bump(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_and_generations() {
        let registry = ViewportTextureRegistry::new();
        let g0 = registry.generation();
        let a = registry.create("comp viewer");
        let b = registry.create("footage viewer");
        assert_ne!(a, b);
        assert!(registry.generation() > g0);
        assert!(registry.get(a).is_none());
        registry.set_display_handle(a, Some(7));
        assert_eq!(registry.display_handle(a), Some(7));
        let g1 = registry.generation();
        assert!(registry.remove(a));
        assert!(!registry.remove(a));
        assert!(registry.generation() > g1);
        assert_eq!(registry.display_handle(a), None);
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].id, b);
        assert_eq!(&*snapshot[0].label, "footage viewer");
    }
}
