// Copyright (c) 2026 Michael Saunders
use std::{collections::VecDeque, sync::Mutex};

#[cfg(any(target_os = "ios", test))]
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "ios", test))]
use tauri::Url;

#[cfg(any(target_os = "ios", test))]
const SUPPORTED_SCORE_EXTENSIONS: &[&str] = &["musicxml", "xml", "mxl", "mid", "midi"];

#[derive(Default)]
pub struct OpenedScores {
    pending: Mutex<VecDeque<String>>,
}

impl OpenedScores {
    #[cfg(any(target_os = "ios", test))]
    pub fn enqueue_urls(&self, urls: Vec<Url>) -> Result<usize, String> {
        let paths = urls
            .into_iter()
            .filter_map(|url| score_path_from_url(&url))
            .filter_map(|path| path.into_os_string().into_string().ok())
            .collect::<Vec<_>>();
        let count = paths.len();
        self.pending
            .lock()
            .map_err(|_| "TapConductor's pending-file state was poisoned.".to_owned())?
            .extend(paths);
        Ok(count)
    }

    pub fn take(&self) -> Result<Vec<String>, String> {
        Ok(self
            .pending
            .lock()
            .map_err(|_| "TapConductor's pending-file state was poisoned.".to_owned())?
            .drain(..)
            .collect())
    }
}

#[cfg(any(target_os = "ios", test))]
fn score_path_from_url(url: &Url) -> Option<PathBuf> {
    if url.scheme() != "file" {
        return None;
    }
    let path = url.to_file_path().ok()?;
    supported_score_path(&path).then_some(path)
}

#[cfg(any(target_os = "ios", test))]
fn supported_score_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SUPPORTED_SCORE_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_local_score_urls() {
        for extension in SUPPORTED_SCORE_EXTENSIONS {
            let url = Url::parse(&format!("file:///private/tmp/Score.{extension}")).unwrap();
            assert_eq!(
                score_path_from_url(&url),
                Some(PathBuf::from(format!("/private/tmp/Score.{extension}")))
            );
        }
        assert!(supported_score_path(Path::new("Upper.MUSICXML")));
    }

    #[test]
    fn rejects_remote_and_unsupported_urls() {
        assert!(
            score_path_from_url(&Url::parse("https://example.com/score.mxl").unwrap()).is_none()
        );
        assert!(
            score_path_from_url(&Url::parse("file:///private/tmp/score.pdf").unwrap()).is_none()
        );
    }

    #[test]
    fn pending_urls_are_drained_once() {
        let state = OpenedScores::default();
        assert_eq!(
            state
                .enqueue_urls(vec![
                    Url::parse("file:///private/tmp/one.mxl").unwrap(),
                    Url::parse("file:///private/tmp/two.mid").unwrap(),
                ])
                .unwrap(),
            2
        );
        assert_eq!(
            state.take().unwrap(),
            vec!["/private/tmp/one.mxl", "/private/tmp/two.mid"]
        );
        assert!(state.take().unwrap().is_empty());
    }
}
