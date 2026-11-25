struct TraceParams {
    input: FxImage,
    roi: vec4i,
    origin: vec2i,
    size: vec2u,
    cell: vec2f,
    radius: f32,
    hardness: f32,
    gradient: vec4f,
    color: vec4f,
    rotation: f32,
    fraction: f32,
    tolerance: f32,
    segments: u32,
    style: u32,
    flags: u32,
    selected: u32,
    seed: u32,
    blocks: u32,
    padding: u32,
    output_origin: vec2i,
}
@group(0) @binding(0) var<uniform> u: TraceParams;
@group(0) @binding(1) var field: texture_2d<f32>;
@group(0) @binding(2) var<storage,read_write> parents: array<atomic<u32>>;
@group(0) @binding(3) var<storage,read_write> links: array<u32>;
@group(0) @binding(4) var<storage,read_write> arcs: array<f32>;
@group(0) @binding(5) var<storage,read_write> lengths: array<f32>;
@group(0) @binding(6) var<storage,read_write> ranks: array<atomic<u32>>;
@group(0) @binding(7) var<storage,read_write> coverage: array<atomic<u32>>;
@group(0) @binding(8) var source: texture_2d<f32>;
@group(0) @binding(9) var dst: texture_storage_2d<WORKING_FORMAT,write>;


// An edge id identifies a horizontal (even) or vertical (odd) grid edge.
const RANKED: u32 = 0x80000000u;
const NONE: u32 = 0xffffffffu;
fn vertex(xy: vec2u, vertical: u32) -> u32 {
    return 2u * (xy.y * u.size.x + xy.x) + vertical;
}
fn origin(id: u32) -> vec2i {
    let i = id >> 1u;
    return vec2i(vec2u(i % u.size.x, i / u.size.x));
}
fn edge(cell: vec2i, side: u32) -> u32 {
    let xy = vec2u(cell);
    switch side {
        case 0u: {
            return vertex(xy, 0u);
        }
        case 1u: {
            return vertex(xy + vec2u(1, 0), 1u);
        }
        case 2u: {
            return vertex(xy + vec2u(0, 1), 0u);
        }
        default: {
        }
    }
    return vertex(xy, 1u);
}
fn value(p: vec2i) -> f32 {
    return textureLoad(field, p, 0).x;
}
fn end_point(p: vec2i, id: u32) -> vec2i {
    return p + select(vec2i(1, 0), vec2i(0, 1), (id & 1u) != 0u);
}
fn crosses(id: u32) -> bool {
    let p = origin(id);
    let q = end_point(p, id);
    if any(q >= vec2i(u.size)) {
        return false;
    }
    return (value(p) >= 0.) != (value(q) >= 0.);
}
fn position(id: u32) -> vec2f {
    let p = origin(id);
    let q = end_point(p, id);
    let a = value(p);
    let b = value(q);
    return (vec2f(p) + vec2f(q - p) * (a / (a - b)) + 0.5) * u.cell;
}
// Resolve the four-crossing saddle from the sign at the cell centre.
fn paired(cell: vec2i, side: u32) -> u32 {
    if any(cell < vec2i(0)) || any(cell + 1 >= vec2i(u.size)) {
        return NONE;
    }
    let signs = vec4f(value(cell), value(cell + vec2i(1, 0)), value(cell + 1), value(cell + vec2i(0, 1)));
    let inside = signs >= vec4f(0.);
    let across = (side + 2u) & 3u;
    let after = (side + 1u) & 3u;
    let before = (side + 3u) & 3u;
    let ca = inside[after] != inside[across];
    let cb = inside[across] != inside[before];
    if ca && cb {
        let centre = dot(signs, vec4f(1.)) >= 0.;
        return edge(cell, select(after, before, inside[after] == centre));
    }
    if ca {
        return edge(cell, after);
    }
    if cb {
        return edge(cell, across);
    }
    return edge(cell, before);
}
fn neighbours(id: u32) -> vec2u {
    let p = origin(id);
    if (id & 1u) == 0u {
        return vec2u(paired(p, 0u), paired(p - vec2i(0, 1), 2u));
    }
    return vec2u(paired(p, 3u), paired(p - vec2i(1, 0), 1u));
}
fn other(id: u32, previous: u32) -> u32 {
    let pair = neighbours(id);
    return select(pair.x, pair.y, pair.x == previous);
}
fn root_of(id: u32) -> u32 {
    var a = id;
    loop {
        let b = atomicLoad(&parents[a]);
        if b == a {
            break;
        }
        let grand = atomicLoad(&parents[b]);
        atomicMin(&parents[a], grand);
        a = grand;
    }
    return a;
}
// Link the greater root to the smaller one. Retries always move toward a lower id.
fn unite(x: u32, y: u32) {
    var a = x;
    var b = y;
    loop {
        a = root_of(a);
        b = root_of(b);
        if a == b {
            break;
        }
        let hi = max(a, b);
        let lo = min(a, b);
        let old = atomicMin(&parents[hi], lo);
        if old == hi {
            break;
        }
        a = old;
        b = lo;
    }
}
@compute @workgroup_size(8,8)
fn initialize(@builtin(global_invocation_id) gid: vec3u) {
    if any(gid.xy >= u.size) {
        return;
    }
    let h = vertex(gid.xy, 0u);
    for (var id = h; id <= h + 1u; id++) {
        atomicStore(&parents[id], select(NONE, id, crosses(id)));
        links[id] = NONE;
        arcs[id] =  - 1.;
    }
}
@compute @workgroup_size(8,8)
fn connect(@builtin(global_invocation_id) gid: vec3u) {
    if any(gid.xy + 1u >= u.size) {
        return;
    }
    let p = vec2i(gid.xy);
    for (var side = 0u; side < 4u; side++) {
        let a = edge(p, side);
        if crosses(a) {
            let b = paired(p, side);
            if a < b {
                unite(a, b);
            }
        }
    }
}

