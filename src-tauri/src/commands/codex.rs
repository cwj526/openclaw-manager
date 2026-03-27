use crate::utils::{file, platform, shell};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tauri::command;

const CODEX_OPENAI_PACKAGE: &str = "@openai/codex";
const CODEX_GAC_INSTALL_URL: &str = "https://gaccode.com/codex/install";
const CODEX_REFERENCE_SCRIPT_PATH: &str =
    "/Users/shuidiyu06/tu/sh.tu-zi.com/sh/setup_codex/install_codex.sh";
const DEFAULT_MODEL: &str = "gpt-5.4";
const DEFAULT_REASONING: &str = "medium";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexModelSettings {
    pub model: String,
    pub model_reasoning_effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRoute {
    pub name: String,
    pub base_url: Option<String>,
    pub has_key: bool,
    pub is_current: bool,
    pub api_key_masked: Option<String>,
    pub model_settings: CodexModelSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexEnvSummary {
    pub codex_api_key_masked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub install_type: Option<String>,
    pub current_route: Option<String>,
    pub state_file_exists: bool,
    pub config_file_exists: bool,
    pub routes: Vec<CodexRoute>,
    pub env_summary: CodexEnvSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexReferenceDocs {
    pub script_markdown: String,
    pub updated_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexActionResult {
    pub success: bool,
    pub message: String,
    pub error: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRoutesResponse {
    pub current_route: Option<String>,
    pub routes: Vec<CodexRoute>,
}

#[derive(Debug, Clone)]
struct InstallState {
    install_type: Option<String>,
    route: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ConfigRouteEntry {
    base_url: Option<String>,
    model: Option<String>,
    model_reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ParsedCodexConfig {
    profile: Option<String>,
    routes: BTreeMap<String, ConfigRouteEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigSection {
    None,
    ModelProvider,
    Profile,
}

fn success_result(message: &str, stdout: String, restart_required: bool) -> CodexActionResult {
    CodexActionResult {
        success: true,
        message: message.to_string(),
        error: None,
        stdout,
        stderr: String::new(),
        restart_required,
    }
}

fn error_result(message: &str, error: String, stdout: String) -> CodexActionResult {
    CodexActionResult {
        success: false,
        message: message.to_string(),
        error: Some(error.clone()),
        stdout,
        stderr: error,
        restart_required: false,
    }
}

fn get_codex_dir() -> String {
    if let Some(home) = dirs::home_dir() {
        if platform::is_windows() {
            format!("{}\\.codex", home.display())
        } else {
            format!("{}/.codex", home.display())
        }
    } else if platform::is_windows() {
        "%USERPROFILE%\\.codex".to_string()
    } else {
        "~/.codex".to_string()
    }
}

fn get_codex_config_file_path() -> String {
    if platform::is_windows() {
        format!("{}\\config.toml", get_codex_dir())
    } else {
        format!("{}/config.toml", get_codex_dir())
    }
}

fn get_codex_state_file_path() -> String {
    if platform::is_windows() {
        format!("{}\\install_state", get_codex_dir())
    } else {
        format!("{}/install_state", get_codex_dir())
    }
}

fn normalize_install_type(value: &str) -> Option<String> {
    let lower = value.trim().to_lowercase();
    match lower.as_str() {
        "openai" | "gac" => Some(lower),
        _ => None,
    }
}

/// gac / tuzi 或符合规则的自定义线路名（小写字母数字与 `-` `_`，1–48 字符）
fn normalize_route_input(value: &str) -> Option<String> {
    let s = value.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    match s.as_str() {
        "gac" | "tuzi" => Some(s),
        "none" => None,
        _ if is_valid_custom_codex_route_name(&s) => Some(s),
        _ => None,
    }
}

fn is_valid_custom_codex_route_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 48
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && s != "gac"
        && s != "tuzi"
        && s != "none"
}

fn parse_route_state_value(value: &str) -> Option<String> {
    let t = value.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("none") {
        return None;
    }
    normalize_route_input(t)
}

fn route_base_url(route: &str) -> Option<&'static str> {
    match route {
        "gac" => Some("https://gaccode.com/codex/v1"),
        "tuzi" => Some("https://api.tu-zi.com/v1"),
        _ => None,
    }
}

fn mask_key(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    if value.len() <= 8 {
        return "****".to_string();
    }
    let head = &value[0..4];
    let tail = &value[value.len() - 4..];
    format!("{}****{}", head, tail)
}

fn parse_install_state(content: &str) -> InstallState {
    let mut install_type: Option<String> = None;
    let mut route: Option<String> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(value) = line.strip_prefix("INSTALL_TYPE=") {
            install_type = normalize_install_type(value);
            continue;
        }

        if let Some(value) = line.strip_prefix("ROUTE=") {
            route = parse_route_state_value(value);
        }
    }

    InstallState {
        install_type,
        route,
    }
}

fn load_install_state() -> InstallState {
    let path = get_codex_state_file_path();
    let content = file::read_file(&path).unwrap_or_default();
    parse_install_state(&content)
}

fn save_install_state(install_type: &str, route: Option<&str>) -> Result<(), String> {
    let install_type = normalize_install_type(install_type)
        .ok_or_else(|| format!("非法安装类型: {}", install_type))?;
    let route_value = match route {
        None | Some("") => "none".to_string(),
        Some(r) => normalize_route_input(r.trim()).ok_or_else(|| format!("非法路线: {}", r))?,
    };

    let content = format!(
        "INSTALL_TYPE={}\nROUTE={}\nMANAGED_BY=sh.tu-zi.com\n",
        install_type, route_value
    );

    file::write_file(&get_codex_state_file_path(), &content)
        .map_err(|e| format!("写入安装状态失败: {}", e))
}

fn clear_install_state() {
    let path = get_codex_state_file_path();
    if Path::new(&path).exists() {
        let _ = std::fs::remove_file(path);
    }
}

fn parse_codex_config(content: &str) -> ParsedCodexConfig {
    let mut parsed = ParsedCodexConfig::default();
    let mut section = ConfigSection::None;
    let mut section_route: Option<String> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = ConfigSection::None;
            section_route = None;

            let section_name = line.trim_start_matches('[').trim_end_matches(']');
            if let Some(route) = section_name.strip_prefix("model_providers.") {
                if let Some(valid_route) = normalize_route_input(route.trim()) {
                    section = ConfigSection::ModelProvider;
                    section_route = Some(valid_route.clone());
                    parsed.routes.entry(valid_route).or_default();
                }
            } else if let Some(route) = section_name.strip_prefix("profiles.") {
                if let Some(valid_route) = normalize_route_input(route.trim()) {
                    section = ConfigSection::Profile;
                    section_route = Some(valid_route.clone());
                    parsed.routes.entry(valid_route).or_default();
                }
            }
            continue;
        }

        if let Some((key, value_raw)) = line.split_once('=') {
            let key = key.trim();
            let value = value_raw.trim().trim_matches('"').to_string();

            if key == "profile" {
                parsed.profile = normalize_route_input(&value);
                continue;
            }

            let Some(route) = &section_route else {
                continue;
            };

            let entry = parsed.routes.entry(route.clone()).or_default();
            match section {
                ConfigSection::ModelProvider => {
                    if key == "base_url" && !value.is_empty() {
                        entry.base_url = Some(value);
                    }
                }
                ConfigSection::Profile => {
                    if key == "model" && !value.is_empty() {
                        entry.model = Some(value);
                    } else if key == "model_reasoning_effort" && !value.is_empty() {
                        entry.model_reasoning_effort = Some(value);
                    }
                }
                ConfigSection::None => {}
            }
        }
    }

    parsed
}

fn filter_codex_config_strips(
    existing_content: &str,
    strip_route_names: &BTreeSet<String>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut skipping_managed_section = false;

    for raw in existing_content.lines() {
        let trimmed = raw.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed.trim_start_matches('[').trim_end_matches(']');
            let should_skip = if let Some(name) = section.strip_prefix("model_providers.") {
                strip_route_names.contains(name)
            } else if let Some(name) = section.strip_prefix("profiles.") {
                strip_route_names.contains(name)
            } else {
                false
            };
            skipping_managed_section = should_skip;
            if should_skip {
                continue;
            }
        }

        if skipping_managed_section {
            continue;
        }

        if trimmed.starts_with("profile") && trimmed.contains('=') {
            continue;
        }

        lines.push(raw.to_string());
    }

    lines.join("\n").trim().to_string()
}

fn collect_strip_route_names(merged: &ParsedCodexConfig, existing: &str) -> BTreeSet<String> {
    let mut strip: BTreeSet<String> = BTreeSet::new();
    for k in parse_codex_config(existing).routes.keys() {
        strip.insert(k.clone());
    }
    for k in merged.routes.keys() {
        strip.insert(k.clone());
    }
    strip.insert("gac".to_string());
    strip.insert("tuzi".to_string());
    strip
}

fn effective_base_url(route: &str, entry: &ConfigRouteEntry) -> Result<String, String> {
    if let Some(url) = &entry.base_url {
        let t = url.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    route_base_url(route).map(|s| s.to_string()).ok_or_else(|| {
        format!(
            "路线「{}」缺少 base_url，请填写兼容 OpenAI Responses 的 API 根地址",
            route
        )
    })
}

fn write_codex_config_merged(
    merged: &ParsedCodexConfig,
    profile_route: &str,
) -> Result<(), String> {
    let normalized_profile = normalize_route_input(profile_route).ok_or_else(|| {
        format!(
            "非法当前路线: {}（支持 gac / tuzi / 自定义线路名）",
            profile_route
        )
    })?;

    if !merged.routes.contains_key(&normalized_profile) {
        return Err(format!(
            "路线「{}」尚未配置。内置线路请用安装页初始化；自定义线路请先在路线管理中新增",
            normalized_profile
        ));
    }

    let config_path = get_codex_config_file_path();
    let existing = file::read_file(&config_path).unwrap_or_default();
    let strip_names = collect_strip_route_names(merged, &existing);
    let filtered = filter_codex_config_strips(&existing, &strip_names);

    let mut output = format!("profile = \"{}\"\n\n", normalized_profile);
    if !filtered.is_empty() {
        output.push_str(filtered.as_str());
        output.push_str("\n\n");
    }

    for (route, entry) in &merged.routes {
        let base_url = effective_base_url(route, entry)?;
        let model = entry
            .model
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_MODEL);
        let reasoning = entry
            .model_reasoning_effort
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_REASONING);
        output.push_str(&format!(
            "[model_providers.{r}]\nname = \"{r}\"\nbase_url = \"{url}\"\nwire_api = \"responses\"\nenv_key = \"CODEX_API_KEY\"\n\n[profiles.{r}]\nmodel_provider = \"{r}\"\nmodel = \"{model}\"\nmodel_reasoning_effort = \"{reasoning}\"\napproval_policy = \"on-request\"\n\n",
            r = route,
            url = base_url,
            model = model,
            reasoning = reasoning,
        ));
    }

    file::write_file(&config_path, &output).map_err(|e| format!("写入 config.toml 失败: {}", e))
}

fn get_shell_rc_candidates() -> Vec<String> {
    if platform::is_windows() {
        return Vec::new();
    }

    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        paths.push(format!("{}/.zshrc", home.display()));
        paths.push(format!("{}/.bashrc", home.display()));
    }
    paths
}

