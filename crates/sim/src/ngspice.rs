//! The ngspice shared library, loaded at runtime.
//!
//! ngspice keeps its state in globals and must be initialized only once per process, so the
//! library is loaded on first use and stays loaded. [`Engine::lock`] hands out exclusive
//! access, and every run starts by clearing the previous circuit and its results.
//!
//! The analysis runs on ngspice's background thread so that [`Engine::halt`] can stop it.
//! The caller's thread waits for the thread to finish and reads the results itself; the
//! callbacks only record what happened and never call back into ngspice.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock, PoisonError};

use libloading::Library;

use crate::deck::Deck;

/// `struct vector_info` from `sharedspice.h`.
#[repr(C)]
struct VectorInfo {
    name: *const c_char,
    kind: c_int,
    flags: i16,
    real: *const f64,
    complex: *const c_void,
    length: c_int,
}

type SendChar = unsafe extern "C" fn(*const c_char, c_int, *mut c_void) -> c_int;
type SendStat = unsafe extern "C" fn(*const c_char, c_int, *mut c_void) -> c_int;
type ControlledExit = unsafe extern "C" fn(c_int, bool, bool, c_int, *mut c_void) -> c_int;
type SendData = unsafe extern "C" fn(*mut c_void, c_int, c_int, *mut c_void) -> c_int;
type SendInitData = unsafe extern "C" fn(*mut c_void, c_int, *mut c_void) -> c_int;
type BgThreadRunning = unsafe extern "C" fn(bool, c_int, *mut c_void) -> c_int;

type InitFn = unsafe extern "C" fn(
    Option<SendChar>,
    Option<SendStat>,
    Option<ControlledExit>,
    Option<SendData>,
    Option<SendInitData>,
    Option<BgThreadRunning>,
    *mut c_void,
) -> c_int;
type CircFn = unsafe extern "C" fn(*mut *mut c_char) -> c_int;
type CommandFn = unsafe extern "C" fn(*const c_char) -> c_int;
type VecInfoFn = unsafe extern "C" fn(*const c_char) -> *const VectorInfo;
type CurPlotFn = unsafe extern "C" fn() -> *const c_char;
type AllVecsFn = unsafe extern "C" fn(*const c_char) -> *const *const c_char;
/// Lines ngspice printed since the last run started, with its `stdout ` or `stderr ` prefix.
static OUTPUT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Set when ngspice asks the process to exit. ngspice cannot be used again after that.
static EXITED: AtomicBool = AtomicBool::new(false);

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

/// `ngSpice_Command`, kept outside the engine lock so a run can be halted while it waits.
static COMMAND: OnceLock<CommandFn> = OnceLock::new();

/// `vector_info.v_type` of a time scale, from ngspice's `sim.h`.
const SV_TIME: c_int = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    /// `bg_run` was sent and has not returned yet.
    Starting,
    /// ngspice's background thread is running the analysis.
    Running,
    /// The background thread finished, or ngspice exited.
    Finished,
}

struct RunState {
    phase: Phase,
    halt_requested: bool,
}

static RUN: Mutex<RunState> = Mutex::new(RunState {
    phase: Phase::Idle,
    halt_requested: false,
});
static RUN_FINISHED: Condvar = Condvar::new();
/// Held while `bg_halt` is in progress, so the next command waits for it to return.
static HALTING: Mutex<()> = Mutex::new(());

/// Receives the analysis progress in percent.
type Progress = Box<dyn Fn(f64) + Send>;

static PROGRESS: Mutex<Option<Progress>> = Mutex::new(None);

/// Every status line ngspice sent, so a test can show them when progress is missing.
#[cfg(test)]
static STATUS_LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn run_state() -> MutexGuard<'static, RunState> {
    RUN.lock().unwrap_or_else(PoisonError::into_inner)
}

fn finish_run() {
    run_state().phase = Phase::Finished;
    RUN_FINISHED.notify_all();
}

extern "C" fn send_char(text: *const c_char, _id: c_int, _user: *mut c_void) -> c_int {
    if !text.is_null() {
        // SAFETY: ngspice passes a NUL-terminated string that lives for this call.
        let line = unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned();
        OUTPUT
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }
    0
}

