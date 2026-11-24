use super::{SIZE, Suite};

pub(super) const SUITE: Suite = Suite {
    default_cases: &[include_str!("../cases-stylize.json")],
    golden_only: &[include_str!("../cases-stylize-2.json")],
    probes: &[
        ("edge", edge),
        ("white", white),
        ("rgb-grid", rgb_grid),
        ("bars", bars),
        ("rectangle", rectangle),
    ],
    ..Suite::EMPTY
};

fn edge(x: u32, y: u32) -> [f64; 4] {
    let a = [255., 192., 128., 64.][(y / 16 % 4) as usize] / 255.;
    [
        f64::from(x / 16 % 2),
        f64::from(y / 16 % 2),
        f64::from(u8::from((x + 2 * y) % 32 < 16)),
        a,
    ]
}

fn white(_x: u32, _y: u32) -> [f64; 4] {
    [1.; 4]
}

fn rgb_grid(x: u32, y: u32) -> [f64; 4] {
    let [w, _] = SIZE;
    let i = (x / 4 + (y / 4) * (w / 4)) % 1000;
    let [r, g, b] = [i % 10, (i / 10) % 10, i / 100].map(|k| f64::from(k) / 9.);
    [r, g, b, 1.]
}

fn bars(x: u32, y: u32) -> [f64; 4] {
    let rows = [
        [0.7, 0.3, 0.1],
        [0.1, 0.5, 0.9],
        [0.8, 0.8, 0.15],
        [0.15, 0.1, 0.9],
    ];
    let [r, g, b] = rows[((x / 32 + y / 16) % 4) as usize];
    [r, g, b, 1.]
}

fn rectangle(x: u32, y: u32) -> [f64; 4] {
    let a = f64::from((80..176).contains(&x) && (30..98).contains(&y));
    [0.8, 0.4, 0.2, a]
}
