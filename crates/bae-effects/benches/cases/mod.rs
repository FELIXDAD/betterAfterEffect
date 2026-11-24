use super::Case;
mod blur_sharpen;
mod channel;
mod color;
mod matte;
mod noise_grain;
mod perspective;
mod stylize;

const HD: [u32; 2] = [1920, 1080];

pub(super) fn all() -> Vec<Case> {
    let mut cases = Vec::new();
    for (size, tag) in [([3840, 2160], "4K"), (HD, "1080p")] {
        blur_sharpen::at_size(&mut cases, size, tag);
        color::at_size(&mut cases, size, tag);
    }
    noise_grain::register(&mut cases);
    blur_sharpen::spatial(&mut cases);
    stylize::glow(&mut cases);
    perspective::register(&mut cases);
    stylize::mosaic(&mut cases);
    channel::register(&mut cases);
    matte::register(&mut cases);
    color::shadow_highlight(&mut cases);
    cases
}