fn apply_env_to_rc(api_key: &str) -> Result<Vec<String>, String> {
    if platform::is_windows() {
        return Ok(Vec::new());
    }

    let rc_paths = get_shell_rc_candidates();
    if rc_paths.is_empty() {
        return Err("无法定位 shell 配置文件".to_string());
    }

    let mut updated = Vec::new();
    for rc_path in rc_paths {
        let content = file::read_file(&rc_path).unwrap_or_default();
        let filtered_lines: Vec<String> = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("export CODEX_API_KEY=")
                    && !trimmed.starts_with("export CODEX_KEY=")
            })
            .map(|line| line.to_string())
            .collect();

        let mut lines = filtered_lines;
        // 与 sh.tu-zi.com/setup_codex 脚本一致：脚本写入 CODEX_KEY；CLI 常用 CODEX_API_KEY，两处同步避免混用
        lines.push(format!("export CODEX_API_KEY=\"{}\"", api_key));
        lines.push(format!("export CODEX_KEY=\"{}\"", api_key));

        file::write_file(&rc_path, &lines.join("\n"))
            .map_err(|e| format!("写入 {} 失败: {}", rc_path, e))?;
        updated.push(rc_path);
    }

    Ok(updated)
}

fn clear_env_in_rc() -> Result<Vec<String>, String> {
    if platform::is_windows() {
        return Ok(Vec::new());
    }

    let rc_paths = get_shell_rc_candidates();
    if rc_paths.is_empty() {
        return Err("无法定位 shell 配置文件".to_string());
    }

    let mut updated = Vec::new();
    for rc_path in rc_paths {
        let content = file::read_file(&rc_path).unwrap_or_default();
        let filtered_lines: Vec<String> = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("export CODEX_API_KEY=")
                    && !trimmed.starts_with("export CODEX_KEY=")
            })
            .map(|line| line.to_string())
            .collect();

        file::write_file(&rc_path, &filtered_lines.join("\n"))
            .map_err(|e| format!("清理 {} 中的 Codex 环境变量失败: {}", rc_path, e))?;
        updated.push(rc_path);
    }

    Ok(updated)
}

