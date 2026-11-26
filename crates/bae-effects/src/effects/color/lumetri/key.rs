use bae_model::Value;

const WIDTH_IDS: [u32; 3] = [1006, 1007, 1008];
const UNITS: [f64; 3] = [360., 100., 100.];
pub(super) const DEFAULT_WIDTHS: [f64; 3] = [18., 5., 5.];

fn hsl([r, g, b, _]: [f64; 4]) -> [f64; 3] {
    let hi = r.max(g).max(b);
    let lo = r.min(g).min(b);
    let chroma = hi - lo;
    let light = (hi + lo) * 0.5;
    if chroma == 0. {
        return [0., 0., light];
    }
    let sector = if hi == r {
        (g - b) / chroma
    } else if hi == g {
        2. + (b - r) / chroma
    } else {
        4. + (r - g) / chroma
    };
    [
        (sector / 6.).rem_euclid(1.),
        chroma / (1. - (2. * light - 1.).abs()),
        light,
    ]
}

fn rgb([h, s, l]: [f64; 3]) -> [f64; 4] {
    let chroma = (1. - (2. * l - 1.).abs()) * s;
    let [r, g, b] = [0., 2. / 3., 1. / 3.].map(|offset| {
        let hue = ((h + offset).rem_euclid(1.) * 6. - 3.).abs() - 1.;
        l + (hue.clamp(0., 1.) - 0.5) * chroma
    });
    [r, g, b, 1.]
}

/// Update the editable HSL selection when a Set, Add or Remove Color sample is committed.
/// The returned edits belong to one undo transaction. Widths use degrees and percent.
pub fn lumetri_key_edits(
    picker: u32,
    sample: [f64; 4],
    mut read: impl FnMut(u32) -> Option<Value>,
) -> Vec<(u32, Value)> {
    if !(70..=72).contains(&picker) || sample[..3].iter().any(|v| !v.is_finite()) {
        return Vec::new();
    }
    let center = match read(70) {
        Some(Value::Color(c)) => Some(c),
        _ => None,
    };
    let active = center.is_some() && read(1017).and_then(|v| v.as_bool()).unwrap_or(true);
    let mut edits = vec![(picker, Value::Color(sample))];
    let mut widths = std::array::from_fn(|i| {
        read(WIDTH_IDS[i])
            .and_then(|v| v.as_f64())
            .unwrap_or(DEFAULT_WIDTHS[i])
    });
    let mut selected = true;
    let clipped = |c: [f64; 4]| c.map(|v| v.clamp(0., 1.));
    let mut midpoint = hsl(clipped(center.unwrap_or(sample)));
    let mut point = hsl(clipped(sample));
    // Unwrap the hue next to the current midpoint, including selections that cross red.
    point[0] = midpoint[0] + (point[0] - midpoint[0] + 0.5).rem_euclid(1.) - 0.5;
    if picker == 70 || (picker == 71 && !active) {
        midpoint = point;
        widths = DEFAULT_WIDTHS;
    } else if !active {
        return edits;
    } else {
        let fade = read(1009).and_then(|v| v.as_f64()).unwrap_or(5.) / 100.;
        let mut intervals = std::array::from_fn::<_, 3, _>(|i| {
            let radius = widths[i] / UNITS[i];
            let bounds = [midpoint[i] - radius, midpoint[i] + radius];
            if i == 0 {
                bounds
            } else {
                bounds.map(|v| v.clamp(0., 1.))
            }
        });
        let previous = intervals;
        if picker == 71 {
            for (i, interval) in intervals.iter_mut().enumerate() {
                interval[0] = interval[0].min(point[i]);
                interval[1] = interval[1].max(point[i]);
            }
        } else {
            // Cut the nearest face that excludes the sample, keeping the largest fraction of the key.
            let margin = fade.max(0.00001) + 0.00001;
            let mut best = None;
            let mut kept = 0.;
            for (i, &[lo, hi]) in intervals.iter().enumerate() {
                if point[i] < lo - margin || point[i] > hi + margin {
                    return edits;
                }
                let candidates = [
                    [lo, hi.min(point[i] - margin)],
                    [lo.max(point[i] + margin), hi],
                ];
                for [mut start, mut end] in candidates {
                    if i == 0 {
                        // The same rejected hue is also one turn away.
                        start = start.max(point[i] - 1. + margin);
                        end = end.min(point[i] + 1. - margin);
                        if hi - lo >= 1. {
                            [start, end] = [point[i] + margin, point[i] + 1. - margin];
                        }
                    }
                    let fraction = (end - start) / (hi - lo).max(1e-12);
                    if fraction > kept {
                        best = Some((i, [start, end]));
                        kept = fraction;
                    }
                }
            }
            if let Some((axis, interval)) = best {
                intervals[axis] = interval;
            } else {
                selected = false;
            }
        }
        if previous == intervals && selected {
            return edits;
        }
        for (i, [lo, hi]) in intervals.into_iter().enumerate() {
            if [lo, hi] != previous[i] {
                midpoint[i] = (lo + hi) * 0.5;
                widths[i] = (hi - lo) * 0.5 * UNITS[i];
            }
        }
    }
    if picker != 70 {
        edits.push((70, Value::Color(rgb(midpoint))));
    }
    edits.extend(WIDTH_IDS.into_iter().zip(widths.map(Value::Float)));
    edits.push((1017, Value::Bool(selected)));
    edits
}
