mod utils;
mod modification;
mod config;
mod logger;

use core::slice;
use std::{os::raw::c_void, time::Instant};
use jni::{JavaVM, sys::{jint, JNI_VERSION_1_6}};
use utils::{find_minecraft_text_section, find_max_less_than, get_global_context, get_package_name};
use log::LevelFilter;

const RET_MASK: u32 = 0xFFFF_FC1F;
const RET_PATTERN: u32 = 0xD65F_0000;
const MOVZ_MASK: u32 = 0xFFFF_FFE0;
const MOVZ_PATTERN: u32 = 0x52A8_4200;
const SUB_MASK: u32 = 0xFF00_0000;
const SUB_PATTERN: u32 = 0xD100_0000;
const INSTR_SIZE: usize = 4;

fn find_water_mob_cap_and_fn_starts(data: &[u8]) -> (Option<usize>, Vec<usize>) {
    let mut seen_ret = false;
    let mut possible_fn_starts = Vec::new();
    let mut last_possible_water_mob_cap: Option<usize> = None;
    let mut water_mob_cap: Option<usize> = None;
    let mut closest_distance = usize::MAX;

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

#[ctor::ctor]
fn init() {
    log::set_logger(&logger::LOGGER).expect("Logger already set");
    log::set_max_level(LevelFilter::Debug);
    log::info!("--------- Logger initialized (no file yet) ---------");

    let mcmap = find_minecraft_text_section().expect("Cannot find libminecraftpe.so in memory maps");
    let time_start = Instant::now();
    let data = unsafe { slice::from_raw_parts(mcmap.start as *const u8, mcmap.size) };

    let mut seen_ret = false;
    let mut possible_fn_starts: Vec<usize> = Vec::new();
    let mut last_possible_water_mob_cap: Option<usize> = None;
    let mut water_mob_cap: Option<usize> = None;
    let mut closest_distance = usize::MAX;
    let len = data.len();

    let (water_mob_cap, possible_fn_starts) = find_water_mob_cap_and_fn_starts(data);

    if water_mob_cap.is_none() {
        log::error!("Cannot find the water mob cap");
        return;
    }
    let Some(function_addr) = find_max_less_than(&possible_fn_starts, water_mob_cap.unwrap()) else {
        log::error!("Cannot get the function where water mob cap is located");
        return;
    };
    log::debug!("Function Offset: 0x{:X}", function_addr);
    modification::setup_hook(function_addr);
    log::debug!("Took: {:?}", time_start.elapsed())
}

#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
    let mut env = vm.get_env().expect("Cannot get reference to the JNIEnv");
    config::init_config(&mut env);
    let is_levi_launcher = get_global_context(&mut env)
    .and_then(|context| get_package_name(&mut env, &context.as_obj()))
    .map_or(false, |name| name == "org.levimc.launcher");
    logger::init_log_file(is_levi_launcher);
    JNI_VERSION_1_6
}
