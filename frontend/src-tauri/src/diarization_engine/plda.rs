//! PLDA transforms + VBx clustering — ported from test ASR
//! `core/speaker_diarization_pure_ort.py` (pyannote Community-1).

use anyhow::{anyhow, Context, Result};
use ndarray::{Array1, Array2, Axis};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PldaData {
    pub mean1: Array1<f64>,
    pub mean2: Array1<f64>,
    pub lda: Array2<f64>,
    pub plda_mu: Array1<f64>,
    pub plda_tr: Array2<f64>,
    pub plda_psi: Array1<f64>,
}

pub fn l2_norm_rows(x: &Array2<f64>) -> Array2<f64> {
    let mut out = x.clone();
    for mut row in out.axis_iter_mut(Axis(0)) {
        let n = row.iter().map(|v| v * v).sum::<f64>().sqrt() + 1e-10;
        row.mapv_inplace(|v| v / n);
    }
    out
}

pub fn l2_norm_vec(x: &Array1<f64>) -> Array1<f64> {
    let n = x.iter().map(|v| v * v).sum::<f64>().sqrt() + 1e-10;
    x / n
}

/// Prefer `plda/plda_prepared.npz` (same as test ASR `load_plda`).
pub fn load_plda(model_dir: &Path) -> Result<PldaData> {
    let prepared = model_dir.join("plda").join("plda_prepared.npz");
    if prepared.exists() {
        return load_plda_prepared(&prepared);
    }
    Err(anyhow!(
        "plda_prepared.npz not found under {}",
        model_dir.display()
    ))
}

fn load_plda_prepared(path: &Path) -> Result<PldaData> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut npz = ndarray_npy::NpzReader::new(BufReader::new(file))
        .with_context(|| format!("npz {}", path.display()))?;

    let mean1: Array1<f64> = read_f64_1d(&mut npz, "mean1")?;
    let mean2: Array1<f64> = read_f64_1d(&mut npz, "mean2")?;
    let lda: Array2<f64> = read_f64_2d(&mut npz, "lda")?;
    let plda_mu: Array1<f64> = read_f64_1d(&mut npz, "mu")?;
    let plda_tr: Array2<f64> = read_f64_2d(&mut npz, "plda_tr")?;
    let plda_psi: Array1<f64> = read_f64_1d(&mut npz, "plda_psi")?;

    Ok(PldaData {
        mean1,
        mean2,
        lda,
        plda_mu,
        plda_tr,
        plda_psi,
    })
}

fn read_f64_1d<R: std::io::Read + std::io::Seek>(
    npz: &mut ndarray_npy::NpzReader<R>,
    key: &str,
) -> Result<Array1<f64>> {
    // Arrays may be f32 or f64 in the archive.
    if let Ok(a) = npz.by_name::<ndarray::OwnedRepr<f64>, ndarray::Ix1>(key) {
        return Ok(a);
    }
    let a32: Array1<f32> = npz
        .by_name::<ndarray::OwnedRepr<f32>, ndarray::Ix1>(key)
        .with_context(|| format!("missing key {key}"))?;
    Ok(a32.mapv(|v| v as f64))
}

fn read_f64_2d<R: std::io::Read + std::io::Seek>(
    npz: &mut ndarray_npy::NpzReader<R>,
    key: &str,
) -> Result<Array2<f64>> {
    if let Ok(a) = npz.by_name::<ndarray::OwnedRepr<f64>, ndarray::Ix2>(key) {
        return Ok(a);
    }
    let a32: Array2<f32> = npz
        .by_name::<ndarray::OwnedRepr<f32>, ndarray::Ix2>(key)
        .with_context(|| format!("missing key {key}"))?;
    Ok(a32.mapv(|v| v as f64))
}

pub fn xvec_transform(embeddings: &Array2<f64>, pd: &PldaData) -> Array2<f64> {
    // (l2_norm(embeddings - mean1) * sqrt(lda.rows)) @ lda - mean2
    // then l2_norm * sqrt(D_out)
    let d_in = pd.lda.nrows() as f64;
    let d_out = pd.lda.ncols() as f64;
    let centered = embeddings - &pd.mean1;
    let normed = l2_norm_rows(&centered) * d_in.sqrt();
    let projected = normed.dot(&pd.lda) - &pd.mean2;
    l2_norm_rows(&projected) * d_out.sqrt()
}

