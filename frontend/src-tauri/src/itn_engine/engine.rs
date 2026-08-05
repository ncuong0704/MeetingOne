use anyhow::{Context, Result};
use arcweight::fst::VectorFst;
use arcweight::prelude::*;
use log::warn;
use std::path::Path;
use std::sync::Mutex;

use crate::config::{ITN_CLASSIFY_FAR, ITN_VERBALIZE_FAR};
use crate::itn_engine::rewrite::{load_fst_from_far, top_rewrite};

pub struct ItnEngine {
    classifier: VectorFst<TropicalWeight>,
    verbalizer: VectorFst<TropicalWeight>,
}

impl ItnEngine {
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let classify_path = dir.join(ITN_CLASSIFY_FAR);
        let verbalize_path = dir.join(ITN_VERBALIZE_FAR);
        let classifier = load_fst_from_far(&classify_path, "tokenize").with_context(|| {
            format!("load classify FAR from {}", classify_path.display())
        })?;
        let verbalizer = load_fst_from_far(&verbalize_path, "verbalize").with_context(|| {
            format!("load verbalize FAR from {}", verbalize_path.display())
        })?;
        Ok(Self {
            classifier,
            verbalizer,
        })
    }

    pub fn inverse_normalize(&self, text: &str) -> Result<String> {
        let token = top_rewrite(text, &self.classifier)?;
        top_rewrite(&token, &self.verbalizer)
    }
}

pub(crate) static ITN_ENGINE: Mutex<Option<ItnEngine>> = Mutex::new(None);

pub fn inverse_normalize_or_pass(text: &str) -> String {
    let guard = ITN_ENGINE.lock().unwrap();
    match guard.as_ref() {
        Some(engine) => match engine.inverse_normalize(text) {
            Ok(s) => s,
            Err(e) => {
                warn!("ITN rewrite failed: {}, passing through", e);
                text.to_string()
            }
        },
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itn_engine::rewrite::resource_dir;

    fn engine() -> ItnEngine {
        ItnEngine::load_from_dir(&resource_dir()).expect("run fetch-itn-far.ps1 first")
    }

    #[test]
    #[ignore]
    fn test_decimal() {
        let e = engine();
        assert_eq!(e.inverse_normalize("một phẩy hai ba").unwrap(), "1,23");
        assert_eq!(e.inverse_normalize("không phẩy một").unwrap(), "0,1");
    }

    #[test]
    #[ignore]
    fn test_money() {
        let e = engine();
        assert_eq!(e.inverse_normalize("một nghìn đồng").unwrap(), "1.000₫");
    }

    #[test]
    #[ignore]
    fn test_cardinal() {
        let e = engine();
        assert_eq!(e.inverse_normalize("âm hai").unwrap(), "-2");
        assert_eq!(e.inverse_normalize("một trăm").unwrap(), "100");
    }

    #[test]
    #[ignore]
    fn test_time() {
        let e = engine();
        assert_eq!(e.inverse_normalize("hai giờ rưỡi").unwrap(), "02h30");
    }

    #[test]
    #[ignore]
    fn test_date() {
        let e = engine();
        assert_eq!(
            e.inverse_normalize("ngày mồng chín tháng tám").unwrap(),
            "ngày 09/08"
        );
    }
}
