//! Persisted main-window geometry (`<state dir>/window.json`).

use bae_platform::VersionedSettings;
use serde::{Deserialize, Serialize};
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

/// File name in the state directory.
pub const FILE_NAME: &str = "window.json";

/// Main window geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    /// Inner size in logical points.
    pub size: [f64; 2],
    /// Outer position in physical pixels (absent: let the OS place the window).
    pub position: Option<[i32; 2]>,
    /// Maximized (zoomed) state.
    pub maximized: bool,
    /// Borderless full screen.
    pub fullscreen: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            size: [1440.0, 900.0],
            position: None,
            maximized: false,
            fullscreen: false,
        }
    }
}

impl VersionedSettings for WindowState {
    const KIND: &'static str = "bae-window-state";
    const VERSION: u32 = 1;
}

impl WindowState {
    /// Smallest window we restore (anything smaller is treated as corrupt).
    const MIN_SIZE: [f64; 2] = [640.0, 400.0];

    /// Window attributes restoring this state. A saved position is only used if
    /// it is still on a connected monitor (displays change between sessions).
    pub fn apply(
        &self,
        mut attributes: WindowAttributes,
        event_loop: &ActiveEventLoop,
    ) -> WindowAttributes {
        let [w, h] = self.size;
        let size = if w.is_finite() && h.is_finite() {
            [w.max(Self::MIN_SIZE[0]), h.max(Self::MIN_SIZE[1])]
        } else {
            Self::default().size
        };
        attributes = attributes
            .with_inner_size(LogicalSize::new(size[0], size[1]))
            .with_min_inner_size(LogicalSize::new(Self::MIN_SIZE[0], Self::MIN_SIZE[1]))
            .with_maximized(self.maximized);
        if let Some([x, y]) = self.position
            && on_some_monitor(event_loop, x, y)
        {
            attributes = attributes.with_position(PhysicalPosition::new(x, y));
        }
        if self.fullscreen {
            attributes =
                attributes.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
        attributes
    }

    /// Captures the current geometry. While maximized or full screen, the
    /// restored (normal) geometry is kept from `previous` so un-maximizing after
    /// the next launch returns to the user's size.
    pub fn capture(window: &Window, previous: &WindowState) -> WindowState {
        let maximized = window.is_maximized();
        let fullscreen = window.fullscreen().is_some();
        let minimized = window.is_minimized().unwrap_or(false);
        if maximized || fullscreen || minimized {
            return WindowState {
                maximized,
                fullscreen,
                ..previous.clone()
            };
        }
        let size = window.inner_size().to_logical::<f64>(window.scale_factor());
        WindowState {
            size: [size.width, size.height],
            position: window.outer_position().ok().map(|p| [p.x, p.y]),
            maximized: false,
            fullscreen: false,
        }
    }
}

fn on_some_monitor(event_loop: &ActiveEventLoop, x: i32, y: i32) -> bool {
    event_loop.available_monitors().any(|monitor| {
        let pos = monitor.position();
        let size = monitor.size();
        // Require the title-bar area (top-left + 40 px) to be visible.
        let (x, y) = (i64::from(x) + 40, i64::from(y) + 20);
        x >= i64::from(pos.x)
            && y >= i64::from(pos.y)
            && x < i64::from(pos.x) + i64::from(size.width)
            && y < i64::from(pos.y) + i64::from(size.height)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_platform::SettingsFile;

    #[test]
    fn round_trips_through_settings_file() {
        let dir = std::env::temp_dir().join(format!("bae-window-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = SettingsFile::<WindowState>::new(dir.join(FILE_NAME));
        assert_eq!(file.load().value, WindowState::default());
        let state = WindowState {
            size: [1200.0, 800.0],
            position: Some([10, 20]),
            maximized: true,
            fullscreen: false,
        };
        file.save(&state).unwrap();
        assert_eq!(file.load().value, state);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
