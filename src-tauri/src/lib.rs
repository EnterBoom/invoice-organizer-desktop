use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OrganizerSettings {
    source_dir: Option<String>,
    target_dir: Option<String>,
    manual_date: Option<String>,
    project_name: Option<String>,
    owner_name: Option<String>,
    group_by_month: bool,
    include_type_folder: bool,
    preserve_originals: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewRequest {
    source_dir: String,
    target_dir: Option<String>,
    manual_date: String,
    project_name: String,
    owner_name: String,
    group_by_month: bool,
    include_type_folder: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteRequest {
    items: Vec<PlanItem>,
    preserve_originals: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PlanItem {
    source_path: String,
    source_name: String,
    proposed_name: String,
    target_path: String,
    relative_output_path: String,
    manual_date: String,
    project_name: String,
    owner_name: String,
    invoice_month: Option<String>,
    issuer: Option<String>,
    amount: Option<String>,
    invoice_type: Option<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewSummary {
    total_files: u32,
    supported_files: u32,
    planned_files: u32,
    ignored_files: u32,
    warning_files: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewResponse {
    resolved_target_dir: String,
    summary: PreviewSummary,
    items: Vec<PlanItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteResponse {
    written_count: u32,
    skipped_count: u32,
    error_count: u32,
    errors: Vec<String>,
}

#[derive(Debug, Default)]
struct ScanStats {
    total_files: u32,
    supported_files: u32,
}

fn organizer_settings_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let base_dir = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| "无法确定 Windows 用户配置目录".to_string())?;

    #[cfg(not(target_os = "windows"))]
    let base_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "无法确定用户主目录".to_string())?
        .join(".codex");

    Ok(base_dir.join("invoice-organizer").join("settings.json"))
}

fn resolve_target_root(source_dir: &Path, target_dir: Option<&str>) -> PathBuf {
    match target_dir.map(str::trim).filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => source_dir.join("已整理发票"),
    }
}

fn is_supported_invoice_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "pdf" | "ofd" | "jpg" | "jpeg" | "png" | "webp" | "heic"
            )
        })
        .unwrap_or(false)
}

fn collect_files(
    current_dir: &Path,
    skip_dir: Option<&Path>,
    files: &mut Vec<PathBuf>,
    stats: &mut ScanStats,
) -> Result<(), String> {
    for entry in fs::read_dir(current_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();

        if let Some(skip_dir) = skip_dir {
            if path.starts_with(skip_dir) {
                continue;
            }
        }

        if path.is_dir() {
            collect_files(&path, skip_dir, files, stats)?;
            continue;
        }

        stats.total_files += 1;
        if is_supported_invoice_file(&path) {
            stats.supported_files += 1;
            files.push(path);
        }
    }

    Ok(())
}

fn detect_invoice_type(stem: &str) -> Option<String> {
    let text = stem.to_ascii_lowercase();
    let mappings = [
        ("火车票", "火车票"),
        ("行程单", "行程单"),
        ("通行费", "通行费"),
        ("过路费", "通行费"),
        ("出租车", "打车票"),
        ("打车", "打车票"),
        ("加油", "加油票"),
        ("专用发票", "专票"),
        ("专票", "专票"),
        ("普通发票", "普票"),
        ("普票", "普票"),
        ("电子发票", "电子发票"),
        ("数电", "数电发票"),
        ("全电", "数电发票"),
    ];

    for (needle, label) in mappings {
        if stem.contains(needle) || text.contains(needle) {
            return Some(label.to_string());
        }
    }

    None
}

