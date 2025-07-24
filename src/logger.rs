use crate::config;
use log::{Level, Log, Metadata, Record};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::Write,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
    ffi::CString
};

unsafe extern "C" {
    unsafe fn __android_log_print(prio: i32, tag: *const u8, fmt: *const u8, ...) -> i32;
}

const ANDROID_LOG_VERBOSE: i32 = 2;
const ANDROID_LOG_DEBUG: i32 = 3;
const ANDROID_LOG_INFO: i32 = 4;
const ANDROID_LOG_WARN: i32 = 5;
const ANDROID_LOG_ERROR: i32 = 6;

pub struct SimpleLogger {
    pub file: OnceLock<Mutex<File>>,
    pub buffer: Mutex<VecDeque<(String, bool)>>,
    pub is_levi_launcher: OnceLock<bool>,
}

impl Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let now = SystemTime::now();
        let duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = duration.as_secs() % 86400;
        let millis = duration.subsec_millis();
        let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);

        let timestamp = format!("{:02}:{:02}:{:02}.{:03}", h, m, s, millis);
        let msg = format!("[{}] [{}] {}\n", timestamp, record.level(), record.args());

        let is_levi = *self.is_levi_launcher.get().unwrap_or(&false);
        let tag = if is_levi { "LeviLogger" } else { "BuildLimitChanger" };
        let c_tag = CString::new(tag).unwrap();
        let c_msg = CString::new(format!("{}", record.args())).unwrap();
        let prio = match record.level() {
            Level::Error => ANDROID_LOG_ERROR,
            Level::Warn => ANDROID_LOG_WARN,
            Level::Info => ANDROID_LOG_INFO,
            Level::Debug => ANDROID_LOG_DEBUG,
            Level::Trace => ANDROID_LOG_VERBOSE,
        };
        unsafe {
            __android_log_print(prio, c_tag.as_ptr(), c_msg.as_ptr());
        }

        if let Some(file_mutex) = self.file.get() {
            let path = config::log_path();
            if let Some(ref path) = path {
                if !path.exists() {
                    if let Ok(new_file) = OpenOptions::new().create(true).append(true).open(path) {
                        let _ = file_mutex.lock().map(|mut f| *f = new_file);
                    }
                }
            }

            if let Ok(mut file) = file_mutex.lock() {
                if let Err(e) = file.write_all(msg.as_bytes()) {
                    unsafe { __android_log_print(ANDROID_LOG_ERROR, c_tag.as_ptr(), CString::new(format!("Log write error: {e}")).unwrap().as_ptr()) };
                }
            }
        } else if let Ok(mut buf) = self.buffer.lock() {
            buf.push_back((msg, is_levi));
        }
    }

    fn flush(&self) {}
}

pub static LOGGER: SimpleLogger = SimpleLogger {
    file: OnceLock::new(),
    buffer: Mutex::new(VecDeque::new()),
    is_levi_launcher: OnceLock::new(),
};

pub fn init_log_file(is_levi_launcher: bool) {
    LOGGER
        .is_levi_launcher
        .set(is_levi_launcher)
        .expect("Logger flag already set");

    if let Some(path) = config::log_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("Failed to open log file");

        LOGGER
            .file
            .set(Mutex::new(file))
            .expect("Logger file already set");

        if let (Some(file_mutex), Ok(mut buffer)) = (LOGGER.file.get(), LOGGER.buffer.lock()) {
            let mut file = file_mutex.lock().unwrap();
            while let Some((msg, _)) = buffer.pop_front() {
                let _ = file.write_all(msg.as_bytes());
                let tag = if is_levi_launcher { "LeviLogger" } else { "BuildLimitChanger" };
                let c_tag = CString::new(tag).unwrap();
                let c_msg = CString::new(msg.clone()).unwrap();
                unsafe {
                    __android_log_print(ANDROID_LOG_DEBUG, c_tag.as_ptr(), c_msg.as_ptr());
                }
            }
        }

        log::info!("Logger file initialized at {}", path.display());
    }
}
