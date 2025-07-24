use crate::{config, utils::{combine_hex, split_hex, ptr_to_str}};
use std::os::raw::c_void;
use core::{mem::transmute};
use dobby_rs::hook;

type Dimension = extern "C" fn(i64, i64, u32, i32, i64, *mut u128) -> i64;
static mut DIMENSION_CONSTRUCTOR: Option<Dimension> = None;

macro_rules! log_dim_change {
    ($cond:expr, $name:expr, $label:expr, $old:expr, $cfg:expr, $new:expr) => {
        if $cond { log::warn!("{name} Dimension Config {label} {cfg} not divisible by 16, aligning to {new}", name=$name, label=$label, cfg=$cfg, new=$new) }
        if $old != $new { log::info!("Changing {name} Dimension {label}: {old} → {new}", name=$name, label=$label, old=$old, new=$new) }
    };
}

#[inline]
fn aligned(val: i16, up: bool) -> i16 {
    let r = val % 16;
    if r == 0 { val } else if up { val + (16 - r) } else { val - r }
}

#[no_mangle]
unsafe extern "C" fn DIMENSION_CONSTRUCTOR_HOOK(
    d: i64, l: i64, id: u32, range: i32, s: i64, label: *mut u128,
) -> i64 {
    match DIMENSION_CONSTRUCTOR {
        Some(orig) => {
            let (max, min) = split_hex(range);
            let name = ptr_to_str(label);
            let (cfg_min, cfg_max) = config::load().get(name).map(|d| (d.min, d.max)).unwrap_or((min, max));
            let new_min = aligned(cfg_min, false);
            let new_max = aligned(cfg_max, true);
            log_dim_change!(cfg_min % 16 != 0, name, "Min", min, cfg_min, new_min);
            log_dim_change!(cfg_max % 16 != 0, name, "Max", max, cfg_max, new_max);

            orig(d, l, id, combine_hex(new_max, new_min), s, label)
        }
        None => {
            log::error!("Original function not set");
            0
        }
    }
}

pub unsafe fn setup_hook(function_addr: usize) {
    let orig_fn_ptr = hook(function_addr as *mut c_void, DIMENSION_CONSTRUCTOR_HOOK as *mut c_void).expect("Failed to install hook");
    DIMENSION_CONSTRUCTOR = Some(transmute(orig_fn_ptr));
    log::debug!("Hooked function at 0x{:X}", function_addr);
}