extern "C" fn send_stat(text: *const c_char, _id: c_int, _user: *mut c_void) -> c_int {
    if text.is_null() {
        return 0;
    }
    // SAFETY: ngspice passes a NUL-terminated string that lives for this call.
    let status = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    #[cfg(test)]
    STATUS_LINES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(status.to_string());
    if let Some(percent) = parse_percent(&status) {
        let progress = PROGRESS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(progress) = progress.as_ref() {
            progress(percent);
        }
    }
    0
}

/// The percentage in an ngspice status line such as `tran: 45.3%`, or `None` for lines
/// without one, such as `--ready--`.
fn parse_percent(status: &str) -> Option<f64> {
    let number = status.trim().strip_suffix('%')?.trim_end();
    let start = number
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_ascii_digit() || *c == '.'))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let percent: f64 = number[start..].parse().ok()?;
    Some(percent.clamp(0.0, 100.0))
}

extern "C" fn controlled_exit(
    _status: c_int,
    _unload: bool,
    _quit: bool,
    _id: c_int,
    _user: *mut c_void,
) -> c_int {
    EXITED.store(true, Ordering::SeqCst);
    finish_run();
    0
}

/// ngspice reports `true` when its background thread stops and `false` when it starts.
extern "C" fn background_running(stopped: bool, _id: c_int, _user: *mut c_void) -> c_int {
    if stopped {
        finish_run();
    }
    0
}

/// Why ngspice could not run a deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// The shared library could not be loaded, or lacks a function Tron needs.
    Load { path: PathBuf, message: String },
    /// A different ngspice library is already loaded in this process.
    AlreadyLoaded { path: PathBuf },
    /// ngspice stopped itself after a fatal error and must be restarted with the app.
    Exited,
    /// ngspice rejected the circuit. The lines are what it printed on standard error.
    Circuit(Vec<String>),
    /// The analysis failed. The lines are what it printed on standard error.
    Analysis(Vec<String>),
    /// [`Engine::halt`] stopped the run, and its partial results were discarded.
    Cancelled,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Load { path, message } => {
                write!(
                    f,
                    "ngspice could not be loaded from {}: {message}",
                    path.display()
                )
            }
            EngineError::AlreadyLoaded { path } => {
                write!(f, "ngspice is already loaded from {}.", path.display())
            }
            EngineError::Exited => write!(
                f,
                "ngspice stopped after a fatal error. Restart Tron to simulate again."
            ),
            EngineError::Circuit(lines) => {
                write!(f, "ngspice rejected the circuit: {}", lines.join(" "))
            }
            EngineError::Analysis(lines) => {
                write!(f, "The simulation failed: {}", lines.join(" "))
            }
            EngineError::Cancelled => write!(f, "The simulation was cancelled."),
        }
    }
}

impl std::error::Error for EngineError {}

/// One result vector, in SI units.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    /// The ngspice name, in lower case: a node such as `/out`, a branch current such as
    /// `v1#branch`, or the scale, such as `time`.
    pub name: String,
    pub values: Vec<f64>,
}

/// The results of one analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Plot {
    /// The ngspice plot name, such as `op1` or `tran1`.
    pub name: String,
    /// The vector the others are plotted against, such as `time`. An operating point has
    /// none: each vector holds one value.
    pub scale: Option<String>,
    pub vectors: Vec<Vector>,
}

impl Plot {
    /// The values of the vector named `name`, ignoring case as ngspice does.
    pub fn vector(&self, name: &str) -> Option<&[f64]> {
        self.vectors
            .iter()
            .find(|vector| vector.name.eq_ignore_ascii_case(name))
            .map(|vector| vector.values.as_slice())
    }
}

/// The loaded ngspice library.
pub struct Engine {
    path: PathBuf,
    circ: CircFn,
    command: CommandFn,
    vec_info: VecInfoFn,
    cur_plot: CurPlotFn,
    all_vecs: AllVecsFn,
    /// Folders whose code models are loaded. ngspice keeps them for the life of the process.
    code_models: Vec<PathBuf>,
    /// Kept loaded for the life of the process; the function pointers above point into it.
    _library: Library,
}

