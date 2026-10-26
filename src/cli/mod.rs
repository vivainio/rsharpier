//! The command line: `format` and `check`, a port of CSharpier.Cli

pub mod config;
pub mod glob;
pub mod ignore_file;

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

use crate::options::{format_contents, FileOptions, FormatOutcome, Formatter};
use config::OptionsProvider;
use ignore_file::IgnoreFile;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum LogLevel {
    Debug,
    Information,
    Warning,
    Error,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogFormat {
    Console,
    MsBuild,
}

#[derive(Clone, Debug)]
pub struct CommandLineOptions {
    pub paths: Vec<String>,
    pub check: bool,
    pub skip_validation: bool,
    pub skip_write: bool,
    pub write_stdout: bool,
    pub include_generated: bool,
    pub syntax_errors_as_warnings: bool,
    pub unformatted_as_warnings: bool,
    pub config_path: Option<String>,
    pub ignore_path: Option<String>,
    pub stdin_path: Option<String>,
    pub log_format: LogFormat,
    pub log_level: LogLevel,
}

impl Default for CommandLineOptions {
    fn default() -> Self {
        CommandLineOptions {
            paths: vec![],
            check: false,
            skip_validation: false,
            skip_write: false,
            write_stdout: false,
            include_generated: false,
            syntax_errors_as_warnings: false,
            unformatted_as_warnings: false,
            config_path: None,
            ignore_path: None,
            stdin_path: None,
            log_format: LogFormat::Console,
            log_level: LogLevel::Information,
        }
    }
}

pub struct Logger {
    level: LogLevel,
    format: LogFormat,
    lock: Mutex<()>,
    out: Mutex<Box<dyn Write + Send>>,
    err: Mutex<Box<dyn Write + Send>>,
}

impl Logger {
    pub fn new(level: LogLevel, format: LogFormat) -> Logger {
        Logger {
            level,
            format,
            lock: Mutex::new(()),
            out: Mutex::new(Box::new(std::io::stdout())),
            err: Mutex::new(Box::new(std::io::stderr())),
        }
    }

    /// for tests: capture output
    pub fn with_writers(level: LogLevel, format: LogFormat, out: Box<dyn Write + Send>, err: Box<dyn Write + Send>) -> Logger {
        Logger { level, format, lock: Mutex::new(()), out: Mutex::new(out), err: Mutex::new(err) }
    }

    fn write(&self, level: LogLevel, message: &str) {
        if level < self.level {
            return;
        }
        let _g = self.lock.lock().unwrap();
        let mut text = String::new();
        if self.format == LogFormat::Console && level >= LogLevel::Warning {
            text.push_str(match level {
                LogLevel::Warning => "Warning ",
                _ => "Error ",
            });
        }
        let mut lines = message.lines();
        if let Some(first) = lines.next() {
            text.push_str(first);
            text.push('\n');
        } else {
            text.push('\n');
        }
        for l in lines {
            text.push_str("  ");
            text.push_str(l);
            text.push('\n');
        }
        if level >= LogLevel::Error {
            let _ = self.err.lock().unwrap().write_all(text.as_bytes());
        } else {
            let _ = self.out.lock().unwrap().write_all(text.as_bytes());
        }
    }

    pub fn info(&self, m: &str) {
        self.write(LogLevel::Information, m)
    }
    pub fn debug(&self, m: &str) {
        self.write(LogLevel::Debug, m)
    }
    fn file_message(&self, path: &str, m: &str) -> String {
        match self.format {
            LogFormat::MsBuild => format!("{path}: error: {m}"),
            LogFormat::Console => format!("{path} - {m}"),
        }
    }
    pub fn file_warning(&self, path: &str, m: &str) {
        self.write(LogLevel::Warning, &self.file_message(path, m))
    }
    pub fn file_error(&self, path: &str, m: &str) {
        self.write(LogLevel::Error, &self.file_message(path, m))
    }
}

#[derive(Default)]
struct Counters {
    files: AtomicUsize,
    unformatted: AtomicUsize,
    failed_compilation: AtomicUsize,
    failed_validation: AtomicUsize,
    exceptions: AtomicUsize,
}

// ---- file reading ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

pub fn decode(bytes: &[u8]) -> (Encoding, String) {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return (Encoding::Utf8Bom, String::from_utf8_lossy(rest).into_owned());
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return (Encoding::Utf16Le, String::from_utf16_lossy(&units));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        return (Encoding::Utf16Be, String::from_utf16_lossy(&units));
    }
    (Encoding::Utf8, String::from_utf8_lossy(bytes).into_owned())
}