fn run_npm_global(command: &str) -> Result<String, String> {
    shell::run_script_output(command)
}

fn is_npm_eexist_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("eexist")
        && (lower.contains("file already exists") || lower.contains("file exists:"))
}

fn extract_npm_file_exists_path(error: &str) -> Option<String> {
    for line in error.lines() {
        let trimmed = line.trim();
        if let Some(path) = trimmed.strip_prefix("npm error File exists:") {
            let value = path.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
        if let Some(path) = trimmed.strip_prefix("npm ERR! File exists:") {
            let value = path.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn run_npm_install_with_recovery(command: &str) -> Result<String, String> {
    match run_npm_global(command) {
        Ok(output) => Ok(output),
        Err(first_error) => {
            if !is_npm_eexist_error(&first_error) {
                return Err(first_error);
            }

            let mut logs = vec![format!("首次安装失败，检测到 EEXIST 冲突\n{}", first_error)];
            if let Some(path) = extract_npm_file_exists_path(&first_error) {
                logs.push(format!(
                    "检测到冲突文件: {}。为避免误删用户现有可执行文件，已停止自动清理，请先确认该文件是否可删除后再重试安装。",
                    path
                ));
            } else {
                logs.push(
                    "检测到文件已存在冲突。为避免误删用户现有可执行文件，已停止自动清理，请手动检查 npm 报错中的冲突路径后再重试安装。"
                        .to_string(),
                );
            }
            Err(logs.join("\n"))
        }
    }
}

fn resolve_model_settings(
    model: Option<String>,
    model_reasoning_effort: Option<String>,
    fallback_model: Option<&str>,
    fallback_reasoning: Option<&str>,
) -> CodexModelSettings {
    let final_model = model
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| fallback_model.map(|v| v.to_string()))
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let final_reasoning = model_reasoning_effort
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| fallback_reasoning.map(|v| v.to_string()))
        .unwrap_or_else(|| DEFAULT_REASONING.to_string());

    CodexModelSettings {
        model: final_model,
        model_reasoning_effort: final_reasoning,
    }
}

fn configure_openai_route(
    route: &str,
    api_key: &str,
    model: Option<String>,
    model_reasoning_effort: Option<String>,
    override_base_url: Option<String>,
) -> Result<Vec<String>, String> {
    let normalized_route = normalize_route_input(route.trim()).ok_or_else(|| {
        format!(
            "非法路线: {}（支持 gac / tuzi / 自定义线路名：小写字母数字与 - _，最长 48）",
            route
        )
    })?;
    if api_key.trim().is_empty() {
        return Err("切换路线需要提供 API Key".to_string());
    }

    let config_path = get_codex_config_file_path();
    let mut merged = parse_codex_config(&file::read_file(&config_path).unwrap_or_default());

    merged.routes.entry(normalized_route.clone()).or_default();

    let existing_entry = merged
        .routes
        .get(&normalized_route)
        .cloned()
        .unwrap_or_default();
    let settings = resolve_model_settings(
        model,
        model_reasoning_effort,
        existing_entry.model.as_deref(),
        existing_entry.model_reasoning_effort.as_deref(),
    );

    let ent = merged.routes.get_mut(&normalized_route).unwrap();
    if let Some(ref ob) = override_base_url {
        let t = ob.trim();
        if !t.is_empty() {
            ent.base_url = Some(t.to_string());
        }
    }

    let is_builtin = normalized_route == "gac" || normalized_route == "tuzi";
    if is_builtin {
        if ent
            .base_url
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            if let Some(b) = route_base_url(&normalized_route) {
                ent.base_url = Some(b.to_string());
            }
        }
    } else if ent
        .base_url
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        return Err(
            "自定义线路需要有效的 BASE_URL（安装或切换时请填写，或先在配置文件中写入）".to_string(),
        );
    }
    ent.model = Some(settings.model.clone());
    ent.model_reasoning_effort = Some(settings.model_reasoning_effort.clone());
    merged.profile = Some(normalized_route.clone());

    write_codex_config_merged(&merged, &normalized_route)?;
    let rc_paths = apply_env_to_rc(api_key.trim())?;
    save_install_state("openai", Some(&normalized_route))?;

    let mut logs = vec![
        format!("已写入配置: {}", get_codex_config_file_path()),
        format!("已写入状态: {}", get_codex_state_file_path()),
        format!(
            "路线={} model={} reasoning={}",
            normalized_route, settings.model, settings.model_reasoning_effort
        ),
    ];
    for path in rc_paths {
        logs.push(format!("已更新环境变量: {}", path));
    }

    Ok(logs)
}

