use std::{error::Error, ffi::CStr, fs::{self, File}, io::ErrorKind, os::raw::c_char, path::Path};
use jni::{objects::{GlobalRef, JObject, JString}, JNIEnv};
use elf::{endian::AnyEndian, ElfBytes};

#[inline(always)]
pub fn combine_hex(max: i16, min: i16) -> i32 { ((max as i32) << 16) | (min as u16 as i32) }

#[inline(always)]
pub fn split_hex(combined: i32) -> (i16, i16) { ((combined >> 16) as i16, (combined & 0xFFFF) as i16) }

pub fn is_dir_writable(dir: &String) -> bool {
    let path = Path::new(&dir);
    if let Err(e) = fs::create_dir_all(path) {
        if e.kind() != ErrorKind::AlreadyExists {
            return false;
        }
    }
    let test_path = path.join("._perm_test");
    match File::create(&test_path) {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(&test_path);
            true
        }
        Err(_) => false,
    }
}

fn get_absolute_path_from_file(env: &mut JNIEnv, file_obj: JObject) -> Option<String> {
    let abs_path = env
        .call_method(file_obj, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .ok()?.l().ok()?;
    
    env.get_string(&JString::from(abs_path))
        .ok()
        .map(|s| s.into())
}

pub fn get_games_directory(env: &mut JNIEnv) -> Option<String> {
    let env_class = env.find_class("android/os/Environment").ok()?;
    
    let storage_dir = env
        .call_static_method(env_class, "getExternalStorageDirectory", "()Ljava/io/File;", &[])
        .ok()?.l().ok()?;
    
    let mut result = get_absolute_path_from_file(env, storage_dir)?;
    result.push_str("/games");
    Some(result)
}

pub fn get_app_external_files_dir(env: &mut JNIEnv, context: &JObject) -> Option<String> {
    let file_obj = env
        .call_method(context, "getExternalFilesDir", "(Ljava/lang/String;)Ljava/io/File;", &[(&JObject::null()).into()])
        .ok()?.l().ok()?;
    
    get_absolute_path_from_file(env, file_obj)
}

pub fn get_global_context(env: &mut JNIEnv) -> Option<GlobalRef> {
    let activity_thread_class = env.find_class("android/app/ActivityThread").ok()?;
    
    let at_instance = env
        .call_static_method(activity_thread_class, "currentActivityThread", "()Landroid/app/ActivityThread;", &[])
        .ok()?.l().ok()?;
    
    let context = env
        .call_method(at_instance, "getApplication", "()Landroid/app/Application;", &[])
        .ok()?.l().ok()?;
    
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return None;
    }
    
    env.new_global_ref(context).ok()
}

pub fn get_package_name(env: &mut JNIEnv, context: &JObject) -> Option<String> {

    let jstr = env
        .call_method(context, "getPackageName", "()Ljava/lang/String;", &[])
        .ok()?.l().ok()?;

    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return None;
    }

    env.get_string(&JString::from(jstr))
        .ok()
        .map(|s| s.into())
}

pub struct TextMapRange {
    pub start: usize,
    pub size: usize,
}

pub fn find_minecraft_text_section() -> Result<TextMapRange, Box<dyn Error>> {
    let contents = fs::read_to_string("/proc/self/maps")?;
        let mut target_line = None;
    for line in contents.lines() {
        if line.ends_with("libminecraftpe.so") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1].starts_with("r-x") {
                target_line = Some(line);
                break;
            }
        }
    }
    
    let line = target_line.ok_or("libminecraftpe.so executable mapping not found")?;
        let parts: Vec<&str> = line.split_whitespace().collect();
    let addr_range = parts[0];
    let pathname = parts[5];
    
    let dash_pos = addr_range.find('-').unwrap();
    let base_addr = usize::from_str_radix(&addr_range[..dash_pos], 16)?;
    
    let file_data = fs::read(pathname)?;
    let elf = ElfBytes::<AnyEndian>::minimal_parse(&file_data)?;
    
    let section = elf
        .section_header_by_name(".text")?
        .ok_or(".text section not found")?;
    
    let text_addr = base_addr + section.sh_offset as usize;
    let text_size = section.sh_size as usize;
    
    log::info!(
        "libminecraftpe.so .text: addr = 0x{:x}, size = 0x{:x}",
        text_addr, text_size
    );
    
    Ok(TextMapRange {
        start: text_addr,
        size: text_size,
    })
}

#[inline(always)]
pub fn find_max_less_than(data: &[usize], target: usize) -> Option<usize> {
    let mut low = 0;
    let mut high = data.len();

    while low < high {
        let mid = low + ((high - low) >> 1);
        let mid_val = unsafe { *data.get_unchecked(mid) };
        low = if mid_val < target { mid + 1 } else { low };
        high = if mid_val < target { high } else { mid };
    }

    if low == 0 { None } else { Some(unsafe { *data.get_unchecked(low - 1) }) }
}

pub unsafe fn ptr_to_str(a6: *mut u128) -> &'static str {
    CStr::from_ptr((a6 as *const c_char).add(1)).to_str().expect("Failed to get str from ptr")
}