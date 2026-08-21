// Copyright (c) 2026 Michael Saunders
use std::{collections::VecDeque, sync::Mutex};

#[cfg(target_os = "ios")]
use objc2_foundation::NSURL;
#[cfg(any(target_os = "windows", test))]
use std::ffi::OsString;
#[cfg(any(target_os = "ios", target_os = "macos", target_os = "windows", test))]
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "ios", test))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(any(target_os = "ios", target_os = "macos", target_os = "windows", test))]
use tauri::Url;

#[cfg(any(target_os = "ios", target_os = "macos", target_os = "windows", test))]
const SUPPORTED_SCORE_EXTENSIONS: &[&str] = &["musicxml", "xml", "mxl", "mid", "midi"];
#[cfg(any(target_os = "ios", test))]
static IMPORT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub struct OpenedScores {
    pending: Mutex<VecDeque<String>>,
}

impl OpenedScores {
    #[cfg(any(target_os = "ios", target_os = "macos", test))]
    pub fn enqueue_urls(&self, urls: Vec<Url>) -> Result<usize, String> {
        let paths = urls
            .into_iter()
            .filter_map(|url| score_path_from_url(&url))
            .collect::<Vec<_>>();
        self.enqueue_paths(paths)
    }

    #[cfg(target_os = "ios")]
    pub fn enqueue_ios_urls(
        &self,
        urls: Vec<Url>,
        import_directory: &Path,
    ) -> Result<usize, String> {
        std::fs::create_dir_all(import_directory).map_err(|error| {
            format!("TapConductor could not prepare its imported-score directory: {error}")
        })?;
        let mut imported = Vec::new();
        for source in urls.iter().filter_map(score_path_from_url) {
            let foundation_url = NSURL::from_file_path(&source).ok_or_else(|| {
                format!(
                    "TapConductor could not access the selected score: {}",
                    source.display()
                )
            })?;
            // SAFETY: The NSURL is retained for the full balanced access scope,
            // and the selected file is copied synchronously before access ends.
            let scoped = unsafe { foundation_url.startAccessingSecurityScopedResource() };
            let result = import_score_path(&source, import_directory);
            if scoped {
                // SAFETY: This balances the successful start call above.
                unsafe { foundation_url.stopAccessingSecurityScopedResource() };
            }
            imported.push(result?);
        }
        self.enqueue_paths(imported)
    }

    #[cfg(any(target_os = "windows", test))]
    pub fn enqueue_arguments<I>(&self, arguments: I) -> Result<usize, String>
    where
        I: IntoIterator<Item = OsString>,
    {
        let paths = arguments
            .into_iter()
            .filter_map(score_path_from_argument)
            .collect::<Vec<_>>();
        self.enqueue_paths(paths)
    }

    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "windows", test))]
    fn enqueue_paths<I>(&self, paths: I) -> Result<usize, String>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let paths = paths
            .into_iter()
            .filter(|path| supported_score_path(path))
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

#[cfg(any(target_os = "ios", target_os = "macos", target_os = "windows", test))]
fn score_path_from_url(url: &Url) -> Option<PathBuf> {
    if url.scheme() != "file" {
        return None;
    }
    let path = url.to_file_path().ok()?;
    supported_score_path(&path).then_some(path)
}

#[cfg(any(target_os = "ios", target_os = "macos", target_os = "windows", test))]
fn supported_score_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SUPPORTED_SCORE_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

#[cfg(any(target_os = "ios", test))]
fn import_score_path(source: &Path, import_directory: &Path) -> Result<PathBuf, String> {
    if !supported_score_path(source) {
        return Err(format!(
            "TapConductor does not support this file: {}",
            source.display()
        ));
    }
    let file_name = source.file_name().ok_or_else(|| {
        format!(
            "The selected score does not have a file name: {}",
            source.display()
        )
    })?;
    let sequence = IMPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let destination = import_directory.join(format!(
        "{}-{sequence}-{}",
        std::process::id(),
        file_name.to_string_lossy()
    ));
    std::fs::copy(source, &destination).map_err(|error| {
        format!(
            "TapConductor could not import {}: {error}",
            source.display()
        )
    })?;
    Ok(destination)
}

#[cfg(any(target_os = "windows", test))]
fn score_path_from_argument(argument: OsString) -> Option<PathBuf> {
    let text = argument.to_str()?;
    if text.starts_with('-') {
        return None;
    }
    if text.starts_with("file:") {
        return Url::parse(text)
            .ok()
            .and_then(|url| score_path_from_url(&url));
    }
    if text.contains("://") {
        return None;
    }
    let path = PathBuf::from(argument);
    supported_score_path(&path).then_some(path)
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

    #[test]
    fn windows_style_startup_arguments_only_queue_supported_local_scores() {
        let state = OpenedScores::default();
        assert_eq!(
            state
                .enqueue_arguments([
                    OsString::from("--flag"),
                    OsString::from("C:\\Scores\\one.musicxml"),
                    OsString::from("file:///C:/Scores/two.MXL"),
                    OsString::from("https://example.com/remote.mid"),
                    OsString::from("C:\\Scores\\notes.txt"),
                ])
                .unwrap(),
            2
        );
        assert_eq!(
            state.take().unwrap(),
            vec!["C:\\Scores\\one.musicxml", "/C:/Scores/two.MXL"]
        );
    }

    #[test]
    fn ios_style_import_copies_a_supported_score_before_security_access_ends() {
        let root = std::env::temp_dir().join(format!(
            "tapconductor-opened-score-test-{}-{}",
            std::process::id(),
            IMPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let source_directory = root.join("provider");
        let import_directory = root.join("imported");
        std::fs::create_dir_all(&source_directory).unwrap();
        std::fs::create_dir_all(&import_directory).unwrap();
        let source = source_directory.join("score.mxl");
        std::fs::write(&source, b"MusicXML test archive").unwrap();

        let imported = import_score_path(&source, &import_directory).unwrap();
        assert_ne!(imported, source);
        assert_eq!(std::fs::read(imported).unwrap(), b"MusicXML test archive");

        std::fs::remove_dir_all(root).unwrap();
    }
}