fn derive_current_route(state: &InstallState, config: &ParsedCodexConfig) -> Option<String> {
    if let Some(route) = &state.route {
        return Some(route.clone());
    }
    config.profile.clone()
}

fn build_routes(
    current_route: Option<&str>,
    config: &ParsedCodexConfig,
    env_api_key: &str,
) -> Vec<CodexRoute> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    names.insert("gac".to_string());
    names.insert("tuzi".to_string());
    for k in config.routes.keys() {
        names.insert(k.clone());
    }

    names
        .into_iter()
        .map(|route_name| {
            let config_entry = config.routes.get(&route_name);
            let settings = resolve_model_settings(
                None,
                None,
                config_entry.and_then(|v| v.model.as_deref()),
                config_entry.and_then(|v| v.model_reasoning_effort.as_deref()),
            );
            let is_current = current_route == Some(route_name.as_str());
            let base_url = config_entry
                .and_then(|v| v.base_url.clone())
                .filter(|u| !u.trim().is_empty())
                .or_else(|| route_base_url(&route_name).map(|v| v.to_string()));
            CodexRoute {
                name: route_name.clone(),
                base_url,
                has_key: is_current && !env_api_key.trim().is_empty(),
                is_current,
                api_key_masked: if is_current && !env_api_key.trim().is_empty() {
                    Some(mask_key(env_api_key.trim()))
                } else {
                    None
                },
                model_settings: settings,
            }
        })
        .collect()
}

