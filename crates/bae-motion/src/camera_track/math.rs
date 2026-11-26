//! Small symmetric eigensystems for eight-point estimation and essential decomposition.
use bae_anim::glam::{DMat3, DVec3};

pub(super) fn eigen<const N: usize>(mut a: [[f64; N]; N]) -> ([f64; N], [[f64; N]; N]) {
    let mut vectors = [[0.; N]; N];
    for (i, row) in vectors.iter_mut().enumerate() {
        row[i] = 1.;
    }
    for _ in 0..40 {
        let mut changed = false;
        for p in 0..N {
            for q in p + 1..N {
                let off = a[p][q];
                if off.abs() <= 1e-14 * (a[p][p].abs() + a[q][q].abs() + 1e-20) {
                    continue;
                }
                changed = true;
                let phi = ((2. * off).atan2(a[q][q] - a[p][p])) * 0.5;
                let (s, c) = phi.sin_cos();
                for row in &mut a {
                    (row[p], row[q]) = (c * row[p] - s * row[q], s * row[p] + c * row[q]);
                }
                let (left, right) = a.split_at_mut(q);
                for (x, y) in left[p].iter_mut().zip(&mut right[0]) {
                    (*x, *y) = (c * *x - s * *y, s * *x + c * *y);
                }
                for row in &mut vectors {
                    (row[p], row[q]) = (c * row[p] - s * row[q], s * row[p] + c * row[q]);
                }
            }
        }
        if !changed {
            break;
        }
    }
    (std::array::from_fn(|i| a[i][i]), vectors)
}

pub(super) fn svd(m: DMat3) -> Option<(DMat3, DVec3, DMat3)> {
    let (e, v) = eigen((m.transpose() * m).to_cols_array_2d());
    let mut order = [0, 1, 2];
    order.sort_by(|&a, &b| e[b].total_cmp(&e[a]));
    let col = |j: usize| DVec3::new(v[0][order[j]], v[1][order[j]], v[2][order[j]]);
    let (x, y) = (col(0), col(1));
    let u = (m * x).try_normalize()?;
    let v = (m * y - u * u.dot(m * y)).try_normalize()?;
    Some((
        DMat3::from_cols(u, v, u.cross(v)),
        DVec3::from_array(order.map(|i| e[i].max(0.).sqrt())),
        DMat3::from_cols(x, y, x.cross(y)),
    ))
}

pub(super) fn null_vector<const N: usize>(rows: impl IntoIterator<Item = [f64; N]>) -> [f64; N] {
    let mut ata = [[0.; N]; N];
    for row in rows {
        for (i, a) in ata.iter_mut().enumerate() {
            for (j, v) in a.iter_mut().enumerate() {
                *v += row[i] * row[j];
            }
        }
    }
    let (values, vectors) = eigen(ata);
    let i = (0..N)
        .min_by(|&a, &b| values[a].total_cmp(&values[b]))
        .unwrap();
    vectors.map(|row| row[i])
}

pub(super) fn linear<const N: usize>(mut a: [[f64; N]; N], mut b: [f64; N]) -> Option<[f64; N]> {
    for i in 0..N {
        let k = (i..N).max_by(|&j, &k| a[j][i].abs().total_cmp(&a[k][i].abs()))?;
        a.swap(i, k);
        b.swap(i, k);
        let pivot = a[i][i];
        if !pivot.is_finite() || pivot.abs() < 1e-14 {
            return None;
        }
        for x in &mut a[i][i..] {
            *x /= pivot;
        }
        b[i] /= pivot;
        let row = a[i];
        let rhs = b[i];
        for j in 0..N {
            if j == i {
                continue;
            }
            let f = a[j][i];
            for (x, r) in a[j][i..].iter_mut().zip(&row[i..]) {
                *x -= f * r;
            }
            b[j] -= f * rhs;
        }
    }
    Some(b)
}
