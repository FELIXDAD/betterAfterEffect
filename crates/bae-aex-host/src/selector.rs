//! Command selectors and the call-order contract.
//!
//! Every interaction between host and AE effect is one call of the plug-in's
//! entry point with a command selector (SDK guide, "Effect Basics › Command
//! Selectors"). [`Selector`] lists every selector the public guide documents,
//! under BAE-owned names; [`Selector::info`] records, per selector, what the
//! host must provide and which BAE plug-in lifecycle hook (workstream 19) it
//! corresponds to; [`ProtocolTracker`] enforces the ordering rules the host
//! itself must respect (and catches host bugs in tests).

use std::collections::HashMap;

use crate::error::{HostError, Result};

/// Every documented `PF_Cmd_*` selector (BAE naming).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Selector {
    /// `PF_Cmd_ABOUT`: show about text (host displays `return_msg`).
    About,
    /// `PF_Cmd_GLOBAL_SETUP`: plug-in reports version and capability flags.
    GlobalSetup,
    /// `PF_Cmd_GLOBAL_SETDOWN`: free global data.
    GlobalSetdown,
    /// `PF_Cmd_PARAM_SETUP` (named PARAMS_SETUP in some places): declare params.
    ParamsSetup,
    /// `PF_Cmd_SEQUENCE_SETUP`: new instance applied; allocate sequence data.
    SequenceSetup,
    /// `PF_Cmd_SEQUENCE_RESETUP`: rebuild (unflatten) sequence data after
    /// load, duplication, precompose or layer change.
    SequenceResetup,
    /// `PF_Cmd_SEQUENCE_FLATTEN`: produce flat, byte-ordered sequence data for
    /// saving / duplication.
    SequenceFlatten,
    /// `PF_Cmd_SEQUENCE_SETDOWN`: free sequence data.
    SequenceSetdown,
    /// `PF_Cmd_GET_FLATTENED_SEQUENCE_DATA`: return a flat *copy* without
    /// disturbing the live data (Multi-Frame Rendering marshalling).
    GetFlattenedSequenceData,
    /// `PF_Cmd_FRAME_SETUP`: legacy per-frame setup / output resize.
    FrameSetup,
    /// `PF_Cmd_RENDER`: legacy render (8/16 bpc only).
    Render,
    /// `PF_Cmd_FRAME_SETDOWN`: legacy per-frame teardown.
    FrameSetdown,
    /// `PF_Cmd_SMART_PRE_RENDER`: SmartFX ROI negotiation and input checkout.
    SmartPreRender,
    /// `PF_Cmd_SMART_RENDER`: SmartFX CPU render (8/16/32 bpc).
    SmartRender,
    /// `PF_Cmd_SMART_RENDER_GPU`: SmartFX GPU render.
    SmartRenderGpu,
    /// `PF_Cmd_AUDIO_SETUP`.
    AudioSetup,
    /// `PF_Cmd_AUDIO_RENDER`.
    AudioRender,
    /// `PF_Cmd_AUDIO_SETDOWN`.
    AudioSetdown,
    /// `PF_Cmd_EVENT`: custom UI event (extra = event record).
    Event,
    /// `PF_Cmd_USER_CHANGED_PARAM`: a supervised param changed by the user.
    UserChangedParam,
    /// `PF_Cmd_UPDATE_PARAMS_UI`: cosmetic param UI refresh.
    UpdateParamsUi,
    /// `PF_Cmd_DO_DIALOG`: options dialog.
    DoDialog,
    /// `PF_Cmd_ARBITRARY_CALLBACK`: arbitrary-data param handler.
    ArbitraryCallback,
    /// `PF_Cmd_GET_EXTERNAL_DEPENDENCIES`: report files/fonts the effect needs.
    GetExternalDependencies,
    /// `PF_Cmd_COMPLETELY_GENERAL`: private message from an AEGP.
    CompletelyGeneral,
    /// `PF_Cmd_QUERY_DYNAMIC_FLAGS`: re-evaluate dynamic capability flags.
    QueryDynamicFlags,
    /// `PF_Cmd_GPU_DEVICE_SETUP`: per-device GPU initialisation / opt-out.
    GpuDeviceSetup,
    /// `PF_Cmd_GPU_DEVICE_SETDOWN`: release per-device GPU data.
    GpuDeviceSetdown,
}

