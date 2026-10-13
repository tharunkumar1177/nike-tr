//! The component library cache. The pinned index is downloaded only after the user agrees,
//! verified against its pinned SHA-256, installed atomically, and searched offline after that.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use flate2::read::GzDecoder;
use reqwest::StatusCode;
use reqwest::header::{CONTENT_RANGE, RANGE};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use tron_library::preview::{FootprintPreview, SymbolPreview};
use tron_library::{
    ATTRIBUTION, FootprintRecord, Index, ItemKind, LibraryError, META_ATTRIBUTION, META_KICAD_TAG,
    Mount, PinnedIndex, SymbolRecord,
};
use ts_rs::TS;

use crate::error::{AppError, AppErrorKind};
use crate::settings::{self, Settings};

pub const LIBRARY_STATUS_EVENT: &str = "library-status";
pub const MAX_SEARCH_RESULTS: u32 = 200;

const CACHE_DIR: &str = "library";
const INDEX_FILE: &str = "library.db";
/// Holds the pinned SHA-256 the cached index was installed from.
const INDEX_CHECKSUM_FILE: &str = "library.db.sha256";
const UNPACKED_FILE: &str = "library.db.unpacked";
const PARTIAL_SUFFIX: &str = ".gz.partial";
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const READ_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LibraryConsent {
    Unasked,
    Declined,
    Accepted,
}