pub fn encode(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Utf8Bom => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            v
        }
        Encoding::Utf16Le => {
            let mut v = vec![0xFF, 0xFE];
            for u in text.encode_utf16() {
                v.extend_from_slice(&u.to_le_bytes());
            }
            v
        }
        Encoding::Utf16Be => {
            let mut v = vec![0xFE, 0xFF];
            for u in text.encode_utf16() {
                v.extend_from_slice(&u.to_be_bytes());
            }
            v
        }
    }
}

// ---- helpers --------------------------------------------------------------------------------

pub fn is_generated_code_file(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name.starts_with("temporarygeneratedfile_") {
        return true;
    }
    if path.extension().is_none() {
        return false;
    }
    let stem = path.file_stem().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    stem.ends_with(".designer") || stem.ends_with(".generated") || stem.ends_with(".g") || stem.ends_with(".g.i")
}

/// Port of StringDiffer.PrintFirstDifference
pub fn print_first_difference(expected: &str, actual: &str) -> String {
    if expected == actual {
        return String::new();
    }
    let ends_with_exactly_one_newline = |v: &str| {
        (!v.ends_with("\r\n") && v.ends_with('\n') && !v.ends_with("\n\n"))
            || (v.ends_with("\r\n") && !v.ends_with("\r\n\r\n"))
    };
    if !ends_with_exactly_one_newline(actual) {
        return "The file did not end with a single newline.".into();
    }
    let visible = |s: &str| s.replace(' ', "·").replace('\t', "→");
    let mut e = expected.lines();
    let mut a = actual.lines();
    let (mut el, mut al) = (e.next(), a.next());
    let mut line = 1;
    let (mut prev_e, mut prev_a): (Option<&str>, Option<&str>) = (None, None);
    while el.is_some() || al.is_some() {
        if el == al {
            line += 1;
            prev_e = el;
            prev_a = al;
            el = e.next();
            al = a.next();
            continue;
        }
        let mut sb = String::new();
        sb.push_str(&format!("----------------------------- Expected: Around Line {line} -----------------------------\n"));
        if let Some(p) = prev_e {
            sb.push_str(p);
            sb.push('\n');
        }
        sb.push_str(el.unwrap_or(""));
        sb.push('\n');
        if let Some(n) = e.next() {
            sb.push_str(n);
            sb.push('\n');
        }
        sb.push_str(&format!("----------------------------- Actual: Around Line {line} -----------------------------\n"));
        if let Some(p) = prev_a {
            sb.push_str(p);
            sb.push('\n');
        }
        let al_text = al.unwrap_or("");
        if al_text.trim().is_empty() {
            sb.push_str(&visible(al_text));
        } else if al_text.ends_with([' ', '\t']) {
            let idx = al_text.trim_end_matches([' ', '\t']).len();
            sb.push_str(&al_text[..idx]);
            sb.push_str(&visible(&al_text[idx..]));
        } else {
            sb.push_str(al_text);
        }
        sb.push('\n');
        if let Some(n) = a.next() {
            sb.push_str(n);
            sb.push('\n');
        }
        return sb;
    }
    "The file contained different line endings than formatting it would result in.".into()
}

struct PhysicalPath {
    actual: PathBuf,
    supplied: String,
    directory: PathBuf,
    is_file: bool,
}

