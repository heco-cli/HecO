use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleType {
    Entry,
    Feature,
    Har,
    Shared,
    Unknown,
}

impl ModuleType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "entry" => ModuleType::Entry,
            "feature" => ModuleType::Feature,
            "har" => ModuleType::Har,
            "shared" => ModuleType::Shared,
            _ => ModuleType::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub module_type: ModuleType,
    pub targets: Vec<String>,
    pub src_path: String,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub modules: Vec<Module>,
    pub products: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectBuildProfile {
    app: App,
    #[serde(rename = "modules")]
    modules: Vec<ProjectModuleInfo>,
}

#[derive(Debug, Deserialize)]
struct App {
    products: Vec<Product>,
}

#[derive(Debug, Deserialize)]
struct Product {
    name: String,
    #[serde(rename = "compileSdkVersion")]
    compile_sdk_version: Option<serde_json::Value>,
    #[serde(rename = "targetSdkVersion")]
    target_sdk_version: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ProjectModuleInfo {
    name: String,
    #[serde(rename = "srcPath")]
    src_path: String,
    targets: Option<Vec<ModuleTarget>>,
}

#[derive(Debug, Deserialize)]
struct ModuleTarget {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModuleJson5 {
    module: Option<ModuleInfo>,
}

#[derive(Debug, Deserialize)]
struct ModuleInfo {
    #[serde(rename = "type")]
    module_type: Option<String>,
    abilities: Option<Vec<AbilityInfo>>,
}

#[derive(Debug, Deserialize)]
struct AbilityInfo {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AppJson5 {
    pub app: Option<AppInfo>,
}

#[derive(Debug, Deserialize)]
pub struct AppInfo {
    #[serde(rename = "bundleName")]
    pub bundle_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OhPackage {
    pub dependencies: Option<HashMap<String, String>>,
}

impl Module {
    fn from_project_module_info(root: &Path, info: &ProjectModuleInfo) -> Self {
        let src_path_trimmed = info.src_path.strip_prefix("./").unwrap_or(&info.src_path);
        let module_path = root.join(src_path_trimmed);
        let name = info.name.clone();

        let targets: Vec<String> = info
            .targets
            .as_ref()
            .map(|t| t.iter().filter_map(|target| target.name.clone()).collect())
            .unwrap_or_default();

        let module_type = if module_path.join("src/main/module.json5").exists() {
            if let Ok(content) = std::fs::read_to_string(module_path.join("src/main/module.json5"))
                && let Ok(json) = serde_json5::from_str::<ModuleJson5>(&content)
                && let Some(module_info) = json.module
            {
                let module_type = module_info
                    .module_type
                    .as_deref()
                    .map(ModuleType::from_str)
                    .unwrap_or(ModuleType::Entry);
                return Module {
                    name,
                    module_type,
                    targets,
                    src_path: info.src_path.clone(),
                };
            }
            ModuleType::Entry
        } else {
            ModuleType::Unknown
        };

        Module {
            name,
            module_type,
            targets,
            src_path: info.src_path.clone(),
        }
    }
}

impl Project {
    pub fn new(root: PathBuf) -> Self {
        Project {
            root,
            modules: Vec::new(),
            products: Vec::new(),
        }
    }

    pub fn discover_modules(&mut self) -> anyhow::Result<()> {
        self.modules.clear();
        self.products.clear();

        let build_profile_path = self.root.join("build-profile.json5");
        if !build_profile_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&build_profile_path)?;
        let project_build: ProjectBuildProfile = serde_json5::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse build-profile.json5: {}", e))?;

        for module_info in &project_build.modules {
            let module = Module::from_project_module_info(&self.root, module_info);
            self.modules.push(module);
        }

        for product in &project_build.app.products {
            self.products.push(product.name.clone());
        }

        Ok(())
    }

    pub fn find_module(&self, name: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.name == name)
    }

    pub fn validate_target(&self, module_name: &str, target_name: &str) -> anyhow::Result<()> {
        if let Some(module) = self.find_module(module_name) {
            if module.targets.is_empty() {
                anyhow::bail!("module '{}' has no targets defined", module_name);
            }
            if !module.targets.iter().any(|x| x == target_name) {
                anyhow::bail!(
                    "target '{}' not found in module '{}'\n\nAvailable targets:\n  {}",
                    target_name,
                    module_name,
                    module.targets.join("\n  ")
                );
            }
            Ok(())
        } else {
            anyhow::bail!("module '{}' not found", module_name);
        }
    }

    pub fn find_module_by_path(&self, current_dir: &Path) -> Option<&Module> {
        let current_dir = current_dir.canonicalize().ok()?;

        let build_profile_path = self.root.join("build-profile.json5");
        let content = std::fs::read_to_string(&build_profile_path).ok()?;
        let project_build: ProjectBuildProfile = serde_json5::from_str(&content).ok()?;

        for module_info in &project_build.modules {
            let src_path_trimmed = module_info
                .src_path
                .strip_prefix("./")
                .unwrap_or(&module_info.src_path);
            let module_path = self.root.join(src_path_trimmed);
            let module_path = module_path.canonicalize().ok()?;

            if current_dir.starts_with(&module_path) {
                return self.find_module(&module_info.name);
            }
        }

        None
    }

    pub fn validate_product(&self, product_name: &str) -> anyhow::Result<()> {
        if self.products.is_empty() {
            anyhow::bail!("no products defined in the project");
        }
        if !self.products.iter().any(|p| p == product_name) {
            anyhow::bail!(
                "product '{}' not found\n\nAvailable products:\n  {}",
                product_name,
                self.products.join("\n  ")
            );
        }
        Ok(())
    }

