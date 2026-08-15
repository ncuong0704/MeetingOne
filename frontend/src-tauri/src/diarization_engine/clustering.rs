//! Senko clustering: spectral + filter_minor + mer_cos + turn post-process.
//! Ported from test ASR `speaker_diarization_senko_campp.py`.

use crate::diarization_engine::align::SpeakerTurn;
use ndarray::{s, Array1, Array2};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

pub fn cosine_similarity(x: &Array2<f64>, y: Option<&Array2<f64>>) -> Array2<f64> {
    let y = y.unwrap_or(x);
    let xn = l2_normalize_rows(x);
    let yn = l2_normalize_rows(y);
    xn.dot(&yn.t())
}

fn l2_normalize_rows(m: &Array2<f64>) -> Array2<f64> {
    let mut out = m.clone();
    for mut row in out.rows_mut() {
        let n = row.iter().map(|v| v * v).sum::<f64>().sqrt();
        if n > 1e-10 {
            row.mapv_inplace(|v| v / n);
        }
    }
    out
}

/// Senko SpectralCluster — p-pruning + unnormalized Laplacian + eigengap + KMeans.
pub fn senko_spectral(
    x: &Array2<f64>,
    min_num_spks: usize,
    max_num_spks: usize,
    pval: f64,
    min_pnum: usize,
    oracle_num: Option<usize>,
) -> Vec<usize> {
    let n = x.nrows();
    if n <= 1 {
        return vec![0; n];
    }

    let mut m = cosine_similarity(x, None);
    let n_elems = ((1.0 - pval) * n as f64) as usize;
    let n_elems = n_elems.min(n.saturating_sub(min_pnum)).max(0);

    for i in 0..n {
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| m[[i, a]].partial_cmp(&m[[i, b]]).unwrap());
        for &j in order.iter().take(n_elems) {
            m[[i, j]] = 0.0;
        }
    }

    m = &m * 0.5 + &m.t().to_owned() * 0.5;
    for i in 0..n {
        m[[i, i]] = 0.0;
    }
    let mut d = Array1::<f64>::zeros(n);
    for i in 0..n {
        d[i] = m.row(i).iter().map(|v| v.abs()).sum();
    }
    let mut l = Array2::<f64>::zeros((n, n));
    for i in 0..n {
        l[[i, i]] = d[i];
        for j in 0..n {
            l[[i, j]] -= m[[i, j]];
        }
    }

    let (lambdas, eig_vecs) = eigh_symmetric(&l);
    let num_of_spk = if let Some(k) = oracle_num {
        k
    } else {
        let lo = min_num_spks.saturating_sub(1);
        let hi = (max_num_spks + 1).min(n);
        if hi <= lo + 1 {
            1
        } else {
            let sub = lambdas.slice(s![lo..hi]);
            let mut best_i = 0usize;
            let mut best_gap = f64::NEG_INFINITY;
            for i in 0..sub.len().saturating_sub(1) {
                let gap = sub[i + 1] - sub[i];
                if gap > best_gap {
                    best_gap = gap;
                    best_i = i;
                }
            }
            best_i + min_num_spks
        }
    };
    let num_of_spk = num_of_spk.max(1).min(n);
    let emb = eig_vecs.slice(s![.., ..num_of_spk]).to_owned();
    kmeans(&emb, num_of_spk, 0)
}

pub fn senko_cluster(
    x: &Array2<f64>,
    mer_cos: f64,
    min_cluster_size: usize,
    min_num_spks: usize,
    max_num_spks: usize,
    pval: f64,
    oracle_num: Option<usize>,
) -> Vec<usize> {
    let n = x.nrows();
    if n < 10 {
        return vec![0; n];
    }
    if n <= 2 {
        return vec![0; n];
    }
    let mut labels = senko_spectral(x, min_num_spks, max_num_spks, pval, 6, oracle_num);
    filter_minor_and_merge(x, &mut labels, mer_cos, min_cluster_size);
    relabel(&labels)
}

