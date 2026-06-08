use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use ureq::RequestExt;
use ureq::http;

const GITHUB_API_ACCEPT: &str = "application/vnd.github.v3+json";
const GITHUB_USER_AGENT: &str = concat!("stipe/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub(crate) struct GitHubClient {
    authorization: Option<String>,
}

pub(crate) fn github_client() -> GitHubClient {
    let authorization = github_token().map(|token| format!("Bearer {token}"));

    GitHubClient { authorization }
}

impl GitHubClient {
    fn request(&self, url: &str) -> Result<ureq::http::Request<()>> {
        let mut builder = http::Request::builder()
            .method(http::Method::GET)
            .uri(url)
            .header("Accept", GITHUB_API_ACCEPT)
            .header("User-Agent", GITHUB_USER_AGENT);

        if let Some(authorization) = self.authorization.as_deref() {
            builder = builder.header("Authorization", authorization);
        }

        builder.body(()).context("failed to build GitHub request")
    }

    pub(crate) fn get(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>> {
        self.request(url)?
            .with_default_agent()
            .configure()
            .http_status_as_error(false)
            .run()
            .with_context(|| format!("failed to fetch {url}"))
    }
}

pub(crate) fn get_github_json(
    client: &GitHubClient,
    url: &str,
    resource: &str,
) -> Result<serde_json::Value> {
    let mut response = client.get(url)?;
    let status = response.status();

    if !status.is_success() {
        let rate_remaining = response
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        let rate_reset = response
            .headers()
            .get("x-ratelimit-reset")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        let body = response
            .body_mut()
            .read_to_string()
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "<empty response>".to_string());

        let guidance = if status.as_u16() == 403 {
            if github_token().is_some() {
                format!(
                    "GitHub API returned {status} for {resource}. rate_limit_remaining={rate_remaining}, rate_limit_reset={rate_reset}. Response: {body}"
                )
            } else {
                format!(
                    "GitHub API returned {status} for {resource}. rate_limit_remaining={rate_remaining}, rate_limit_reset={rate_reset}. Set GH_TOKEN or GITHUB_TOKEN to avoid anonymous rate limits. Response: {body}"
                )
            }
        } else {
            format!("GitHub API returned {status} for {resource}. Response: {body}")
        };

        return Err(anyhow!(guidance));
    }

    let body = response
        .body_mut()
        .read_to_string()
        .context("failed to read GitHub JSON response")?;

    serde_json::from_str(&body)
        .with_context(|| format!("failed to parse GitHub JSON for {resource}"))
}

/// Fetch the latest GitHub release tag for a single tool (e.g., "v0.11.3").
pub(crate) fn fetch_release_tag(tool: &str, client: &GitHubClient) -> Result<String> {
    use crate::commands::install::release::{release_api_base, release_latest_url};
    use crate::commands::tool_registry;
    let repo = tool_registry::find(tool).map_or(tool, |spec| spec.release_repo);
    // Route through release_api_base() + release_latest_url() — the same single
    // source of truth the install/self-update path uses — so STIPE_GITHUB_API_BASE
    // redirects the update-check path too (it previously hardcoded api.github.com).
    let url = release_latest_url(&release_api_base(), repo);
    let data = get_github_json(client, &url, &format!("latest release for {repo}"))?;
    data.get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Could not parse tag_name for {repo}"))
        .map(str::to_string)
}

/// Fetch the latest released version for all given tools.
/// Best-effort: failures for individual tools are skipped silently.
/// Returns a map of tool name → normalized version string (e.g., "0.11.3").
pub(crate) fn fetch_live_tool_versions(
    tools: &[&str],
    client: &GitHubClient,
) -> HashMap<String, String> {
    use crate::commands::install::release::normalize_version;
    tools
        .iter()
        .filter_map(|tool| {
            fetch_release_tag(tool, client)
                .ok()
                .map(|tag| ((*tool).to_string(), normalize_version(&tag).to_string()))
        })
        .collect()
}

fn github_token() -> Option<String> {
    ["GH_TOKEN", "GITHUB_TOKEN"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .filter(|value| !value.trim().is_empty())
}
