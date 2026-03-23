use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};

const GITHUB_API_ACCEPT: &str = "application/vnd.github.v3+json";
const GITHUB_USER_AGENT: &str = concat!("stipe/", env!("CARGO_PKG_VERSION"));

pub(crate) fn github_client() -> Result<Client> {
    let mut default_headers = HeaderMap::new();
    default_headers.insert(ACCEPT, HeaderValue::from_static(GITHUB_API_ACCEPT));
    default_headers.insert(USER_AGENT, HeaderValue::from_static(GITHUB_USER_AGENT));

    if let Some(token) = github_token() {
        let value = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("failed to build GitHub authorization header")?;
        default_headers.insert(AUTHORIZATION, value);
    }

    Client::builder()
        .default_headers(default_headers)
        .build()
        .context("failed to build GitHub HTTP client")
}

pub(crate) fn get_github_json(
    client: &Client,
    url: &str,
    resource: &str,
) -> Result<serde_json::Value> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to fetch {resource}"))?;

    if !response.status().is_success() {
        let status = response.status();
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
            .text()
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

    response
        .json()
        .with_context(|| format!("failed to parse GitHub JSON for {resource}"))
}

fn github_token() -> Option<String> {
    ["GH_TOKEN", "GITHUB_TOKEN"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .filter(|value| !value.trim().is_empty())
}