fn filter_minor_and_merge(
    x: &Array2<f64>,
    labels: &mut [usize],
    mer_cos: f64,
    min_cluster_size: usize,
) {
    let n = labels.len();
    let mut cset: Vec<usize> = labels.iter().copied().collect();
    cset.sort_unstable();
    cset.dedup();
    let csize: Vec<usize> = cset
        .iter()
        .map(|&c| labels.iter().filter(|&&l| l == c).count())
        .collect();
    let minor: Vec<usize> = cset
        .iter()
        .zip(csize.iter())
        .filter(|(_, &s)| s < min_cluster_size)
        .map(|(&c, _)| c)
        .collect();
    let major: Vec<usize> = cset
        .iter()
        .zip(csize.iter())
        .filter(|(_, &s)| s >= min_cluster_size)
        .map(|(&c, _)| c)
        .collect();

    if !minor.is_empty() {
        if major.is_empty() {
            labels.fill(0);
        } else {
            let mut centers = Array2::<f64>::zeros((major.len(), x.ncols()));
            for (k, &c) in major.iter().enumerate() {
                let mut acc = Array1::<f64>::zeros(x.ncols());
                let mut cnt = 0.0;
                for i in 0..n {
                    if labels[i] == c {
                        acc = acc + x.row(i);
                        cnt += 1.0;
                    }
                }
                centers.row_mut(k).assign(&(acc / cnt));
            }
            for i in 0..n {
                if minor.contains(&labels[i]) {
                    let row = x.slice(s![i..i + 1, ..]).to_owned();
                    let sim = cosine_similarity(&row, Some(&centers));
                    let mut best = 0usize;
                    let mut best_v = f64::NEG_INFINITY;
                    for j in 0..major.len() {
                        if sim[[0, j]] > best_v {
                            best_v = sim[[0, j]];
                            best = j;
                        }
                    }
                    labels[i] = major[best];
                }
            }
        }
    }

    if mer_cos > 0.0 {
        loop {
            let mut uniq: Vec<usize> = labels.iter().copied().collect();
            uniq.sort_unstable();
            uniq.dedup();
            if uniq.len() <= 1 {
                break;
            }
            let mut centers = Array2::<f64>::zeros((uniq.len(), x.ncols()));
            for (k, &c) in uniq.iter().enumerate() {
                let mut acc = Array1::<f64>::zeros(x.ncols());
                let mut cnt = 0.0f64;
                for i in 0..n {
                    if labels[i] == c {
                        acc = acc + x.row(i);
                        cnt += 1.0;
                    }
                }
                centers.row_mut(k).assign(&(acc / cnt.max(1.0)));
            }
            let mut aff = cosine_similarity(&centers, None);
            for i in 0..uniq.len() {
                for j in 0..=i {
                    aff[[i, j]] = f64::NEG_INFINITY;
                }
            }
            let mut best = f64::NEG_INFINITY;
            let mut bi = 0usize;
            let mut bj = 1usize;
            for i in 0..uniq.len() {
                for j in 0..uniq.len() {
                    if aff[[i, j]] > best {
                        best = aff[[i, j]];
                        bi = i;
                        bj = j;
                    }
                }
            }
            if best < mer_cos {
                break;
            }
            let c1 = uniq[bi];
            let c2 = uniq[bj];
            for l in labels.iter_mut() {
                if *l == c2 {
                    *l = c1;
                }
            }
        }
    }
}

fn relabel(labels: &[usize]) -> Vec<usize> {
    let mut uniq: Vec<usize> = labels.to_vec();
    uniq.sort_unstable();
    uniq.dedup();
    let map: std::collections::HashMap<usize, usize> =
        uniq.iter().enumerate().map(|(n, &o)| (o, n)).collect();
    labels.iter().map(|l| map[l]).collect()
}

fn kmeans(emb: &Array2<f64>, k: usize, seed: u64) -> Vec<usize> {
    let n = emb.nrows();
    let d = emb.ncols();
    let k = k.max(1).min(n);
    if k == 1 {
        return vec![0; n];
    }
    let mut rng = StdRng::seed_from_u64(seed);
    let mut centers = Array2::<f64>::zeros((k, d));
    let first = rng.gen_range(0..n);
    centers.row_mut(0).assign(&emb.row(first));
    for c in 1..k {
        let mut dist = vec![0.0; n];
        for i in 0..n {
            let mut best = f64::INFINITY;
            for j in 0..c {
                let mut s = 0.0;
                for t in 0..d {
                    let diff = emb[[i, t]] - centers[[j, t]];
                    s += diff * diff;
                }
                best = best.min(s);
            }
            dist[i] = best;
        }
        let sum: f64 = dist.iter().sum();
        let mut r = rng.gen::<f64>() * sum.max(1e-12);
        let mut pick = n - 1;
        for i in 0..n {
            r -= dist[i];
            if r <= 0.0 {
                pick = i;
                break;
            }
        }
        centers.row_mut(c).assign(&emb.row(pick));
    }

    let mut labels = vec![0usize; n];
    for _ in 0..25 {
        for i in 0..n {
            let mut best = 0usize;
            let mut best_d = f64::INFINITY;
            for c in 0..k {
                let mut s = 0.0;
                for t in 0..d {
                    let diff = emb[[i, t]] - centers[[c, t]];
                    s += diff * diff;
                }
                if s < best_d {
                    best_d = s;
                    best = c;
                }
            }
            labels[i] = best;
        }
        let mut new_c = Array2::<f64>::zeros((k, d));
        let mut cnt = vec![0.0; k];
        for i in 0..n {
            let c = labels[i];
            cnt[c] += 1.0;
            for t in 0..d {
                new_c[[c, t]] += emb[[i, t]];
            }
        }
        for c in 0..k {
            if cnt[c] > 0.0 {
                for t in 0..d {
                    new_c[[c, t]] /= cnt[c];
                }
            } else {
                new_c.row_mut(c).assign(&centers.row(c));
            }
        }
        centers = new_c;
    }
    labels
}

