pub fn is_http_or_https_scheme(scheme: &str) -> bool {
    matches!(scheme, "http" | "https")
}
