//! SmartFX: region-of-interest negotiation and input checkout.
//!
//! SmartFX splits rendering into `PF_Cmd_SMART_PRE_RENDER` (the host states
//! the output it wants; the effect checks out the inputs it may need — any
//! layer param, any rect, any time — and reports the rect it will produce and
//! the largest rect it could ever produce) and `PF_Cmd_SMART_RENDER` /
//! `PF_Cmd_SMART_RENDER_GPU` (the effect fetches the pixels of the inputs it
//! checked out, each at most once, then fetches its output buffer) (SDK guide,
//! "SmartFX").
//!
//! For BAE's render graph (04) this means: a node's *dependencies are
//! discovered at plan time by calling the node*, not declared statically.
//! [`PreRenderSession`] records them; [`PreRenderSession::finish`] validates
//! the plug-in's answer and yields a [`RenderPlan`] that 04 schedules
//! (request R04-01); [`SmartRenderSession`] enforces the render-time
//! checkout rules.

use std::collections::{HashMap, HashSet};

use crate::error::{HostError, Result};
use crate::geometry::{LRect, RationalScale};
use crate::params::ParamIndex;
use crate::time::Flicks;

/// Field being rendered (interlaced output).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Field {
    /// Whole frame.
    #[default]
    Frame,
    /// Upper field only.
    Upper,
    /// Lower field only.
    Lower,
}

/// Channels the effect must produce (`PF_ChannelMask_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChannelMask {
    /// Alpha.
    pub a: bool,
    /// Red.
    pub r: bool,
    /// Green.
    pub g: bool,
    /// Blue.
    pub b: bool,
}

impl ChannelMask {
    /// All four channels.
    pub const ARGB: ChannelMask = ChannelMask {
        a: true,
        r: true,
        g: true,
        b: true,
    };
}

/// What the host asks a node (or a node asks an input) to render.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderRequest {
    /// Requested rect in the node's layer coordinates. Empty = "only tell me
    /// your max result rect".
    pub rect: LRect,
    /// Field.
    pub field: Field,
    /// Channels needed.
    pub channels: ChannelMask,
    /// Keep colour of zero-alpha pixels (must be propagated to checkouts).
    pub preserve_rgb_of_zero_alpha: bool,
}

/// One `checkout_layer` call made during pre-render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckoutRequest {
    /// Plug-in chosen id: positive and unique within this pre-render.
    pub checkout_id: i32,
    /// Which layer param (0 = the effect's own input).
    pub param: ParamIndex,
    /// What is requested from that input.
    pub request: RenderRequest,
    /// Requested time (already converted from the plug-in's (time, step,
    /// scale) triple to flicks).
    pub time: Flicks,
    /// Requested frame duration (for motion-blur-aware inputs).
    pub time_step: Flicks,
}

/// Host answer to a checkout (`PF_CheckoutResult`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckoutResult {
    /// Rect the input will actually provide (may be empty).
    pub result_rect: LRect,
    /// Largest rect the input could ever provide (its content bounds).
    pub max_result_rect: LRect,
    /// Pixel aspect ratio of the input.
    pub pixel_aspect: RationalScale,
    /// The whole result rect is opaque.
    pub solid: bool,
    /// Original layer size before effects and downsampling.
    pub ref_size: (u32, u32),
}

/// What the plug-in reports at the end of pre-render (`PF_PreRenderOutput`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreRenderOutput {
    /// Rect it will render; ⊆ request unless `returns_extra_pixels`.
    pub result_rect: LRect,
    /// Content bounds; must not depend on the request.
    pub max_result_rect: LRect,
    /// Output fully opaque.
    pub solid: bool,
    /// May render more than requested (`RETURNS_EXTRA_PIXELS`).
    pub returns_extra_pixels: bool,
    /// GPU render possible for this request (`GPU_RENDER_POSSIBLE`).
    pub gpu_render_possible: bool,
    /// Plug-in attached private pre-render data (host owns disposal: it must
    /// call the plug-in's delete function even if render never happens).
    pub has_pre_render_data: bool,
}