pub fn plda_transform(embeddings: &Array2<f64>, pd: &PldaData, lda_dim: usize) -> Array2<f64> {
    // (embeddings - plda_mu) @ plda_tr.T[:, :lda_dim]
    let centered = embeddings - &pd.plda_mu;
    let tr_t = pd.plda_tr.t();
    let cols = tr_t.slice(ndarray::s![.., ..lda_dim]);
    centered.dot(&cols)
}

fn softmax_rows(logits: &Array2<f64>) -> Array2<f64> {
    let mut out = Array2::<f64>::zeros(logits.raw_dim());
    for (i, row) in logits.axis_iter(Axis(0)).enumerate() {
        let max = row.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let exps: Vec<f64> = row.iter().map(|v| (v - max).exp()).collect();
        let sum: f64 = exps.iter().sum();
        for (j, e) in exps.into_iter().enumerate() {
            out[[i, j]] = e / sum;
        }
    }
    out
}

fn logsumexp_row(row: ndarray::ArrayView1<f64>) -> f64 {
    let max = row.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let sum: f64 = row.iter().map(|v| (v - max).exp()).sum();
    max + sum.ln()
}

/// VBx clustering — exact from pyannote/vbx.py / pure_ort.py
pub fn vbx_cluster(
    fea: &Array2<f64>,
    plda_psi: &Array1<f64>,
    ahc_labels: &[usize],
    fa: f64,
    fb: f64,
    max_iters: usize,
) -> (Array2<f64>, Array1<f64>) {
    let (t, d) = (fea.nrows(), fea.ncols());
    let n_clusters = ahc_labels.iter().copied().max().unwrap_or(0) + 1;

    let mut qinit = Array2::<f64>::zeros((t, n_clusters));
    for (i, &lab) in ahc_labels.iter().enumerate() {
        qinit[[i, lab]] = 1.0;
    }
    let mut gamma = softmax_rows(&(qinit * 7.0));
    let mut pi = Array1::<f64>::ones(n_clusters) / n_clusters as f64;

    let g = {
        let mut g = Array2::<f64>::zeros((t, 1));
        for i in 0..t {
            let sum_sq: f64 = fea.row(i).iter().map(|v| v * v).sum();
            g[[i, 0]] = -0.5 * (sum_sq + d as f64 * (2.0 * std::f64::consts::PI).ln());
        }
        g
    };

    let v = plda_psi.mapv(f64::sqrt);
    let rho = fea * &v; // broadcast columns

    let mut prev_elbo = f64::NEG_INFINITY;
    for ii in 0..max_iters {
        // invL = 1 / (1 + Fa/Fb * gamma.sum(0).T * psi)
        let gamma_sum = gamma.sum_axis(Axis(0)); // (n_clusters,)
        let mut inv_l = Array2::<f64>::zeros((n_clusters, d));
        for c in 0..n_clusters {
            for j in 0..d {
                inv_l[[c, j]] = 1.0 / (1.0 + fa / fb * gamma_sum[c] * plda_psi[j]);
            }
        }

        // alpha = Fa/Fb * invL * gamma.T @ rho
        let gamma_t = gamma.t();
        let gt_rho = gamma_t.dot(&rho); // (n_clusters, d)
        let mut alpha = Array2::<f64>::zeros((n_clusters, d));
        for c in 0..n_clusters {
            for j in 0..d {
                alpha[[c, j]] = fa / fb * inv_l[[c, j]] * gt_rho[[c, j]];
            }
        }

        // log_p_ = Fa * (rho @ alpha.T - 0.5 * (invL + alpha^2) @ psi + G)
        let alpha_t = alpha.t();
        let term1 = rho.dot(&alpha_t); // (t, n_clusters)
        let mut mid = Array2::<f64>::zeros((n_clusters, d));
        for c in 0..n_clusters {
            for j in 0..d {
                mid[[c, j]] = inv_l[[c, j]] + alpha[[c, j]] * alpha[[c, j]];
            }
        }
        let mut term2 = Array1::<f64>::zeros(n_clusters);
        for c in 0..n_clusters {
            let mut s = 0.0;
            for j in 0..d {
                s += mid[[c, j]] * plda_psi[j];
            }
            term2[c] = s;
        }

        let mut log_p = Array2::<f64>::zeros((t, n_clusters));
        for i in 0..t {
            for c in 0..n_clusters {
                log_p[[i, c]] = fa * (term1[[i, c]] - 0.5 * term2[c] + g[[i, 0]]);
            }
        }

        let lpi = pi.mapv(|p| (p + 1e-8).ln());
        let mut log_p_x = Array1::<f64>::zeros(t);
        for i in 0..t {
            let mut row = log_p.row(i).to_owned();
            for c in 0..n_clusters {
                row[c] += lpi[c];
            }
            log_p_x[i] = logsumexp_row(row.view());
            for c in 0..n_clusters {
                gamma[[i, c]] = (log_p[[i, c]] + lpi[c] - log_p_x[i]).exp();
            }
        }

        pi = gamma.sum_axis(Axis(0));
        let pi_sum = pi.sum();
        pi.mapv_inplace(|p| p / pi_sum);

        let mut elbo = log_p_x.sum();
        for c in 0..n_clusters {
            for j in 0..d {
                elbo += fb
                    * 0.5
                    * (inv_l[[c, j]].ln() - inv_l[[c, j]] - alpha[[c, j]] * alpha[[c, j]] + 1.0);
            }
        }

        if ii > 0 && elbo - prev_elbo < 1e-4 {
            break;
        }
        prev_elbo = elbo;
    }

    (gamma, pi)
}