#[command]
pub async fn get_codex_status() -> Result<CodexStatus, String> {
    let installed = shell::command_exists("codex");
    let version = if installed {
        shell::run_command_output("codex", &["--version"]).ok()
    } else {
        None
    };

    let state_path = get_codex_state_file_path();
    let config_path = get_codex_config_file_path();
    let state_file_exists = Path::new(&state_path).exists();
    let config_file_exists = Path::new(&config_path).exists();

    let state = load_install_state();
    let config = parse_codex_config(&file::read_file(&config_path).unwrap_or_default());
    let current_route = derive_current_route(&state, &config);
    let env_api_key = std::env::var("CODEX_API_KEY").ok().unwrap_or_default();
    let env_codex_key = std::env::var("CODEX_KEY").ok().unwrap_or_default();
    let env_key_effective = if env_api_key.trim().is_empty() {
        env_codex_key
    } else {
        env_api_key
    };
    let routes = build_routes(current_route.as_deref(), &config, &env_key_effective);

    let install_type = state.install_type.or_else(|| {
        if installed {
            Some("unknown".to_string())
        } else {
            None
        }
    });

    Ok(CodexStatus {
        installed,
        version,
        install_type,
        current_route,
        state_file_exists,
        config_file_exists,
        routes,
        env_summary: CodexEnvSummary {
            codex_api_key_masked: if env_key_effective.trim().is_empty() {
                None
            } else {
                Some(mask_key(env_key_effective.trim()))
            },
        },
    })
}

#[command]
pub async fn get_codex_install_reference() -> Result<CodexReferenceDocs, String> {
    let script_markdown = match file::read_file(CODEX_REFERENCE_SCRIPT_PATH) {
        Ok(content) => content,
        Err(e) => {
            return Ok(CodexReferenceDocs {
                script_markdown: String::new(),
                updated_at: None,
                error: Some(format!("读取 install_codex.sh 失败: {}", e)),
            })
        }
    };

    let updated_at = std::fs::metadata(CODEX_REFERENCE_SCRIPT_PATH)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .map(|time| {
            let datetime: chrono::DateTime<chrono::Utc> = time.into();
            datetime.to_rfc3339()
        });

    Ok(CodexReferenceDocs {
        script_markdown,
        updated_at,
        error: None,
    })
}

#[command]
pub async fn add_codex_route(
    route_name: String,
    base_url: String,
    api_key: String,
    model: Option<String>,
    model_reasoning_effort: Option<String>,
) -> Result<CodexActionResult, String> {
    let state = load_install_state();
    if state.install_type.as_deref() == Some("gac") {
        return Ok(error_result(
            "添加线路失败",
            "gac 改版安装不支持路线管理，请使用原版 Codex".to_string(),
            String::new(),
        ));
    }

    let trimmed_name = route_name.trim().to_lowercase();
    if trimmed_name == "gac" || trimmed_name == "tuzi" {
        return Ok(error_result(
            "添加线路失败",
            "gac / tuzi 为内置线路，请通过安装页或切换线路使用，勿用自定义添加".to_string(),
            String::new(),
        ));
    }

    let name = match normalize_route_input(&trimmed_name) {
        Some(v) => v,
        None => {
            return Ok(error_result(
                "添加线路失败",
                "线路名须为小写字母、数字、- 或 _，最长 48 字符".to_string(),
                String::new(),
            ))
        }
    };

    let base = base_url.trim();
    if base.is_empty() {
        return Ok(error_result(
            "添加线路失败",
            "Base URL 不能为空（需为兼容 OpenAI Responses 的 API 根地址，通常以 /v1 结尾）"
                .to_string(),
            String::new(),
        ));
    }

    if api_key.trim().is_empty() {
        return Ok(error_result(
            "添加线路失败",
            "API Key 不能为空".to_string(),
            String::new(),
        ));
    }

    let config_path = get_codex_config_file_path();
    let mut merged = parse_codex_config(&file::read_file(&config_path).unwrap_or_default());
    if merged.routes.contains_key(&name) {
        return Ok(error_result(
            "添加线路失败",
            format!("线路「{}」已存在", name),
            String::new(),
        ));
    }

    let settings = resolve_model_settings(
        model,
        model_reasoning_effort,
        Some(DEFAULT_MODEL),
        Some(DEFAULT_REASONING),
    );

    merged.routes.insert(
        name.clone(),
        ConfigRouteEntry {
            base_url: Some(base.to_string()),
            model: Some(settings.model.clone()),
            model_reasoning_effort: Some(settings.model_reasoning_effort.clone()),
        },
    );
    merged.profile = Some(name.clone());

    if let Err(e) = write_codex_config_merged(&merged, &name) {
        return Ok(error_result("添加线路失败", e, String::new()));
    }

    let rc_paths = match apply_env_to_rc(api_key.trim()) {
        Ok(paths) => paths,
        Err(e) => return Ok(error_result("添加线路失败", e, String::new())),
    };

    if let Err(e) = save_install_state("openai", Some(&name)) {
        return Ok(error_result("添加线路失败", e, String::new()));
    }

    let mut logs = vec![
        format!("add_codex_route route={}", name),
        format!("已写入配置: {}", get_codex_config_file_path()),
        format!("已写入状态: {}", get_codex_state_file_path()),
    ];
    for path in rc_paths {
        logs.push(format!("已更新环境变量: {}", path));
    }

    Ok(success_result(
        "自定义线路已添加并切换，请重开终端后执行 codex",
        logs.join("\n"),
        true,
    ))
}

