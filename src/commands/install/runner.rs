use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use spore::logging::{SpanContext, workflow_span};
use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::bin_paths;
use crate::commands::developer_tools;
use crate::commands::github;
use crate::commands::github::GitHubClient;
use crate::commands::install::profile_surface::ManualProfileMember;
use crate::commands::install::release::{
    self, download_binary, download_sha256sums, extract_tarball, fetch_latest_release,
    find_checksum_asset, find_matching_asset, normalize_version, platform_key,
    verify_asset_checksum, verify_binary, verify_functional,
};
use crate::commands::install::save_selected_profile;
use crate::commands::install::selection::{
    print_install_preview, render_embedded_profile_install_preview, resolve_requested_tools,
    split_requested_tools,
};
use crate::commands::output;
use crate::commands::runtime_policy;
use crate::commands::tool_registry::{self, InstallProfile, ToolSpec};
use crate::install_state;
use crate::verify;

/// Configuration options for the install flow.
/// These bools represent distinct, related configuration options that are only meaningful together.
#[allow(clippy::struct_excessive_bools)]
pub struct InstallOptions {
    /// Install all tools from the active profile.
    pub all: bool,
    /// Profile to use for installation (None means interactive selection).
    pub profile: Option<InstallProfile>,
    /// Print preview and exit without installing.
    pub dry_run: bool,
    /// Build tools from local source instead of downloading releases.
    pub from_source: bool,
    /// Local source directory (used when `from_source` is true).
    pub source_dir: Option<PathBuf>,
    /// Force installation even if tool already exists.
    pub force: bool,
}

pub(crate) fn install_tool(
    tool: &str,
    prefix: &Path,
    force: bool,
    client: &GitHubClient,
) -> Result<()> {
    install_tool_with_source(tool, prefix, force, client, None)
}

pub(crate) fn install_tool_with_source(
    tool: &str,
    prefix: &Path,
    force: bool,
    client: &GitHubClient,
    source: Option<&str>,
) -> Result<()> {
    let install_path = prefix.join(tool);

    if check_already_installed(&install_path, force) {
        return Ok(());
    }

    let (release, data) = fetch_and_download_binary(tool, client)?;
    let platform_key = platform_key();
    let asset = find_matching_asset(&release, platform_key)?;
    verify_download_checksum(&data, asset, &release, tool, client)?;

    let temp_guard =
        tempfile::TempDir::new().context("Failed to create temporary directory for extraction")?;
    let extracted_path = extract_and_verify_binary(&data, temp_guard.path(), &release)?;

    deploy_binary(&install_path, &extracted_path)?;
    let version = verify_binary(&extracted_path)?;

    verify_and_report_installation(tool, &install_path, &version, prefix)?;
    record_install_state(tool, &install_path, &version, source.unwrap_or("lamella"));

    Ok(())
}

fn check_already_installed(install_path: &Path, force: bool) -> bool {
    if install_path.exists() && !force {
        let tool = install_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        println!(
            "  {} {} already installed at {}. Use --force to replace.",
            "⊘".yellow(),
            tool,
            install_path.display()
        );
        true
    } else {
        false
    }
}

fn fetch_and_download_binary(
    tool: &str,
    client: &GitHubClient,
) -> Result<(release::GitHubRelease, Vec<u8>)> {
    println!("  {} Fetching release information...", "⏳".yellow());

    let release = fetch_latest_release(tool, client)?;
    let platform_key = platform_key();
    let asset = find_matching_asset(&release, platform_key)?;

    println!("  {} Found {}: {}", "✓".green(), tool, release.version);

    println!("  {} Downloading {}...", "⏳".yellow(), asset.name);
    let progress = ProgressBar::new(0);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{bar:30.cyan/blue} {bytes}/{total_bytes}")
            .expect("valid progress bar template")
            .progress_chars("=>-"),
    );
    let data = download_binary(asset, &progress, client)?;

    Ok((release, data))
}