/// Selector family as grouped in the SDK guide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SelectorGroup {
    /// Plug-in-wide, not tied to an effect instance.
    Global,
    /// Tied to one effect instance (AE "sequence").
    Sequence,
    /// Per rendered frame.
    Frame,
    /// Per audio render.
    Audio,
    /// UI / messaging.
    Messaging,
    /// Per GPU device.
    Gpu,
}

/// Which thread the host may issue a selector on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreadAffinity {
    /// Main thread, with no other selector of this plug-in in flight
    /// (global setup/setdown under Multi-Frame Rendering).
    MainExclusive,
    /// Main (UI) thread; render selectors may run concurrently on other
    /// threads if the plug-in opted into threaded rendering.
    Main,
    /// Any render thread; concurrent with other render calls only if the
    /// plug-in declared thread safety, otherwise the host serialises.
    Render,
}

/// Which parameter values the host must have filled in `params[]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamsValidity {
    /// `params` is not valid.
    None,
    /// Non-layer params valid; layer params are not checked out.
    NonLayer,
    /// All params including checked-out layer worlds (legacy render).
    All,
}

/// Type of the `extra` argument for a selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExtraKind {
    /// Unused.
    None,
    /// SmartFX pre-render record ([`crate::smart`]).
    PreRender,
    /// SmartFX render record (CPU and GPU share it).
    SmartRender,
    /// UI event record ([`crate::ui`]).
    Event,
    /// Arbitrary-data function selector + arguments ([`crate::params`]).
    ArbitraryCallback,
    /// External dependencies request.
    ExternalDependencies,
    /// GPU device setup record ([`crate::gpu`]).
    GpuDeviceSetup,
    /// GPU device setdown record.
    GpuDeviceSetdown,
    /// Opaque pointer supplied by an AEGP.
    AegpMessage,
    /// Output slot for the flattened copy.
    FlattenedSequenceData,
}

/// BAE plug-in lifecycle hook (workstream 19) a selector maps onto. The 19
/// effect interface must expose an equivalent for each of these, or the
/// compat host cannot be a pure translator (spec request R19-01).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LifecycleHook {
    /// Static/about information.
    About,
    /// Plug-in-wide initialisation returning capabilities.
    GlobalInit,
    /// Plug-in-wide shutdown.
    GlobalShutdown,
    /// Parameter definition.
    DefineParams,
    /// Effect instance creation (fresh state).
    InstanceCreate,
    /// Instance state restore from serialized bytes (load, paste, duplicate,
    /// remote CRDT update).
    InstanceRestore,
    /// Instance state serialization (save, copy, sync).
    InstanceSerialize,
    /// Serialized snapshot without disturbing live state (render-thread copy).
    InstanceSnapshot,
    /// Instance destruction.
    InstanceDestroy,
    /// Legacy frame preparation (output size/origin).
    LegacyFramePrepare,
    /// Legacy full-frame render.
    LegacyRender,
    /// Legacy frame finish.
    LegacyFrameFinish,
    /// ROI / dependency negotiation before rendering.
    PlanRender,
    /// CPU render of a negotiated request.
    RenderCpu,
    /// GPU render of a negotiated request.
    RenderGpu,
    /// Audio preparation.
    AudioPrepare,
    /// Audio render.
    AudioRender,
    /// Audio finish.
    AudioFinish,
    /// Custom UI event.
    UiEvent,
    /// Supervised parameter changed (may rewrite other params).
    ParamChanged,
    /// Cosmetic parameter UI refresh (may not change values).
    RefreshParamUi,
    /// Modal options dialog.
    OptionsDialog,
    /// Custom (arbitrary data) value operation: new/copy/flatten/interp/...
    CustomValueOp,
    /// Report external file/font dependencies (packaging, 21).
    ExternalDependencies,
    /// Private message from another plug-in.
    PrivateMessage,
    /// Re-query dynamic capabilities (cache/ROI behaviour).
    QueryDynamicCaps,
    /// GPU device attach.
    GpuAttach,
    /// GPU device detach.
    GpuDetach,
}

