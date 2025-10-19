use std::{ffi::CStr, fs::{self, File}, io::ErrorKind, os::raw::c_char, path::Path};

#[inline(always)]
pub fn combine_hex(max: i16, min: i16) -> i32 {
    ((max as i32) << 16) | (min as u16 as i32)
}

#[inline(always)]
pub fn split_hex(combined: i32) -> (i16, i16) {
    ((combined >> 16) as i16, (combined & 0xFFFF) as i16)
}

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

    if low == 0 {
        None
    } else {
        Some(unsafe { *data.get_unchecked(low - 1) })
    }
}

pub unsafe fn ptr_to_str(a6: *mut u128) -> &'static str {
    CStr::from_ptr((a6 as *const c_char).add(if cfg!(target_os = "android") { 1 } else { 0 }))
            .to_str()
            .expect("Failed to get str from ptr")
}

pub struct TextMapRange { pub start: usize,pub size: usize }

#[cfg(target_os = "android")]
pub use android_specific::*;
#[cfg(target_os = "android")]
mod android_specific {
    use super::TextMapRange;
    use elf::{endian::AnyEndian, ElfBytes};
    use jni::{objects::{GlobalRef, JObject, JString}, JNIEnv};
    use std::{error::Error, fs};

    pub fn get_config_directory(env: &mut JNIEnv) -> Option<String> {
        get_global_context(env).and_then(|ctx| get_games_directory(env).or_else(|| get_app_external_files_dir(env, ctx.as_obj())))
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

        log::info!("libminecraftpe.so .text: addr = 0x{:x}, size = 0x{:x}", text_addr, text_size );

        Ok(TextMapRange { start: text_addr, size: text_size })
    }

    pub fn is_levi_launcher(env: &mut JNIEnv) -> bool {
        get_global_context(env).and_then(|context| get_package_name(env, &context.as_obj())).map_or(false, |name| name == "org.levimc.launcher")
    }

    fn get_absolute_path_from_file(env: &mut JNIEnv, file_obj: JObject) -> Option<String> {
        let abs_path = env
            .call_method(file_obj, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;

        env.get_string(&JString::from(abs_path))
            .ok()
            .map(|s| s.into())
    }

    fn get_games_directory(env: &mut JNIEnv) -> Option<String> {
        let env_class = env.find_class("android/os/Environment").ok()?;

        let storage_dir = env
            .call_static_method(
                env_class,
                "getExternalStorageDirectory",
                "()Ljava/io/File;",
                &[],
            )
            .ok()?
            .l()
            .ok()?;

        let mut result = get_absolute_path_from_file(env, storage_dir)?;
        result.push_str("/games");
        Some(result)
    }

    fn get_app_external_files_dir(env: &mut JNIEnv, context: &JObject) -> Option<String> {
        let file_obj = env
            .call_method(
                context,
                "getExternalFilesDir",
                "(Ljava/lang/String;)Ljava/io/File;",
                &[(&JObject::null()).into()],
            )
            .ok()?
            .l()
            .ok()?;

        get_absolute_path_from_file(env, file_obj)
    }

    fn get_global_context(env: &mut JNIEnv) -> Option<GlobalRef> {
        let activity_thread_class = env.find_class("android/app/ActivityThread").ok()?;

        let at_instance = env
            .call_static_method(
                activity_thread_class,
                "currentActivityThread",
                "()Landroid/app/ActivityThread;",
                &[],
            )
            .ok()?
            .l()
            .ok()?;

        let context = env
            .call_method(
                at_instance,
                "getApplication",
                "()Landroid/app/Application;",
                &[],
            )
            .ok()?
            .l()
            .ok()?;

        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            return None;
        }

        env.new_global_ref(context).ok()
    }

    fn get_package_name(env: &mut JNIEnv, context: &JObject) -> Option<String> {
        let jstr = env
            .call_method(context, "getPackageName", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;

        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            return None;
        }

        env.get_string(&JString::from(jstr)).ok().map(|s| s.into())
    }
}