pub fn vbx_hard_labels(gamma: &Array2<f64>) -> Vec<usize> {
    gamma
        .axis_iter(Axis(0))
        .map(|row| {
            row.iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray_npy::ReadNpyExt;
    use std::path::PathBuf;

    fn model_dir() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("MEETINGONE_DIARIZATION_MODEL_DIR") {
            return Some(PathBuf::from(p));
        }
        let default = PathBuf::from(r"C:\Users\HP\Desktop\test ASR\models\pyannote\speaker-diarization-community-1");
        if default.join("plda").join("plda_prepared.npz").exists() {
            Some(default)
        } else {
            None
        }
    }

    fn testdata(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/diarization_engine/testdata")
            .join(name)
    }

    #[test]
    fn xvec_plda_vbx_match_python_golden() {
        let Some(dir) = model_dir() else {
            eprintln!("skip: diarization model dir not found");
            return;
        };
        let pd = load_plda(&dir).expect("load_plda");

        let emb_f32: Array2<f32> =
            Array2::read_npy(File::open(testdata("emb_seed42.npy")).unwrap()).unwrap();
        let emb = emb_f32.mapv(|v| v as f64);

        let xt = xvec_transform(&emb, &pd);
        let pt = plda_transform(&xt, &pd, 128);

        let xt_gold: Array2<f64> =
            Array2::read_npy(File::open(testdata("xt_seed42.npy")).unwrap()).unwrap();
        let pt_gold: Array2<f64> =
            Array2::read_npy(File::open(testdata("pt_seed42.npy")).unwrap()).unwrap();

        let xt_err = (&xt - &xt_gold).mapv(f64::abs).mean().unwrap();
        let pt_err = (&pt - &pt_gold).mapv(f64::abs).mean().unwrap();
        assert!(xt_err < 1e-5, "xvec mean abs err {xt_err}");
        assert!(pt_err < 1e-5, "plda mean abs err {pt_err}");

        let psi = pd.plda_psi.slice(ndarray::s![..128]).to_owned();
        let (gamma, _) = vbx_cluster(&pt, &psi, &[0, 0, 1, 1], 0.07, 0.8, 20);
        let labels = vbx_hard_labels(&gamma);
        assert_eq!(labels, vec![0, 0, 1, 1]);
    }

    #[test]
    fn l2_norm_unit_length() {
        let x = Array2::from_shape_vec((1, 3), vec![3.0, 0.0, 4.0]).unwrap();
        let n = l2_norm_rows(&x);
        let len = n.row(0).iter().map(|v| v * v).sum::<f64>().sqrt();
        assert!((len - 1.0).abs() < 1e-9);
    }
}