/// Static facts about a selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectorInfo {
    /// Family.
    pub group: SelectorGroup,
    /// Thread the host issues it on.
    pub affinity: ThreadAffinity,
    /// Which params must be valid.
    pub params: ParamsValidity,
    /// Whether `output` (a world) is valid.
    pub output: bool,
    /// Type of `extra`.
    pub extra: ExtraKind,
    /// Corresponding BAE lifecycle hook.
    pub hook: LifecycleHook,
    /// True when the call needs a live effect instance (sequence data).
    pub needs_instance: bool,
}

impl Selector {
    /// All selectors, for exhaustive tests and tables.
    pub const ALL: [Selector; 28] = [
        Selector::About,
        Selector::GlobalSetup,
        Selector::GlobalSetdown,
        Selector::ParamsSetup,
        Selector::SequenceSetup,
        Selector::SequenceResetup,
        Selector::SequenceFlatten,
        Selector::SequenceSetdown,
        Selector::GetFlattenedSequenceData,
        Selector::FrameSetup,
        Selector::Render,
        Selector::FrameSetdown,
        Selector::SmartPreRender,
        Selector::SmartRender,
        Selector::SmartRenderGpu,
        Selector::AudioSetup,
        Selector::AudioRender,
        Selector::AudioSetdown,
        Selector::Event,
        Selector::UserChangedParam,
        Selector::UpdateParamsUi,
        Selector::DoDialog,
        Selector::ArbitraryCallback,
        Selector::GetExternalDependencies,
        Selector::CompletelyGeneral,
        Selector::QueryDynamicFlags,
        Selector::GpuDeviceSetup,
        Selector::GpuDeviceSetdown,
    ];

    /// Static facts (see the per-selector table in the spec for citations).
    pub fn info(self) -> SelectorInfo {
        use ExtraKind as X;
        use LifecycleHook as H;
        use ParamsValidity as P;
        use SelectorGroup as G;
        use ThreadAffinity as T;
        let (group, affinity, params, output, extra, hook, needs_instance) = match self {
            Selector::About => (G::Global, T::Main, P::None, false, X::None, H::About, false),
            Selector::GlobalSetup => (
                G::Global,
                T::MainExclusive,
                P::None,
                false,
                X::None,
                H::GlobalInit,
                false,
            ),
            Selector::GlobalSetdown => (
                G::Global,
                T::MainExclusive,
                P::None,
                false,
                X::None,
                H::GlobalShutdown,
                false,
            ),
            Selector::ParamsSetup => (
                G::Global,
                T::Main,
                P::None,
                false,
                X::None,
                H::DefineParams,
                false,
            ),
            Selector::SequenceSetup => (
                G::Sequence,
                T::Render,
                P::NonLayer,
                false,
                X::None,
                H::InstanceCreate,
                false,
            ),
            Selector::SequenceResetup => (
                G::Sequence,
                T::Render,
                P::NonLayer,
                false,
                X::None,
                H::InstanceRestore,
                false,
            ),
            Selector::SequenceFlatten => (
                G::Sequence,
                T::Main,
                P::NonLayer,
                false,
                X::None,
                H::InstanceSerialize,
                true,
            ),
            Selector::SequenceSetdown => (
                G::Sequence,
                T::Render,
                P::None,
                false,
                X::None,
                H::InstanceDestroy,
                true,
            ),
            Selector::GetFlattenedSequenceData => (
                G::Sequence,
                T::Main,
                P::None,
                false,
                X::FlattenedSequenceData,
                H::InstanceSnapshot,
                true,
            ),
            Selector::FrameSetup => (
                G::Frame,
                T::Render,
                P::NonLayer,
                false,
                X::None,
                H::LegacyFramePrepare,
                true,
            ),
            Selector::Render => (
                G::Frame,
                T::Render,
                P::All,
                true,
                X::None,
                H::LegacyRender,
                true,
            ),
            Selector::FrameSetdown => (
                G::Frame,
                T::Render,
                P::NonLayer,
                false,
                X::None,
                H::LegacyFrameFinish,
                true,
            ),
            Selector::SmartPreRender => (
                G::Frame,
                T::Render,
                P::None,
                false,
                X::PreRender,
                H::PlanRender,
                true,
            ),
            Selector::SmartRender => (
                G::Frame,
                T::Render,
                P::None,
                false,
                X::SmartRender,
                H::RenderCpu,
                true,
            ),
            Selector::SmartRenderGpu => (
                G::Frame,
                T::Render,
                P::None,
                false,
                X::SmartRender,
                H::RenderGpu,
                true,
            ),
            Selector::AudioSetup => (
                G::Audio,
                T::Render,
                P::NonLayer,
                false,
                X::None,
                H::AudioPrepare,
                true,
            ),
            Selector::AudioRender => (
                G::Audio,
                T::Render,
                P::NonLayer,
                false,
                X::None,
                H::AudioRender,
                true,
            ),
            Selector::AudioSetdown => (
                G::Audio,
                T::Render,
                P::None,
                false,
                X::None,
                H::AudioFinish,
                true,
            ),
            Selector::Event => (
                G::Messaging,
                T::Main,
                P::NonLayer,
                false,
                X::Event,
                H::UiEvent,
                true,
            ),
            Selector::UserChangedParam => (
                G::Messaging,
                T::Main,
                P::NonLayer,
                false,
                X::None,
                H::ParamChanged,
                true,
            ),
            Selector::UpdateParamsUi => (
                G::Messaging,
                T::Main,
                P::NonLayer,
                false,
                X::None,
                H::RefreshParamUi,
                true,
            ),
            Selector::DoDialog => (
                G::Messaging,
                T::Main,
                P::NonLayer,
                false,
                X::None,
                H::OptionsDialog,
                true,
            ),
            Selector::ArbitraryCallback => (
                G::Messaging,
                T::Render,
                P::None,
                false,
                X::ArbitraryCallback,
                H::CustomValueOp,
                false,
            ),
            Selector::GetExternalDependencies => (
                G::Messaging,
                T::Main,
                P::NonLayer,
                false,
                X::ExternalDependencies,
                H::ExternalDependencies,
                true,
            ),
            Selector::CompletelyGeneral => (
                G::Messaging,
                T::Main,
                P::None,
                false,
                X::AegpMessage,
                H::PrivateMessage,
                true,
            ),
            Selector::QueryDynamicFlags => (
                G::Messaging,
                T::Main,
                P::NonLayer,
                false,
                X::None,
                H::QueryDynamicCaps,
                true,
            ),
            Selector::GpuDeviceSetup => (
                G::Gpu,
                T::Main,
                P::None,
                false,
                X::GpuDeviceSetup,
                H::GpuAttach,
                false,
            ),
            Selector::GpuDeviceSetdown => (
                G::Gpu,
                T::Main,
                P::None,
                false,
                X::GpuDeviceSetdown,
                H::GpuDetach,
                false,
            ),
        };
        SelectorInfo {
            group,
            affinity,
            params,
            output,
            extra,
            hook,
            needs_instance,
        }
    }
}