fn verify_download_checksum(
    data: &[u8],
    asset: &release::ReleaseAsset,
    release: &release::GitHubRelease,
    tool: &str,
    client: &GitHubClient,
) -> Result<()> {
    let sha256sums = find_checksum_asset(release)
        .map(|cs_asset| download_sha256sums(cs_asset, client))
        .transpose()?;
    if let Some(sums) = &sha256sums {
        verify_asset_checksum(data, &asset.name, sums)
            .with_context(|| format!("Checksum verification failed for {}", asset.name))?;
    } else {
        tracing::warn!(
            "no SHA256SUMS asset found for {} {}; skipping checksum verification",
            tool,
            release.version
        );
    }
    Ok(())
}

fn extract_and_verify_binary(
    data: &[u8],
    temp_dir: &Path,
    release: &release::GitHubRelease,
) -> Result<PathBuf> {
    println!("  {} Extracting...", "⏳".yellow());
    let extracted_path = extract_tarball(data, temp_dir)?;

    println!("  {} Verifying...", "⏳".yellow());
    let version = verify_binary(&extracted_path)?;

    if normalize_version(&version) != normalize_version(&release.version) {
        return Err(anyhow!(
            "Version mismatch after extraction: expected {}, binary reports {}",
            release.version,
            version
        ));
    }

    Ok(extracted_path)
}

fn deploy_binary(install_path: &Path, extracted_path: &Path) -> Result<()> {
    // Stage to a sibling temp path, set permissions, then atomically rename
    // into place. This avoids ETXTBSY on Linux when replacing a running binary
    // and prevents a partial overwrite from corrupting the existing installation.
    let staging_path = install_path.with_extension("installing");

    let result = deploy_to_staging(extracted_path, &staging_path, install_path);
    if result.is_err() {
        // Remove the staging file on any failure so a subsequent install attempt
        // does not find a stale or partially-written binary.
        let _ = fs::remove_file(&staging_path);
    }
    result
}

fn deploy_to_staging(
    extracted_path: &Path,
    staging_path: &Path,
    install_path: &Path,
) -> Result<()> {
    fs::copy(extracted_path, staging_path).with_context(|| {
        format!(
            "Failed to copy {} to {}",
            extracted_path.display(),
            staging_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(staging_path, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("Failed to make {} executable", staging_path.display()))?;
    }

    #[cfg(target_os = "macos")]
    {
        let path_str = staging_path.to_str().ok_or_else(|| {
            anyhow!(
                "staging path is not valid UTF-8: {}",
                staging_path.display()
            )
        })?;
        let _ = std::process::Command::new("codesign")
            .args(["--force", "--sign", "-", path_str])
            .output();
    }

    // Atomic rename: same filesystem guaranteed because staging_path is a sibling.
    fs::rename(staging_path, install_path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            staging_path.display(),
            install_path.display()
        )
    })?;

    Ok(())
}

fn verify_and_report_installation(
    tool: &str,
    install_path: &Path,
    version: &str,
    prefix: &Path,
) -> Result<()> {
    let spec = tool_registry::find(tool).ok_or_else(|| anyhow!("Unknown tool: {tool}"))?;

    match verify_functional(install_path, spec) {
        Ok(()) => {
            if spec.smoke_test_args.is_some() {
                println!("  {} {} functional check passed", "✓".green(), tool);
            }
        }
        Err(error) => {
            return Err(anyhow!("{tool} functional check failed: {error}"));
        }
    }

    println!(
        "  {} {} installed: {} → {}",
        "✓".green(),
        tool,
        version,
        install_path.display()
    );

    let report = verify::check_completeness(tool, prefix);
    if !report.all_passed() {
        let failing: Vec<String> = report
            .failed_points()
            .iter()
            .filter_map(|r| r.detail.clone())
            .collect();
        eprintln!(
            "  {} Post-install note: some integration points are not yet active: {}",
            "!".yellow(),
            failing.join("; ")
        );
        eprintln!(
            "  {} Run `stipe init` to complete host wiring, then `stipe doctor` to verify.",
            "→".yellow()
        );
    }

    if let Err(error) = verify::write_ownership_state(tool, &report) {
        eprintln!(
            "  {} Could not write ownership state: {error}",
            "!".yellow()
        );
    }

    Ok(())
}

