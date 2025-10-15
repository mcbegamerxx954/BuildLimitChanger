mod config;
mod logger;
mod modification;
mod utils;

use log::LevelFilter;
use std::time::Instant;

#[cfg(target_arch = "aarch64")]
fn find_water_mob_cap_and_fn_starts(data: &[u8]) -> (Option<usize>, Vec<usize>) {
    let mut seen_ret = false;
    let mut possible_fn_starts = Vec::new();
    let mut last_possible_water_mob_cap: Option<usize> = None;
    let mut water_mob_cap: Option<usize> = None;
    let mut closest_distance = usize::MAX;

    const RET_MASK: u32 = 0xFFFF_FC1F;
    const RET_PATTERN: u32 = 0xD65F_0000;
    const MOVZ_MASK: u32 = 0xFFFF_FFE0;
    const MOVZ_PATTERN: u32 = 0x52A8_4200;
    const SUB_MASK: u32 = 0xFF00_0000;
    const SUB_PATTERN: u32 = 0xD100_0000;
    const INSTR_SIZE: usize = 4;

    for inst in data.chunks_exact(INSTR_SIZE) {
        let addr = inst.as_ptr() as usize;
        let instr = u32::from_le_bytes(inst.try_into().unwrap());
        if (instr & RET_MASK) == RET_PATTERN {
            seen_ret = true;
        } else if (instr & MOVZ_MASK) == MOVZ_PATTERN {
            if let Some(prev) = last_possible_water_mob_cap {
                let dist = addr.wrapping_sub(prev);
                if dist < closest_distance {
                    closest_distance = dist;
                    water_mob_cap = Some(addr);
                }
            }
            last_possible_water_mob_cap = Some(addr);
        } else if seen_ret && (instr & SUB_MASK) == SUB_PATTERN {
            possible_fn_starts.push(addr);
            seen_ret = false;
        }
    }

    (water_mob_cap, possible_fn_starts)
}

#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
fn find_water_mob_cap_and_fn_starts(data: &[u8]) -> (Option<usize>, Vec<usize>) {
    let mut seen_ret = false;
    let mut possible_fn_starts = Vec::new();
    let mut last_possible_water_mob_cap: Option<usize> = None;
    let mut water_mob_cap: Option<usize> = None;
    let mut closest_distance = usize::MAX;

    #[cfg(target_arch = "x86_64")]
    const BITNESS: u32 = 64;
    #[cfg(target_arch = "x86")]
    const BITNESS: u32 = 32;
    use iced_x86::{Decoder, Instruction, Mnemonic};
    let mut decoder = Decoder::new(BITNESS, data, iced_x86::DecoderOptions::NO_INVALID_CHECK);
    decoder.set_ip(data.as_ptr() as u64);
    let mut instruction = Instruction::default();

    const TARGET_IMMEDIATE: u64 = 0x42100000;

    while decoder.can_decode() {
        decoder.decode_out(&mut instruction);

        match instruction.mnemonic() {
            Mnemonic::Ret => {
                seen_ret = true;
            }

            Mnemonic::Mov => {
                #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
                if seen_ret {
                    possible_fn_starts.push(instruction.ip() as usize);
                    seen_ret = false;
                }
                if instruction.try_immediate(1).unwrap_or(0) == TARGET_IMMEDIATE {
                    let addr = instruction.ip() as usize;
                    log::info!("found at: 0x{:X}", addr);
                    if let Some(prev) = last_possible_water_mob_cap {
                        let dist = addr.wrapping_sub(prev);
                        if dist < closest_distance {
                            closest_distance = dist;
                            water_mob_cap = Some(addr);
                        }
                    }
                    last_possible_water_mob_cap = Some(addr);
                }
            }

            #[cfg(any(target_os = "android", all(target_os = "windows", target_arch = "x86")))]
            Mnemonic::Push if seen_ret => {
                possible_fn_starts.push(instruction.ip() as usize);
                seen_ret = false;
            }

            _ => {}
        }
    }

    (water_mob_cap, possible_fn_starts)
}

#[ctor::ctor]
fn init() {
    let time_start = Instant::now();
    log::set_logger(&logger::LOGGER).expect("Logger already set");
    log::set_max_level(LevelFilter::Debug);
    log::info!("--------- Logger initialized ---------");

    let mcmap = utils::find_minecraft_text_section().expect("Cannot find Minecraft .text section");
    let data = unsafe { std::slice::from_raw_parts(mcmap.start as *const u8, mcmap.size) };

    let (water_mob_cap, possible_fn_starts) = find_water_mob_cap_and_fn_starts(data);

    if water_mob_cap.is_none() {
        log::error!("Cannot find the water mob cap");
        return;
    }
    let Some(function_addr) = utils::find_max_less_than(&possible_fn_starts, water_mob_cap.unwrap())
    else {
        log::error!("Cannot get the function where water mob cap is located");
        return;
    };
    log::debug!("Function Offset: 0x{:X}", function_addr);
    log::debug!("{:02X?}", &data[function_addr - mcmap.start..(function_addr - mcmap.start + 10).min(data.len())]);

    modification::setup_hook(function_addr);
    log::debug!("{:02X?}", &data[function_addr - mcmap.start..(function_addr - mcmap.start + 10).min(data.len())]);
    log::debug!("Took: {:?}", time_start.elapsed());
    #[cfg(target_os = "windows")] {
        crate::config::init_config();
        crate::logger::init_log_file(false);
    }
}

#[no_mangle]
#[cfg(target_os = "android")]
pub extern "system" fn JNI_OnLoad(vm: jni::JavaVM, _: *mut core::ffi::c_void) -> i32 {
    use utils::{get_global_context, get_package_name};
    let mut env = vm.get_env().expect("Cannot get reference to the JNIEnv");
    config::init_config(&mut env);

    let is_levi_launcher = get_global_context(&mut env)
        .and_then(|context| get_package_name(&mut env, &context.as_obj()))
        .map_or(false, |name| name == "org.levimc.launcher");

    logger::init_log_file(is_levi_launcher);
    return jni::sys::JNI_VERSION_1_6;
}