impl LibraryConsent {
    fn from_setting(setting: Option<bool>) -> LibraryConsent {
        match setting {
            None => LibraryConsent::Unasked,
            Some(false) => LibraryConsent::Declined,
            Some(true) => LibraryConsent::Accepted,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MissingLibrary {
    pub kicad_tag: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    /// Bytes kept from an interrupted download; the next download resumes after them.
    #[ts(type = "number")]
    pub resume_bytes: u64,
    pub consent: LibraryConsent,
    /// Why the last download failed, if it did.
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DownloadingLibrary {
    pub kicad_tag: String,
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstallingLibrary {
    pub kicad_tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReadyLibrary {
    pub kicad_tag: String,
    pub symbol_count: usize,
    pub footprint_count: usize,
    /// Must be shown wherever the library is browsed.
    pub attribution: String,
}

/// Sent as the payload of [`LIBRARY_STATUS_EVENT`] on every change, including download
/// progress at most every 100 ms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "camelCase")]
#[ts(export)]
pub enum LibraryStatus {
    Missing(MissingLibrary),
    Downloading(DownloadingLibrary),
    /// Verifying the checksum and unpacking the index.
    Installing(InstallingLibrary),
    Ready(ReadyLibrary),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LibraryItemKind {
    Symbol,
    Footprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryHit {
    pub kind: LibraryItemKind,
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub lib: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibrarySymbol {
    pub lib: String,
    pub name: String,
    pub description: String,
    pub keywords: String,
    /// A URL, or empty.
    pub datasheet: String,
    pub reference: String,
    /// The default footprint as `Library:Footprint`, or empty.
    pub footprint: String,
    pub unit_count: u32,
    pub pin_count: u32,
    pub power: bool,
    pub preview: SymbolPreview,
}

impl From<SymbolRecord> for LibrarySymbol {
    fn from(record: SymbolRecord) -> LibrarySymbol {
        LibrarySymbol {
            lib: record.lib,
            name: record.name,
            description: record.description,
            keywords: record.keywords,
            datasheet: record.datasheet,
            reference: record.reference,
            footprint: record.footprint,
            unit_count: record.unit_count,
            pin_count: record.pin_count,
            power: record.power,
            preview: record.preview,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FootprintMount {
    Smd,
    ThroughHole,
    Unspecified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryFootprint {
    pub lib: String,
    pub name: String,
    pub description: String,
    pub tags: String,
    pub pad_count: u32,
    pub mount: FootprintMount,
    pub preview: FootprintPreview,
}

impl From<FootprintRecord> for LibraryFootprint {
    fn from(record: FootprintRecord) -> LibraryFootprint {
        LibraryFootprint {
            lib: record.lib,
            name: record.name,
            description: record.description,
            tags: record.tags,
            pad_count: record.pad_count,
            mount: match record.mount {
                Mount::Smd => FootprintMount::Smd,
                Mount::ThroughHole => FootprintMount::ThroughHole,
                Mount::Unspecified => FootprintMount::Unspecified,
            },
            preview: record.preview,
        }
    }
}

struct Cached {
    index: Index,
    info: ReadyLibrary,
}

pub struct LibraryState {
    inner: Mutex<Inner>,
}

struct Inner {
    pin: PinnedIndex,
    /// `None` when the OS gave no app data folder; downloading then reports an error.
    dir: Option<PathBuf>,
    consent: LibraryConsent,
    cache: Option<Cached>,
    status: LibraryStatus,
    download: Option<CancellationToken>,
}

impl LibraryState {
    /// Reads the consent and opens the cached index. Makes no network calls.
    pub fn load(app: &AppHandle) -> LibraryState {
        let pin = PinnedIndex::current();
        let consent = match settings::settings_path(app).and_then(|path| Settings::read(&path)) {
            Ok(settings) => LibraryConsent::from_setting(settings.library_download),
            Err(error) => {
                eprintln!(
                    "could not read the library download consent: {}",
                    error.message
                );
                LibraryConsent::Unasked
            }
        };
        let dir = app
            .path()
            .app_local_data_dir()
            .ok()
            .map(|dir| dir.join(CACHE_DIR));
        let cache = dir.as_deref().and_then(|dir| open_cache(dir, &pin));
        let mut inner = Inner {
            pin,
            dir,
            consent,
            cache,
            status: LibraryStatus::Installing(InstallingLibrary {
                kicad_tag: String::new(),
            }),
            download: None,
        };
        inner.status = inner.settled_status(None);
        LibraryState {
            inner: Mutex::new(inner),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Inner {
    fn settled_status(&self, error: Option<AppError>) -> LibraryStatus {
        match &self.cache {
            Some(cached) => LibraryStatus::Ready(cached.info.clone()),
            None => LibraryStatus::Missing(MissingLibrary {
                kicad_tag: self.pin.kicad_tag.clone(),
                size_bytes: self.pin.size,
                resume_bytes: self
                    .dir
                    .as_deref()
                    .map_or(0, |dir| partial_len(dir, &self.pin)),
                consent: self.consent,
                error,
            }),
        }
    }

    fn set_status(&mut self, app: &AppHandle, status: LibraryStatus) {
        self.status = status;
        if let Err(error) = app.emit(LIBRARY_STATUS_EVENT, &self.status) {
            eprintln!("could not report the library status: {error}");
        }
    }

    /// Starts a background download unless one is running or the index is already cached.
    fn start_download(&mut self, app: &AppHandle) -> Result<(), AppError> {
        if self.download.is_some() || self.cache.is_some() {
            return Ok(());
        }
        let dir = self.dir.clone().ok_or_else(|| {
            AppError::new(
                AppErrorKind::Io,
                "Could not locate the Tron data folder for the component library.",
                None,
            )
        })?;
        let cancel = CancellationToken::new();
        self.download = Some(cancel.clone());
        let status = LibraryStatus::Downloading(DownloadingLibrary {
            kicad_tag: self.pin.kicad_tag.clone(),
            downloaded_bytes: partial_len(&dir, &self.pin),
            total_bytes: self.pin.size,
        });
        self.set_status(app, status);
        let app = app.clone();
        let pin = self.pin.clone();
        tauri::async_runtime::spawn(async move {
            let outcome = download(&app, &dir, &pin, &cancel).await;
            let state = app.state::<LibraryState>();
            let mut inner = state.lock();
            inner.download = None;
            let status = match outcome {
                Ok(Some(cached)) => {
                    inner.cache = Some(cached);
                    inner.settled_status(None)
                }
                Ok(None) => inner.settled_status(None),
                Err(error) => inner.settled_status(Some(error)),
            };
            inner.set_status(&app, status);
        });
        Ok(())
    }
}

/// Resumes the download at launch when the user already agreed and no index is cached, which
/// also replaces a cache that was stale or damaged.
pub fn resume(app: &AppHandle) {
    let state = app.state::<LibraryState>();
    let mut inner = state.lock();
    if inner.consent != LibraryConsent::Accepted {
        return;
    }
    if let Err(error) = inner.start_download(app) {
        let status = inner.settled_status(Some(error));
        inner.set_status(app, status);
    }
}

fn save_consent(app: &AppHandle, accepted: bool) -> Result<(), AppError> {
    let path = settings::settings_path(app)?;
    let mut settings = Settings::read(&path)?;
    settings.library_download = Some(accepted);
    settings.write(&path)
}

#[tauri::command]
pub fn get_library_status(state: State<'_, LibraryState>) -> LibraryStatus {
    state.lock().status.clone()
}

/// Records the user's consent and starts downloading. Progress and the outcome arrive as
/// [`LIBRARY_STATUS_EVENT`]s.
#[tauri::command]
pub fn download_library(
    app: AppHandle,
    state: State<'_, LibraryState>,
) -> Result<LibraryStatus, AppError> {
    save_consent(&app, true)?;
    let mut inner = state.lock();
    inner.consent = LibraryConsent::Accepted;
    inner.start_download(&app)?;
    Ok(inner.status.clone())
}

/// Records that the user declined. The app keeps working with an empty library.
#[tauri::command]
pub fn decline_library_download(
    app: AppHandle,
    state: State<'_, LibraryState>,
) -> Result<LibraryStatus, AppError> {
    save_consent(&app, false)?;
    let mut inner = state.lock();
    inner.consent = LibraryConsent::Declined;
    match &inner.download {
        Some(cancel) => cancel.cancel(),
        None => {
            let status = inner.settled_status(None);
            inner.set_status(&app, status);
        }
    }
    Ok(inner.status.clone())
}

/// Stops a running download. The bytes received so far are kept for the next attempt.
#[tauri::command]
pub fn cancel_library_download(state: State<'_, LibraryState>) -> LibraryStatus {
    let inner = state.lock();
    if let Some(cancel) = &inner.download {
        cancel.cancel();
    }
    inner.status.clone()
}

#[tauri::command]
pub fn search_library(
    app: AppHandle,
    state: State<'_, LibraryState>,
    query: String,
    limit: u32,
) -> Result<Vec<LibraryHit>, AppError> {
    let limit = limit.clamp(1, MAX_SEARCH_RESULTS) as usize;
    let hits = read_index(&app, &state, |index| index.search(&query, limit))?;
    Ok(hits.into_iter().map(LibraryHit::from).collect())
}

/// The symbol with `id` from a search hit, or `None` if the index has no such symbol.
#[tauri::command]
pub fn get_library_symbol(
    app: AppHandle,
    state: State<'_, LibraryState>,
    id: i64,
) -> Result<Option<LibrarySymbol>, AppError> {
    let record = read_index(&app, &state, |index| index.symbol(id))?;
    Ok(record.map(LibrarySymbol::from))
}

/// The footprint with `id` from a search hit, or `None` if the index has no such footprint.
#[tauri::command]
pub fn get_library_footprint(
    app: AppHandle,
    state: State<'_, LibraryState>,
    id: i64,
) -> Result<Option<LibraryFootprint>, AppError> {
    let record = read_index(&app, &state, |index| index.footprint(id))?;
    Ok(record.map(LibraryFootprint::from))
}

pub(crate) fn read_index<T>(
    app: &AppHandle,
    state: &LibraryState,
    read: impl FnOnce(&Index) -> Result<T, LibraryError>,
) -> Result<T, AppError> {
    let mut inner = state.lock();
    let Some(cached) = &inner.cache else {
        return Err(AppError::new(
            AppErrorKind::LibraryNotReady,
            "The component library has not been downloaded yet.",
            None,
        ));
    };
    match read(&cached.index) {
        Ok(value) => Ok(value),
        Err(error) => {
            // The index opened and passed its checks, so a failed read means the file is
            // damaged. It is discarded and, with the user's consent, downloaded again.
            inner.cache = None;
            if let Some(dir) = &inner.dir {
                discard_cache(dir);
            }
            let status = inner.settled_status(None);
            inner.set_status(app, status);
            if inner.consent == LibraryConsent::Accepted
                && let Err(download_error) = inner.start_download(app)
            {
                let status = inner.settled_status(Some(download_error));
                inner.set_status(app, status);
            }
            Err(AppError::new(
                AppErrorKind::Library,
                "The component library is damaged and must be downloaded again.",
                Some(error.to_string()),
            ))
        }
    }
}

impl From<tron_library::SearchHit> for LibraryHit {
    fn from(hit: tron_library::SearchHit) -> LibraryHit {
        LibraryHit {
            kind: match hit.kind {
                ItemKind::Symbol => LibraryItemKind::Symbol,
                ItemKind::Footprint => LibraryItemKind::Footprint,
            },
            id: hit.id,
            name: hit.name,
            lib: hit.lib,
            description: hit.description,
        }
    }
}

/// Downloads, verifies, and installs the pinned index. Returns `None` when cancelled.
async fn download(
    app: &AppHandle,
    dir: &Path,
    pin: &PinnedIndex,
    cancel: &CancellationToken,
) -> Result<Option<Cached>, AppError> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|error| io_error(dir, error))?;
    remove_stale_partials(dir, pin);
    let client = reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .user_agent(format!("Tron/{}", app.package_info().version))
        .build()
        .map_err(network_error)?;
    let mut last_report: Option<Instant> = None;
    let progress = |downloaded: u64| {
        if downloaded < pin.size
            && last_report.is_some_and(|reported| reported.elapsed() < PROGRESS_INTERVAL)
        {
            return;
        }
        last_report = Some(Instant::now());
        app.state::<LibraryState>().lock().set_status(
            app,
            LibraryStatus::Downloading(DownloadingLibrary {
                kicad_tag: pin.kicad_tag.clone(),
                downloaded_bytes: downloaded,
                total_bytes: pin.size,
            }),
        );
    };
    let partial = partial_path(dir, pin);
    if !fetch(&client, pin, &partial, cancel, progress).await? || cancel.is_cancelled() {
        return Ok(None);
    }
    app.state::<LibraryState>().lock().set_status(
        app,
        LibraryStatus::Installing(InstallingLibrary {
            kicad_tag: pin.kicad_tag.clone(),
        }),
    );
    let dir = dir.to_path_buf();
    let pin = pin.clone();
    tauri::async_runtime::spawn_blocking(move || install(&dir, &pin))
        .await
        .map_err(|error| {
            AppError::new(
                AppErrorKind::Library,
                "The component library could not be installed.",
                Some(error.to_string()),
            )
        })?
        .map(Some)
}

/// Downloads the pinned file into `partial`, resuming after the bytes already there. Returns
/// `false` when cancelled; the bytes received so far stay on disk for the next attempt.
async fn fetch(
    client: &reqwest::Client,
    pin: &PinnedIndex,
    partial: &Path,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64),
) -> Result<bool, AppError> {
    let mut have = match tokio::fs::metadata(partial).await {
        Ok(meta) if meta.len() <= pin.size => meta.len(),
        Ok(_) => 0,
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => return Err(io_error(partial, error)),
    };
    // A server that refuses the resume gets one more request for the whole file.
    for _ in 0..2 {
        if have == pin.size {
            return Ok(true);
        }
        let mut request = client.get(&pin.url);
        if have > 0 {
            request = request.header(RANGE, format!("bytes={have}-"));
        }
        let response = tokio::select! {
            response = request.send() => response.map_err(network_error)?,
            () = cancel.cancelled() => return Ok(false),
        };
        let status = response.status();
        let append = if status == StatusCode::PARTIAL_CONTENT && have > 0 {
            if range_start(&response) != Some(have) {
                have = 0;
                continue;
            }
            true
        } else if status == StatusCode::OK {
            false
        } else if status == StatusCode::RANGE_NOT_SATISFIABLE && have > 0 {
            have = 0;
            continue;
        } else {
            return Err(AppError::new(
                AppErrorKind::Network,
                "The library server could not send the component library.",
                Some(format!("HTTP status {status}")),
            ));
        };
        if !append {
            have = 0;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(partial)
            .await
            .map_err(|error| io_error(partial, error))?;
        progress(have);
        let received = receive(
            response,
            &mut file,
            partial,
            pin.size,
            &mut have,
            cancel,
            &mut progress,
        )
        .await;
        let flushed = match file.flush().await {
            Ok(()) => file.sync_all().await,
            Err(error) => Err(error),
        };
        let completed = received?;
        flushed.map_err(|error| io_error(partial, error))?;
        if !completed {
            return Ok(false);
        }
        if have != pin.size {
            return Err(AppError::new(
                AppErrorKind::Network,
                "The download ended before the whole component library arrived.",
                Some(format!("Received {have} of {} bytes.", pin.size)),
            ));
        }
        return Ok(true);
    }
    Err(AppError::new(
        AppErrorKind::Network,
        "The library server would not resume the download.",
        None,
    ))
}

/// Appends the response body to `file`. Returns `false` when cancelled.
async fn receive(
    mut response: reqwest::Response,
    file: &mut tokio::fs::File,
    path: &Path,
    size: u64,
    have: &mut u64,
    cancel: &CancellationToken,
    progress: &mut impl FnMut(u64),
) -> Result<bool, AppError> {
    loop {
        let chunk = tokio::select! {
            chunk = response.chunk() => chunk.map_err(network_error)?,
            () = cancel.cancelled() => return Ok(false),
        };
        let Some(chunk) = chunk else {
            return Ok(true);
        };
        let received = *have + chunk.len() as u64;
        if received > size {
            return Err(AppError::new(
                AppErrorKind::Checksum,
                "The downloaded component library is larger than expected.",
                Some(format!("Expected {size} bytes.")),
            ));
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| io_error(path, error))?;
        *have = received;
        progress(received);
    }
}

fn range_start(response: &reqwest::Response) -> Option<u64> {
    let value = response.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    let (start, _) = value.strip_prefix("bytes ")?.split_once('-')?;
    start.trim().parse().ok()
}

/// Verifies the downloaded file, unpacks it, checks that it opens as an index, and moves it
/// over the cache. Nothing replaces the cache until every check has passed.
fn install(dir: &Path, pin: &PinnedIndex) -> Result<Cached, AppError> {
    let partial = partial_path(dir, pin);
    let actual = sha256_file(&partial).map_err(|error| io_error(&partial, error))?;
    if actual != pin.sha256 {
        remove_logged(&partial);
        return Err(AppError::new(
            AppErrorKind::Checksum,
            "The downloaded component library failed its integrity check.",
            Some(format!("Expected SHA-256 {}, got {actual}.", pin.sha256)),
        ));
    }
    let unpacked = dir.join(UNPACKED_FILE);
    let opened = unpack(&partial, &unpacked)
        .map_err(|error| error.to_string())
        .and_then(|()| {
            let index = Index::open(&unpacked).map_err(|error| error.to_string())?;
            describe(&index, pin).map_err(|error| error.to_string())
        });
    if let Err(detail) = opened {
        remove_logged(&unpacked);
        remove_logged(&partial);
        return Err(AppError::new(
            AppErrorKind::Library,
            "The downloaded component library could not be opened.",
            Some(detail),
        ));
    }
    let index_path = dir.join(INDEX_FILE);
    let checksum_path = dir.join(INDEX_CHECKSUM_FILE);
    remove_if_exists(&checksum_path).map_err(|error| io_error(&checksum_path, error))?;
    fs::rename(&unpacked, &index_path).map_err(|error| io_error(&index_path, error))?;
    tron_core::write_atomic(&checksum_path, &format!("{}\n", pin.sha256))?;
    remove_logged(&partial);
    let index = Index::open(&index_path).map_err(|error| library_error(&error))?;
    let info = describe(&index, pin).map_err(|error| library_error(&error))?;
    Ok(Cached { index, info })
}

/// Opens the cached index if it was installed from `pin` and is readable. A stale or damaged
/// cache is deleted so the pinned index is downloaded again.
fn open_cache(dir: &Path, pin: &PinnedIndex) -> Option<Cached> {
    let index_path = dir.join(INDEX_FILE);
    if !index_path.exists() {
        return None;
    }
    let checksum = fs::read_to_string(dir.join(INDEX_CHECKSUM_FILE)).unwrap_or_default();
    let reason = if checksum.trim() == pin.sha256 {
        let opened = Index::open(&index_path)
            .and_then(|index| describe(&index, pin).map(|info| Cached { index, info }));
        match opened {
            Ok(cached) => return Some(cached),
            Err(error) => error.to_string(),
        }
    } else {
        "it was not installed from the pinned index".to_owned()
    };
    eprintln!("discarding the cached component library because {reason}");
    discard_cache(dir);
    None
}

fn describe(index: &Index, pin: &PinnedIndex) -> Result<ReadyLibrary, LibraryError> {
    Ok(ReadyLibrary {
        kicad_tag: index
            .meta(META_KICAD_TAG)?
            .unwrap_or_else(|| pin.kicad_tag.clone()),
        symbol_count: index.item_count(ItemKind::Symbol)?,
        footprint_count: index.item_count(ItemKind::Footprint)?,
        attribution: index
            .meta(META_ATTRIBUTION)?
            .unwrap_or_else(|| ATTRIBUTION.to_owned()),
    })
}

fn discard_cache(dir: &Path) {
    remove_logged(&dir.join(INDEX_CHECKSUM_FILE));
    remove_logged(&dir.join(INDEX_FILE));
}

fn partial_path(dir: &Path, pin: &PinnedIndex) -> PathBuf {
    dir.join(format!("{}{PARTIAL_SUFFIX}", pin.sha256))
}

fn partial_len(dir: &Path, pin: &PinnedIndex) -> u64 {
    fs::metadata(partial_path(dir, pin))
        .map(|meta| meta.len().min(pin.size))
        .unwrap_or(0)
}

/// Deletes partial downloads of other pinned indexes.
fn remove_stale_partials(dir: &Path, pin: &PinnedIndex) {
    let keep = partial_path(dir, pin);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != keep
            && entry
                .file_name()
                .to_string_lossy()
                .ends_with(PARTIAL_SUFFIX)
        {
            remove_logged(&path);
        }
    }
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize().iter() {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

fn unpack(source: &Path, target: &Path) -> io::Result<()> {
    let mut decoder = GzDecoder::new(BufReader::new(File::open(source)?));
    let mut output = BufWriter::new(File::create(target)?);
    io::copy(&mut decoder, &mut output)?;
    output
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?
        .sync_all()
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Removes a file whose leftover copy is harmless, logging any failure.
fn remove_logged(path: &Path) {
    if let Err(error) = remove_if_exists(path) {
        eprintln!("could not remove {}: {error}", path.display());
    }
}

fn io_error(path: &Path, error: io::Error) -> AppError {
    AppError::new(
        AppErrorKind::Io,
        format!("Could not access {}.", path.display()),
        Some(error.to_string()),
    )
}

fn network_error(error: reqwest::Error) -> AppError {
    AppError::new(
        AppErrorKind::Network,
        "Could not download the component library.",
        Some(error.to_string()),
    )
}

fn library_error(error: &LibraryError) -> AppError {
    AppError::new(
        AppErrorKind::Library,
        "The component library could not be opened.",
        Some(error.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::thread;

    use flate2::Compression;
    use flate2::write::GzEncoder;

    #[derive(Clone, Copy)]
    enum Reply {
        /// 200 with the whole body.
        Whole,
        /// 206 from the requested offset.
        Resume,
        /// 416 Range Not Satisfiable.
        Refuse,
        /// Announces the whole body, then closes after this many bytes.
        Cut(usize),
        /// Sends this many bytes, then holds the connection open.
        Stall(usize),
    }

    struct Server {
        url: String,
        ranges: Arc<Mutex<Vec<Option<u64>>>>,
    }

    impl Server {
        fn ranges(&self) -> Vec<Option<u64>> {
            self.ranges.lock().unwrap().clone()
        }
    }

    fn serve(body: Arc<Vec<u8>>, replies: Vec<Reply>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/library.db.gz", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&ranges);
        thread::spawn(move || {
            for reply in replies {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let start = read_range(&stream);
                seen.lock().unwrap().push(start);
                respond(stream, &body, reply, start);
            }
        });
        Server { url, ranges }
    }

    fn read_range(stream: &TcpStream) -> Option<u64> {
        let mut reader = BufReader::new(stream);
        let mut range = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                return range;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("range")
            {
                range = value
                    .trim()
                    .strip_prefix("bytes=")
                    .and_then(|value| value.strip_suffix('-'))
                    .and_then(|value| value.parse().ok());
            }
        }
    }

    fn head(status: &str, length: usize, extra: &str) -> String {
        format!("HTTP/1.1 {status}\r\nContent-Length: {length}\r\n{extra}Connection: close\r\n\r\n")
    }

    fn respond(mut stream: TcpStream, body: &[u8], reply: Reply, start: Option<u64>) {
        let total = body.len();
        let (header, bytes) = match (reply, start) {
            (Reply::Whole, _) | (Reply::Resume, None) => (head("200 OK", total, ""), body),
            (Reply::Resume, Some(start)) => {
                let from = usize::try_from(start).unwrap();
                let range = format!("Content-Range: bytes {from}-{}/{total}\r\n", total - 1);
                (
                    head("206 Partial Content", total - from, &range),
                    &body[from..],
                )
            }
            (Reply::Refuse, _) => (head("416 Range Not Satisfiable", 0, ""), &body[..0]),
            (Reply::Cut(sent) | Reply::Stall(sent), _) => {
                (head("200 OK", total, ""), &body[..sent])
            }
        };
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(bytes);
        let _ = stream.flush();
        if let Reply::Stall(_) = reply {
            thread::sleep(Duration::from_secs(5));
        }
    }

    /// A real index built from the KiCad fixtures, compressed the way CI publishes it.
    fn packed_index(dir: &Path) -> Vec<u8> {
        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/library/tests/fixtures/kicad");
        let built = dir.join("built.db");
        tron_library::build_index(
            &fixtures.join("symbols"),
            &fixtures.join("footprints"),
            "10.0.7",
            &built,
        )
        .unwrap();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&fs::read(&built).unwrap()).unwrap();
        encoder.finish().unwrap()
    }

    fn pin_for(body: &[u8], url: &str) -> PinnedIndex {
        let mut sha256 = String::new();
        for byte in Sha256::digest(body).iter() {
            write!(sha256, "{byte:02x}").unwrap();
        }
        PinnedIndex {
            kicad_tag: "10.0.7".to_owned(),
            release: "library-kicad-10.0.7-r1".to_owned(),
            url: url.to_owned(),
            sha256,
            size: body.len() as u64,
        }
    }

    struct Setup {
        _temp: tempfile::TempDir,
        dir: PathBuf,
        body: Arc<Vec<u8>>,
    }

    fn setup() -> Setup {
        let temp = tempfile::tempdir().unwrap();
        let body = Arc::new(packed_index(temp.path()));
        let dir = temp.path().join(CACHE_DIR);
        fs::create_dir_all(&dir).unwrap();
        Setup {
            _temp: temp,
            dir,
            body,
        }
    }

    fn run_fetch(pin: &PinnedIndex, partial: &Path) -> Result<bool, AppError> {
        let client = reqwest::Client::new();
        let cancel = CancellationToken::new();
        tauri::async_runtime::block_on(fetch(&client, pin, partial, &cancel, |_| {}))
    }

    #[test]
    fn a_whole_download_matches_the_published_file() {
        let setup = setup();
        let server = serve(Arc::clone(&setup.body), vec![Reply::Whole]);
        let pin = pin_for(&setup.body, &server.url);
        let partial = partial_path(&setup.dir, &pin);
        assert!(run_fetch(&pin, &partial).unwrap());
        assert_eq!(fs::read(&partial).unwrap(), *setup.body);
        assert_eq!(server.ranges(), [None]);
    }

    #[test]
    fn an_interrupted_download_resumes_where_it_stopped() {
        let setup = setup();
        let half = setup.body.len() / 2;
        let server = serve(
            Arc::clone(&setup.body),
            vec![Reply::Cut(half), Reply::Resume],
        );
        let pin = pin_for(&setup.body, &server.url);
        let partial = partial_path(&setup.dir, &pin);

        let error = run_fetch(&pin, &partial).unwrap_err();
        assert_eq!(error.kind, AppErrorKind::Network);
        let kept = fs::metadata(&partial).unwrap().len();
        assert!(kept <= half as u64);

        assert!(run_fetch(&pin, &partial).unwrap());
        assert_eq!(fs::read(&partial).unwrap(), *setup.body);
        let ranges = server.ranges();
        assert_eq!(ranges[0], None);
        assert_eq!(ranges[1], (kept > 0).then_some(kept));
    }

    #[test]
    fn a_server_that_ignores_the_range_restarts_the_file() {
        let setup = setup();
        let server = serve(Arc::clone(&setup.body), vec![Reply::Whole]);
        let pin = pin_for(&setup.body, &server.url);
        let partial = partial_path(&setup.dir, &pin);
        fs::write(&partial, &setup.body[..100]).unwrap();
        assert!(run_fetch(&pin, &partial).unwrap());
        assert_eq!(fs::read(&partial).unwrap(), *setup.body);
        assert_eq!(server.ranges(), [Some(100)]);
    }

    #[test]
    fn a_refused_range_requests_the_whole_file_again() {
        let setup = setup();
        let server = serve(Arc::clone(&setup.body), vec![Reply::Refuse, Reply::Whole]);
        let pin = pin_for(&setup.body, &server.url);
        let partial = partial_path(&setup.dir, &pin);
        fs::write(&partial, b"stale bytes").unwrap();
        assert!(run_fetch(&pin, &partial).unwrap());
        assert_eq!(fs::read(&partial).unwrap(), *setup.body);
        assert_eq!(server.ranges(), [Some(11), None]);
    }

    #[test]
    fn a_complete_partial_file_needs_no_request() {
        let setup = setup();
        let pin = pin_for(&setup.body, "http://127.0.0.1:9/unreachable");
        let partial = partial_path(&setup.dir, &pin);
        fs::write(&partial, &*setup.body).unwrap();
        assert!(run_fetch(&pin, &partial).unwrap());
    }

    #[test]
    fn cancelling_keeps_the_bytes_received_so_far() {
        let setup = setup();
        let server = serve(
            Arc::clone(&setup.body),
            vec![Reply::Stall(setup.body.len() / 2)],
        );
        let pin = pin_for(&setup.body, &server.url);
        let partial = partial_path(&setup.dir, &pin);
        let client = reqwest::Client::new();
        let cancel = CancellationToken::new();
        let completed =
            tauri::async_runtime::block_on(fetch(&client, &pin, &partial, &cancel, |downloaded| {
                if downloaded > 0 {
                    cancel.cancel();
                }
            }))
            .unwrap();
        assert!(!completed);
        let kept = fs::read(&partial).unwrap();
        assert!(!kept.is_empty());
        assert_eq!(kept, setup.body[..kept.len()]);
    }

    #[test]
    fn a_verified_download_is_installed_and_searchable() {
        let setup = setup();
        let pin = pin_for(&setup.body, "https://example.org/library.db.gz");
        let partial = partial_path(&setup.dir, &pin);
        fs::write(&partial, &*setup.body).unwrap();

        let cached = install(&setup.dir, &pin).unwrap();
        assert!(!partial.exists());
        assert!(!setup.dir.join(UNPACKED_FILE).exists());
        assert_eq!(cached.info.kicad_tag, "10.0.7");
        assert!(cached.info.symbol_count > 0);
        assert!(cached.info.footprint_count > 0);
        assert_eq!(cached.info.attribution, ATTRIBUTION);
        drop(cached);

        let reopened = open_cache(&setup.dir, &pin).unwrap();
        let hits = reopened.index.search("R", 10).unwrap();
        assert_eq!(hits[0].name, "R");
    }

    #[test]
    fn search_hits_resolve_to_symbols_and_footprints_with_previews() {
        let setup = setup();
        let pin = pin_for(&setup.body, "https://example.org/library.db.gz");
        fs::write(partial_path(&setup.dir, &pin), &*setup.body).unwrap();
        let cached = install(&setup.dir, &pin).unwrap();

        let symbol_hit = cached.index.search("R", 1).unwrap().remove(0);
        let symbol = LibrarySymbol::from(cached.index.symbol(symbol_hit.id).unwrap().unwrap());
        assert_eq!(symbol.name, "R");
        assert_eq!(symbol.reference, "R");
        assert_eq!(symbol.preview.pins.len(), 2);
        let json = serde_json::to_value(&symbol).unwrap();
        assert_eq!(json["pinCount"], 2);
        assert!(json["preview"]["shapes"].is_array());

        let footprint_hit = cached
            .index
            .search("R_0603_1608Metric", 1)
            .unwrap()
            .remove(0);
        assert_eq!(
            LibraryHit::from(footprint_hit.clone()).kind,
            LibraryItemKind::Footprint
        );
        let footprint =
            LibraryFootprint::from(cached.index.footprint(footprint_hit.id).unwrap().unwrap());
        assert_eq!(footprint.mount, FootprintMount::Smd);
        assert_eq!(footprint.preview.pads.len(), 2);
    }

    #[test]
    fn a_checksum_mismatch_is_rejected_and_discarded() {
        let setup = setup();
        let mut pin = pin_for(&setup.body, "https://example.org/library.db.gz");
        pin.sha256 = "0".repeat(64);
        let partial = partial_path(&setup.dir, &pin);
        fs::write(&partial, &*setup.body).unwrap();

        let error = install(&setup.dir, &pin).err().unwrap();
        assert_eq!(error.kind, AppErrorKind::Checksum);
        assert!(!partial.exists());
        assert!(!setup.dir.join(INDEX_FILE).exists());
    }

    #[test]
    fn a_cache_from_another_pin_is_discarded() {
        let setup = setup();
        let pin = pin_for(&setup.body, "https://example.org/library.db.gz");
        fs::write(partial_path(&setup.dir, &pin), &*setup.body).unwrap();
        drop(install(&setup.dir, &pin).unwrap());

        let mut newer = pin.clone();
        newer.sha256 = "f".repeat(64);
        assert!(open_cache(&setup.dir, &newer).is_none());
        assert!(!setup.dir.join(INDEX_FILE).exists());
        assert!(!setup.dir.join(INDEX_CHECKSUM_FILE).exists());
    }

    #[test]
    fn a_damaged_cache_is_discarded() {
        let setup = setup();
        let pin = pin_for(&setup.body, "https://example.org/library.db.gz");
        fs::write(setup.dir.join(INDEX_FILE), b"not a database").unwrap();
        fs::write(
            setup.dir.join(INDEX_CHECKSUM_FILE),
            format!("{}\n", pin.sha256),
        )
        .unwrap();
        assert!(open_cache(&setup.dir, &pin).is_none());
        assert!(!setup.dir.join(INDEX_FILE).exists());
    }

    #[test]
    fn partial_downloads_of_other_pins_are_removed() {
        let setup = setup();
        let pin = pin_for(&setup.body, "https://example.org/library.db.gz");
        let stale = setup
            .dir
            .join(format!("{}{PARTIAL_SUFFIX}", "a".repeat(64)));
        fs::write(&stale, b"old").unwrap();
        fs::write(partial_path(&setup.dir, &pin), b"new").unwrap();
        remove_stale_partials(&setup.dir, &pin);
        assert!(!stale.exists());
        assert!(partial_path(&setup.dir, &pin).exists());
    }

    #[test]
    fn statuses_serialize_with_a_state_tag() {
        let status = LibraryStatus::Downloading(DownloadingLibrary {
            kicad_tag: "10.0.7".to_owned(),
            downloaded_bytes: 5,
            total_bytes: 10,
        });
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            serde_json::json!({
                "state": "downloading",
                "kicadTag": "10.0.7",
                "downloadedBytes": 5,
                "totalBytes": 10
            })
        );
    }
}