fn record_install_state(tool: &str, install_path: &Path, version: &str, source: &str) {
    let install_path_str = install_path.to_string_lossy();
    let checksum = install_state::compute_checksum(install_path).ok();
    let checksum_ref = checksum.as_deref();
    if let Ok(conn) = install_state::open() {
        if let Err(error) = install_state::record_install(
            &conn,
            tool,
            "binary",
            Some(install_path_str.as_ref()),
            Some(version),
            Some(source),
            checksum_ref,
        ) {
            eprintln!("  {} Could not record install state: {error}", "!".yellow());
        }
    }
}

/// Default root directory for local source checkouts.
fn default_monorepo_root() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| {
        eprintln!(
            "  {} Could not determine home directory; defaulting to current directory",
            "!".yellow()
        );
        PathBuf::from(".")
    });
    home.join("projects").join("basidiocarp")
}

/// Resolve the cargo install path for a tool inside the monorepo.
///
/// Some tools live inside workspace sub-crates (e.g. `hyphae-cli` inside
/// the hyphae workspace). This function maps tool names to their `--path`
/// argument for `cargo install`.
fn source_install_path(tool_name: &str, source_dir: &Path) -> PathBuf {
    match tool_name {
        "hyphae" => source_dir.join("crates").join("hyphae-cli"),
        "rhizome" => source_dir.join("crates").join("rhizome-cli"),
        _ => source_dir.to_path_buf(),
    }
}

/// Build and install a tool from local source using `cargo install --path`.
pub(crate) fn install_from_source(
    tool_name: &str,
    spec: &ToolSpec,
    source_dir: &Path,
) -> Result<String> {
    let cargo_toml = source_dir.join("Cargo.toml");
    if !cargo_toml.exists() {
        anyhow::bail!(
            "No Cargo.toml found at {} — verify the source directory exists",
            source_dir.display()
        );
    }

    let install_path = source_install_path(tool_name, source_dir);

    println!(
        "  {} Building {} from source ({})",
        "⏳".yellow(),
        tool_name,
        install_path.display()
    );

    let status = std::process::Command::new("cargo")
        .args(["install", "--path"])
        .arg(&install_path)
        .status()
        .with_context(|| format!("Failed to run cargo install for {tool_name}"))?;

    if !status.success() {
        anyhow::bail!("cargo install failed for {tool_name}");
    }

    let binary = which::which(tool_name)
        .with_context(|| format!("{tool_name} not found in PATH after cargo install"))?;
    let version = verify_binary(&binary)?;

    match verify_functional(&binary, spec) {
        Ok(()) => {
            if spec.smoke_test_args.is_some() {
                println!("  {} {} functional check passed", "✓".green(), tool_name);
            }
        }
        Err(error) => {
            return Err(anyhow!("{tool_name} functional check failed: {error}"));
        }
    }

    println!(
        "  {} {} installed from source: {} → {}",
        "✓".green(),
        tool_name,
        version,
        binary.display()
    );

    // Best-effort completeness check and ownership record for source installs.
    let source_install_dir = binary.parent().map_or_else(
        || std::path::PathBuf::from("."),
        std::path::Path::to_path_buf,
    );
    let report = verify::check_completeness(tool_name, &source_install_dir);
    if !report.all_passed() {
        let failing: Vec<String> = report
            .failed_points()
            .iter()
            .filter_map(|r| r.detail.clone())
            .collect();
        eprintln!(
            "  {} Post-install note: some integration points are not yet active: {}",
            "!".yellow(),
            failing.join("; ")
        );
    }

    if let Err(error) = verify::write_ownership_state(tool_name, &report) {
        eprintln!(
            "  {} Could not write ownership state: {error}",
            "!".yellow()
        );
    }

    // Record the install in the SQLite install-state database.
    // Best-effort: a failure here does not abort the install.
    let binary_str = binary.to_string_lossy();
    let checksum = install_state::compute_checksum(&binary).ok();
    let checksum_ref = checksum.as_deref();
    if let Ok(conn) = install_state::open() {
        if let Err(error) = install_state::record_install(
            &conn,
            tool_name,
            "binary",
            Some(binary_str.as_ref()),
            Some(&version),
            Some("stipe-source"),
            checksum_ref,
        ) {
            eprintln!("  {} Could not record install state: {error}", "!".yellow());
        }
    }

    Ok(version)
}