#[command]
pub async fn list_codex_routes() -> Result<CodexRoutesResponse, String> {
    let status = get_codex_status().await?;
    Ok(CodexRoutesResponse {
        current_route: status.current_route,
        routes: status.routes,
    })
}

#[command]
pub async fn install_codex(
    variant: String,
    route: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    model_reasoning_effort: Option<String>,
    route_base_url: Option<String>,
) -> Result<CodexActionResult, String> {
    let normalized_variant = variant.trim().to_lowercase();
    if normalized_variant != "openai" && normalized_variant != "gac" {
        return Ok(error_result(
            "Codex 安装失败",
            format!("未知安装类型: {}", variant),
            String::new(),
        ));
    }

    if normalized_variant == "gac" {
        let command = format!("npm install -g {}", CODEX_GAC_INSTALL_URL);
        match run_npm_install_with_recovery(&command) {
            Ok(output) => {
                save_install_state("gac", None)?;
                return Ok(success_result(
                    "gac 改版 Codex 安装成功",
                    format!("$ {}\n{}", command, output),
                    true,
                ));
            }
            Err(e) => {
                return Ok(error_result("Codex 安装失败", e, String::new()));
            }
        }
    }

    let install_command = format!("npm install -g {}", CODEX_OPENAI_PACKAGE);
    let install_output = match run_npm_install_with_recovery(&install_command) {
        Ok(value) => value,
        Err(e) => return Ok(error_result("Codex 安装失败", e, String::new())),
    };

    let mut logs = vec![format!("$ {}", install_command), install_output];

    if let Some(selected_route) = route {
        let key = api_key.unwrap_or_default();
        match configure_openai_route(
            &selected_route,
            &key,
            model,
            model_reasoning_effort,
            route_base_url,
        ) {
            Ok(route_logs) => {
                logs.extend(route_logs);
                return Ok(success_result(
                    "原版 Codex 安装并配置成功，请重开终端后执行 codex",
                    logs.join("\n"),
                    true,
                ));
            }
            Err(e) => {
                return Ok(error_result(
                    "Codex 安装成功，但路线配置失败",
                    e,
                    logs.join("\n"),
                ));
            }
        }
    }

    save_install_state("openai", None)?;
    logs.push(format!("已写入状态: {}", get_codex_state_file_path()));

    Ok(success_result(
        "原版 Codex 安装成功",
        logs.join("\n"),
        false,
    ))
}

#[command]
pub async fn switch_codex_route(
    route_name: String,
    api_key: String,
    model: Option<String>,
    model_reasoning_effort: Option<String>,
) -> Result<CodexActionResult, String> {
    let state = load_install_state();
    if state.install_type.as_deref() == Some("gac") {
        return Ok(error_result(
            "路线切换失败",
            "只有原版 Codex 才支持路线切换".to_string(),
            String::new(),
        ));
    }

    match configure_openai_route(
        route_name.trim(),
        api_key.trim(),
        model,
        model_reasoning_effort,
        None,
    ) {
        Ok(logs) => Ok(success_result(
            "路线切换成功，请重开终端后执行 codex",
            logs.join("\n"),
            true,
        )),
        Err(e) => Ok(error_result("路线切换失败", e, String::new())),
    }
}

