use crate::{AppState, dto::LoadedScoreDto, session::ScoreSession};
use serde::Serialize;
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, State};

const MAX_PDF_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CALLBACK_BYTES: u64 = 128 * 1024 * 1024;
const CALLBACK_EXPORT_FLAG: &str = "--audiveris-export";
const CALLBACK_INBOX_FLAG: &str = "--audiveris-inbox";

#[derive(Clone)]
pub struct OmrRuntime {
    resource_root: PathBuf,
    profile_root: PathBuf,
    staging_root: PathBuf,
    inbox_root: PathBuf,
    tapconductor_exe: PathBuf,
}

struct PendingRecognition {
    staging_directory: PathBuf,
    omr_path: PathBuf,
    music_xml_path: PathBuf,
}

#[derive(Clone)]
struct OmrProject {
    omr_path: PathBuf,
    music_xml_path: PathBuf,
    inbox_path: PathBuf,
}

pub struct OmrManager {
    runtime: OmrRuntime,
    pending: HashMap<String, PendingRecognition>,
    current: Option<OmrProject>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OmrRecognitionDto {
    recognition_id: String,
    suggested_file_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OmrProjectDto {
    omr_path: String,
    music_xml_path: String,
}

struct RecognitionFiles {
    id: String,
    suggested_file_name: String,
    staging_directory: PathBuf,
    omr_path: PathBuf,
    music_xml_path: PathBuf,
}

impl OmrManager {
    pub fn new(resource_dir: &Path, app_local_data_dir: &Path) -> Result<Self, String> {
        let tapconductor_exe = std::env::current_exe()
            .map_err(|error| format!("Unable to locate the TapConductor executable: {error}"))?;
        Ok(Self {
            runtime: OmrRuntime {
                resource_root: resource_dir.join("audiveris"),
                profile_root: app_local_data_dir.join("audiveris-profile"),
                staging_root: app_local_data_dir.join("omr-staging"),
                inbox_root: app_local_data_dir.join("omr-inbox"),
                tapconductor_exe,
            },
            pending: HashMap::new(),
            current: None,
        })
    }

    fn register(&mut self, files: RecognitionFiles) -> OmrRecognitionDto {
        let dto = OmrRecognitionDto {
            recognition_id: files.id.clone(),
            suggested_file_name: files.suggested_file_name,
        };
        self.pending.insert(
            files.id,
            PendingRecognition {
                staging_directory: files.staging_directory,
                omr_path: files.omr_path,
                music_xml_path: files.music_xml_path,
            },
        );
        dto
    }

    fn finish(
        &mut self,
        recognition_id: &str,
        destination: PathBuf,
    ) -> Result<OmrProjectDto, String> {
        ensure_extension(&destination, &["mxl"]).map_err(|_| {
            "Save the recognized score as a compressed MusicXML (.mxl) file.".to_owned()
        })?;
        let destination_parent = destination.parent().ok_or_else(|| {
            "The selected MusicXML destination has no parent directory.".to_owned()
        })?;
        if !destination_parent.is_dir() {
            return Err(format!(
                "The destination folder does not exist: {}",
                destination_parent.display()
            ));
        }
        let omr_destination = destination.with_extension("omr");
        if omr_destination.exists() {
            return Err(format!(
                "An Audiveris project already exists at {}. Choose a different MusicXML name so the existing lossless recognition project is not overwritten.",
                omr_destination.display()
            ));
        }
        let pending = self
            .pending
            .remove(recognition_id)
            .ok_or_else(|| "That recognition result is no longer available.".to_owned())?;

        if let Err(error) = copy_file(&pending.omr_path, &omr_destination) {
            self.pending.insert(recognition_id.to_owned(), pending);
            return Err(error);
        }
        if let Err(error) = copy_file(&pending.music_xml_path, &destination) {
            let _ = fs::remove_file(&omr_destination);
            self.pending.insert(recognition_id.to_owned(), pending);
            return Err(error);
        }

        let inbox_path = self.runtime.inbox_root.join(recognition_id);
        fs::create_dir_all(&inbox_path).map_err(|error| {
            format!(
                "Unable to create the Audiveris return inbox {}: {error}",
                inbox_path.display()
            )
        })?;
        self.current = Some(OmrProject {
            omr_path: omr_destination.clone(),
            music_xml_path: destination.clone(),
            inbox_path,
        });
        let _ = fs::remove_dir_all(&pending.staging_directory);

        Ok(OmrProjectDto {
            omr_path: omr_destination.to_string_lossy().into_owned(),
            music_xml_path: destination.to_string_lossy().into_owned(),
        })
    }

    fn discard(&mut self, recognition_id: &str) -> Result<(), String> {
        if let Some(pending) = self.pending.remove(recognition_id) {
            fs::remove_dir_all(&pending.staging_directory).map_err(|error| {
                format!(
                    "Unable to discard the temporary recognition files in {}: {error}",
                    pending.staging_directory.display()
                )
            })?;
        }
        Ok(())
    }

    pub fn select_score(&mut self, path: &Path) {
        if self
            .current
            .as_ref()
            .is_some_and(|project| !paths_equal(&project.music_xml_path, path))
        {
            self.current = None;
        }
    }

    fn launch_editor(&self) -> Result<(), String> {
        let project = self.current.as_ref().ok_or_else(|| {
            "The current score does not have an Audiveris recognition project.".to_owned()
        })?;
        self.runtime.prepare_profile(project)?;
        let launcher = self.runtime.find_launcher()?;
        Command::new(&launcher)
            .env("APPDATA", &self.runtime.profile_root)
            .arg("--")
            .arg(&project.omr_path)
            .spawn()
            .map_err(|error| format!("Unable to launch {}: {error}", launcher.display()))?;
        Ok(())
    }

    fn take_returned_export(&self) -> Result<Option<PathBuf>, String> {
        let Some(project) = &self.current else {
            return Ok(None);
        };
        if !project.inbox_path.exists() {
            return Ok(None);
        }
        let callback_error = project.inbox_path.join("callback-error.txt");
        if callback_error.is_file() {
            let message = fs::read_to_string(&callback_error).unwrap_or_else(|_| {
                "Audiveris could not return its corrected MusicXML export.".to_owned()
            });
            let _ = fs::remove_file(callback_error);
            return Err(message);
        }
        let mut exports = fs::read_dir(&project.inbox_path)
            .map_err(|error| format!("Unable to inspect the Audiveris return inbox: {error}"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| has_extension(path, &["mxl", "xml", "musicxml"]))
            .collect::<Vec<_>>();
        exports.sort();
        Ok(exports.into_iter().next())
    }

    fn current_destination(&self) -> Option<PathBuf> {
        self.current
            .as_ref()
            .map(|project| project.music_xml_path.clone())
    }

    fn project_for_score(&mut self, music_xml_path: PathBuf) -> Option<OmrProjectDto> {
        if let Some(project) = &self.current
            && paths_equal(&project.music_xml_path, &music_xml_path)
        {
            return Some(project_dto(project));
        }
        if !has_extension(&music_xml_path, &["mxl", "xml", "musicxml"]) {
            return None;
        }
        let omr_path = music_xml_path.with_extension("omr");
        if !omr_path.is_file() {
            return None;
        }
        let inbox_path = self.runtime.inbox_root.join(unique_id("project"));
        let project = OmrProject {
            omr_path,
            music_xml_path,
            inbox_path,
        };
        let dto = project_dto(&project);
        self.current = Some(project);
        Some(dto)
    }
}

impl OmrRuntime {
    fn recognize_pdf(&self, source_pdf: PathBuf) -> Result<RecognitionFiles, String> {
        if !cfg!(target_os = "windows") {
            return Err(
                "Optical music recognition is currently supported only on Windows.".to_owned(),
            );
        }
        ensure_extension(&source_pdf, &["pdf"])
            .map_err(|_| "Audiveris recognition currently accepts scanned PDF files.".to_owned())?;
        let metadata = fs::metadata(&source_pdf)
            .map_err(|error| format!("Unable to inspect {}: {error}", source_pdf.display()))?;
        if !metadata.is_file() {
            return Err(format!("{} is not a file.", source_pdf.display()));
        }
        if metadata.len() > MAX_PDF_BYTES {
            return Err(format!(
                "The selected PDF is {} bytes; the recognition limit is {} bytes.",
                metadata.len(),
                MAX_PDF_BYTES
            ));
        }

        let id = unique_id("recognition");
        let staging_directory = self.staging_root.join(&id);
        fs::create_dir_all(&staging_directory).map_err(|error| {
            format!(
                "Unable to create recognition workspace {}: {error}",
                staging_directory.display()
            )
        })?;
        self.prepare_profile_seed()?;
        let launcher = self.find_launcher()?;
        let mut command = Command::new(&launcher);
        command
            .env("APPDATA", &self.profile_root)
            .arg("-batch")
            .arg("-transcribe")
            .arg("-save")
            .arg("-export")
            .arg("-output")
            .arg(&staging_directory)
            .arg("--")
            .arg(&source_pdf);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let output = command
            .output()
            .map_err(|error| format!("Unable to start {}: {error}", launcher.display()))?;
        if !output.status.success() {
            let detail = command_failure_detail(&output.stdout, &output.stderr);
            let _ = fs::remove_dir_all(&staging_directory);
            return Err(format!(
                "Audiveris recognition failed with status {}.{detail}",
                output.status
            ));
        }

        let omr_path = one_output(&staging_directory, &["omr"], "Audiveris project")?;
        let music_xml_path = one_output(
            &staging_directory,
            &["mxl", "musicxml", "xml"],
            "MusicXML export",
        )?;
        let suggested_file_name = source_pdf
            .file_stem()
            .and_then(OsStr::to_str)
            .filter(|stem| !stem.is_empty())
            .map(|stem| format!("{stem}.mxl"))
            .unwrap_or_else(|| "recognized-score.mxl".to_owned());
        Ok(RecognitionFiles {
            id,
            suggested_file_name,
            staging_directory,
            omr_path,
            music_xml_path,
        })
    }

    fn prepare_profile_seed(&self) -> Result<(), String> {
        let bundled_profile = self.resource_root.join("profile");
        if bundled_profile.is_dir() {
            copy_directory_missing(&bundled_profile, &self.profile_root)?;
        }
        let config = audiveris_config_directory(&self.profile_root);
        fs::create_dir_all(&config).map_err(|error| {
            format!(
                "Unable to create private Audiveris configuration {}: {error}",
                config.display()
            )
        })
    }

    fn prepare_profile(&self, project: &OmrProject) -> Result<(), String> {
        self.prepare_profile_seed()?;
        fs::create_dir_all(&project.inbox_path).map_err(|error| {
            format!(
                "Unable to create Audiveris return inbox {}: {error}",
                project.inbox_path.display()
            )
        })?;
        let plugin_path = audiveris_config_directory(&self.profile_root).join("plugins.xml");
        let xml = plugin_xml(&self.tapconductor_exe, &project.inbox_path);
        fs::write(&plugin_path, xml).map_err(|error| {
            format!(
                "Unable to configure the Audiveris export plugin {}: {error}",
                plugin_path.display()
            )
        })
    }

    fn find_launcher(&self) -> Result<PathBuf, String> {
        if let Some(configured) = std::env::var_os("TAPCONDUCTOR_AUDIVERIS_EXE") {
            let configured = PathBuf::from(configured);
            if configured.is_file() {
                return Ok(configured);
            }
            return Err(format!(
                "TAPCONDUCTOR_AUDIVERIS_EXE does not name a file: {}",
                configured.display()
            ));
        }
        let mut matches = find_files(&self.resource_root, 6, |path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.eq_ignore_ascii_case("Audiveris.exe"))
        })?;
        matches.sort_by_key(|path| path.components().count());
        matches.into_iter().next().ok_or_else(|| {
            format!(
                "The bundled Audiveris runtime is missing from {}. Reinstall TapConductor with the Windows OMR component.",
                self.resource_root.display()
            )
        })
    }
}

#[tauri::command]
pub async fn recognize_pdf(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<OmrRecognitionDto, String> {
    let runtime = state
        .omr
        .lock()
        .map_err(|_| "TapConductor's OMR state is unavailable.".to_owned())?
        .runtime
        .clone();
    let files = tauri::async_runtime::spawn_blocking(move || runtime.recognize_pdf(path.into()))
        .await
        .map_err(|error| {
            format!("The Audiveris recognition worker stopped unexpectedly: {error}")
        })??;
    Ok(state
        .omr
        .lock()
        .map_err(|_| "TapConductor's OMR state is unavailable.".to_owned())?
        .register(files))
}

#[tauri::command]
pub fn omr_available() -> bool {
    cfg!(target_os = "windows")
}

#[tauri::command]
pub fn finish_omr_recognition(
    state: State<'_, Arc<AppState>>,
    recognition_id: String,
    music_xml_path: String,
) -> Result<OmrProjectDto, String> {
    state
        .omr
        .lock()
        .map_err(|_| "TapConductor's OMR state is unavailable.".to_owned())?
        .finish(&recognition_id, music_xml_path.into())
}

#[tauri::command]
pub fn discard_omr_recognition(
    state: State<'_, Arc<AppState>>,
    recognition_id: String,
) -> Result<(), String> {
    state
        .omr
        .lock()
        .map_err(|_| "TapConductor's OMR state is unavailable.".to_owned())?
        .discard(&recognition_id)
}

#[tauri::command]
pub fn review_recognition(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .omr
        .lock()
        .map_err(|_| "TapConductor's OMR state is unavailable.".to_owned())?
        .launch_editor()
}

#[tauri::command]
pub fn omr_project_for_score(
    state: State<'_, Arc<AppState>>,
    music_xml_path: String,
) -> Result<Option<OmrProjectDto>, String> {
    Ok(state
        .omr
        .lock()
        .map_err(|_| "TapConductor's OMR state is unavailable.".to_owned())?
        .project_for_score(music_xml_path.into()))
}

#[tauri::command]
pub fn poll_omr_export(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<LoadedScoreDto>, String> {
    let (returned_export, destination) = {
        let omr = state
            .omr
            .lock()
            .map_err(|_| "TapConductor's OMR state is unavailable.".to_owned())?;
        (omr.take_returned_export()?, omr.current_destination())
    };
    let (Some(returned_export), Some(destination)) = (returned_export, destination) else {
        return Ok(None);
    };

    if let Err(error) = ScoreSession::load(returned_export.clone(), 0) {
        quarantine_callback(&returned_export);
        return Err(format!(
            "Audiveris returned MusicXML that TapConductor could not validate: {error}"
        ));
    }
    copy_file(&returned_export, &destination)?;
    let (score, event) = state
        .core
        .lock()
        .map_err(|_| "TapConductor's native state is unavailable.".to_owned())?
        .load_score(destination)?;
    if let Some(event) = event {
        use tauri::Emitter;
        app.emit("performance-event", event)
            .map_err(|error| error.to_string())?;
    }
    fs::remove_file(&returned_export).map_err(|error| {
        format!("The corrected score loaded, but its inbox copy could not be removed: {error}")
    })?;
    Ok(Some(score))
}

pub fn handle_export_callback_arguments(args: impl IntoIterator<Item = OsString>) -> bool {
    let args = args.into_iter().collect::<Vec<_>>();
    let Some(export_index) = argument_index(&args, CALLBACK_EXPORT_FLAG) else {
        return false;
    };
    let result = (|| {
        let source = args
            .get(export_index + 1)
            .map(PathBuf::from)
            .ok_or_else(|| "The Audiveris callback did not include an export path.".to_owned())?;
        let inbox_index = argument_index(&args, CALLBACK_INBOX_FLAG).ok_or_else(|| {
            "The Audiveris callback did not include a TapConductor inbox.".to_owned()
        })?;
        let inbox = args
            .get(inbox_index + 1)
            .map(PathBuf::from)
            .ok_or_else(|| "The Audiveris callback inbox path is missing.".to_owned())?;
        deliver_export_to_inbox(&source, &inbox)
    })();
    if let Err(error) = result
        && let Some(inbox_index) = argument_index(&args, CALLBACK_INBOX_FLAG)
        && let Some(inbox) = args.get(inbox_index + 1)
    {
        let _ = fs::create_dir_all(inbox);
        let _ = fs::write(PathBuf::from(inbox).join("callback-error.txt"), error);
    }
    true
}

fn deliver_export_to_inbox(source: &Path, inbox: &Path) -> Result<(), String> {
    ensure_extension(source, &["mxl", "xml", "musicxml"])
        .map_err(|_| "Audiveris returned an unsupported export type.".to_owned())?;
    let metadata = fs::metadata(source).map_err(|error| {
        format!(
            "Unable to inspect Audiveris export {}: {error}",
            source.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() > MAX_CALLBACK_BYTES {
        return Err("The Audiveris export is not a valid, bounded file.".to_owned());
    }
    fs::create_dir_all(inbox)
        .map_err(|error| format!("Unable to create TapConductor callback inbox: {error}"))?;
    let extension = source
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("mxl")
        .to_ascii_lowercase();
    let base = unique_id("export");
    let temporary = inbox.join(format!(".{base}.tmp"));
    let destination = inbox.join(format!("{base}.{extension}"));
    copy_file(source, &temporary)?;
    fs::rename(&temporary, &destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("Unable to publish the corrected Audiveris export: {error}")
    })
}

fn argument_index(args: &[OsString], expected: &str) -> Option<usize> {
    args.iter().position(|argument| argument == expected)
}

fn plugin_xml(tapconductor_exe: &Path, inbox: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plugins>\n  <plugin id=\"Send to TapConductor\" tip=\"Export corrected MusicXML to the open TapConductor project\">\n    <arg>{}</arg>\n    <arg>{CALLBACK_EXPORT_FLAG}</arg>\n    <arg>{{}}</arg>\n    <arg>{CALLBACK_INBOX_FLAG}</arg>\n    <arg>{}</arg>\n  </plugin>\n</plugins>\n",
        xml_escape(&tapconductor_exe.to_string_lossy()),
        xml_escape(&inbox.to_string_lossy())
    )
}

fn audiveris_config_directory(profile_root: &Path) -> PathBuf {
    profile_root
        .join("AudiverisLtd")
        .join("audiveris")
        .join("config")
}

fn project_dto(project: &OmrProject) -> OmrProjectDto {
    OmrProjectDto {
        omr_path: project.omr_path.to_string_lossy().into_owned(),
        music_xml_path: project.music_xml_path.to_string_lossy().into_owned(),
    }
}

fn one_output(directory: &Path, extensions: &[&str], label: &str) -> Result<PathBuf, String> {
    let mut outputs = find_files(directory, 5, |path| has_extension(path, extensions))?;
    outputs.sort();
    match outputs.as_slice() {
        [path] => Ok(path.clone()),
        [] => Err(format!(
            "Audiveris completed without creating a {label} in {}.",
            directory.display()
        )),
        _ => Err(format!(
            "Audiveris created multiple {label} files; TapConductor currently requires one score per PDF."
        )),
    }
}

fn find_files(
    root: &Path,
    remaining_depth: usize,
    predicate: impl Fn(&Path) -> bool + Copy,
) -> Result<Vec<PathBuf>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(root)
        .map_err(|error| format!("Unable to inspect {}: {error}", root.display()))?
    {
        let entry = entry.map_err(|error| format!("Unable to inspect an output file: {error}"))?;
        let path = entry.path();
        if path.is_file() && predicate(&path) {
            files.push(path);
        } else if remaining_depth > 0 && path.is_dir() {
            files.extend(find_files(&path, remaining_depth - 1, predicate)?);
        }
    }
    Ok(files)
}

fn copy_directory_missing(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("Unable to create {}: {error}", destination.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("Unable to inspect {}: {error}", source.display()))?
    {
        let entry =
            entry.map_err(|error| format!("Unable to inspect bundled OMR data: {error}"))?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory_missing(&entry.path(), &target)?;
        } else if !target.exists() {
            copy_file(&entry.path(), &target)?;
        }
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::copy(source, destination).map(|_| ()).map_err(|error| {
        format!(
            "Unable to copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })
}

fn quarantine_callback(path: &Path) {
    let failed = path.with_extension(format!(
        "{}.failed",
        path.extension().and_then(OsStr::to_str).unwrap_or("mxl")
    ));
    let _ = fs::rename(path, failed);
}

fn command_failure_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );
    let trimmed = text.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        let tail = trimmed.chars().rev().take(4_000).collect::<String>();
        format!("\n{}", tail.chars().rev().collect::<String>())
    }
}

fn ensure_extension(path: &Path, allowed: &[&str]) -> Result<(), ()> {
    if has_extension(path, allowed) {
        Ok(())
    } else {
        Err(())
    }
}

fn has_extension(path: &Path, allowed: &[&str]) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            allowed
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        })
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn unique_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{nanos}-{}", std::process::id())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(unique_id(name));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn plugin_uses_an_escaped_executable_and_project_inbox() {
        let xml = plugin_xml(
            Path::new(r"C:\Program Files\Tap & Conductor\TapConductor.exe"),
            Path::new(r"C:\Users\A&B\inbox"),
        );
        assert!(xml.contains("Tap &amp; Conductor"));
        assert!(xml.contains("A&amp;B"));
        assert!(xml.contains("--audiveris-export"));
        assert!(xml.contains("<arg>{}</arg>"));
    }

    #[test]
    fn callback_delivery_is_atomic_and_preserves_the_extension() {
        let root = test_directory("tapconductor-omr-callback");
        let source = root.join("corrected.mxl");
        let inbox = root.join("inbox");
        fs::write(&source, b"recognized score").unwrap();

        deliver_export_to_inbox(&source, &inbox).unwrap();

        let entries = fs::read_dir(&inbox)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].extension().unwrap(), "mxl");
        assert_eq!(fs::read(&entries[0]).unwrap(), b"recognized score");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn output_discovery_rejects_ambiguous_exports() {
        let root = test_directory("tapconductor-omr-outputs");
        fs::write(root.join("score.mxl"), b"one").unwrap();
        assert_eq!(
            one_output(&root, &["mxl"], "MusicXML export").unwrap(),
            root.join("score.mxl")
        );
        fs::write(root.join("movement-2.mxl"), b"two").unwrap();
        assert!(one_output(&root, &["mxl"], "MusicXML export").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sibling_omr_is_reassociated_when_musicxml_is_reopened() {
        let root = test_directory("tapconductor-omr-reassociate");
        let score = root.join("score.mxl");
        let project = root.join("score.omr");
        fs::write(&score, b"music xml").unwrap();
        fs::write(&project, b"omr project").unwrap();
        let mut manager = OmrManager {
            runtime: OmrRuntime {
                resource_root: root.join("resources"),
                profile_root: root.join("profile"),
                staging_root: root.join("staging"),
                inbox_root: root.join("inbox"),
                tapconductor_exe: root.join("TapConductor.exe"),
            },
            pending: HashMap::new(),
            current: None,
        };

        let associated = manager.project_for_score(score.clone()).unwrap();

        assert_eq!(associated.music_xml_path, score.to_string_lossy());
        assert_eq!(associated.omr_path, project.to_string_lossy());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finishing_never_overwrites_an_existing_omr_project() {
        let root = test_directory("tapconductor-omr-no-overwrite");
        let staging = root.join("staging");
        fs::create_dir_all(&staging).unwrap();
        let staged_omr = staging.join("score.omr");
        let staged_mxl = staging.join("score.mxl");
        fs::write(&staged_omr, b"new project").unwrap();
        fs::write(&staged_mxl, b"new export").unwrap();
        let destination = root.join("saved.mxl");
        let existing_project = root.join("saved.omr");
        fs::write(&existing_project, b"keep me").unwrap();
        let mut manager = OmrManager {
            runtime: OmrRuntime {
                resource_root: root.join("resources"),
                profile_root: root.join("profile"),
                staging_root: root.join("staging-root"),
                inbox_root: root.join("inbox"),
                tapconductor_exe: root.join("TapConductor.exe"),
            },
            pending: HashMap::from([(
                "pending".to_owned(),
                PendingRecognition {
                    staging_directory: staging,
                    omr_path: staged_omr,
                    music_xml_path: staged_mxl,
                },
            )]),
            current: None,
        };

        assert!(manager.finish("pending", destination).is_err());
        assert_eq!(fs::read(existing_project).unwrap(), b"keep me");
        assert!(manager.pending.contains_key("pending"));
        fs::remove_dir_all(root).unwrap();
    }
}