/// Exclusive access to the engine. Other callers wait until it is dropped.
pub struct EngineGuard(MutexGuard<'static, Option<Engine>>);

impl Deref for EngineGuard {
    type Target = Engine;

    fn deref(&self) -> &Engine {
        self.0
            .as_ref()
            .expect("the guard always holds a loaded engine")
    }
}

impl DerefMut for EngineGuard {
    fn deref_mut(&mut self) -> &mut Engine {
        self.0
            .as_mut()
            .expect("the guard always holds a loaded engine")
    }
}

impl Engine {
    /// Locks the engine, loading and initializing ngspice from `path` on first use.
    pub fn lock(path: &Path) -> Result<EngineGuard, EngineError> {
        let mut slot = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
        match slot.as_ref() {
            Some(engine) if engine.path != path => {
                return Err(EngineError::AlreadyLoaded {
                    path: engine.path.clone(),
                });
            }
            Some(_) => {}
            None => *slot = Some(Engine::load(path)?),
        }
        Ok(EngineGuard(slot))
    }

    fn load(path: &Path) -> Result<Engine, EngineError> {
        let failed = |message: String| EngineError::Load {
            path: path.to_owned(),
            message,
        };
        // ngspice would run a `spinit` from its default folders, whose code model paths are
        // relative to the working directory, so Tron points it at the library's folder,
        // which has none, and loads the code models itself.
        skip_spinit(path.parent().unwrap_or(path)).map_err(failed)?;
        // SAFETY: loading ngspice runs no initializers that depend on Tron's state.
        let library = unsafe { open(path) }.map_err(|error| failed(with_sources(&error)))?;
        // SAFETY: each type matches the prototype in ngspice's `sharedspice.h`.
        let (init, circ, command, vec_info, cur_plot, all_vecs) = unsafe {
            (
                symbol::<InitFn>(&library, b"ngSpice_Init\0"),
                symbol::<CircFn>(&library, b"ngSpice_Circ\0"),
                symbol::<CommandFn>(&library, b"ngSpice_Command\0"),
                symbol::<VecInfoFn>(&library, b"ngGet_Vec_Info\0"),
                symbol::<CurPlotFn>(&library, b"ngSpice_CurPlot\0"),
                symbol::<AllVecsFn>(&library, b"ngSpice_AllVecs\0"),
            )
        };
        let (init, circ, command, vec_info, cur_plot, all_vecs) = (
            init.map_err(failed)?,
            circ.map_err(failed)?,
            command.map_err(failed)?,
            vec_info.map_err(failed)?,
            cur_plot.map_err(failed)?,
            all_vecs.map_err(failed)?,
        );
        // SAFETY: the callbacks are `extern "C"` functions that never unwind, and they
        // reach only process-wide statics, so no user data pointer is needed.
        let status = unsafe {
            init(
                Some(send_char as SendChar),
                Some(send_stat as SendStat),
                Some(controlled_exit as ControlledExit),
                None,
                None,
                Some(background_running as BgThreadRunning),
                ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(failed(format!("ngSpice_Init returned {status}.")));
        }
        // The library stays loaded for the life of the process, so the pointer stays valid.
        let _ = COMMAND.set(command);
        Ok(Engine {
            path: path.to_owned(),
            circ,
            command,
            vec_info,
            cur_plot,
            all_vecs,
            code_models: Vec::new(),
            _library: library,
        })
    }

    /// Loads every XSPICE code model (`*.cm`) in `dir`. A folder already loaded is
    /// skipped, since ngspice keeps code models until the process ends.
    pub fn load_code_models(&mut self, dir: &Path) -> Result<(), EngineError> {
        if self.code_models.iter().any(|loaded| loaded == dir) {
            return Ok(());
        }
        let failed = |message: String| EngineError::Load {
            path: dir.to_owned(),
            message,
        };
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => return Err(failed(error.to_string())),
        };
        let mut models: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| is_code_model(path))
            .collect();
        if models.is_empty() {
            return Err(failed("the folder has no code models (*.cm).".to_owned()));
        }
        models.sort();
        take_errors();
        for model in &models {
            // ngspice's command parser treats a backslash as an escape, and Windows
            // accepts forward slashes.
            let path = model.display().to_string().replace('\\', "/");
            let status = self.command(&format!("codemodel \"{path}\""))?;
            let errors = take_errors();
            if status != 0 || !errors.is_empty() {
                return Err(failed(format!("{path}: {}", errors.join(" "))));
            }
        }
        self.code_models.push(dir.to_owned());
        Ok(())
    }

    /// Loads `deck`, runs its analysis to completion, and returns the results.
    pub fn run(&mut self, deck: &Deck) -> Result<Plot, EngineError> {
        self.run_cancellable(deck, &AtomicBool::new(false), |_| {})
    }

    /// Like [`Engine::run`], and calls `progress` with the percentage ngspice reports as
    /// the analysis advances. `progress` runs on ngspice's thread.
    ///
    /// To cancel, set `cancel` and then call [`Engine::halt`]. Setting the flag first
    /// covers a cancel that arrives before the analysis has started.
    pub fn run_cancellable(
        &mut self,
        deck: &Deck,
        cancel: &AtomicBool,
        progress: impl Fn(f64) + Send + 'static,
    ) -> Result<Plot, EngineError> {
        *PROGRESS.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(progress));
        let result = self.load_and_run(deck, cancel);
        *PROGRESS.lock().unwrap_or_else(PoisonError::into_inner) = None;
        let mut state = run_state();
        state.phase = Phase::Idle;
        state.halt_requested = false;
        result
    }

    /// Stops the analysis that is running, which then fails with
    /// [`EngineError::Cancelled`]. A halt that arrives while a run is being set up stops it
    /// as soon as it starts. Does nothing when no run is in progress.
    pub fn halt() {
        let _halting = HALTING.lock().unwrap_or_else(PoisonError::into_inner);
        let send = {
            let mut state = run_state();
            if state.phase == Phase::Idle {
                return;
            }
            state.halt_requested = true;
            state.phase == Phase::Running
        };
        if send {
            send_halt();
        }
    }

    fn load_and_run(&mut self, deck: &Deck, cancel: &AtomicBool) -> Result<Plot, EngineError> {
        {
            let mut state = run_state();
            state.phase = Phase::Starting;
            state.halt_requested = false;
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(EngineError::Cancelled);
        }
        self.reset()?;
        let lines: Vec<CString> = deck
            .lines
            .iter()
            .map(|line| CString::new(line.as_str()))
            .collect::<Result<_, _>>()
            .map_err(|_| EngineError::Circuit(vec!["The deck contains a NUL byte.".to_owned()]))?;
        let mut pointers: Vec<*mut c_char> =
            lines.iter().map(|line| line.as_ptr().cast_mut()).collect();
        pointers.push(ptr::null_mut());
        // SAFETY: a NULL-terminated array of NUL-terminated lines; ngspice copies them.
        let status = unsafe { (self.circ)(pointers.as_mut_ptr()) };
        let errors = take_errors();
        if status != 0 || !errors.is_empty() {
            return Err(EngineError::Circuit(errors));
        }
        if run_state().halt_requested {
            return Err(EngineError::Cancelled);
        }
        let status = self.command("bg_run")?;
        if status != 0 {
            return Err(EngineError::Analysis(take_errors()));
        }
        let halt_now = {
            let mut state = run_state();
            if state.phase == Phase::Starting {
                state.phase = Phase::Running;
            }
            state.halt_requested && state.phase == Phase::Running
        };
        if halt_now {
            let _halting = HALTING.lock().unwrap_or_else(PoisonError::into_inner);
            send_halt();
        }
        let halted = {
            let mut state = run_state();
            while state.phase != Phase::Finished {
                state = RUN_FINISHED
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            state.halt_requested
        };
        drop(HALTING.lock().unwrap_or_else(PoisonError::into_inner));
        if EXITED.load(Ordering::SeqCst) {
            return Err(EngineError::Exited);
        }
        if halted {
            return Err(EngineError::Cancelled);
        }
        let errors = take_errors();
        if !errors.is_empty() {
            return Err(EngineError::Analysis(errors));
        }
        self.plot()
    }

    /// Removes the loaded circuit and every result, and clears ngspice's output.
    fn reset(&mut self) -> Result<(), EngineError> {
        self.command("destroy all")?;
        self.command("remcirc")?;
        // SAFETY: a null command clears ngspice's command memory, as documented.
        unsafe { (self.command)(ptr::null()) };
        OUTPUT
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        Ok(())
    }

    fn command(&mut self, command: &str) -> Result<c_int, EngineError> {
        if EXITED.load(Ordering::SeqCst) {
            return Err(EngineError::Exited);
        }
        let command = CString::new(command).expect("commands and file paths contain no NUL");
        // SAFETY: a NUL-terminated command string that outlives the call.
        let status = unsafe { (self.command)(command.as_ptr()) };
        if EXITED.load(Ordering::SeqCst) {
            return Err(EngineError::Exited);
        }
        Ok(status)
    }

    /// Copies every vector of the current plot.
    fn plot(&mut self) -> Result<Plot, EngineError> {
        let no_results = || EngineError::Analysis(vec!["ngspice produced no results.".to_owned()]);
        // SAFETY: ngspice returns a NUL-terminated name it owns, or null.
        let name = unsafe { owned_string((self.cur_plot)()) }.ok_or_else(no_results)?;
        if name == "const" {
            return Err(no_results());
        }
        let plot = CString::new(name.as_str()).map_err(|_| no_results())?;
        // SAFETY: ngspice returns a NULL-terminated array of names it owns, or null.
        let names = unsafe { owned_strings((self.all_vecs)(plot.as_ptr())) };
        let mut vectors = Vec::with_capacity(names.len());
        let mut scale = None;
        for vector in names {
            let qualified = CString::new(format!("{name}.{vector}")).map_err(|_| no_results())?;
            // SAFETY: ngspice returns a pointer to its own vector record, or null. The data
            // stays valid until the next command, and is copied before then.
            let data = unsafe { real_values((self.vec_info)(qualified.as_ptr())) };
            if let Some((kind, values)) = data {
                let name = vector.to_ascii_lowercase();
                if kind == SV_TIME && scale.is_none() {
                    scale = Some(name.clone());
                }
                vectors.push(Vector { name, values });
            }
        }
        Ok(Plot {
            name,
            scale,
            vectors,
        })
    }
}