struct PhysicalFile {
    actual: PathBuf,
    supplied: String,
    warn_for_unsupported: bool,
    ignore_root: PathBuf,
}

fn full_path(p: &str) -> PathBuf {
    let path = Path::new(p);
    let abs = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
    // normalise `.` and `..` without touching the filesystem
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

struct Engine<'a> {
    opts: &'a CommandLineOptions,
    logger: &'a Logger,
    provider: OptionsProvider,
    ignore_files: Mutex<std::collections::HashMap<PathBuf, Arc<IgnoreFile>>>,
    specified_ignore: Option<PathBuf>,
    counters: Counters,
}

impl Engine<'_> {
    fn ignore_for(&self, dir: &Path, ignore_root: Option<&Path>) -> Result<Arc<IgnoreFile>, String> {
        if let (Some(spec), Some(root)) = (&self.specified_ignore, ignore_root) {
            let mut cache = self.ignore_files.lock().unwrap();
            if let Some(f) = cache.get(root) {
                return Ok(f.clone());
            }
            let f = Arc::new(IgnoreFile::create(root, Some(spec))?);
            cache.insert(root.to_path_buf(), f.clone());
            return Ok(f);
        }
        // walk up to the nearest directory that has an ignore file, cache by directory
        let mut cache = self.ignore_files.lock().unwrap();
        if let Some(f) = cache.get(dir) {
            return Ok(f.clone());
        }
        let f = Arc::new(IgnoreFile::create(dir, None)?);
        cache.insert(dir.to_path_buf(), f.clone());
        Ok(f)
    }

    fn is_ignored(&self, path: &Path, is_dir: bool, ignore_root: Option<&Path>) -> Result<bool, String> {
        let dir = if is_dir { path.parent() } else { path.parent() }.unwrap_or(Path::new("/"));
        Ok(self.ignore_for(dir, ignore_root)?.is_ignored(path, is_dir))
    }

    fn enumerate(&self, paths: &[PhysicalPath]) -> Result<Vec<PhysicalFile>, String> {
        let mut seen = std::collections::HashSet::new();
        let mut explicit = std::collections::HashMap::new();
        for p in paths.iter().filter(|p| p.is_file) {
            explicit.entry(p.actual.clone()).or_insert((p.supplied.clone(), p.directory.clone()));
        }
        let mut result = Vec::new();
        for p in paths {
            if p.is_file {
                if seen.insert(p.actual.clone()) {
                    result.push(PhysicalFile {
                        actual: p.actual.clone(),
                        supplied: p.supplied.clone(),
                        warn_for_unsupported: true,
                        ignore_root: p.directory.clone(),
                    });
                }
                continue;
            }
            let mut files = Vec::new();
            self.enumerate_non_ignored(&p.actual, &p.directory, &mut files)?;
            for f in files {
                if seen.insert(f.clone()) {
                    result.push(match explicit.get(&f) {
                        Some((supplied, dir)) => PhysicalFile {
                            actual: f,
                            supplied: supplied.clone(),
                            warn_for_unsupported: true,
                            ignore_root: dir.clone(),
                        },
                        None => {
                            let rel = f.strip_prefix(&p.actual).unwrap_or(&f).to_string_lossy().to_string();
                            PhysicalFile {
                                supplied: format!("{}{}{}", p.supplied, std::path::MAIN_SEPARATOR, rel),
                                actual: f,
                                warn_for_unsupported: false,
                                ignore_root: p.directory.clone(),
                            }
                        }
                    });
                }
            }
        }
        Ok(result)
    }

    fn enumerate_non_ignored(&self, dir: &Path, ignore_root: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        let mut entries: Vec<_> = match std::fs::read_dir(dir) {
            Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
            Err(_) => return Ok(()),
        };
        entries.sort_by_key(|e| e.file_name());
        for e in &entries {
            if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                out.push(e.path());
            }
        }
        for e in &entries {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let p = e.path();
                if self.is_ignored(&p, true, Some(ignore_root))? {
                    continue;
                }
                self.enumerate_non_ignored(&p, ignore_root, out)?;
            }
        }
        Ok(())
    }

    fn format_file(&self, file: &PhysicalFile, writer: &dyn Fn(&Path, &str, Encoding, &str)) {
        if (!self.opts.include_generated && is_generated_code_file(&file.actual))
            || self.is_ignored(&file.actual, false, Some(&file.ignore_root)).unwrap_or(false)
        {
            return;
        }
        let options = self.provider.options_for(&file.actual);
        let unsupported = !matches!(&options, Some(o) if o.formatter != Formatter::Unknown);
        if unsupported {
            if file.warn_for_unsupported {
                self.logger.file_warning(&file.supplied, "Is an unsupported file type.");
            }
            return;
        }
        let options = options.unwrap();
        self.logger.debug(&format!("{} - {}", if self.opts.check { "Checking" } else { "Formatting" }, file.supplied));
        let bytes = match std::fs::read(&file.actual) {
            Ok(b) => b,
            Err(e) => {
                self.logger.file_error(&file.supplied, &format!("Threw exception while reading: {e}"));
                self.counters.exceptions.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let (encoding, contents) = decode(&bytes);
        self.format_steps(&file.actual, &file.supplied, &contents, encoding, &options, writer);
    }

    fn format_steps(&self, path: &Path, supplied: &str, contents: &str, encoding: Encoding, options: &FileOptions, writer: &dyn Fn(&Path, &str, Encoding, &str)) {
        if contents.is_empty() {
            return;
        }
        self.counters.files.fetch_add(1, Ordering::Relaxed);
        let outcome = format_contents(contents, options, self.opts.include_generated, !self.opts.skip_validation);
        let code = match outcome {
            FormatOutcome::Formatted(code) => code,
            FormatOutcome::SyntaxError(e) => {
                let msg = format!("Was not formatted due to syntax errors.\n{e}");
                if self.opts.syntax_errors_as_warnings {
                    self.logger.file_warning(supplied, &msg);
                } else {
                    self.logger.file_error(supplied, &msg);
                }
                self.counters.failed_compilation.fetch_add(1, Ordering::Relaxed);
                return;
            }
            FormatOutcome::Warning(w) => {
                self.logger.file_warning(supplied, &w);
                return;
            }
            FormatOutcome::Failure(f) => {
                self.logger.file_error(supplied, &f);
                if f.starts_with("Failed formatting validation") {
                    self.counters.failed_validation.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.counters.exceptions.fetch_add(1, Ordering::Relaxed);
                }
                return;
            }
        };

        if self.opts.check && !self.opts.write_stdout && code != contents {
            let difference = print_first_difference(&code, contents);
            let message = format!("Was not formatted.\n{difference}\n");
            if self.opts.unformatted_as_warnings {
                self.logger.file_warning(supplied, &message);
            } else {
                self.logger.file_error(supplied, &message);
            }
            self.counters.unformatted.fetch_add(1, Ordering::Relaxed);
        }
        writer(path, &code, encoding, contents);
    }

    fn exit_code(&self) -> i32 {
        let c = &self.counters;
        let bad = (!self.opts.syntax_errors_as_warnings && c.failed_compilation.load(Ordering::Relaxed) > 0)
            || (self.opts.check && c.unformatted.load(Ordering::Relaxed) > 0 && !self.opts.unformatted_as_warnings)
            || c.failed_validation.load(Ordering::Relaxed) > 0
            || c.exceptions.load(Ordering::Relaxed) > 0;
        i32::from(bad)
    }
}

/// Runs `format` or `check`, returns the exit code
pub fn run(opts: CommandLineOptions) -> i32 {
    let logger = Logger::new(opts.log_level, opts.log_format);
    run_with(opts, &logger, &mut std::io::stdin().lock())
}

pub fn run_with(mut opts: CommandLineOptions, logger: &Logger, stdin: &mut dyn Read) -> i32 {
    let start = std::time::Instant::now();
    let stdin_is_piped = !std::io::stdin().is_terminal();

    let mut standard_in: Option<String> = None;
    if opts.paths.is_empty() {
        if !stdin_is_piped {
            eprintln!("directoryOrFile is required when not piping stdin to CSharpier");
            return 1;
        }
        let mut bytes = Vec::new();
        let _ = stdin.read_to_end(&mut bytes);
        standard_in = Some(decode(&bytes).1);
        opts.write_stdout = true;
    }

    let config_path = opts.config_path.as_deref().map(Path::new);
    let provider = match OptionsProvider::new(config_path) {
        Ok(p) => p,
        Err(e) => {
            logger.write(LogLevel::Error, &format!("Failed to read the configuration file: {e}"));
            return 1;
        }
    };
    let engine = Engine {
        opts: &opts,
        logger,
        provider,
        ignore_files: Mutex::new(Default::default()),
        specified_ignore: opts.ignore_path.as_deref().map(|p| full_path(p)),
        counters: Counters::default(),
    };

    if let Some(contents) = standard_in {
        let cwd = std::env::current_dir().unwrap_or_default();
        let (file_path, path_supplied) = match &opts.stdin_path {
            Some(p) => (full_path(p), true),
            None => {
                let ext = if contents.trim_start().starts_with('<') { "xml" } else { "cs" };
                (cwd.join(format!("stdin.{ext}")), false)
            }
        };
        if path_supplied && engine.is_ignored(&file_path, false, None).unwrap_or(false) {
            // ignored files are written back untouched
            print!("{contents}");
            return 0;
        }
        let Some(options) = engine.provider.options_for(&file_path) else {
            print!("{contents}");
            return 0;
        };
        let stdout_writer = |_: &Path, code: &str, _: Encoding, _: &str| {
            let _ = std::io::stdout().write_all(code.as_bytes());
        };
        engine.format_steps(&file_path, opts.stdin_path.as_deref().unwrap_or("stdin"), &contents, Encoding::Utf8, &options, &stdout_writer);
        return engine.exit_code();
    }

    // physical files
    let mut paths = Vec::new();
    for supplied in &opts.paths {
        let actual = full_path(supplied);
        let is_file = actual.is_file();
        let is_dir = actual.is_dir();
        if !is_file && !is_dir {
            eprintln!("There was no file or directory found at {supplied}");
            return 1;
        }
        let directory = if is_file { actual.parent().unwrap_or(Path::new("/")).to_path_buf() } else { actual.clone() };
        let mut supplied_path = supplied.clone();
        if !Path::new(&supplied_path).is_absolute() && !supplied_path.starts_with('.') {
            supplied_path = format!(".{}{}", std::path::MAIN_SEPARATOR, supplied_path);
        }
        paths.push(PhysicalPath { actual, supplied: supplied_path, directory, is_file });
    }

    let files = match engine.enumerate(&paths) {
        Ok(f) => f,
        Err(e) => {
            logger.write(LogLevel::Error, &e);
            return 1;
        }
    };

    let write_stdout = opts.write_stdout;
    let check = opts.check;
    let skip_write = opts.skip_write;
    let writer = move |path: &Path, code: &str, encoding: Encoding, original: &str| {
        if write_stdout {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(code.as_bytes());
        } else if !check && !skip_write && code != original {
            let _ = std::fs::write(path, encode(code, encoding));
        }
    };
    if write_stdout {
        files.iter().for_each(|f| engine.format_file(f, &writer));
    } else {
        files.par_iter().for_each(|f| engine.format_file(f, &writer));
    }

    if !opts.write_stdout {
        logger.info(&format!(
            "{} {} files in {}ms.",
            if opts.check { "Checked" } else { "Formatted" },
            engine.counters.files.load(Ordering::Relaxed),
            start.elapsed().as_millis()
        ));
    }
    engine.exit_code()
}
