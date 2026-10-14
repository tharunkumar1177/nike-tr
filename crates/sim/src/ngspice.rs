//! The ngspice shared library, loaded at runtime.
//!
//! ngspice keeps its state in globals and must be initialized only once per process, so the
//! library is loaded on first use and stays loaded. [`Engine::lock`] hands out exclusive
//! access, and every run starts by clearing the previous circuit and its results.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

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

extern "C" fn send_stat(_text: *const c_char, _id: c_int, _user: *mut c_void) -> c_int {
    0
}

extern "C" fn controlled_exit(
    _status: c_int,
    _unload: bool,
    _quit: bool,
    _id: c_int,
    _user: *mut c_void,
) -> c_int {
    EXITED.store(true, Ordering::SeqCst);
    0
}

extern "C" fn background_running(_running: bool, _id: c_int, _user: *mut c_void) -> c_int {
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
        // SAFETY: loading ngspice runs no initializers that depend on Tron's state.
        let library = unsafe { open(path) }.map_err(|error| failed(error.to_string()))?;
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
        Ok(Engine {
            path: path.to_owned(),
            circ,
            command,
            vec_info,
            cur_plot,
            all_vecs,
            _library: library,
        })
    }

    /// Loads `deck`, runs its analysis to completion, and returns the results.
    pub fn run(&mut self, deck: &Deck) -> Result<Plot, EngineError> {
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
        let status = self.command("run")?;
        let errors = take_errors();
        if status != 0 || !errors.is_empty() {
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
        let command = CString::new(command).expect("commands are fixed ASCII text");
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
        for vector in names {
            let qualified = CString::new(format!("{name}.{vector}")).map_err(|_| no_results())?;
            // SAFETY: ngspice returns a pointer to its own vector record, or null. The data
            // stays valid until the next command, and is copied before then.
            let values = unsafe { real_values((self.vec_info)(qualified.as_ptr())) };
            if let Some(values) = values {
                vectors.push(Vector {
                    name: vector.to_ascii_lowercase(),
                    values,
                });
            }
        }
        Ok(Plot { name, vectors })
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
        .map_err(|error| error.to_string())
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

/// # Safety
///
/// `info` must be null or point to a valid `vector_info` whose data is live.
unsafe fn real_values(info: *const VectorInfo) -> Option<Vec<f64>> {
    // SAFETY: the caller guarantees the pointer is null or valid.
    let info = unsafe { info.as_ref() }?;
    let length = usize::try_from(info.length).ok()?;
    if info.real.is_null() {
        return None;
    }
    // SAFETY: ngspice stores `v_length` doubles at `v_realdata`.
    Some(unsafe { std::slice::from_raw_parts(info.real, length) }.to_vec())
}