/// Lifecycle state of one plug-in module as tracked by the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleState {
    /// Loaded but `GlobalSetup` not yet sent.
    Loaded,
    /// `GlobalSetup` succeeded.
    GlobalReady,
    /// `ParamsSetup` succeeded; instances may be created.
    ParamsDefined,
    /// `GlobalSetdown` sent; only unload is allowed.
    Shutdown,
}

/// State of one effect instance's sequence data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceState {
    /// Live (unflattened) data owned by the plug-in.
    Live,
    /// Flat data (after flatten); the host must resetup before any call that
    /// needs live data. AE may dispose flat data without a setdown.
    Flat,
}

/// Host-side identifier of an effect instance (one effect Prim in BAE).
pub type InstanceId = u64;

/// Tracks module and instance state and rejects selector sequences the
/// protocol (or BAE host policy, where stricter) forbids. The real host runs
/// it in debug builds; it is also the executable form of the call-order
/// table in the spec.
#[derive(Debug)]
pub struct ProtocolTracker {
    module: ModuleState,
    instances: HashMap<InstanceId, InstanceState>,
    gpu_devices: HashMap<u32, ()>,
}

impl Default for ProtocolTracker {
    fn default() -> Self {
        ProtocolTracker {
            module: ModuleState::Loaded,
            instances: HashMap::new(),
            gpu_devices: HashMap::new(),
        }
    }
}

impl ProtocolTracker {
    /// New tracker for a freshly loaded module.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current module state.
    pub fn module_state(&self) -> ModuleState {
        self.module
    }

    /// State of an instance, if it exists.
    pub fn instance_state(&self, id: InstanceId) -> Option<InstanceState> {
        self.instances.get(&id).copied()
    }