    pub fn get_bundle_name(&self) -> anyhow::Result<String> {
        let app_json5_path = self.root.join("AppScope").join("app.json5");
        if app_json5_path.exists() {
            let content = std::fs::read_to_string(&app_json5_path)?;
            if let Ok(json) = serde_json5::from_str::<AppJson5>(&content)
                && let Some(app) = json.app
                && let Some(bundle_name) = app.bundle_name
            {
                return Ok(bundle_name);
            }
        }
        anyhow::bail!("Could not find bundleName in AppScope/app.json5")
    }

    pub fn get_main_ability(&self, module: &Module) -> anyhow::Result<String> {
        let module_json5_path = self
            .root
            .join(&module.src_path)
            .join("src/main/module.json5");
        if module_json5_path.exists() {
            let content = std::fs::read_to_string(&module_json5_path)?;
            if let Ok(json) = serde_json5::from_str::<ModuleJson5>(&content)
                && let Some(module_info) = json.module
                && let Some(abilities) = module_info.abilities
                && let Some(first) = abilities.first()
                && let Some(name) = &first.name
            {
                return Ok(name.clone());
            }
        }
        // Fallback
        Ok("EntryAbility".to_string())
    }

    pub fn resolve_hsp_dependencies<'a>(
        &'a self,
        module: &Module,
        hsp_modules: &mut Vec<&'a Module>,
    ) -> anyhow::Result<()> {
        let pkg_path = self.root.join(&module.src_path).join("oh-package.json5");
        if !pkg_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&pkg_path)?;
        if let Ok(pkg) = serde_json5::from_str::<OhPackage>(&content)
            && let Some(deps) = pkg.dependencies
        {
            for (_, path) in deps {
                if path.starts_with("file:") {
                    let relative_path = path.trim_start_matches("file:");
                    let dep_dir = self.root.join(&module.src_path).join(relative_path);
                    let dep_dir_canonical = dep_dir.canonicalize().unwrap_or(dep_dir);

                    // Find which module this is
                    if let Some(dep_module) = self.modules.iter().find(|m| {
                        let m_dir = self.root.join(&m.src_path);
                        m_dir.canonicalize().unwrap_or(m_dir) == dep_dir_canonical
                    }) && dep_module.module_type == ModuleType::Shared
                        && !hsp_modules.iter().any(|m| m.name == dep_module.name)
                    {
                        hsp_modules.push(dep_module);
                        // Recursively resolve
                        self.resolve_hsp_dependencies(dep_module, hsp_modules)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// 检查目录是否为有效的项目根目录
/// 条件：同时包含 build-profile.json5 和 oh-package.json5，
/// 且 build-profile.json5 中必须包含 modules 字段
fn check_project_root(path: &std::path::Path) -> Option<PathBuf> {
    let build_profile_path = path.join("build-profile.json5");
    let oh_package_path = path.join("oh-package.json5");

    // 检查两个文件是否都存在
    if !build_profile_path.exists() || !oh_package_path.exists() {
        return None;
    }

    // 解析 build-profile.json5，检查是否包含 modules 字段
    let content = std::fs::read_to_string(&build_profile_path).ok()?;
    let build_profile: serde_json5::Result<ProjectBuildProfile> = serde_json5::from_str(&content);

    match build_profile {
        Ok(profile) => {
            if !profile.modules.is_empty() {
                Some(path.to_path_buf())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

pub fn find_project_root() -> Option<PathBuf> {
    let current_dir = std::env::current_dir().ok()?;

    // 从当前目录向上查找，找到同时满足所有条件的目录
    // 如果有多个满足条件的目录，选择最高层（最靠近根目录）的那个
    let mut best_root: Option<PathBuf> = None;

    for path in current_dir.ancestors() {
        if let Some(root) = check_project_root(path) {
            // 更新 best_root，因为我们想要最高层（最靠近根目录）的目录
            best_root = Some(root);
        }
    }

    best_root
}

pub fn load_project() -> anyhow::Result<Project> {
    let root = find_project_root().ok_or_else(|| {
        anyhow::anyhow!(
            "no HMOS project root found (missing build-profile.json5 or oh-package.json5)"
        )
    })?;

    let mut project = Project::new(root);
    project.discover_modules()?;

    Ok(project)
}

fn extract_api_version(value: &Option<serde_json::Value>) -> Option<String> {
    if let Some(val) = value {
        let mut s = val.to_string().trim_matches('"').to_string();
        // 兼容形如 "6.0.2(22)" 或 "5.0.0(12)" 的格式，从中提取出括号里的数字 "22" 或 "12"
        if let Some(start) = s.find('(')
            && let Some(end) = s.find(')')
            && start < end
        {
            s = s[start + 1..end].to_string();
        }
        return Some(s);
    }
    None
}

pub fn get_compile_sdk_version(project_root: &Path) -> Option<String> {
    let build_profile_path = project_root.join("build-profile.json5");
    if !build_profile_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&build_profile_path).ok()?;
    let profile: ProjectBuildProfile = serde_json5::from_str(&content).ok()?;

    if let Some(product) = profile.app.products.first() {
        if let Some(version) = extract_api_version(&product.compile_sdk_version) {
            return Some(version);
        }
        if let Some(version) = extract_api_version(&product.target_sdk_version) {
            return Some(version);
        }
    }

    None
}