pub(crate) fn install_bin_dir() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_install_bin_dir_override() {
        return Ok(path);
    }

    bin_paths::local_bin_dir().ok_or_else(|| anyhow!("Could not determine local bin directory"))
}

pub(crate) fn run(opts: &InstallOptions, tools: &[String]) -> Result<()> {
    let _lock =
        crate::lockfile::acquire_lock(opts.force).context("could not acquire install lock")?;
    let span_context = install_span_context();
    let _workflow_span = workflow_span("install", &span_context).entered();
    let prefix = install_bin_dir()?;

    print_install_banner();

    if let Some(profile) = opts
        .profile
        .filter(|profile| *profile != InstallProfile::DeveloperTools)
    {
        check_and_enforce_policy(profile)?;
    }

    if opts.profile == Some(InstallProfile::DeveloperTools) {
        handle_developer_tools_profile(tools);
        return Ok(());
    }

    let tools_to_install = resolve_requested_tools(opts.all, opts.profile, tools);

    if opts.dry_run {
        print_install_preview_and_exit(&prefix, tools_to_install.as_deref());
        return Ok(());
    }

    let (tools_to_install, manual_tools) = resolve_and_split_tools(tools_to_install)?;
    println!();

    let installed_binary_paths = build_binary_paths(&prefix, &tools_to_install);
    create_preinstall_backup(&installed_binary_paths)?;

    let mut failures = Vec::new();
    if opts.from_source {
        install_from_source_phase(opts, &tools_to_install, &mut failures);
    } else {
        install_from_releases_phase(&tools_to_install, &prefix, &mut failures);
    }

    let has_manual_follow_up = !manual_tools.is_empty();
    print_manual_follow_up(&manual_tools);
    println!();

    finalize_install(&failures, opts.profile, has_manual_follow_up)
}

fn print_install_banner() {
    crate::banner::print_banner();
    println!("{}", "Basidiocarp Ecosystem Installer".bold());
    println!("{}", "─".repeat(75));
    println!(
        "{}",
        "Bring the local operator canopy online with a deliberate rollout.".dimmed()
    );
    println!();
}

fn check_and_enforce_policy(profile: InstallProfile) -> Result<()> {
    let runtime_policy = runtime_policy::collect_runtime_policy(Some(profile));
    for line in runtime_policy::render_install_policy_lines(profile, &runtime_policy) {
        println!("{line}");
    }
    runtime_policy::enforce_install_profile_policy(profile, &runtime_policy)?;
    println!();
    Ok(())
}

fn handle_developer_tools_profile(tools: &[String]) {
    let unknown = developer_tools::unknown_requested_tools(tools);
    let report = developer_tools::install_report(tools);

    for line in developer_tools::render_install_advice(&report) {
        println!("{line}");
    }

    if !unknown.is_empty() {
        println!("Unknown developer tools:");
        for name in unknown {
            println!("  - {name}");
        }
        println!();
    }
}

fn print_install_preview_and_exit(prefix: &Path, tools_to_install: Option<&[String]>) {
    match tools_to_install {
        Some(requested) => {
            let label = "all".to_string();
            print_install_preview(prefix, requested, &label);
        }
        None => {
            print_install_preview(prefix, &[], "interactive selection");
        }
    }
    println!();
}

fn resolve_and_split_tools(
    tools_to_install: Option<Vec<String>>,
) -> Result<(Vec<String>, Vec<ManualProfileMember>)> {
    let tools_to_install: Vec<String> = if let Some(tools) = tools_to_install {
        tools
    } else {
        prompt_tool_selection()?
    };
    Ok(split_requested_tools(&tools_to_install))
}