// The minimum id in each connected component owns its ordered polyline.
// Trace open paths from the frame boundary; orient closed paths clockwise
// in image space, starting at their top-leftmost point.
fn before(a: u32, b: u32) -> bool {
    let p = position(a);
    let q = position(b);
    return p.y < q.y || (p.y == q.y && p.x < q.x);
}
fn boundary_order(id: u32) -> u32 {
    let p = vec2u(origin(id));
    if (id & 1u) == 0u {
        return 2u * p.x + u32(p.y != 0u);
    }
    return 2u * u.size.x + 2u * p.y + u32(p.x != 0u);
}
// Ramer–Douglas–Peucker works in place on the per-component linked list.
fn simplify(start: u32, finish: u32, count: u32) {
    if u.tolerance <= 0. || count < 2u {
        return;
    }
    // Process the shorter half first: stack depth is logarithmic in edge count.
    var pending: array<vec3u, 32>;
    var depth = 0u;
    var span = vec3u(start, finish, count);
    loop {
        let a = position(span.x);
        let delta = position(span.y) - a;
        let denom = max(dot(delta, delta), 1e-12);
        var limit = u.tolerance * u.tolerance;
        var split = 0u;
        var chosen = NONE;
        var node = links[span.x];
        for (var i = 1u; i < span.z; i++) {
            let q = position(node) - a;
            let d = q - delta * clamp(dot(q, delta) / denom, 0., 1.);
            let error = dot(d, d);
            if error > limit {
                limit = error;
                split = i;
                chosen = node;
            }
            node = links[node];
        }
        if chosen != NONE {
            let left = vec3u(span.x, chosen, split);
            let right = vec3u(chosen, span.y, span.z - split);
            pending[depth] = select(left, right, left.z <= right.z);
            span = select(right, left, left.z <= right.z);
            depth++ ;
        } else {
            links[span.x] = span.y;
            if depth == 0u {
                break;
            }
            depth -- ;
            span = pending[depth];
        }
    }
}
// Each root owns disjoint links. Only kept segment starts receive an arc length.
fn trace_path(root: u32) {
    let pair = neighbours(root);
    var prev = root;
    var cur = select(pair.x, pair.y, pair.x == NONE);
    var anchor = root;
    var after = cur;
    var area = 0.;
    var closed = false;
    loop {
        let a = position(prev);
        let b = position(cur);
        area += a.x * b.y - a.y * b.x;
        if cur == root {
            closed = true;
            break;
        }
        let next = other(cur, prev);
        if next == NONE {
            break;
        }
        if before(cur, anchor) {
            anchor = cur;
            after = next;
        }
        prev = cur;
        cur = next;
    }
    if closed {
        if area < 0. {
            after = other(anchor, after);
        }
    } else {
        // Find the other endpoint before choosing the canonical start.
        anchor = cur;
        after = prev;
        prev = cur;
        cur = after;
        loop {
            let next = other(cur, prev);
            if next == NONE {
                break;
            }
            prev = cur;
            cur = next;
        }
        if boundary_order(cur) < boundary_order(anchor) {
            anchor = cur;
            after = prev;
        }
    }
    var n = 1u;
    prev = anchor;
    cur = after;
    links[anchor] = after;
    loop {
        if cur == anchor {
            break;
        }
        let next = other(cur, prev);
        links[cur] = next;
        if next == NONE {
            break;
        }
        prev = cur;
        cur = next;
        n++ ;
    }
    simplify(anchor, cur, n);
    var length = 0.;
    cur = anchor;
    loop {
        let next = links[cur];
        if next == NONE {
            break;
        }
        arcs[cur] = length;
        length += distance(position(cur), position(next));
        cur = next;
        if cur == anchor {
            break;
        }
    }
    lengths[root] = length;
    atomicMax(&ranks[0], bitcast < u32 > (length));
    atomicAdd(&ranks[1u + root / 256u], 1u);
}
@compute @workgroup_size(8,8)
fn trace(@builtin(global_invocation_id) gid: vec3u) {
    if any(gid.xy >= u.size) {
        return;
    }
    let h = vertex(gid.xy, 0u);
    for (var id = h; id <= h + 1u; id++) {
        if atomicLoad(&parents[id]) == id {
            trace_path(id);
        }
    }
}
// Rank components in scan order for Selected Contour and seeded rotation.
// Two tiers keep the number of sums per block bounded even on large frames.
@compute @workgroup_size(64)
fn count_blocks(@builtin(global_invocation_id) gid: vec3u) {
    let groups = (u.blocks + 255u) / 256u;
    if gid.x >= groups {
        return;
    }
    var sum = 0u;
    for (var i = gid.x * 256u; i < min((gid.x + 1u) * 256u, u.blocks); i++) {
        sum += atomicLoad(&ranks[1u + i]);
    }
    atomicStore(&ranks[1u + u.blocks + gid.x], sum);
}
@compute @workgroup_size(64)
fn rank_blocks(@builtin(global_invocation_id) gid: vec3u) {
    let block = gid.x;
    if block >= u.blocks {
        return;
    }
    let group = block / 256u;
    var rank = 0u;
    for (var i = 0u; i < group; i++) {
        rank += atomicLoad(&ranks[1u + u.blocks + i]);
    }
    for (var i = group * 256u; i < block; i++) {
        rank += atomicLoad(&ranks[1u + i]);
    }
    let end = min((block + 1u) * 256u, 2u * u.size.x * u.size.y);
    for (var i = block * 256u; i < end; i++) {
        if atomicLoad(&parents[i]) == i {
            atomicStore(&parents[i], RANKED | rank);
            rank++ ;
        }
    }
}

