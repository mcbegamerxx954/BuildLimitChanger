mod config;
mod hook;
mod logger;
mod utils;

use std::time::Instant;

fn find_water_mob_cap_and_fn_starts(data: &[u8]) -> (Option<usize>, Vec<usize>) {
    let mut seen_ret = false;
    let mut possible_fn_starts = Vec::new();
    let mut last_possible_water_mob_cap: Option<usize> = None;
    let mut water_mob_cap: Option<usize> = None;
    let mut closest_distance = usize::MAX;

    #[cfg(target_arch = "aarch64")] {
        const MASKS: [u32; 3] = [0xFFFF_FC1F, 0xFFFF_FFE0, 0xFF00_0000];
        const PATTERNS: [u32; 3] = [0xD65F_0000, 0x52A8_4200, 0xD100_0000];

        for inst in data.chunks_exact(4) {
            let addr = inst.as_ptr() as usize;
            let instr = u32::from_le_bytes(inst.try_into().unwrap());
            if (instr & MASKS[0]) == PATTERNS[0] {
                seen_ret = true;
            } else if (instr & MASKS[1]) == PATTERNS[1] {
                if let Some(prev) = last_possible_water_mob_cap {
                    let dist = addr.wrapping_sub(prev);
                    if dist < closest_distance {
                        closest_distance = dist;
                        water_mob_cap = Some(addr);
                    }
                }
                last_possible_water_mob_cap = Some(addr);
            } else if seen_ret && (instr & MASKS[2]) == PATTERNS[2] {
                possible_fn_starts.push(addr);
                seen_ret = false;
            }
        }
    }
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))] {
        use iced_x86::{Decoder, Instruction, Mnemonic};
        #[cfg(target_arch = "x86_64")] const BITNESS: u32 = 64;
        #[cfg(target_arch = "x86")] const BITNESS: u32 = 32;
        let mut decoder = Decoder::new(BITNESS, data, iced_x86::DecoderOptions::NO_INVALID_CHECK);
        decoder.set_ip(data.as_ptr() as u64);
        let mut instruction = Instruction::default();

        const TARGET_IMMEDIATE: u64 = 0x42100000;

        while decoder.can_decode() {
            decoder.decode_out(&mut instruction);

            match instruction.mnemonic() {
                Mnemonic::Ret => seen_ret = true,
                Mnemonic::Mov => {
                    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
                    if seen_ret {
                        possible_fn_starts.push(instruction.ip() as usize);
                        seen_ret = false;
                        continue;
                    }
                    if instruction.try_immediate(1).unwrap_or(0) == TARGET_IMMEDIATE {
                        let addr = instruction.ip() as usize;
                        if let Some(prev) = last_possible_water_mob_cap {
                            let dist = addr.wrapping_sub(prev);
                            if dist < closest_distance {
                                closest_distance = dist;
                                water_mob_cap = Some(addr);
                            }
                        }
                        last_possible_water_mob_cap = Some(addr);
                    }
                },
                #[cfg(any(target_os = "android", target_os = "linux", all(target_os = "windows", target_arch = "x86")))]
                Mnemonic::Push if seen_ret => {
                    possible_fn_starts.push(instruction.ip() as usize);
                    seen_ret = false;
                }
                _ => {}
            }
        }
    }

    (water_mob_cap, possible_fn_starts)
}

#[ctor::ctor]
fn init() {
    println!("Starting BuildLimitChanger");
    let time_start = Instant::now();
    log::set_logger(&logger::LOGGER).expect("Logger already set");
    log::set_max_level(log::LevelFilter::Debug);

    #[cfg(any(target_os = "windows", target_os = "linux"))] {
        crate::config::init_config();
        crate::logger::init_log_file(false);
    }
    let mcmap = utils::find_minecraft_text_section().expect("Cannot find Minecraft .text section");
    let data = unsafe { std::slice::from_raw_parts(mcmap.start as *const u8, mcmap.size) };

    let (water_mob_cap, possible_fn_starts) = find_water_mob_cap_and_fn_starts(data);

    if water_mob_cap.is_none() {
        log::error!("Cannot find the water mob cap");
        return;
    }
    let Some(function_addr) = utils::find_max_less_than(&possible_fn_starts, water_mob_cap.unwrap()) else {
        log::error!("Cannot get the function where water mob cap is located");
        return;
    };
    log::debug!("Function Offset: 0x{:X}", function_addr);
    log::debug!("{:02X?}", &data[function_addr - mcmap.start..(function_addr - mcmap.start + 10).min(data.len())]);

    hook::setup_hook(function_addr);
    log::debug!("{:02X?}", &data[function_addr - mcmap.start..(function_addr - mcmap.start + 10).min(data.len())]);
    log::info!("Took: {:?}", time_start.elapsed());
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: jni::JavaVM, _: *mut core::ffi::c_void) -> i32 {
    let mut env = vm.get_env().expect("Cannot get reference to the JNIEnv");
    config::init_config(&mut env);
    logger::init_log_file(crate::utils::is_levi_launcher(&mut env));
    return jni::sys::JNI_VERSION_1_6;
}