use anyhow::{Context, Result};
use arcweight::fst::VectorFst;
use arcweight::io::open_far;
use arcweight::prelude::*;
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

const OPENFST_FAR_HEADER_MAGIC: i32 = 0x7EB2_F35C;
const OPENFST_FST_MAGIC_PRIMARY: i32 = 0x7EB2_F35C;
const OPENFST_FST_MAGIC_ALT: i32 = 0x7EB2_FDD6;

fn read_i32_le<R: Read>(reader: &mut R) -> Result<i32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

fn read_f32_le<R: Read>(reader: &mut R) -> Result<f32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

fn read_i64_le<R: Read>(reader: &mut R) -> Result<i64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(i64::from_le_bytes(buf))
}

/// Build a linear acceptor for Pynini/OpenFST string input.
/// NeMo/Pynini Vietnamese grammars use UTF-8 **byte** labels (ilabel == olabel).
pub fn linear_fst_from_utf8(s: &str) -> VectorFst<TropicalWeight> {
    linear_fst_from_labels(s.as_bytes().iter().copied().map(u32::from))
}

/// Char-level variant for grammars compiled in UTF8 mode.
pub fn linear_fst_from_chars(s: &str) -> VectorFst<TropicalWeight> {
    linear_fst_from_labels(s.chars().map(|ch| ch as u32))
}

fn linear_fst_from_labels(labels: impl IntoIterator<Item = u32>) -> VectorFst<TropicalWeight> {
    let labels: Vec<u32> = labels.into_iter().collect();
    let mut fst = VectorFst::new();

    if labels.is_empty() {
        let state = fst.add_state();
        fst.set_start(state);
        fst.set_final(state, TropicalWeight::one());
        return fst;
    }

    let states: Vec<_> = (0..=labels.len()).map(|_| fst.add_state()).collect();
    fst.set_start(states[0]);
    fst.set_final(*states.last().unwrap(), TropicalWeight::one());

    for (i, label) in labels.iter().enumerate() {
        fst.add_arc(
            states[i],
            Arc::new(*label, *label, TropicalWeight::one(), states[i + 1]),
        );
    }

    fst
}

fn labels_to_string(labels: &[u32]) -> Result<String> {
    let bytes: Vec<u8> = labels
        .iter()
        .filter(|&&label| label != 0)
        .map(|&label| {
            u8::try_from(label).with_context(|| format!("label {label} is not a valid byte"))
        })
        .collect::<Result<_>>()?;

    String::from_utf8(bytes).context("output labels are not valid UTF-8")
}

/// Compose input with transducer and return the single best output string.
pub fn top_rewrite(input: &str, transducer: &VectorFst<TropicalWeight>) -> Result<String> {
    top_rewrite_with(input, transducer, linear_fst_from_utf8)
}

pub fn top_rewrite_with(
    input: &str,
    transducer: &VectorFst<TropicalWeight>,
    linearize: fn(&str) -> VectorFst<TropicalWeight>,
) -> Result<String> {
    let _ = linearize;
    top_rewrite_direct(input, transducer)
}

