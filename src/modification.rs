macro_rules! log_dim_change {
    ($cond:expr, $name:expr, $label:expr, $old:expr, $cfg:expr, $new:expr) => {
        if $cond { log::warn!("{name} Dimension Config {label} {cfg} not divisible by 16, aligning to {new}", name=$name, label=$label, cfg=$cfg, new=$new) }
        if $old != $new { log::info!("Changing {name} Dimension {label}: {old} → {new}", name=$name, label=$label, old=$old, new=$new) }
    };
}

macro_rules! aligned {
    ($val:expr, $up:expr) => {{
        let r = $val % 16;
        if r == 0 { $val } else if $up { $val + (16 - r) } else { $val - r }
    }};
}

bhook::hook_fn! {
    fn hook(d: i64, l: i64, id: u32, range: i32, s: i64, label: *mut u128) -> i64 = {
        use crate::{config, utils::{combine_hex, split_hex, ptr_to_str}};
        let (max, min) = split_hex(range);
        let name = ptr_to_str(label);
        let (cfg_min, cfg_max) = config::load().get(name).map(|d| (d.min, d.max)).unwrap_or((min, max));
        let new_min = aligned!(cfg_min, false);
        let new_max = aligned!(cfg_max, true);
        log_dim_change!(cfg_min % 16 != 0, name, "Min", min, cfg_min, new_min);
        log_dim_change!(cfg_max % 16 != 0, name, "Max", max, cfg_max, new_max);
        call_original(d, l, id, combine_hex(new_max, new_min), s, label)
    }
}

pub unsafe fn setup_hook(function_addr: usize) {
    hook::hook_address(function_addr as *mut u8);
    log::debug!("Hooked function at 0x{:X}", function_addr);
}