/// Adapter the render graph implements so the compat host can answer
/// checkouts: bounds of an upstream node for a request at a time, *without*
/// rendering pixels.
pub trait InputBounds {
    /// Resolve the bounds of `param`'s source at `time` for `request`.
    fn bounds(
        &mut self,
        param: ParamIndex,
        time: Flicks,
        request: &RenderRequest,
    ) -> Result<CheckoutResult>;
}

/// Streaming 64-bit FNV-1a used to fold plug-in GUID mix-ins into a cache
/// key. (Placeholder: the production host feeds the same bytes into 04's
/// cache hasher.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheKeyMixer(u64);

impl Default for CacheKeyMixer {
    fn default() -> Self {
        CacheKeyMixer(0xcbf2_9ce4_8422_2325)
    }
}

impl CacheKeyMixer {
    /// Fold bytes in.
    pub fn mix(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Current digest.
    pub fn digest(&self) -> u64 {
        self.0
    }
}

/// A dependency the render graph must satisfy before `SmartRender`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannedInput {
    /// Checkout id the plug-in will use at render time.
    pub checkout_id: i32,
    /// Layer param.
    pub param: ParamIndex,
    /// Time.
    pub time: Flicks,
    /// Pixels to produce: request ∩ input result rect.
    pub rect: LRect,
    /// Full answer given to the plug-in.
    pub result: CheckoutResult,
}

/// Validated outcome of pre-render: what 04 must render, and what to allocate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderPlan {
    /// Request this plan answers.
    pub request: RenderRequest,
    /// Output buffer rect (layer coordinates).
    pub output_rect: LRect,
    /// Content bounds (cacheable per node/time/params).
    pub max_result_rect: LRect,
    /// Output opaque.
    pub solid: bool,
    /// GPU path allowed.
    pub gpu: bool,
    /// Inputs to render first.
    pub inputs: Vec<PlannedInput>,
    /// Plug-in GUID mix-in digest (None if the plug-in mixed nothing).
    pub guid: Option<u64>,
}

/// Host-side record of one pre-render call.
#[derive(Debug)]
pub struct PreRenderSession {
    request: RenderRequest,
    checkouts: Vec<(CheckoutRequest, CheckoutResult)>,
    ids: HashSet<i32>,
    guid: Option<CacheKeyMixer>,
}

impl PreRenderSession {
    /// Begin pre-render for `request`.
    pub fn new(request: RenderRequest) -> Self {
        PreRenderSession {
            request,
            checkouts: Vec::new(),
            ids: HashSet::new(),
            guid: None,
        }
    }

    /// Handle the plug-in's `checkout_layer` callback.
    pub fn checkout_layer(
        &mut self,
        req: CheckoutRequest,
        inputs: &mut dyn InputBounds,
    ) -> Result<CheckoutResult> {
        if req.checkout_id <= 0 {
            return Err(HostError::Checkout {
                reason: "checkout id must be positive",
            });
        }
        if !self.ids.insert(req.checkout_id) {
            return Err(HostError::Checkout {
                reason: "checkout id reused within one pre-render",
            });
        }
        let mut r = req;
        // The SDK requires preserve_rgb_of_zero_alpha to propagate recursively.
        r.request.preserve_rgb_of_zero_alpha |= self.request.preserve_rgb_of_zero_alpha;
        let result = inputs.bounds(r.param, r.time, &r.request)?;
        self.checkouts.push((r, result));
        Ok(result)
    }

    /// Handle a GUID mix-in callback (only for effects that declared
    /// `I_MIX_GUID_DEPENDENCIES`).
    pub fn mix_guid(&mut self, bytes: &[u8]) {
        self.guid
            .get_or_insert_with(CacheKeyMixer::default)
            .mix(bytes);
    }