#[command]
pub async fn set_codex_route_model(
    route_name: String,
    model: String,
    model_reasoning_effort: Option<String>,
) -> Result<CodexActionResult, String> {
    let state = load_install_state();
    if state.install_type.as_deref() == Some("gac") {
        return Ok(error_result(
            "模型参数更新失败",
            "只有原版 Codex 才支持路线模型设置".to_string(),
            String::new(),
        ));
    }

    let normalized_route = match normalize_route_input(route_name.trim()) {
        Some(v) => v,
        None => {
            return Ok(error_result(
                "模型参数更新失败",
                "非法路线名（支持 gac / tuzi / 自定义线路）".to_string(),
                String::new(),
            ))
        }
    };

    let config_path = get_codex_config_file_path();
    let mut merged = parse_codex_config(&file::read_file(&config_path).unwrap_or_default());
    if !merged.routes.contains_key(&normalized_route) {
        return Ok(error_result(
            "模型参数更新失败",
            format!(
                "路线「{}」不存在，请先添加或切换写入过该线路",
                normalized_route
            ),
            String::new(),
        ));
    }

    let existing = merged.routes.get(&normalized_route);
    let settings = resolve_model_settings(
        Some(model),
        model_reasoning_effort,
        existing.and_then(|v| v.model.as_deref()),
        existing.and_then(|v| v.model_reasoning_effort.as_deref()),
    );

    if settings.model.trim().is_empty() {
        return Ok(error_result(
            "模型参数更新失败",
            "model 不能为空".to_string(),
            String::new(),
        ));
    }

    let ent = merged.routes.get_mut(&normalized_route).unwrap();
    ent.model = Some(settings.model.clone());
    ent.model_reasoning_effort = Some(settings.model_reasoning_effort.clone());

    let state = load_install_state();
    let active_profile = derive_current_route(&state, &merged)
        .filter(|name| merged.routes.contains_key(name))
        .unwrap_or_else(|| normalized_route.clone());

    if let Err(e) = write_codex_config_merged(&merged, &active_profile) {
        return Ok(error_result("模型参数更新失败", e, String::new()));
    }

    Ok(success_result(
        "模型参数更新成功",
        format!(
            "route={} model={} reasoning={}\n已写入: {}",
            normalized_route,
            settings.model,
            settings.model_reasoning_effort,
            get_codex_config_file_path()
        ),
        false,
    ))
}

#[command]
pub async fn upgrade_codex(target_variant: Option<String>) -> Result<CodexActionResult, String> {
    let variant = target_variant
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
        .or_else(|| load_install_state().install_type)
        .unwrap_or_else(|| "openai".to_string());

    let command = if variant == "gac" {
        format!("npm install -g {}", CODEX_GAC_INSTALL_URL)
    } else {
        format!("npm install -g {}@latest", CODEX_OPENAI_PACKAGE)
    };

    match run_npm_install_with_recovery(&command) {
        Ok(output) => Ok(success_result(
            "Codex 升级成功",
            format!("$ {}\n{}", command, output),
            false,
        )),
        Err(e) => Ok(error_result("Codex 升级失败", e, String::new())),
    }
}

fn try_uninstall_codex() -> (bool, Vec<String>, Vec<String>) {
    let mut logs = Vec::new();
    let mut errors = Vec::new();

    let commands = [
        format!("npm uninstall -g {}", CODEX_OPENAI_PACKAGE),
        "npm uninstall -g codex".to_string(),
    ];

    for command in commands {
        match run_npm_global(&command) {
            Ok(output) => {
                logs.push(format!("$ {}\n{}", command, output));
                return (true, logs, errors);
            }
            Err(e) => {
                errors.push(format!("$ {}\n{}", command, e));
            }
        }
    }

    if !shell::command_exists("codex") {
        return (true, logs, errors);
    }

    (false, logs, errors)
}

#[command]
pub async fn uninstall_codex(clear_config: bool) -> Result<CodexActionResult, String> {
    let (success, mut logs, errors) = try_uninstall_codex();

    if !errors.is_empty() {
        logs.extend(errors);
    }

    if !success {
        return Ok(error_result(
            "Codex 卸载失败",
            "执行 npm uninstall 后仍检测到 codex 命令".to_string(),
            logs.join("\n\n"),
        ));
    }

    clear_install_state();
    logs.push(format!("已删除状态: {}", get_codex_state_file_path()));

    if clear_config {
        match clear_env_in_rc() {
            Ok(paths) => {
                if paths.is_empty() {
                    logs.push("环境变量清理：当前平台无需处理或未找到 shell rc".to_string());
                } else {
                    for path in paths {
                        logs.push(format!("已清理环境变量: {}", path));
                    }
                }
            }
            Err(e) => {
                logs.push(format!("清理环境变量失败: {}", e));
            }
        }
    }

    if clear_config {
        let config_path = get_codex_config_file_path();
        if Path::new(&config_path).exists() {
            let _ = std::fs::remove_file(&config_path);
            logs.push(format!("已删除配置: {}", config_path));
        }
    }

    Ok(success_result(
        if clear_config {
            "Codex 已卸载，配置已清理"
        } else {
            "Codex 已卸载，配置已保留"
        },
        logs.join("\n\n"),
        true,
    ))
}