/// Direct shortest-path rewrite on a transducer, allowing input-side epsilon moves.
/// Mirrors Pynini `top_rewrite` closely enough for ITN FARs; arcweight's
/// `compose_default` lacks OpenFST's `alt_sequence` filter and admits spurious
/// epsilon paths that skip input bytes.
fn top_rewrite_direct(input: &str, transducer: &VectorFst<TropicalWeight>) -> Result<String> {
    let bytes: Vec<u32> = input.as_bytes().iter().map(|b| u32::from(*b)).collect();
    let start = transducer
        .start()
        .ok_or_else(|| anyhow::anyhow!("transducer has no start state"))?;

    #[derive(Clone)]
    struct Frame {
        state: StateId,
        index: usize,
        output: Vec<u32>,
        weight: TropicalWeight,
    }

    impl Eq for Frame {}
    impl PartialEq for Frame {
        fn eq(&self, other: &Self) -> bool {
            self.weight == other.weight && self.state == other.state && self.index == other.index
        }
    }
    impl PartialOrd for Frame {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Frame {
        fn cmp(&self, other: &Self) -> Ordering {
            self.weight.cmp(&other.weight)
        }
    }

    let mut heap = BinaryHeap::new();
    heap.push(Reverse(Frame {
        state: start,
        index: 0,
        output: Vec::new(),
        weight: TropicalWeight::one(),
    }));
    let mut best: Option<(TropicalWeight, Vec<u32>)> = None;
    let mut best_dist: HashMap<(StateId, usize), TropicalWeight> = HashMap::new();

    while let Some(Reverse(frame)) = heap.pop() {
        let key = (frame.state, frame.index);
        if best_dist.get(&key).is_some_and(|w| frame.weight >= *w) {
            continue;
        }
        best_dist.insert(key, frame.weight);

        if frame.index == bytes.len() && transducer.is_final(frame.state) {
            let total = match transducer.final_weight(frame.state) {
                Some(w) => frame.weight.plus(w),
                None => frame.weight.clone(),
            };
            if best.as_ref().is_none_or(|(w, _)| total < *w) {
                best = Some((total, frame.output.clone()));
            }
        }

        for arc in transducer.arcs(frame.state) {
            let mut next = frame.clone();
            next.weight = next.weight.times(&arc.weight);
            next.state = arc.nextstate;
            if arc.olabel != 0 {
                next.output.push(arc.olabel);
            }

            if arc.ilabel == 0 {
                // input-side epsilon
            } else if frame.index < bytes.len() && arc.ilabel == bytes[frame.index] {
                next.index += 1;
            } else {
                continue;
            }

            let next_key = (next.state, next.index);
            if best_dist.get(&next_key).is_none_or(|w| next.weight < *w) {
                heap.push(Reverse(next));
            }
        }
    }

    let (_, labels) = best.ok_or_else(|| anyhow::anyhow!("no accepting path for input {input:?}"))?;
    labels_to_string(&labels)
}

pub fn load_fst_from_far(far_path: &Path, entry_hint: &str) -> Result<VectorFst<TropicalWeight>> {
    match load_fst_from_arcweight_far(far_path, entry_hint) {
        Ok(fst) => Ok(fst),
        Err(arcweight_err) => load_fst_from_openfst_sequential_far(far_path, entry_hint).with_context(
            || {
                format!(
                    "arcweight FAR loader failed ({arcweight_err}); \
                     OpenFST sequential FAR fallback also failed"
                )
            },
        ),
    }
}

fn load_fst_from_arcweight_far(
    far_path: &Path,
    entry_hint: &str,
) -> Result<VectorFst<TropicalWeight>> {
    let mut reader = open_far(far_path).with_context(|| {
        format!(
            "failed to open arcweight FAR archive at {}",
            far_path.display()
        )
    })?;

    let names: Vec<String> = reader.list().into_iter().cloned().collect();
    let name = names
        .iter()
        .find(|n| n.contains(entry_hint))
        .or_else(|| names.first())
        .with_context(|| format!("empty FAR archive: {}", far_path.display()))?;

    reader
        .read(name)?
        .with_context(|| format!("FST {name} not found in FAR {}", far_path.display()))
}

/// Parse Pynini/OpenFST sequential FAR (header magic + keyed FST blobs).
/// arcweight::open_far uses a different custom index-at-EOF format.
fn load_fst_from_openfst_sequential_far(
    far_path: &Path,
    entry_hint: &str,
) -> Result<VectorFst<TropicalWeight>> {
    let data = std::fs::read(far_path)
        .with_context(|| format!("failed to read FAR file {}", far_path.display()))?;
    let mut cursor = Cursor::new(data);

    let header_magic = read_i32_le(&mut cursor)?;
    if header_magic != OPENFST_FAR_HEADER_MAGIC {
        anyhow::bail!(
            "unexpected OpenFST FAR header magic {header_magic} ({header_magic:#x}), expected {} ({OPENFST_FAR_HEADER_MAGIC:#x}) in {}",
            OPENFST_FAR_HEADER_MAGIC,
            far_path.display()
        );
    }

    let _version = read_i32_le(&mut cursor)?;
    let mut first_fst: Option<VectorFst<TropicalWeight>> = None;

    while (cursor.position() as usize) < cursor.get_ref().len() {
        let key_len = read_i32_le(&mut cursor)?;
        if key_len <= 0 {
            break;
        }

        let key_len = key_len as usize;
        let mut key_bytes = vec![0u8; key_len];
        cursor.read_exact(&mut key_bytes)?;
        if key_bytes.iter().all(|&b| b == 0) {
            break;
        }
        let key = String::from_utf8(key_bytes)
            .with_context(|| format!("invalid UTF-8 FAR key in {}", far_path.display()))?;

        let fst: VectorFst<TropicalWeight> = read_openfst_pynini(&mut cursor)?;

        if key
            .to_ascii_lowercase()
            .contains(&entry_hint.to_ascii_lowercase())
        {
            return Ok(fst);
        }

        if first_fst.is_none() {
            first_fst = Some(fst);
        }
    }

    first_fst.with_context(|| {
            format!(
                "no FAR entry matching {entry_hint:?} in {}",
                far_path.display()
            )
        })
}

/// Read OpenFST vector FST, accepting both arcweight and Pynini magic numbers.
fn read_openfst_pynini<M, Reader>(reader: &mut Reader) -> Result<M>
where
    M: MutableFst<TropicalWeight> + Default,
    Reader: Read,
{
    let magic = read_i32_le(reader)?;
    if magic != OPENFST_FST_MAGIC_PRIMARY && magic != OPENFST_FST_MAGIC_ALT {
        anyhow::bail!("invalid OpenFST FST magic {magic:#x}");
    }

    // Skip fst type and arc type string blobs (length-prefixed).
    for _ in 0..2 {
        let len = read_i32_le(reader)?;
        if len > 0 {
            let mut buf = vec![0u8; len as usize];
            reader.read_exact(&mut buf)?;
        }
    }

    let version = read_i32_le(reader)?;
    let _flags = read_i32_le(reader)?;
    let _properties = read_i64_le(reader)?;
    let start = read_i64_le(reader)?;
    let num_states = read_i64_le(reader)? as usize;
    let _num_arcs = read_i64_le(reader)?;

    let mut fst = M::default();
    for _ in 0..num_states {
        fst.add_state();
    }

    if start >= 0 {
        fst.set_start(start as StateId);
    }

    for state in 0..num_states {
        let final_weight = read_f32_le(reader)?;
        if final_weight != f32::INFINITY {
            fst.set_final(state as StateId, TropicalWeight::new(final_weight));
        }

        let num_arcs = read_i64_le(reader)? as usize;
        for _ in 0..num_arcs {
            let ilabel = read_i32_le(reader)? as u32;
            let olabel = read_i32_le(reader)? as u32;
            let weight = read_f32_le(reader)?;
            let nextstate = read_i32_le(reader)? as u32;
            fst.add_arc(
                state as StateId,
                Arc::new(ilabel, olabel, TropicalWeight::new(weight), nextstate),
            );
        }
    }

    Ok(fst)
}

pub fn resource_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/itn-vi")
}