/// Symmetric eigendecomposition. Eigenvalues ascending; columns of V.
fn eigh_symmetric(a: &Array2<f64>) -> (Array1<f64>, Array2<f64>) {
    let n = a.nrows();
    let mut data = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in 0..n {
            data.push(a[[i, j]]);
        }
    }
    let m = nalgebra::DMatrix::from_row_slice(n, n, &data);
    let eig = nalgebra::SymmetricEigen::new(m);
    let mut pairs: Vec<(f64, usize)> = (0..n).map(|i| (eig.eigenvalues[i], i)).collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut evals = Array1::<f64>::zeros(n);
    let mut evecs = Array2::<f64>::zeros((n, n));
    for (new, &(val, old)) in pairs.iter().enumerate() {
        evals[new] = val;
        for r in 0..n {
            evecs[[r, new]] = eig.eigenvectors[(r, old)];
        }
    }
    (evals, evecs)
}

pub fn windows_to_turns(
    window_times: &[(f64, f64)],
    labels: &[usize],
    min_duration_off: f64,
) -> Vec<SpeakerTurn> {
    if window_times.is_empty() {
        return Vec::new();
    }
    let mut segs: Vec<(f64, f64, usize)> = Vec::new();
    let mut cs = window_times[0].0;
    let mut ce = window_times[0].1;
    let mut cl = labels[0];
    for i in 1..window_times.len() {
        let (ws, we) = window_times[i];
        let label = labels[i];
        if label == cl && (ws - ce) < min_duration_off + 0.01 {
            ce = we;
        } else {
            segs.push((cs, ce, cl));
            cs = ws;
            ce = we;
            cl = label;
        }
    }
    segs.push((cs, ce, cl));

    for i in 0..segs.len().saturating_sub(1) {
        if segs[i].1 > segs[i + 1].0 {
            let mid = (segs[i].1 + segs[i + 1].0) / 2.0;
            segs[i].1 = mid;
            segs[i + 1].0 = mid;
        }
    }

    // Merge adjacent same speaker with gap <= 4s
    let mut merged = vec![segs[0]];
    for seg in segs.into_iter().skip(1) {
        let prev = merged.last_mut().unwrap();
        if seg.2 == prev.2 && seg.0 - prev.1 <= 4.0 {
            prev.1 = seg.1;
        } else {
            merged.push(seg);
        }
    }

    // Drop segments <= 0.78s
    if merged.len() > 1 {
        let orig = merged.clone();
        let mut filtered: Vec<(f64, f64, usize)> = Vec::new();
        for (i, seg) in orig.iter().enumerate() {
            if seg.1 - seg.0 > 0.78 {
                filtered.push(*seg);
            } else {
                let prev_spk = filtered.last().map(|s| s.2);
                let next_spk = orig.get(i + 1).map(|s| s.2);
                if prev_spk.is_some() && prev_spk == next_spk {
                    if let Some(last) = filtered.last_mut() {
                        last.1 = seg.1;
                    }
                }
            }
        }
        if !filtered.is_empty() {
            merged = filtered;
        }
    }

    let mut final_segs = vec![merged[0]];
    for seg in merged.into_iter().skip(1) {
        let last = final_segs.last_mut().unwrap();
        if seg.2 == last.2 {
            last.1 = seg.1;
        } else {
            final_segs.push(seg);
        }
    }

    let mut dur: std::collections::HashMap<usize, f64> = std::collections::HashMap::new();
    for s in &final_segs {
        *dur.entry(s.2).or_insert(0.0) += s.1 - s.0;
    }
    let mut ranked: Vec<(usize, f64)> = dur.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    let remap: std::collections::HashMap<usize, usize> =
        ranked.iter().enumerate().map(|(n, (old, _))| (*old, n)).collect();

    final_segs
        .into_iter()
        .map(|(start_sec, end_sec, old)| SpeakerTurn {
            start_sec,
            end_sec,
            cluster_index: remap[&old],
        })
        .collect()
}