fn is_code_model(path: &Path) -> bool {
    let extension = path.extension().and_then(|ext| ext.to_str());
    extension.is_some_and(|ext| ext.eq_ignore_ascii_case("cm"))
}

fn send_halt() {
    if let Some(&command) = COMMAND.get() {
        // SAFETY: a NUL-terminated command string. `bg_halt` only interrupts ngspice's
        // background thread and waits for it to stop.
        unsafe { command(c"bg_halt".as_ptr()) };
    }
}

/// The lines ngspice printed on standard error since the last reset, without the prefix.
/// ngspice reports warnings there too, so only lines naming an error count.
fn take_errors() -> Vec<String> {
    let mut output = OUTPUT.lock().unwrap_or_else(PoisonError::into_inner);
    let errors = output
        .iter()
        .filter_map(|line| line.strip_prefix("stderr "))
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error") || lower.contains("aborted")
        })
        .map(|line| line.trim().to_owned())
        .collect();
    output.clear();
    errors
}

/// The error and every cause under it, such as the operating system error behind a failed
/// library load.
fn with_sources(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// Makes ngspice look for `spinit` only in `dir`. `ngSpice_nospinit` can't be used: in
/// ngspice 46 it dereferences simulator state that only `ngSpice_Init` creates.
#[cfg(windows)]
fn skip_spinit(dir: &Path) -> Result<(), String> {
    unsafe extern "C" {
        fn _putenv_s(name: *const c_char, value: *const c_char) -> c_int;
    }
    // ngspice reads the C runtime's copy of the environment, which `std::env::set_var`
    // does not update. `_putenv_s` updates both.
    let value = CString::new(dir.display().to_string());
    let value = value.map_err(|error| error.to_string())?;
    // SAFETY: both arguments are NUL-terminated strings that the C runtime copies.
    let status = unsafe { _putenv_s(c"SPICE_SCRIPTS".as_ptr(), value.as_ptr()) };
    if status != 0 {
        return Err(format!("SPICE_SCRIPTS could not be set (error {status})."));
    }
    Ok(())
}

#[cfg(not(windows))]
fn skip_spinit(dir: &Path) -> Result<(), String> {
    // SAFETY: called once, under the engine lock, before ngspice starts its threads.
    unsafe { std::env::set_var("SPICE_SCRIPTS", dir) };
    Ok(())
}

#[cfg(windows)]
unsafe fn open(path: &Path) -> Result<Library, libloading::Error> {
    use libloading::os::windows::{LOAD_WITH_ALTERED_SEARCH_PATH, Library as WindowsLibrary};
    // SAFETY: forwarded from the caller. The flag makes Windows find the DLLs ngspice
    // depends on in its own directory.
    unsafe { WindowsLibrary::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH) }
        .map(Library::from)
}

