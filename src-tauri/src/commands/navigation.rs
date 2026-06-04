use crate::{
    commands::settings::get_settings,
    models::{AppSettings, PageMetadata},
};
use reqwest::Url;
use scraper::{Html, Selector};
use std::time::Duration;

const DEFAULT_METADATA_TIMEOUT_SECS: u64 = 10;
const METADATA_USER_AGENT: &str = "RustBrowser/0.1 metadata fetcher";

#[derive(Debug, PartialEq, Eq)]
struct ExtractedMetadata {
    title: Option<String>,
    favicon_url: Option<String>,
}

pub fn normalize_url_with_settings(input: &str, settings: &AppSettings) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("URL or search query cannot be empty".to_string());
    }

    // Project decision: bare loopback hosts default to http://. Keep an
    // explicitly typed https://localhost URL untouched for callers that need
    // HTTPS.
    if is_loopback_host_without_scheme(trimmed) {
        return validate_web_url(&format!("http://{trimmed}"));
    }

    if looks_like_absolute_url(trimmed) {
        return validate_web_url(trimmed);
    }

    if looks_like_domain(trimmed) {
        return validate_web_url(&format!("https://{trimmed}"));
    }

    Ok(search_url(trimmed, settings))
}

#[tauri::command]
pub fn normalize_url(app: tauri::AppHandle, url: String) -> Result<String, String> {
    let settings = get_settings(app)?;
    normalize_url_with_settings(&url, &settings)
}

#[tauri::command]
pub async fn fetch_page_metadata(url: String) -> Result<PageMetadata, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_METADATA_TIMEOUT_SECS))
        .user_agent(METADATA_USER_AGENT)
        .build()
        .map_err(|error| format!("failed to create metadata client: {error}"))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("failed to fetch page metadata: {error}"))?
        .error_for_status()
        .map_err(|error| format!("failed to fetch page metadata: {error}"))?;

    let final_url = response.url().to_string();
    let html = response
        .text()
        .await
        .map_err(|error| format!("failed to read page metadata response: {error}"))?;

    Ok(page_metadata_from_html(&html, &final_url, &url))
}

fn looks_like_absolute_url(input: &str) -> bool {
    Url::parse(input).is_ok()
}

fn is_loopback_host_without_scheme(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    let local_names = ["localhost", "127.0.0.1", "[::1]"];

    local_names.iter().any(|name| {
        lower == *name
            || lower.starts_with(&format!("{name}:"))
            || lower.starts_with(&format!("{name}/"))
            || lower.starts_with(&format!("{name}?"))
            || lower.starts_with(&format!("{name}#"))
    })
}

fn looks_like_domain(input: &str) -> bool {
    if input.chars().any(char::is_whitespace) {
        return false;
    }

    let host_part = input
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('.');

    host_part.contains('.')
}