pub fn energy_vad(
    audio: &[f32],
    sr: u32,
    energy_ratio: f32,
    merge_gap: f64,
    min_duration: f64,
) -> Vec<(f64, f64)> {
    let frame_ms = 25.0;
    let hop_ms = 10.0;
    let frame_len = (sr as f64 * frame_ms / 1000.0) as usize;
    let hop_len = (sr as f64 * hop_ms / 1000.0) as usize;
    if audio.len() < frame_len {
        return Vec::new();
    }
    let n_frames = 1 + (audio.len() - frame_len) / hop_len;
    let mut energies = vec![0.0f32; n_frames];
    for i in 0..n_frames {
        let start = i * hop_len;
        let frame = &audio[start..start + frame_len];
        energies[i] = frame.iter().map(|s| s * s).sum();
    }
    let mut sorted = energies.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((n_frames as f64 - 1.0) * 0.95).round() as usize;
    let p95 = sorted[idx.min(n_frames - 1)];
    let threshold = p95 * energy_ratio;

    let mut regions = Vec::new();
    let mut in_speech = false;
    let mut start_time = 0.0;
    for i in 0..n_frames {
        let t = i as f64 * hop_ms / 1000.0;
        if energies[i] > threshold && !in_speech {
            start_time = t;
            in_speech = true;
        } else if energies[i] <= threshold && in_speech {
            regions.push((start_time, t + frame_ms / 1000.0));
            in_speech = false;
        }
    }
    if in_speech {
        let t = (n_frames - 1) as f64 * hop_ms / 1000.0 + frame_ms / 1000.0;
        regions.push((start_time, t));
    }
    if regions.is_empty() {
        return Vec::new();
    }
    let mut merged = vec![regions[0]];
    for (s, e) in regions.into_iter().skip(1) {
        let prev = merged.last_mut().unwrap();
        if s - prev.1 < merge_gap {
            prev.1 = prev.1.max(e);
        } else {
            merged.push((s, e));
        }
    }
    let audio_dur = audio.len() as f64 / sr as f64;
    merged
        .into_iter()
        .filter(|(s, e)| e - s >= min_duration)
        .map(|(s, e)| (s.max(0.0), e.min(audio_dur)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectral_separates_two_blobs() {
        let mut x = Array2::<f64>::zeros((20, 4));
        for i in 0..10 {
            x[[i, 0]] = 1.0 + (i as f64) * 0.01;
            x[[i, 1]] = 0.1;
        }
        for i in 10..20 {
            x[[i, 0]] = -1.0 - ((i - 10) as f64) * 0.01;
            x[[i, 1]] = 0.1;
        }
        let labels = senko_cluster(&x, 0.875, 4, 1, 15, 0.012, None);
        let uniq: std::collections::HashSet<usize> = labels.iter().copied().collect();
        assert_eq!(uniq.len(), 2, "labels={labels:?}");
        let a = labels[0];
        assert!(labels[..10].iter().all(|&l| l == a));
        assert!(labels[10..].iter().all(|&l| l != a));
    }

    #[test]
    fn mer_cos_merges_near_duplicates() {
        let mut x = Array2::<f64>::zeros((12, 2));
        for i in 0..12 {
            x[[i, 0]] = 1.0;
            x[[i, 1]] = 0.01 * i as f64;
        }
        let mut labels: Vec<usize> = (0..12).map(|i| if i < 6 { 0 } else { 1 }).collect();
        filter_minor_and_merge(&x, &mut labels, 0.875, 4);
        let uniq: std::collections::HashSet<_> = labels.iter().copied().collect();
        assert_eq!(uniq.len(), 1);
    }

    #[test]
    fn energy_vad_finds_speech() {
        let mut audio = vec![0.0f32; 16000];
        for i in 4000..12000 {
            audio[i] = 0.3 * ((i as f32) * 0.1).sin();
        }
        let regs = energy_vad(&audio, 16000, 0.1, 0.3, 0.3);
        assert!(!regs.is_empty());
        assert!(regs[0].1 - regs[0].0 > 0.4);
    }

    #[test]
    fn windows_to_turns_merges_same_speaker() {
        let times = vec![(0.0, 1.5), (0.6, 2.1), (3.0, 4.5)];
        let labels = vec![0, 0, 1];
        let turns = windows_to_turns(&times, &labels, 0.0);
        assert!(turns.len() >= 2);
        assert_eq!(turns[0].cluster_index, 0);
    }
}