#[cfg(not(windows))]
unsafe fn open(path: &Path) -> Result<Library, libloading::Error> {
    // SAFETY: forwarded from the caller.
    unsafe { Library::new(path) }
}

/// Looks up the function `name` and copies its pointer out of the library.
///
/// # Safety
///
/// `T` must be the function's exact type.
unsafe fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, String> {
    // SAFETY: forwarded from the caller.
    unsafe { library.get::<T>(name) }
        .map(|symbol| *symbol)
        .map_err(|error| with_sources(&error))
}

/// # Safety
///
/// `text` must be null or a valid NUL-terminated string.
unsafe fn owned_string(text: *const c_char) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: checked for null above; validity is the caller's contract.
    Some(
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// # Safety
///
/// `list` must be null or a NULL-terminated array of valid NUL-terminated strings.
unsafe fn owned_strings(list: *const *const c_char) -> Vec<String> {
    let mut strings = Vec::new();
    if list.is_null() {
        return strings;
    }
    let mut index = 0;
    loop {
        // SAFETY: the array is NULL-terminated, so every index up to the null is in bounds.
        let item = unsafe { *list.add(index) };
        // SAFETY: each entry is a valid NUL-terminated string.
        match unsafe { owned_string(item) } {
            Some(text) => strings.push(text),
            None => return strings,
        }
        index += 1;
    }
}

/// The vector's type (`v_type`) and a copy of its values.
///
/// # Safety
///
/// `info` must be null or point to a valid `vector_info` whose data is live.
unsafe fn real_values(info: *const VectorInfo) -> Option<(c_int, Vec<f64>)> {
    // SAFETY: the caller guarantees the pointer is null or valid.
    let info = unsafe { info.as_ref() }?;
    let length = usize::try_from(info.length).ok()?;
    if info.real.is_null() {
        return None;
    }
    // SAFETY: ngspice stores `v_length` doubles at `v_realdata`.
    let values = unsafe { std::slice::from_raw_parts(info.real, length) };
    Some((info.kind, values.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lines_yield_their_percentage() {
        assert_eq!(parse_percent("tran: 45.3%"), Some(45.3));
        assert_eq!(parse_percent("Transient analysis 100%"), Some(100.0));
        assert_eq!(parse_percent("tran 120.0%"), Some(100.0));
        assert_eq!(parse_percent("tran 7.5 %"), Some(7.5));
        assert_eq!(parse_percent("--ready--"), None);
        assert_eq!(parse_percent("%"), None);
    }

    #[test]
    fn a_transient_reports_its_progress() {
        let path = std::env::var_os("TRON_NGSPICE")
            .map(PathBuf::from)
            .expect("TRON_NGSPICE must name the ngspice shared library");
        let lines = [
            "* progress",
            "V1 a 0 DC 1",
            "R1 a b 1e3",
            "C1 b 0 1e-6",
            ".tran 1e-5 1e-1 0 1e-7 uic",
            ".end",
        ];
        let deck = Deck {
            lines: Vec::from(lines.map(str::to_owned)),
            nodes: Default::default(),
            devices: Default::default(),
        };
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        let mut engine = match Engine::lock(&path) {
            Ok(engine) => engine,
            Err(error) => panic!("{error}"),
        };
        let cancel = AtomicBool::new(false);
        let result = engine.run_cancellable(&deck, &cancel, move |percent| {
            sink.lock().unwrap().push(percent);
        });
        let statuses = STATUS_LINES.lock().unwrap().clone();
        if let Err(error) = result {
            panic!("{error}; status lines: {statuses:?}");
        }
        let seen = seen.lock().unwrap();
        let midway = seen.iter().any(|&value| value > 0.0 && value < 100.0);
        assert!(midway, "progress {seen:?}; status lines: {statuses:?}");
    }
}