fn parse_contiguous_date(chars: &[char], start: usize) -> Option<String> {
    if start + 8 > chars.len() {
        return None;
    }

    let slice: String = chars[start..start + 8].iter().collect();
    if !slice.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let year = slice[0..4].parse::<u32>().ok()?;
    let month = slice[4..6].parse::<u32>().ok()?;
    let day = slice[6..8].parse::<u32>().ok()?;

    if !(2000..=2100).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn parse_delimited_date(chars: &[char], start: usize) -> Option<String> {
    if start + 8 >= chars.len() {
        return None;
    }

    let year: String = chars.get(start..start + 4)?.iter().collect();
    if !year.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let sep_one = *chars.get(start + 4)?;
    if !matches!(sep_one, '-' | '_' | '.' | '/' | '年') {
        return None;
    }

    let mut cursor = start + 5;
    let mut month = String::new();
    while let Some(current) = chars.get(cursor) {
        if current.is_ascii_digit() && month.len() < 2 {
            month.push(*current);
            cursor += 1;
            continue;
        }
        break;
    }

    if month.is_empty() {
        return None;
    }

    let sep_two = *chars.get(cursor)?;
    if !matches!(sep_two, '-' | '_' | '.' | '/' | '月') {
        return None;
    }
    cursor += 1;

    let mut day = String::new();
    while let Some(current) = chars.get(cursor) {
        if current.is_ascii_digit() && day.len() < 2 {
            day.push(*current);
            cursor += 1;
            continue;
        }
        break;
    }

    if day.is_empty() {
        return None;
    }

    let year = year.parse::<u32>().ok()?;
    let month = month.parse::<u32>().ok()?;
    let day = day.parse::<u32>().ok()?;

    if !(2000..=2100).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn extract_invoice_date(stem: &str) -> Option<String> {
    let chars: Vec<char> = stem.chars().collect();

    for index in 0..chars.len() {
        if let Some(date) = parse_delimited_date(&chars, index) {
            return Some(date);
        }
    }

    for index in 0..chars.len() {
        if let Some(date) = parse_contiguous_date(&chars, index) {
            return Some(date);
        }
    }

    None
}

#[derive(Debug)]
struct AmountCandidate {
    value: f64,
    has_decimal: bool,
}

fn collect_amount_candidates(text: &str) -> Vec<AmountCandidate> {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    let mut candidates = Vec::new();

    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }

        let mut token = String::new();
        let mut seen_decimal = false;
        while index < chars.len() {
            let current = chars[index];
            if current.is_ascii_digit() {
                token.push(current);
                index += 1;
                continue;
            }

            if current == '.' && !seen_decimal {
                seen_decimal = true;
                token.push(current);
                index += 1;
                continue;
            }

            break;
        }

        if token.ends_with('.') {
            token.pop();
            seen_decimal = false;
        }

        if token.is_empty() {
            continue;
        }

        if let Ok(value) = token.parse::<f64>() {
            if value > 0.0 && value < 100_000.0 {
                candidates.push(AmountCandidate {
                    value,
                    has_decimal: seen_decimal,
                });
            }
        }
    }

    candidates
}

fn extract_invoice_amount(stem: &str) -> Option<String> {
    let markers = ["价税合计", "合计", "金额", "小写", "¥", "￥", "rmb", "cny"];
    let lowercase = stem.to_ascii_lowercase();

    for marker in markers {
        if let Some(position) = lowercase.find(marker) {
            let suffix = &stem[position + marker.len()..];
            if let Some(candidate) = collect_amount_candidates(suffix).into_iter().next() {
                return Some(format!("{:.2}", candidate.value));
            }
        }
    }

    let mut candidates = collect_amount_candidates(stem)
        .into_iter()
        .filter(|candidate| candidate.has_decimal)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.value.total_cmp(&right.value));

    candidates.last().map(|candidate| format!("{:.2}", candidate.value))
}