fn validate_web_url(input: &str) -> Result<String, String> {
    let parsed = Url::parse(input).map_err(|error| format!("invalid URL: {error}"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(format!("unsupported URL scheme: {}", parsed.scheme()));
    }
    if parsed.host_str().is_none() {
        return Err("URL must include a host".to_string());
    }

    Ok(input.to_string())
}

fn search_url(query: &str, settings: &AppSettings) -> String {
    let encoded_query = urlencoding::encode(query);
    if settings
        .default_search_engine
        .eq_ignore_ascii_case("google")
    {
        format!("https://www.google.com/search?q={encoded_query}")
    } else {
        format!("https://yandex.com/search/?text={encoded_query}")
    }
}

fn page_metadata_from_html(html: &str, page_url: &str, fallback_title: &str) -> PageMetadata {
    let extracted = extract_metadata_from_html(html, page_url);

    match extracted.title {
        Some(title) => PageMetadata {
            title,
            favicon_url: extracted.favicon_url,
        },
        None => PageMetadata {
            title: fallback_title.to_string(),
            favicon_url: None,
        },
    }
}

fn extract_metadata_from_html(html: &str, page_url: &str) -> ExtractedMetadata {
    let document = Html::parse_document(html);
    let title_selector = Selector::parse("title").expect("title selector is valid");
    let link_selector = Selector::parse("link").expect("link selector is valid");

    let title = document.select(&title_selector).find_map(|element| {
        let value = element.text().collect::<String>().trim().to_string();
        (!value.is_empty()).then_some(value)
    });

    let favicon_url = document.select(&link_selector).find_map(|element| {
        let rel = element.value().attr("rel")?.to_ascii_lowercase();
        let is_icon = rel.split_whitespace().any(|token| token == "icon");
        if !is_icon {
            return None;
        }

        let href = element.value().attr("href")?;
        resolve_relative_url(page_url, href)
    });

    ExtractedMetadata { title, favicon_url }
}

fn resolve_relative_url(page_url: &str, href: &str) -> Option<String> {
    Url::parse(page_url)
        .ok()
        .and_then(|base| base.join(href).ok())
        .map(|url| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(search_engine: &str) -> AppSettings {
        AppSettings {
            default_search_engine: search_engine.to_string(),
            ..AppSettings::default()
        }
    }

    #[test]
    fn bare_domain_normalizes_to_https_url() {
        let normalized =
            normalize_url_with_settings("example.com", &settings("yandex")).expect("normalize URL");

        assert_eq!(normalized, "https://example.com");
    }

    #[test]
    fn absolute_https_url_stays_unchanged() {
        let normalized =
            normalize_url_with_settings("https://example.com/path", &settings("yandex"))
                .expect("normalize URL");

        assert_eq!(normalized, "https://example.com/path");
    }

    #[test]
    fn localhost_with_port_normalizes_to_http_url() {
        let normalized = normalize_url_with_settings("localhost:3000", &settings("yandex"))
            .expect("normalize localhost URL");

        assert_eq!(normalized, "http://localhost:3000");
    }

    #[test]
    fn multi_word_input_uses_yandex_search_url_by_default() {
        let normalized = normalize_url_with_settings("rust tauri docs", &settings("yandex"))
            .expect("normalize search");

        assert_eq!(
            normalized,
            "https://yandex.com/search/?text=rust%20tauri%20docs"
        );
    }

    #[test]
    fn no_dot_word_uses_search_url() {
        let normalized =
            normalize_url_with_settings("no-dot-word", &settings("yandex")).expect("normalize");

        assert_eq!(normalized, "https://yandex.com/search/?text=no-dot-word");
    }

    #[test]
    fn default_search_engine_switches_between_yandex_and_google() {
        let yandex = normalize_url_with_settings("rust tauri docs", &settings("yandex"))
            .expect("normalize yandex search");
        let google = normalize_url_with_settings("rust tauri docs", &settings("google"))
            .expect("normalize google search");

        assert_eq!(
            yandex,
            "https://yandex.com/search/?text=rust%20tauri%20docs"
        );
        assert_eq!(
            google,
            "https://www.google.com/search?q=rust%20tauri%20docs"
        );
    }

    #[test]
    fn empty_input_returns_descriptive_error() {
        let error = normalize_url_with_settings("   ", &settings("yandex"))
            .expect_err("empty input should fail");

        assert!(error.contains("empty"));
    }

    #[test]
    fn extracts_trimmed_title_and_resolves_relative_favicon() {
        let html = r#"
            <!doctype html>
            <html>
                <head>
                    <title>  Example Page  </title>
                    <link rel="icon" href="/favicon.ico">
                </head>
            </html>
        "#;

        let metadata =
            page_metadata_from_html(html, "https://example.com/docs/page", "https://example.com");

        assert_eq!(metadata.title, "Example Page");
        assert_eq!(
            metadata.favicon_url,
            Some("https://example.com/favicon.ico".to_string())
        );
    }

    #[test]
    fn html_without_title_uses_requested_url_and_no_favicon() {
        let metadata = page_metadata_from_html(
            r#"<html><head><link rel="icon" href="/favicon.ico"></head></html>"#,
            "https://example.com/redirected",
            "https://example.com/requested",
        );

        assert_eq!(metadata.title, "https://example.com/requested");
        assert_eq!(metadata.favicon_url, None);
    }
}