// Each simplified segment writes only its expanded strip. All dashes use
// the same colour, so maximum coverage is an order-independent reduction.
fn stroke(a: vec2f, b: vec2f, interval: vec2f, dash_length: f32) {
    let vertical = abs(b.y - a.y) > abs(b.x - a.x);
    let p = select(a, a.yx, vertical);
    let q = select(b, b.yx, vertical);
    let lo = select(u.roi.xy, u.roi.yx, vertical);
    let hi = select(u.roi.zw, u.roi.wz, vertical);
    let extent = u.radius + 1.5;
    let slope = (q.y - p.y) / select(q.x - p.x, 1e-9, abs(q.x - p.x) < 1e-9);
    let xmin = max(lo.x, i32(floor(min(p.x, q.x) - extent)));
    let xmax = min(hi.x, i32(ceil(max(p.x, q.x) + extent)));
    for (var x = xmin; x < xmax; x++) {
        let fx = clamp(f32(x) + 0.5, min(p.x, q.x), max(p.x, q.x));
        let center = p.y + (fx - p.x) * slope;
        let band = extent * sqrt(1. + slope * slope);
        let ymin = max(lo.y, i32(floor(center - band)));
        let ymax = min(hi.y, i32(ceil(center + band)));
        for (var y = ymin; y < ymax; y++) {
            let xy = select(vec2i(x, y), vec2i(y, x), vertical);
            let alpha = dash_coverage(vec2f(xy) + 0.5, a, b, u.radius, u.hardness, interval, dash_length, u.gradient);
            let offset = xy - u.roi.xy;
            atomicMax(&coverage[u32(offset.y * (u.roi.z - u.roi.x) + offset.x)], bitcast < u32 > (alpha));
        }
    }
}
fn draw(id: u32) {
    if arcs[id] < 0. {
        return;
    }
    var root = id;
    var tag = atomicLoad(&parents[root]);
    loop {
        if (tag & RANKED) != 0u {
            break;
        }
        root = tag;
        tag = atomicLoad(&parents[root]);
    }
    let ordinal = tag & ~RANKED;
    if u.selected != 0u && u.selected != ordinal + 1u {
        return;
    }
    let len = lengths[root];
    if len <= 0. || u.fraction <= 0. {
        return;
    }
    let a = position(id) + vec2f(u.origin);
    let b = position(links[id]) + vec2f(u.origin);
    let distance0 = arcs[id];
    let span = distance(a, b);
    if span <= 0. {
        return;
    }
    let longest = bitcast < f32 > (atomicLoad(&ranks[0]));
    var count = u.segments;
    if (u.flags & 2u) != 0u {
        count = u32(max(1., round(f32(count) * len / longest)));
    }
    let period = len / f32(count);
    let dash_length = period * u.fraction;
    let spacing = select(period, dash_length, (u.flags & 1u) != 0u);
    var angle = u.rotation;
    if (u.flags & 4u) != 0u {
        let n = (u.seed + ordinal) * 0x9e3779b9u;
        angle += f32((n << 13u) | (n >> 19u)) / 4294967295.;
    }
    let phase = fract(angle) * len;
    // A segment and a dash can meet on either side of the path origin.
    for (var lap = 0u; lap <= 1u; lap++) {
        let base = phase - f32(lap) * len;
        let first = max(0, i32(floor((distance0 - base - dash_length) / spacing)));
        let stop = min(i32(count), i32(ceil((distance0 + span - base) / spacing)));
        for (var j = first; j < stop; j++) {
            let start = base + f32(j) * spacing;
            let lo = max(distance0, start);
            let hi = min(distance0 + span, start + dash_length);
            if hi > lo {
                let pa = mix(a, b, (lo - distance0) / span);
                let pb = mix(a, b, (hi - distance0) / span);
                stroke(pa, pb, (vec2f(lo, hi) - start) / dash_length, dash_length);
            }
        }
    }
}
@compute @workgroup_size(8,8)
fn raster(@builtin(global_invocation_id) gid: vec3u) {
    if any(gid.xy >= u.size) {
        return;
    }
    let h = vertex(gid.xy, 0u);
    draw(h);
    draw(h + 1u);
}
@compute @workgroup_size(8,8)
fn composite(@builtin(global_invocation_id) gid: vec3u) {
    let xy = u.roi.xy + vec2i(gid.xy);
    if any(xy >= u.roi.zw) {
        return;
    }
    let alpha = bitcast < f32 > (atomicLoad(&coverage[gid.y * u32(u.roi.z - u.roi.x) + gid.x]));
    let paint = vec4f(u.color.rgb * alpha, alpha);
    let base = fx_sample(source, u.input, vec2f(xy) + 0.5, false);
    var result = paint;
    switch u.style {
        case 1u: {
            result = paint + base * (1. - alpha);
        }
        case 3u: {
            result = base * alpha;
        }
        case 4u: {
            result = base + paint * (1. - base.a);
        }
        default: {
        }
    }
    if (u.flags & 8u) == 0u {
        result = saturate(result);
    }
    textureStore(dst, xy - u.output_origin, result);
}
