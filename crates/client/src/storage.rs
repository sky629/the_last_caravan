//! 기기의 저장소입니다. `Memory`의 글자 하나를 읽고 씁니다. 읽지 못하면 빈 기억으로 시작하고, 쓰지 못하면 경고만 남깁니다.
//!
//! 웹: `localStorage`의 `caravan.memory`입니다. 새로 고침해도 남지만 사생활 보호 창에서는 막힐 수 있습니다.
//! PC: 설정 폴더의 `the-last-caravan/memory.txt`입니다. 설정 폴더는 Windows가 `%APPDATA%`, macOS가 `~/Library/Application Support`,
//! 그 밖은 `$XDG_CONFIG_HOME` 또는 `~/.config`이고, 환경 변수만 읽습니다.

use bevy::prelude::warn;
use caravan_client_core::Memory;

/// 기기에 저장된 기억입니다. 저장된 글자가 없거나 읽을 수 없으면 빈 기억입니다.
pub fn load() -> Memory {
    read_text()
        .map(|text| Memory::parse(&text))
        .unwrap_or_default()
}

/// `Memory::to_text`의 글자를 저장합니다.
pub fn save(text: &str) {
    if let Err(reason) = write_text(text) {
        warn!("cannot save what this device remembers: {reason}");
    }
}

#[cfg(target_arch = "wasm32")]
const KEY: &str = "caravan.memory";

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
fn read_text() -> Option<String> {
    local_storage()?.get_item(KEY).ok()?
}

#[cfg(target_arch = "wasm32")]
fn write_text(text: &str) -> Result<(), String> {
    local_storage()
        .ok_or_else(|| "the browser gives no local storage".to_string())?
        .set_item(KEY, text)
        .map_err(|err| format!("{err:?}"))
}

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use std::path::{Path, PathBuf};

    /// 운영체제에 따른 설정 폴더입니다. `var`는 환경 변수를 읽는 함수이고, 시험이 값을 넣어 줍니다.
    pub fn config_dir(os: &str, var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
        match os {
            "windows" => var("APPDATA").map(PathBuf::from),
            "macos" => var("HOME").map(|home| Path::new(&home).join("Library/Application Support")),
            _ => var("XDG_CONFIG_HOME")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
                .or_else(|| var("HOME").map(|home| Path::new(&home).join(".config"))),
        }
    }

    pub fn memory_path() -> Option<PathBuf> {
        let dir = config_dir(std::env::consts::OS, |name| std::env::var(name).ok())?;
        Some(dir.join("the-last-caravan").join("memory.txt"))
    }

    /// 같은 폴더의 임시 파일에 쓴 뒤 바꿔 넣으므로, 쓰다가 끊겨도 이전 파일이 남습니다.
    pub fn write_to(path: &Path, text: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("txt.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, path)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_text() -> Option<String> {
    std::fs::read_to_string(desktop::memory_path()?).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn write_text(text: &str) -> Result<(), String> {
    let path = desktop::memory_path().ok_or_else(|| "no settings folder is known".to_string())?;
    desktop::write_to(&path, text).map_err(|err| format!("{}: {err}", path.display()))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::desktop::*;
    use std::path::Path;

    fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn each_system_keeps_its_settings_in_its_own_folder() {
        assert_eq!(
            config_dir(
                "windows",
                vars(&[("APPDATA", "C:\\Users\\k\\AppData\\Roaming")])
            ),
            Some(Path::new("C:\\Users\\k\\AppData\\Roaming").to_path_buf())
        );
        assert_eq!(
            config_dir("macos", vars(&[("HOME", "/Users/k")])),
            Some(Path::new("/Users/k/Library/Application Support").to_path_buf())
        );
        assert_eq!(
            config_dir("linux", vars(&[("HOME", "/home/k")])),
            Some(Path::new("/home/k/.config").to_path_buf())
        );
        assert_eq!(
            config_dir(
                "linux",
                vars(&[("HOME", "/home/k"), ("XDG_CONFIG_HOME", "/data/cfg")])
            ),
            Some(Path::new("/data/cfg").to_path_buf())
        );
        assert_eq!(
            config_dir(
                "linux",
                vars(&[("HOME", "/home/k"), ("XDG_CONFIG_HOME", "")])
            ),
            Some(Path::new("/home/k/.config").to_path_buf()),
            "an empty variable counts as unset"
        );
    }

    #[test]
    fn a_system_without_the_variable_has_no_folder() {
        assert_eq!(config_dir("macos", vars(&[])), None);
        assert_eq!(config_dir("windows", vars(&[("HOME", "/x")])), None);
    }

    #[test]
    fn what_is_written_is_read_back_and_a_second_write_replaces_it() {
        let dir = std::env::temp_dir().join(format!("caravan-storage-{}", std::process::id()));
        let path = dir.join("the-last-caravan").join("memory.txt");

        write_to(&path, "name\tKANG\n").expect("the folder is made on the first write");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "name\tKANG\n");
        write_to(&path, "name\tMIRO\n").expect("the file is replaced");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "name\tMIRO\n");
        assert!(
            !path.with_extension("txt.tmp").exists(),
            "no temporary file is left"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