fn prompt_tool_selection() -> Result<Vec<String>> {
    let theme = ColorfulTheme::default();
    let installable_specs = tool_registry::installable_specs();
    println!("{}", "Choose your operator kit.".bold());
    println!(
        "{}",
        "Managed tools start selected. Trim the list to fit this machine.".dimmed()
    );
    println!();

    let tool_items: Vec<(String, bool)> = installable_specs
        .iter()
        .map(|spec| (format!("{:<15} — {}", spec.name, spec.description), true))
        .collect();

    let selections = MultiSelect::with_theme(&theme)
        .items_checked(tool_items)
        .interact()?;

    if selections.is_empty() {
        println!();
        println!("{}", "No tools selected. Exiting.".yellow());
        println!();
        return Ok(Vec::new());
    }

    Ok(selections
        .iter()
        .map(|&idx| installable_specs[idx].name.to_string())
        .collect())
}

fn build_binary_paths(prefix: &Path, tools: &[String]) -> Vec<(String, PathBuf)> {
    tools
        .iter()
        .map(|tool| (tool.clone(), prefix.join(tool)))
        .collect()
}

fn create_preinstall_backup(installed_binary_paths: &[(String, PathBuf)]) -> Result<()> {
    let timestamp = crate::backup::backup_timestamp();
    let stipe_version = env!("CARGO_PKG_VERSION");
    let _backup_path =
        crate::backup::create_backup(&timestamp, stipe_version, installed_binary_paths, &[])
            .context("could not create pre-install backup")?;
    Ok(())
}

fn install_from_source_phase(opts: &InstallOptions, tools: &[String], failures: &mut Vec<String>) {
    let monorepo_root = opts
        .source_dir
        .clone()
        .unwrap_or_else(default_monorepo_root);

    for tool in tools {
        let tool_source = monorepo_root.join(tool);
        let spec = tool_registry::find(tool);
        let Some(spec) = spec else {
            eprintln!("  {} Unknown tool: {}", "!".red(), tool);
            continue;
        };
        match install_from_source(tool, spec, &tool_source) {
            Ok(_version) => {}
            Err(error) => {
                eprintln!(
                    "  {} Failed to build {} from source: {}",
                    "!".red(),
                    tool,
                    error
                );
                failures.push(format!("{tool}: {error}"));
            }
        }
    }
}

fn install_from_releases_phase(tools: &[String], prefix: &Path, failures: &mut Vec<String>) {
    let client = github::github_client();

    for tool in tools {
        match install_tool_for_run(tool, prefix, false, &client) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("  {} Failed to install {}: {}", "!".red(), tool, error);
                failures.push(format!("{tool}: {error}"));
            }
        }
    }
}

fn print_manual_follow_up(manual_tools: &[ManualProfileMember]) {
    if !manual_tools.is_empty() {
        println!();
        println!("{}", "Manual follow-up:".bold());
        for member in manual_tools {
            println!("  - {}: {}", member.name, member.install_hint);
        }
    }
}

