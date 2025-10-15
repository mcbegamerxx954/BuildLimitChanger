use crate::config;
use log::{Level, Log, Metadata, Record};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::Write,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
#[cfg(target_os = "windows")]
use std::ffi::OsStr;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;

#[cfg(target_os = "android")]
unsafe extern "C" {
    fn __android_log_print(prio: i32, tag: *const u8, fmt: *const u8, ...) -> i32;
}

#[cfg(target_os = "android")]
macro_rules! platform_print {
    ($level:expr, $tag:expr, $msg: expr) => {
        let c_tag = std::ffi::CString::new($tag).unwrap();
        let c_msg = std::ffi::CString::new($msg).unwrap();
        unsafe { __android_log_print(($level as i32 - 7) * -1, c_tag.as_ptr() as *const u8, c_msg.as_ptr() as *const u8); }
    };
}

#[cfg(target_os = "windows")]
macro_rules! platform_print {
    ($level:expr, $tag:expr, $msg: expr) => {
        let formatted_msg = format!("[{}] [{}]: {}\n\0", $tag, $level, $msg);
        let wide_msg: Vec<u16> = OsStr::new(&formatted_msg).encode_wide().collect();
        unsafe { windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW(wide_msg.as_ptr()); }
    };
}
pub struct SimpleLogger {
    pub file: OnceLock<Mutex<File>>,
    pub buffer: Mutex<VecDeque<(String, String)>>,
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
        let msg_less = format!("{}", record.args());
        platform_print!(record.level(), tag, msg_less.clone());
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
                    platform_print!(Level::Error, tag, format!("Log write error: {e}"));
                }
            }
        } else if let Ok(mut buf) = self.buffer.lock() {
            buf.push_back((msg, msg_less));
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

        let file = OpenOptions::new().create(true).append(true).open(&path).expect("Failed to open log file");

        LOGGER
            .file
            .set(Mutex::new(file))
            .expect("Logger file already set");

        if let (Some(file_mutex), Ok(mut buffer)) = (LOGGER.file.get(), LOGGER.buffer.lock()) {
            let mut file = file_mutex.lock().unwrap();
            while let Some((msg, msg_less)) = buffer.pop_front() {
                let _ = file.write_all(msg.as_bytes());
                let tag = if is_levi_launcher { "LeviLogger" } else { "BuildLimitChanger" };
                platform_print!(Level::Debug, tag, msg_less);
            }
        }

        log::info!("Logger file initialized at {}", path.display());
    }
}