#[cfg(target_os = "windows")]
pub use windows_specific::*;
#[cfg(target_os = "windows")]
mod windows_specific {
    use super::TextMapRange;
    use std::error::Error;
    use windows_sys::Win32::System::{ LibraryLoader::GetModuleHandleW, Threading::GetCurrentProcess, ProcessStatus::{GetModuleInformation, MODULEINFO} };
    use windows::{Storage::ApplicationData, ApplicationModel::Package};

    pub fn get_config_directory() -> Option<String> {
        if Package::Current().is_ok() {
            ApplicationData::Current().ok()?.RoamingFolder().ok()?.Path().ok().map(|p| p.to_string_lossy())
        } else {
            std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|p| p.to_string_lossy().to_string()))
        }
    }

    pub fn find_minecraft_text_section() -> Result<TextMapRange, Box<dyn Error>> {
        unsafe {
            let h_module = GetModuleHandleW(std::ptr::null());
            if h_module == 0 {
                return Err("Failed to get module handle for main executable".into());
            }

            let mut mod_info = std::mem::zeroed::<MODULEINFO>();
            if GetModuleInformation(GetCurrentProcess(), h_module, &mut mod_info, std::mem::size_of::<MODULEINFO>() as u32) == 0 {
                return Err("GetModuleInformation failed".into());
            }

            let base_addr = mod_info.lpBaseOfDll as usize;
            let image_slice =
                std::slice::from_raw_parts(base_addr as *const u8, mod_info.SizeOfImage as usize);

            let text_section = pelite::PeView::from_bytes(image_slice)?
                .section_headers()
                .iter()
                .find(|s| s.Name.starts_with(b".text"))
                .ok_or(".text section not found")?;

            let text_addr = base_addr + text_section.VirtualAddress as usize;
            let text_size = text_section.VirtualSize as usize;

            log::debug!("Minecraft.Windows.exe .text: addr = 0x{:x}, size = 0x{:x}", text_addr, text_size);

            Ok(TextMapRange { start: text_addr, size: text_size })
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_specific::*;
#[cfg(target_os = "linux")]
mod linux_specific {
    use elf::{endian::AnyEndian, ElfBytes};
    use std::{fs, error::Error};

    use crate::utils::TextMapRange;

    pub fn get_config_directory() -> Option<String> {
        std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|p| p.to_string_lossy().to_string()))
    }
    pub fn find_minecraft_text_section() -> Result<TextMapRange, Box<dyn Error>> {
        let maps = fs::read_to_string("/proc/self/maps")?;
        let exe_path = std::env::current_exe()?;
        let exe_path_str = exe_path.to_string_lossy();

        let mut target_line = None;
        for line in maps.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                continue;
            }

            let perms = parts[1];
            let pathname = parts[5..].join(" ");
            if pathname == exe_path_str && perms.starts_with("r-x") {
                target_line = Some(line.to_string());
                break;
            }
        }

        let line = target_line.ok_or("Current executable 'r-x' mapping not found")?;
        let parts: Vec<&str> = line.split_whitespace().collect();

        let addr_range = parts[0];
        let map_file_offset_str = parts[2];

        let dash_pos = addr_range.find('-').ok_or("Invalid address range")?;
        let map_base_addr = usize::from_str_radix(&addr_range[..dash_pos], 16)?;
        let map_file_offset = usize::from_str_radix(map_file_offset_str, 16)?;

        let file_data = fs::read(&exe_path)?;
        let elf = ElfBytes::<AnyEndian>::minimal_parse(&file_data)?;

        let section = elf
            .section_header_by_name(".text")?
            .ok_or(".text section not found")?;

        let text_section_file_offset = section.sh_offset as usize;
        let text_size = section.sh_size as usize;

        let text_addr = map_base_addr + (text_section_file_offset - map_file_offset);

        log::info!("Current exe .text: addr = 0x{:x}, size = 0x{:x}", text_addr, text_size);

        Ok(TextMapRange { start: text_addr, size: text_size })
    }
}