fn finalize_install(
    failures: &[String],
    profile: Option<InstallProfile>,
    has_manual_follow_up: bool,
) -> Result<()> {
    if failures.is_empty() {
        let persisted_profile = selected_profile_for_persistence(failures, profile);

        if let Some(profile) = persisted_profile {
            persist_install_profile_state(profile)?;
        }

        print_install_success_summary(persisted_profile, has_manual_follow_up);
        println!();
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "installation failed for {} tool(s): {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}

pub(crate) fn run_embedded_preview(profile: InstallProfile) -> Result<()> {
    let prefix = install_bin_dir()?;
    let requested = resolve_requested_tools(false, Some(profile), &[]).unwrap_or_else(|| {
        tool_registry::specs_for_profile(profile)
            .into_iter()
            .map(|spec| spec.name.to_string())
            .collect()
    });

    for (index, line) in render_embedded_profile_install_preview(&prefix, profile, &requested)
        .into_iter()
        .enumerate()
    {
        if index == 0 {
            println!("{}", line.yellow());
        } else if line.starts_with("Profile:") || line.ends_with(':') {
            println!("{}", line.bold());
        } else {
            println!("{line}");
        }
    }

    println!();
    Ok(())
}

pub(crate) fn selected_profile_for_persistence(
    failures: &[String],
    profile: Option<InstallProfile>,
) -> Option<InstallProfile> {
    if failures.is_empty() {
        profile.filter(|selected| *selected != InstallProfile::DeveloperTools)
    } else {
        None
    }
}

fn persist_install_profile_state(profile: InstallProfile) -> Result<()> {
    if let Some(config_path) = save_selected_profile(profile)? {
        let policy_path = runtime_policy::remember_install_profile_approval(profile)?;
        println!();
        println!(
            "{} {} ({})",
            "✓".green(),
            format_args!("Saved install profile: {}", profile.mode_label()),
            config_path.display()
        );
        if let Some(policy_path) = policy_path {
            println!(
                "{} Updated approval memory and runtime policy ({})",
                "✓".green(),
                policy_path.display()
            );
        }
    }

    Ok(())
}

pub(crate) fn render_install_success_summary(
    profile: Option<InstallProfile>,
    has_manual_follow_up: bool,
) -> Vec<String> {
    let mut lines = vec!["Installation complete.".to_string()];

    let state = if has_manual_follow_up {
        "the managed canopy is in place; finish the manual follow-up to complete setup"
    } else {
        "the local canopy is ready for host wiring"
    };

    if let Some(profile) = profile.filter(|profile| *profile != InstallProfile::DeveloperTools) {
        lines.push(format!(
            "Profile checkpoint: {} is saved for this project.",
            profile.mode_label()
        ));
    }

    lines.extend(output::render_footer(
        state,
        "run `stipe init` to wire hosts and shared MCP state",
        Some(
            "run `stipe doctor` first if you want a status readout before wiring hosts".to_string(),
        ),
    ));

    lines
}

fn print_install_success_summary(profile: Option<InstallProfile>, has_manual_follow_up: bool) {
    let lines = render_install_success_summary(profile, has_manual_follow_up);

    for (index, line) in lines.into_iter().enumerate() {
        if index == 0 {
            println!("{}", line.green().bold());
        } else if line.starts_with("Profile checkpoint:") {
            println!("{}", line.dimmed());
        } else if line.starts_with("Next step:") {
            println!("{}", line.bold());
        } else {
            println!("{}", line.dimmed());
        }
    }
}

fn install_tool_for_run(
    tool: &str,
    prefix: &Path,
    force: bool,
    client: &GitHubClient,
) -> Result<()> {
    #[cfg(test)]
    if let Some(result) = test_install_outcome_override() {
        return result;
    }

    install_tool(tool, prefix, force, client)
}

#[cfg(test)]
thread_local! {
    static TEST_INSTALL_BIN_DIR_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static TEST_INSTALL_OUTCOME_OVERRIDE: std::cell::RefCell<Option<std::result::Result<(), String>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn test_install_bin_dir_override() -> Option<PathBuf> {
    TEST_INSTALL_BIN_DIR_OVERRIDE.with(|path| path.borrow().clone())
}

#[cfg(test)]
fn test_install_outcome_override() -> Option<Result<()>> {
    TEST_INSTALL_OUTCOME_OVERRIDE.with(|outcome| {
        outcome
            .borrow()
            .clone()
            .map(|result| result.map_err(anyhow::Error::msg))
    })
}

#[cfg(test)]
pub(crate) fn with_install_test_overrides<T>(
    bin_dir: PathBuf,
    install_result: std::result::Result<(), String>,
    f: impl FnOnce() -> T,
) -> T {
    TEST_INSTALL_BIN_DIR_OVERRIDE.with(|bin_override| {
        TEST_INSTALL_OUTCOME_OVERRIDE.with(|install_override| {
            let previous_bin = bin_override.replace(Some(bin_dir));
            let previous_install = install_override.replace(Some(install_result));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            install_override.replace(previous_install);
            bin_override.replace(previous_bin);
            match result {
                Ok(value) => value,
                Err(payload) => std::panic::resume_unwind(payload),
            }
        })
    })
}

fn install_span_context() -> SpanContext {
    let context = SpanContext::for_app("stipe");
    match crate::commands::host_policy::project_root().or_else(|| std::env::current_dir().ok()) {
        Some(path) => context.with_workspace_root(path.display().to_string()),
        None => context,
    }
}