    /// Validate (and record) that the host is about to send `sel`, for
    /// instance `inst` where relevant and GPU device `device` for GPU device
    /// selectors. Call only after the plug-in returned success for the
    /// previous selector; a failed `GlobalSetup` leaves the module unusable.
    pub fn send(
        &mut self,
        sel: Selector,
        inst: Option<InstanceId>,
        device: Option<u32>,
    ) -> Result<()> {
        let v = |rule: &'static str| Err(HostError::ProtocolViolation { rule });
        match (self.module, sel) {
            (ModuleState::Shutdown, _) => return v("no selector after GLOBAL_SETDOWN"),
            (ModuleState::Loaded, Selector::GlobalSetup) => {
                self.module = ModuleState::GlobalReady;
                return Ok(());
            }
            (ModuleState::Loaded, _) => return v("GLOBAL_SETUP must be the first selector"),
            (_, Selector::GlobalSetup) => return v("GLOBAL_SETUP sent twice"),
            (ModuleState::GlobalReady, Selector::ParamsSetup) => {
                self.module = ModuleState::ParamsDefined;
                return Ok(());
            }
            (ModuleState::ParamsDefined, Selector::ParamsSetup) => {
                return v("PARAMS_SETUP is sent once per module load");
            }
            (_, Selector::GlobalSetdown) => {
                if !self.instances.is_empty() {
                    return v("BAE policy: set down every instance before GLOBAL_SETDOWN");
                }
                if !self.gpu_devices.is_empty() {
                    return v("BAE policy: set down every GPU device before GLOBAL_SETDOWN");
                }
                self.module = ModuleState::Shutdown;
                return Ok(());
            }
            (_, Selector::About) => return Ok(()),
            (_, Selector::GpuDeviceSetup) => {
                let d = device.ok_or(HostError::ProtocolViolation {
                    rule: "GPU selector without device index",
                })?;
                if self.gpu_devices.insert(d, ()).is_some() {
                    return v("GPU_DEVICE_SETUP at most once per device");
                }
                return Ok(());
            }
            (_, Selector::GpuDeviceSetdown) => {
                let d = device.ok_or(HostError::ProtocolViolation {
                    rule: "GPU selector without device index",
                })?;
                if self.gpu_devices.remove(&d).is_none() {
                    return v("GPU_DEVICE_SETDOWN without matching setup");
                }
                return Ok(());
            }
            (ModuleState::GlobalReady, s)
                if s.info().group != SelectorGroup::Global && s != Selector::ArbitraryCallback =>
            {
                return v("PARAMS_SETUP must precede instance selectors");
            }
            _ => {}
        }

        let info = sel.info();
        match sel {
            Selector::SequenceSetup => {
                let id = inst.ok_or(HostError::ProtocolViolation {
                    rule: "SEQUENCE_SETUP needs an instance id",
                })?;
                if self.instances.contains_key(&id) {
                    return v("SEQUENCE_SETUP on an existing instance");
                }
                self.instances.insert(id, InstanceState::Live);
                Ok(())
            }
            Selector::SequenceResetup => {
                // Valid after load (no live instance yet), after duplication
                // (both old and new instance) and after flatten.
                let id = inst.ok_or(HostError::ProtocolViolation {
                    rule: "SEQUENCE_RESETUP needs an instance id",
                })?;
                self.instances.insert(id, InstanceState::Live);
                Ok(())
            }
            Selector::SequenceFlatten => {
                let id = inst.ok_or(HostError::ProtocolViolation {
                    rule: "SEQUENCE_FLATTEN needs an instance id",
                })?;
                match self.instances.get_mut(&id) {
                    Some(s @ InstanceState::Live) => {
                        *s = InstanceState::Flat;
                        Ok(())
                    }
                    Some(InstanceState::Flat) => v("SEQUENCE_FLATTEN on already-flat data"),
                    None => v("SEQUENCE_FLATTEN on unknown instance"),
                }
            }
            Selector::SequenceSetdown => {
                let id = inst.ok_or(HostError::ProtocolViolation {
                    rule: "SEQUENCE_SETDOWN needs an instance id",
                })?;
                if self.instances.remove(&id).is_none() {
                    return v("SEQUENCE_SETDOWN on unknown instance");
                }
                Ok(())
            }
            _ if info.needs_instance => {
                let id = inst.ok_or(HostError::ProtocolViolation {
                    rule: "selector needs an instance id",
                })?;
                match self.instances.get(&id) {
                    Some(InstanceState::Live) => Ok(()),
                    Some(InstanceState::Flat) => {
                        v("BAE policy: RESETUP a flattened instance before using it")
                    }
                    None => v("selector for unknown instance"),
                }
            }
            _ => Ok(()),
        }
    }

