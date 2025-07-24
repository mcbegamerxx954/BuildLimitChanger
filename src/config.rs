use crate::utils::{
    get_app_external_files_dir, get_games_directory, get_global_context, is_dir_writable,
};
use jni::JNIEnv;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Dimension {
    pub min: i16,
    pub max: i16,
}

type DimensionMap = HashMap<String, Dimension>;

static CONFIG_DIR: OnceLock<String> = OnceLock::new();
const CONFIG_FILE: &str = "dimensions.json";
const LOG_FILE: &str = "log.txt";

fn config_path() -> Option<PathBuf> {
    CONFIG_DIR
        .get()
        .map(|dir: &String| Path::new(dir).join(CONFIG_FILE))
}

pub fn log_path() -> Option<PathBuf> {
    CONFIG_DIR
        .get()
        .map(|dir: &String| Path::new(dir).join(LOG_FILE))
}
fn set_config_dir(path: String) {
    if CONFIG_DIR.set(path).is_err() {
        log::error!("CONFIG_DIR can only be set once");
    }
}

pub fn save() -> Result<(), ()> {
    let dimensions = HashMap::from([
        ("Overworld".to_string(), Dimension { min: -64, max: 320 }),
        ("Nether".to_string(), Dimension { min: 0, max: 128 }),
        ("TheEnd".to_string(), Dimension { min: 0, max: 256 }),
    ]);

    let json = serde_json::to_string_pretty(&dimensions)
        .map_err(|e| log::error!("Failed to serialize config: {}", e))?;

    if let Some(path) = config_path() {
        fs::write(path, json).map_err(|e| log::error!("Failed to write config: {}", e))?;
    } else {
        log::error!("CONFIG_DIR is not set");
        return Err(());
    }

    Ok(())
}

pub fn load() -> DimensionMap {
    let Some(path) = config_path() else {
        log::error!("CONFIG_DIR not set");
        return DimensionMap::new();
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) if !c.trim().is_empty() => c,
        _ => {
            save().ok();
            fs::read_to_string(&path).unwrap_or_else(|e| {
                log::error!("Failed to read config after saving default: {}", e);
                String::new()
            })
        }
    };

    match serde_json::from_str::<DimensionMap>(&content) {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to parse config: {}. Regenerating default...", e);
            save().ok();
            DimensionMap::new()
        }
    }
}

pub fn init_config(env: &mut JNIEnv) {
    let Some(context) = get_global_context(env) else {
        log::error!("Failed to get global context");
        return;
    };

    let Some(mut path) =
        get_games_directory(env).or_else(|| get_app_external_files_dir(env, context.as_obj()))
    else {
        log::error!("Failed to get a valid external directory");
        return;
    };

    path.push_str("/BuildLimitChanger/");

    if !is_dir_writable(&path) {
        log::error!("Config directory not writable: {}", path);
        return;
    }

    set_config_dir(path.clone());

    if !config_path().map_or(false, |p| p.exists()) {
        save().ok();
    }
}