    /// Validate the plug-in's output and produce the plan.
    pub fn finish(self, out: PreRenderOutput, gpu_requested: bool) -> Result<RenderPlan> {
        if !out.returns_extra_pixels && !self.request.rect.contains(&out.result_rect) {
            return Err(HostError::Checkout {
                reason: "result rect exceeds request without RETURNS_EXTRA_PIXELS",
            });
        }
        if !out.max_result_rect.contains(&out.result_rect) {
            return Err(HostError::Checkout {
                reason: "result rect outside max result rect",
            });
        }
        let inputs = self
            .checkouts
            .iter()
            .map(|(req, res)| PlannedInput {
                checkout_id: req.checkout_id,
                param: req.param,
                time: req.time,
                rect: req.request.rect.intersect(&res.result_rect),
                result: *res,
            })
            .collect();
        Ok(RenderPlan {
            request: self.request,
            output_rect: out.result_rect,
            max_result_rect: out.max_result_rect,
            solid: out.solid,
            gpu: gpu_requested && out.gpu_render_possible,
            inputs,
            guid: self.guid.map(|g| g.digest()),
        })
    }
}

/// Enforces render-time checkout rules for one `SmartRender` call.
#[derive(Debug)]
pub struct SmartRenderSession<'p> {
    plan: &'p RenderPlan,
    by_id: HashMap<i32, usize>,
    taken: HashSet<i32>,
    output_taken: bool,
}

impl<'p> SmartRenderSession<'p> {
    /// Start rendering `plan`.
    pub fn new(plan: &'p RenderPlan) -> Self {
        let by_id = plan
            .inputs
            .iter()
            .enumerate()
            .map(|(i, p)| (p.checkout_id, i))
            .collect();
        SmartRenderSession {
            plan,
            by_id,
            taken: HashSet::new(),
            output_taken: false,
        }
    }