#[command]
pub async fn reinstall_codex(
    variant: String,
    route: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    model_reasoning_effort: Option<String>,
    route_base_url: Option<String>,
    clear_config: Option<bool>,
) -> Result<CodexActionResult, String> {
    let clear = clear_config.unwrap_or(false);

    let uninstall = uninstall_codex(clear).await?;
    if !uninstall.success {
        return Ok(uninstall);
    }

    let install = install_codex(
        variant,
        route,
        api_key,
        model,
        model_reasoning_effort,
        route_base_url,
    )
    .await?;

    let combined_output = [uninstall.stdout, install.stdout]
        .into_iter()
        .filter(|v| !v.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if install.success {
        Ok(success_result(
            "Codex 重装成功",
            combined_output,
            install.restart_required,
        ))
    } else {
        Ok(error_result(
            "Codex 重装失败",
            install
                .error
                .unwrap_or_else(|| "安装阶段发生未知错误".to_string()),
            combined_output,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_npm_file_exists_path, filter_codex_config_strips, is_npm_eexist_error,
        normalize_route_input, parse_codex_config, parse_install_state,
    };
    use std::collections::BTreeSet;

    #[test]
    fn parse_install_state_works() {
        let state =
            parse_install_state("INSTALL_TYPE=openai\nROUTE=gac\nMANAGED_BY=sh.tu-zi.com\n");
        assert_eq!(state.install_type.as_deref(), Some("openai"));
        assert_eq!(state.route.as_deref(), Some("gac"));

        let unknown = parse_install_state("INSTALL_TYPE=other\nROUTE=bad.name\n");
        assert!(unknown.install_type.is_none());
        assert!(unknown.route.is_none());

        let custom =
            parse_install_state("INSTALL_TYPE=openai\nROUTE=my-line\nMANAGED_BY=sh.tu-zi.com\n");
        assert_eq!(custom.install_type.as_deref(), Some("openai"));
        assert_eq!(custom.route.as_deref(), Some("my-line"));
    }

    #[test]
    fn normalize_route_input_works() {
        assert_eq!(normalize_route_input("gac").as_deref(), Some("gac"));
        assert_eq!(normalize_route_input("tuzi").as_deref(), Some("tuzi"));
        assert_eq!(
            normalize_route_input("my-proxy").as_deref(),
            Some("my-proxy")
        );
        assert!(normalize_route_input("bad.name").is_none());
        assert!(normalize_route_input("").is_none());
    }

    #[test]
    fn parse_npm_eexist_error_works() {
        let sample = "npm error code EEXIST\nnpm error File exists: /Users/test/.local/bin/codex\n";
        assert!(is_npm_eexist_error(sample));
        assert_eq!(
            extract_npm_file_exists_path(sample).as_deref(),
            Some("/Users/test/.local/bin/codex")
        );
    }

    #[test]
    fn parse_npm_eexist_error_missing_path() {
        let sample = "npm error code EEXIST\nnpm error EEXIST: file already exists\n";
        assert!(is_npm_eexist_error(sample));
        assert!(extract_npm_file_exists_path(sample).is_none());
    }

    #[test]
    fn filter_codex_sections_and_profile() {
        let raw = r#"profile = "gac"

[foo]
a = 1

[model_providers.gac]
name = "gac"
base_url = "https://gaccode.com/codex/v1"

[profiles.gac]
model_provider = "gac"

[bar]
b = 2
"#;
        let mut strip = BTreeSet::new();
        strip.insert("gac".to_string());
        strip.insert("tuzi".to_string());
        let filtered = filter_codex_config_strips(raw, &strip);
        assert!(filtered.contains("[foo]"));
        assert!(filtered.contains("[bar]"));
        assert!(!filtered.contains("[model_providers.gac]"));
        assert!(!filtered.contains("[profiles.gac]"));
        assert!(!filtered.contains("profile ="));
    }

    #[test]
    fn parse_codex_config_reads_sections() {
        let raw = r#"profile = "tuzi"

[model_providers.tuzi]
base_url = "https://api.tu-zi.com/v1"

[profiles.tuzi]
model = "gpt-5.5"
model_reasoning_effort = "high"
"#;

        let parsed = parse_codex_config(raw);
        assert_eq!(parsed.profile.as_deref(), Some("tuzi"));
        let route = parsed.routes.get("tuzi").expect("tuzi route missing");
        assert_eq!(route.base_url.as_deref(), Some("https://api.tu-zi.com/v1"));
        assert_eq!(route.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(route.model_reasoning_effort.as_deref(), Some("high"));
    }

    #[test]
    fn parse_codex_config_custom_route_section() {
        let raw = r#"profile = "corp"

[model_providers.corp]
base_url = "https://api.example.com/v1"

[profiles.corp]
model = "gpt-4o"
model_reasoning_effort = "low"
"#;
        let parsed = parse_codex_config(raw);
        assert_eq!(parsed.profile.as_deref(), Some("corp"));
        let route = parsed.routes.get("corp").expect("corp route");
        assert_eq!(
            route.base_url.as_deref(),
            Some("https://api.example.com/v1")
        );
    }
}