fn normalize_vendor_text(stem: &str, invoice_type: Option<&str>) -> String {
    let mut cleaned = stem.to_string();
    let replacements = [
        "电子发票",
        "普通发票",
        "专用发票",
        "发票",
        "票据",
        "报销",
        "附件",
        "扫描件",
        "截图",
        "图片",
        "影像",
        "凭证",
        "下载",
        "invoice",
        "fapiao",
        "pdf",
        "ofd",
        "jpg",
        "jpeg",
        "png",
        "webp",
        "heic",
        "金额",
        "价税合计",
        "合计",
        "小写",
        "蓝字",
        "红字",
        "数电",
        "全电",
        "普票",
        "专票",
    ];

    for item in replacements {
        cleaned = cleaned.replace(item, " ");
    }

    if let Some(kind) = invoice_type {
        cleaned = cleaned.replace(kind, " ");
    }

    cleaned
        .chars()
        .map(|character| {
            if character.is_ascii_digit()
                || matches!(
                    character,
                    '_' | '-' | '.' | '/' | '\\' | '[' | ']' | '(' | ')' | '【' | '】' | '（' | '）'
                        | '，' | ',' | '。' | '、' | ':' | '：' | '+' | '#'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn is_generic_vendor_token(token: &str) -> bool {
    let generic = [
        "未识别",
        "文件",
        "票",
        "单",
        "通知",
        "平台",
        "客户",
        "订单",
        "消费",
        "开票",
        "记账",
        "报销单",
        "抬头",
        "公司",
    ];

    generic.iter().any(|item| token == *item)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn extract_vendor(stem: &str, invoice_type: Option<&str>) -> Option<String> {
    let cleaned = normalize_vendor_text(stem, invoice_type);
    let mut tokens = cleaned
        .split_whitespace()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .filter(|token| !is_generic_vendor_token(token))
        .filter(|token| token.chars().any(|character| character.is_alphabetic()))
        .map(|token| truncate_chars(token, 18))
        .collect::<Vec<_>>();

    if tokens.is_empty() {
        return None;
    }

    tokens.retain(|token| token.chars().count() >= 2);
    if tokens.is_empty() {
        return None;
    }

    let combined = tokens
        .iter()
        .take(3)
        .fold(String::new(), |mut output, token| {
            if output.chars().count() + token.chars().count() <= 16 {
                output.push_str(token);
            }
            output
        });

    if combined.chars().count() >= 3 {
        return Some(combined);
    }

    tokens
        .into_iter()
        .max_by_key(|token| token.chars().count())
        .map(|token| truncate_chars(&token, 16))
}

fn build_name_hint(stem: &str) -> String {
    let cleaned = stem
        .chars()
        .map(|character| {
            if character.is_ascii_digit()
                || matches!(
                    character,
                    '_' | '-' | '.' | '/' | '\\' | '[' | ']' | '(' | ')' | '【' | '】' | '（' | '）'
                        | '，' | ',' | '。' | '、' | ':' | '：'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();

    let condensed = cleaned.split_whitespace().collect::<String>();
    if condensed.is_empty() {
        "待确认主体".to_string()
    } else {
        truncate_chars(&condensed, 12)
    }
}

fn normalize_manual_date(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("请先填写本批发票的日期。".to_string());
    }

    extract_invoice_date(trimmed).ok_or_else(|| "日期格式无效，请使用类似 2026-05-06 的格式。".to_string())
}

fn normalize_project_name(input: &str) -> Result<String, String> {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err("请先填写这批发票的项目归属。".to_string());
    }

    Ok(normalized)
}

fn normalize_owner_name(input: &str) -> Result<String, String> {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err("请先填写这批发票的归属人。".to_string());
    }

    Ok(normalized)
}

fn sanitize_filename_stem(name: &str) -> String {
    let mut result = String::new();
    let mut previous_was_separator = false;

    for character in name.chars() {
        let mapped = if matches!(character, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            || character.is_control()
            || character.is_whitespace()
        {
            '_'
        } else {
            character
        };

        if mapped == '_' {
            if !previous_was_separator {
                result.push(mapped);
            }
            previous_was_separator = true;
        } else {
            result.push(mapped);
            previous_was_separator = false;
        }
    }

    let trimmed = result.trim_matches(|character| character == '_' || character == '.');
    if trimmed.is_empty() {
        "发票文件".to_string()
    } else {
        trimmed.to_string()
    }
}

fn unique_target_path(base_target: &Path, reserved: &mut HashSet<PathBuf>) -> PathBuf {
    let parent = base_target.parent().unwrap_or_else(|| Path::new(""));
    let extension = base_target.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    let stem = base_target
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("发票文件");

    let mut candidate = base_target.to_path_buf();
    let mut counter = 2;
    while candidate.exists() || reserved.contains(&candidate) {
        let next_name = if extension.is_empty() {
            format!("{stem} ({counter})")
        } else {
            format!("{stem} ({counter}).{extension}")
        };
        candidate = parent.join(next_name);
        counter += 1;
    }

    reserved.insert(candidate.clone());
    candidate
}

fn build_plan_item(
    path: &Path,
    target_root: &Path,
    manual_date: &str,
    project_name: &str,
    owner_name: &str,
    group_by_month: bool,
    include_type_folder: bool,
    reserved: &mut HashSet<PathBuf>,
) -> Result<PlanItem, String> {
    let source_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "无法读取文件名".to_string())?
        .to_string();

    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "无法读取文件主体".to_string())?;
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

    let invoice_type = detect_invoice_type(stem);
    let source_invoice_date = extract_invoice_date(stem);
    let mut amount = extract_invoice_amount(stem);
    if let (Some(date), Some(total)) = (source_invoice_date.as_ref(), amount.as_ref()) {
        if total.starts_with(&date[0..4]) {
            amount = None;
        }
    }
    let issuer = extract_vendor(stem, invoice_type.as_deref());
    let invoice_month = Some(manual_date[0..7].to_string());
    let issuer_label = issuer.clone().unwrap_or_else(|| build_name_hint(stem));
    let amount_label = amount.clone().unwrap_or_else(|| "待确认金额".to_string());

    let mut warnings = Vec::new();
    if issuer.is_none() {
        warnings.push("未识别开票主体，已用原名片段".to_string());
    }
    if amount.is_none() {
        warnings.push("未识别金额".to_string());
    }
    if include_type_folder && invoice_type.is_none() {
        warnings.push("未识别票种，已归入待确认类型".to_string());
    }

    let mut name_parts = Vec::new();
    name_parts.push(amount_label);
    name_parts.push(issuer_label);
    name_parts.push(project_name.to_string());
    name_parts.push(owner_name.to_string());

    let stem_name = sanitize_filename_stem(&name_parts.join("-"));
    let proposed_name = if extension.is_empty() {
        stem_name
    } else {
        format!("{stem_name}.{extension}")
    };

    let mut relative_parts = Vec::new();
    let mut target_path = target_root.to_path_buf();
    if group_by_month {
        let month_folder = invoice_month.clone().unwrap_or_else(|| "未识别月份".to_string());
        relative_parts.push(month_folder.clone());
        target_path.push(month_folder);
    }
    if include_type_folder {
        let type_folder = invoice_type.clone().unwrap_or_else(|| "待确认类型".to_string());
        relative_parts.push(type_folder.clone());
        target_path.push(type_folder);
    }
    target_path.push(&proposed_name);

    let unique_target = unique_target_path(&target_path, reserved);
    let unique_name = unique_target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&proposed_name)
        .to_string();

    if unique_name != proposed_name {
        warnings.push("检测到重名，已自动补后缀".to_string());
    }

    relative_parts.push(unique_name.clone());

    Ok(PlanItem {
        source_path: path.display().to_string(),
        source_name,
        proposed_name: unique_name,
        target_path: unique_target.display().to_string(),
        relative_output_path: relative_parts.join("/"),
        manual_date: manual_date.to_string(),
        project_name: project_name.to_string(),
        owner_name: owner_name.to_string(),
        invoice_month,
        issuer,
        amount,
        invoice_type,
        warnings,
    })
}

fn move_or_copy_file(source: &Path, target: &Path, preserve_originals: bool) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    if preserve_originals {
        fs::copy(source, target).map_err(|error| error.to_string())?;
        return Ok(());
    }

    match fs::rename(source, target) {
        Ok(_) => Ok(()),
        Err(_) => {
            fs::copy(source, target).map_err(|error| error.to_string())?;
            fs::remove_file(source).map_err(|error| error.to_string())
        }
    }
}

#[tauri::command]
fn load_organizer_settings() -> Result<Option<OrganizerSettings>, String> {
    let path = organizer_settings_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let settings = serde_json::from_str(&content).map_err(|error| error.to_string())?;
    Ok(Some(settings))
}

#[tauri::command]
fn save_organizer_settings(settings: OrganizerSettings) -> Result<(), String> {
    let path = organizer_settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let content = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn preview_invoice_plan(request: PreviewRequest) -> Result<PreviewResponse, String> {
    let source_dir = PathBuf::from(&request.source_dir);
    if !source_dir.exists() || !source_dir.is_dir() {
        return Err("原始发票文件夹不存在，或不是一个目录。".to_string());
    }

    let manual_date = normalize_manual_date(&request.manual_date)?;
    let project_name = normalize_project_name(&request.project_name)?;
    let owner_name = normalize_owner_name(&request.owner_name)?;
    let target_root = resolve_target_root(&source_dir, request.target_dir.as_deref());
    let skip_dir = if target_root != source_dir && target_root.starts_with(&source_dir) {
        Some(target_root.as_path())
    } else {
        None
    };
    let mut files = Vec::new();
    let mut stats = ScanStats::default();
    collect_files(&source_dir, skip_dir, &mut files, &mut stats)?;
    files.sort();

    let mut reserved_targets = HashSet::new();
    let mut items = Vec::new();
    for path in files {
        items.push(build_plan_item(
            &path,
            &target_root,
            &manual_date,
            &project_name,
            &owner_name,
            request.group_by_month,
            request.include_type_folder,
            &mut reserved_targets,
        )?);
    }

    let warning_files = items.iter().filter(|item| !item.warnings.is_empty()).count() as u32;

    Ok(PreviewResponse {
        resolved_target_dir: target_root.display().to_string(),
        summary: PreviewSummary {
            total_files: stats.total_files,
            supported_files: stats.supported_files,
            planned_files: items.len() as u32,
            ignored_files: stats.total_files.saturating_sub(stats.supported_files),
            warning_files,
        },
        items,
    })
}

#[tauri::command]
fn execute_invoice_plan(request: ExecuteRequest) -> Result<ExecuteResponse, String> {
    let mut written_count = 0;
    let mut skipped_count = 0;
    let mut errors = Vec::new();
    let mut reserved_targets = HashSet::new();

    for item in request.items {
        let source = PathBuf::from(&item.source_path);
        if !source.exists() {
            skipped_count += 1;
            errors.push(format!("文件已不存在，已跳过：{}", item.source_name));
            continue;
        }

        let requested_target = PathBuf::from(&item.target_path);
        let actual_target = unique_target_path(&requested_target, &mut reserved_targets);

        if let Err(error) = move_or_copy_file(&source, &actual_target, request.preserve_originals) {
            errors.push(format!("处理失败：{}，原因：{error}", item.source_name));
            continue;
        }

        written_count += 1;
    }

    Ok(ExecuteResponse {
        written_count,
        skipped_count,
        error_count: errors.len() as u32,
        errors,
    })
}

#[tauri::command]
fn reveal_in_finder(path: String) -> Result<(), String> {
    let target = PathBuf::from(path);
    let directory = if target.is_dir() {
        target
    } else {
        target
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| "无效路径".to_string())?
    };

    #[cfg(target_os = "windows")]
    let status = Command::new("explorer").arg(&directory).status();

    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(&directory).status();

    #[cfg(all(unix, not(target_os = "macos")))]
    let status = Command::new("xdg-open").arg(&directory).status();

    let status = status.map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("无法打开文件夹：{}", directory.display()));
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            load_organizer_settings,
            save_organizer_settings,
            preview_invoice_plan,
            execute_invoice_plan,
            reveal_in_finder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
