use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevEcoConfig {
    pub path: PathBuf,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    #[serde(default)]
    pub deveco_studios: HashMap<String, DevEcoConfig>,
    #[serde(default)]
    pub default_deveco_studio: Option<PathBuf>,

    #[serde(skip)]
    pub resolved_deveco_studio: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    #[serde(rename = "env", default)]
    pub env: Option<Config>,
}

impl Config {
    pub fn global_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config").join("heco").join("config.toml"))
    }

    pub fn load(project_root: Option<&PathBuf>) -> anyhow::Result<Self> {
        let mut config = Self::load_from_file(Self::global_path()).unwrap_or_default();

        // 获取当前工程需要的 SDK 版本
        let compile_sdk_version =
            project_root.and_then(|r| crate::project::get_compile_sdk_version(r));

        // 尝试解析具体的生效路径
        let mut resolved: Option<PathBuf> = None;

        if let Some(ref version) = compile_sdk_version
            && let Some(cfg) = config.deveco_studios.get(version)
        {
            resolved = Some(cfg.path.clone());
        }

        if resolved.is_none() {
            if let Some(ref default_path) = config.default_deveco_studio {
                resolved = Some(default_path.clone());
            } else {
                resolved = Self::get_auto_detected_deveco_studio();
            }
        }

        config.resolved_deveco_studio = resolved;

        Ok(config)
    }

    pub fn get_auto_detected_deveco_studio() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            let global_path = PathBuf::from("/Applications/DevEco-Studio.app");
            if global_path.exists() {
                return Some(global_path);
            }
            if let Some(home) = dirs::home_dir() {
                let user_path = home.join("Applications").join("DevEco-Studio.app");
                if user_path.exists() {
                    return Some(user_path);
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            let mut paths = Vec::new();

            // 1. 通过注册表获取 DevEco Studio (基于 JetBrains 系列的注册表特征)
            // 常见的卸载列表路径
            let uninstall_keys = [
                (
                    winreg::enums::HKEY_LOCAL_MACHINE,
                    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
                ),
                (
                    winreg::enums::HKEY_CURRENT_USER,
                    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
                ),
                (
                    winreg::enums::HKEY_LOCAL_MACHINE,
                    "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
                ),
                (
                    winreg::enums::HKEY_CURRENT_USER,
                    "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
                ),
            ];

            for (hkey, subkey) in uninstall_keys {
                let hk = winreg::RegKey::predef(hkey);
                if let Ok(uninstall_key) = hk.open_subkey(subkey) {
                    for name in uninstall_key.enum_keys().filter_map(|k| k.ok()) {
                        if name.starts_with("DevEco Studio") {
                            if let Ok(app_key) = uninstall_key.open_subkey(&name) {
                                if let Ok(install_location) =
                                    app_key.get_value::<String, _>("InstallLocation")
                                {
                                    let path = PathBuf::from(install_location);
                                    if path.exists() {
                                        paths.push(path);
                                    }
                                } else if let Ok(uninstall_string) =
                                    app_key.get_value::<String, _>("UninstallString")
                                {
                                    // UninstallString 通常类似于 "C:\Program Files\Huawei\DevEco Studio\bin\uninstall.exe"
                                    let clean_str = uninstall_string.trim_matches('"');
                                    let uninstall_path = PathBuf::from(clean_str);
                                    if let Some(bin_dir) = uninstall_path.parent() {
                                        if let Some(root_dir) = bin_dir.parent() {
                                            if root_dir.exists() {
                                                paths.push(root_dir.to_path_buf());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 2. 兜底方案：常见的默认安装路径
            let default_path = PathBuf::from("C:\\Program Files\\Huawei\\DevEco Studio");
            if default_path.exists() {
                paths.push(default_path);
            }

            for path in paths {
                if path.exists() {
                    return Some(path);
                }
            }
        }

        None
    }

    pub fn load_from_file(path: Option<PathBuf>) -> Option<Config> {
        let path = path?;
        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&path).ok()?;
        let config_file: ConfigFile = toml::from_str(&content).ok()?;
        config_file.env
    }

    pub fn node_path(&self) -> Option<PathBuf> {
        if let Some(ref root) = self.resolved_deveco_studio {
            #[cfg(target_os = "macos")]
            let node = root
                .join("Contents")
                .join("tools")
                .join("node")
                .join("bin")
                .join("node");

            #[cfg(target_os = "windows")]
            let node = root.join("tools").join("node").join("node.exe");

            if node.exists() {
                return Some(node);
            }
        }
        None
    }

    pub fn hvigorw_js_path(&self) -> Option<PathBuf> {
        if let Some(ref root) = self.resolved_deveco_studio {
            #[cfg(target_os = "macos")]
            let hvigorw_js = root
                .join("Contents")
                .join("tools")
                .join("hvigor")
                .join("bin")
                .join("hvigorw.js");

            #[cfg(target_os = "windows")]
            let hvigorw_js = root
                .join("tools")
                .join("hvigor")
                .join("bin")
                .join("hvigorw.js");

            if hvigorw_js.exists() {
                return Some(hvigorw_js);
            }
        }
        None
    }

    pub fn sdk_path(&self) -> Option<PathBuf> {
        if let Some(ref root) = self.resolved_deveco_studio {
            #[cfg(target_os = "macos")]
            let sdk = root.join("Contents").join("sdk");

            #[cfg(target_os = "windows")]
            let sdk = root.join("sdk");

            if sdk.exists() {
                return Some(sdk);
            }
        }
        None
    }

    pub fn ohpm_path(&self) -> Option<PathBuf> {
        if let Some(ref root) = self.resolved_deveco_studio {
            #[cfg(target_os = "macos")]
            let ohpm = root
                .join("Contents")
                .join("tools")
                .join("ohpm")
                .join("bin")
                .join("ohpm");

            #[cfg(target_os = "windows")]
            let ohpm = root.join("tools").join("ohpm").join("bin").join("ohpm.bat");

            #[cfg(any(target_os = "macos", target_os = "windows"))]
            if ohpm.exists() {
                return Some(ohpm);
            }
        }
        None
    }

    pub fn hdc_path(&self) -> Option<PathBuf> {
        if let Some(sdk_path) = self.sdk_path() {
            let default_hdc = sdk_path
                .join("default")
                .join("openharmony")
                .join("toolchains")
                .join("hdc");

            if default_hdc.exists() {
                return Some(default_hdc);
            }

            #[cfg(target_os = "windows")]
            {
                let default_hdc_exe = sdk_path
                    .join("default")
                    .join("openharmony")
                    .join("toolchains")
                    .join("hdc.exe");
                if default_hdc_exe.exists() {
                    return Some(default_hdc_exe);
                }
            }
        }
        None
    }

    pub fn java_path(&self) -> Option<PathBuf> {
        if let Some(ref root) = self.resolved_deveco_studio {
            #[cfg(target_os = "macos")]
            {
                let jbr_java = root
                    .join("Contents")
                    .join("jbr")
                    .join("Contents")
                    .join("Home")
                    .join("bin")
                    .join("java");
                if jbr_java.exists() {
                    return Some(jbr_java);
                }
            }

            #[cfg(target_os = "windows")]
            {
                let jbr_java = root.join("jbr").join("bin").join("java.exe");
                if jbr_java.exists() {
                    return Some(jbr_java);
                }
            }
        }
        None
    }

    pub fn emulator_path(&self) -> Option<PathBuf> {
        if let Some(ref root) = self.resolved_deveco_studio {
            #[cfg(target_os = "macos")]
            let emulator_path = root
                .join("Contents")
                .join("tools")
                .join("emulator")
                .join("Emulator");

            #[cfg(target_os = "windows")]
            let emulator_path = root.join("tools").join("emulator").join("Emulator.exe");

            if emulator_path.exists() {
                return Some(emulator_path);
            }
        }

        None
    }

    pub fn codelinter_path(&self) -> Option<PathBuf> {
        if let Some(ref root) = self.resolved_deveco_studio {
            #[cfg(target_os = "macos")]
            let codelinter_path = root
                .join("Contents")
                .join("plugins")
                .join("codelinter")
                .join("run")
                .join("index.js");

            #[cfg(target_os = "windows")]
            let codelinter_path = root
                .join("plugins")
                .join("codelinter")
                .join("run")
                .join("index.js");

            if codelinter_path.exists() {
                return Some(codelinter_path);
            }
        }
        None
    }

    pub fn get_emulator_instance_path(&self) -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        let home = std::env::var("HOME").unwrap_or_default();
        #[cfg(target_os = "macos")]
        let default_paths = vec![
            PathBuf::from(&home)
                .join(".Huawei")
                .join("Emulator")
                .join("deployed"),
        ];

        #[cfg(target_os = "windows")]
        let default_paths = vec![
            PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default())
                .join("Huawei")
                .join("Emulator")
                .join("deployed"),
        ];

        default_paths.into_iter().find(|path| path.exists())
    }

    pub fn get_emulator_image_root(&self) -> Option<PathBuf> {
        if let Some(sdk_path) = self.sdk_path() {
            let image_path = sdk_path.join("system-images");
            if image_path.exists() {
                return Some(image_path);
            }
        }

        #[cfg(target_os = "macos")]
        let home = std::env::var("HOME").unwrap_or_default();
        #[cfg(target_os = "macos")]
        let default_paths = vec![
            PathBuf::from(&home)
                .join("Library")
                .join("Huawei")
                .join("Sdk"),
        ];

        #[cfg(target_os = "windows")]
        let default_paths = vec![
            PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default())
                .join("Huawei")
                .join("Sdk"),
        ];

        for path in default_paths {
            if path.exists() {
                let image_path = path.join("system-image");
                if image_path.exists() {
                    return Some(path);
                }
            }
        }
        None
    }
}