    /// `checkout_layer_pixels`: each id from pre-render may be fetched once.
    pub fn checkout_layer_pixels(&mut self, id: i32) -> Result<&'p PlannedInput> {
        let idx = *self.by_id.get(&id).ok_or(HostError::Checkout {
            reason: "pixels requested for an id not checked out in pre-render",
        })?;
        if !self.taken.insert(id) {
            return Err(HostError::Checkout {
                reason: "checkout_layer_pixels called twice for one id",
            });
        }
        Ok(&self.plan.inputs[idx])
    }

    /// `checkout_output`: allowed after at least one input was checked out
    /// (or when the plan has no inputs).
    pub fn checkout_output(&mut self) -> Result<LRect> {
        if !self.plan.inputs.is_empty() && self.taken.is_empty() {
            return Err(HostError::Checkout {
                reason: "output checked out before any input",
            });
        }
        if self.output_taken {
            return Err(HostError::Checkout {
                reason: "output checked out twice",
            });
        }
        self.output_taken = true;
        Ok(self.plan.output_rect)
    }

    /// Inputs planned but never fetched — the graph may skip rendering them
    /// next time (the SDK's main SmartFX optimisation).
    pub fn unused_inputs(&self) -> impl Iterator<Item = &PlannedInput> {
        self.plan
            .inputs
            .iter()
            .filter(|p| !self.taken.contains(&p.checkout_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Layer(LRect);
    impl InputBounds for Layer {
        fn bounds(
            &mut self,
            _p: ParamIndex,
            _t: Flicks,
            req: &RenderRequest,
        ) -> Result<CheckoutResult> {
            Ok(CheckoutResult {
                result_rect: req.rect.intersect(&self.0),
                max_result_rect: self.0,
                pixel_aspect: RationalScale::ONE,
                solid: false,
                ref_size: (self.0.width(), self.0.height()),
            })
        }
    }

    fn req(rect: LRect) -> RenderRequest {
        RenderRequest {
            rect,
            field: Field::Frame,
            channels: ChannelMask::ARGB,
            preserve_rgb_of_zero_alpha: false,
        }
    }

    #[test]
    fn blur_style_negotiation() {
        let layer = LRect::from_xywh(0, 0, 1920, 1080);
        let mut inputs = Layer(layer);
        let want = LRect::from_xywh(100, 100, 200, 200);
        let mut s = PreRenderSession::new(req(want));
        let r = s
            .checkout_layer(
                CheckoutRequest {
                    checkout_id: 1,
                    param: 0,
                    request: req(want.inflate(10)),
                    time: Flicks(0),
                    time_step: Flicks(0),
                },
                &mut inputs,
            )
            .unwrap();
        assert_eq!(r.result_rect, want.inflate(10));
        s.mix_guid(b"seed=3");
        let plan = s
            .finish(
                PreRenderOutput {
                    result_rect: want,
                    max_result_rect: layer.inflate(10),
                    solid: false,
                    returns_extra_pixels: false,
                    gpu_render_possible: true,
                    has_pre_render_data: false,
                },
                false,
            )
            .unwrap();
        assert!(!plan.gpu);
        assert!(plan.guid.is_some());
        let mut rs = SmartRenderSession::new(&plan);
        assert!(rs.checkout_output().is_err());
        assert_eq!(rs.checkout_layer_pixels(1).unwrap().rect, want.inflate(10));
        assert!(rs.checkout_layer_pixels(1).is_err());
        assert!(rs.checkout_layer_pixels(2).is_err());
        assert_eq!(rs.checkout_output().unwrap(), want);
        assert_eq!(rs.unused_inputs().count(), 0);
    }

    #[test]
    fn rule_violations() {
        let mut inputs = Layer(LRect::from_xywh(0, 0, 10, 10));
        let mut s = PreRenderSession::new(req(LRect::from_xywh(0, 0, 5, 5)));
        let c = CheckoutRequest {
            checkout_id: 0,
            param: 0,
            request: req(LRect::EMPTY),
            time: Flicks(0),
            time_step: Flicks(0),
        };
        assert!(s.checkout_layer(c, &mut inputs).is_err());
        let c = CheckoutRequest {
            checkout_id: 7,
            ..c
        };
        // Empty request = bounds query only.
        let r = s.checkout_layer(c, &mut inputs).unwrap();
        assert!(r.result_rect.is_empty() && !r.max_result_rect.is_empty());
        assert!(s.checkout_layer(c, &mut inputs).is_err());
        let out = PreRenderOutput {
            result_rect: LRect::from_xywh(0, 0, 8, 8),
            max_result_rect: LRect::from_xywh(0, 0, 10, 10),
            solid: true,
            returns_extra_pixels: false,
            gpu_render_possible: false,
            has_pre_render_data: false,
        };
        assert!(s.finish(out, false).is_err());
    }

    #[test]
    fn preserve_rgb_propagates() {
        struct Spy(bool);
        impl InputBounds for Spy {
            fn bounds(
                &mut self,
                _p: ParamIndex,
                _t: Flicks,
                r: &RenderRequest,
            ) -> Result<CheckoutResult> {
                self.0 = r.preserve_rgb_of_zero_alpha;
                Ok(CheckoutResult {
                    result_rect: r.rect,
                    max_result_rect: r.rect,
                    pixel_aspect: RationalScale::ONE,
                    solid: false,
                    ref_size: (0, 0),
                })
            }
        }
        let mut top = req(LRect::from_xywh(0, 0, 4, 4));
        top.preserve_rgb_of_zero_alpha = true;
        let mut s = PreRenderSession::new(top);
        let mut spy = Spy(false);
        s.checkout_layer(
            CheckoutRequest {
                checkout_id: 1,
                param: 2,
                request: req(LRect::from_xywh(0, 0, 4, 4)),
                time: Flicks(5),
                time_step: Flicks(1),
            },
            &mut spy,
        )
        .unwrap();
        assert!(spy.0);
    }
}