    /// The host disposed flat data of an instance without SETDOWN (allowed by
    /// the protocol after a recent flatten).
    pub fn dispose_flat(&mut self, id: InstanceId) -> Result<()> {
        match self.instances.get(&id) {
            Some(InstanceState::Flat) => {
                self.instances.remove(&id);
                Ok(())
            }
            _ => Err(HostError::ProtocolViolation {
                rule: "only flat sequence data may be disposed without SETDOWN",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_selector_has_info() {
        for s in Selector::ALL {
            let i = s.info();
            if i.group == SelectorGroup::Frame {
                assert!(i.needs_instance, "{s:?}");
            }
        }
        assert_eq!(Selector::Render.info().params, ParamsValidity::All);
        assert!(Selector::Render.info().output);
        assert_eq!(
            Selector::GlobalSetup.info().affinity,
            ThreadAffinity::MainExclusive
        );
    }

    #[test]
    fn documented_happy_path() {
        let mut t = ProtocolTracker::new();
        t.send(Selector::GlobalSetup, None, None).unwrap();
        t.send(Selector::ParamsSetup, None, None).unwrap();
        t.send(Selector::GpuDeviceSetup, None, Some(0)).unwrap();
        t.send(Selector::SequenceSetup, Some(1), None).unwrap();
        for _ in 0..3 {
            t.send(Selector::SmartPreRender, Some(1), None).unwrap();
        }
        t.send(Selector::SmartRender, Some(1), None).unwrap();
        t.send(Selector::UserChangedParam, Some(1), None).unwrap();
        // Save: flatten, then the BAE host resetups before further use.
        t.send(Selector::SequenceFlatten, Some(1), None).unwrap();
        assert!(t.send(Selector::SmartRender, Some(1), None).is_err());
        t.send(Selector::SequenceResetup, Some(1), None).unwrap();
        t.send(Selector::SmartRender, Some(1), None).unwrap();
        // Duplicate: flatten old, resetup old and new.
        t.send(Selector::SequenceFlatten, Some(1), None).unwrap();
        t.send(Selector::SequenceResetup, Some(1), None).unwrap();
        t.send(Selector::SequenceResetup, Some(2), None).unwrap();
        t.send(Selector::SequenceSetdown, Some(1), None).unwrap();
        t.send(Selector::SequenceFlatten, Some(2), None).unwrap();
        t.dispose_flat(2).unwrap();
        assert!(t.send(Selector::GlobalSetdown, None, None).is_err());
        t.send(Selector::GpuDeviceSetdown, None, Some(0)).unwrap();
        t.send(Selector::GlobalSetdown, None, None).unwrap();
        assert_eq!(t.module_state(), ModuleState::Shutdown);
        assert!(t.send(Selector::About, None, None).is_err());
    }

    #[test]
    fn ordering_violations() {
        let mut t = ProtocolTracker::new();
        assert!(t.send(Selector::ParamsSetup, None, None).is_err());
        t.send(Selector::GlobalSetup, None, None).unwrap();
        assert!(t.send(Selector::GlobalSetup, None, None).is_err());
        assert!(t.send(Selector::SequenceSetup, Some(1), None).is_err());
        t.send(Selector::ParamsSetup, None, None).unwrap();
        assert!(t.send(Selector::Render, Some(9), None).is_err());
        assert!(t.send(Selector::SequenceSetup, None, None).is_err());
        t.send(Selector::SequenceSetup, Some(9), None).unwrap();
        assert!(t.send(Selector::SequenceSetup, Some(9), None).is_err());
        assert!(t.dispose_flat(9).is_err());
        assert!(t.send(Selector::GpuDeviceSetdown, None, Some(3)).is_err());
        t.send(Selector::GpuDeviceSetup, None, Some(3)).unwrap();
        assert!(t.send(Selector::GpuDeviceSetup, None, Some(3)).is_err());
    }
}
