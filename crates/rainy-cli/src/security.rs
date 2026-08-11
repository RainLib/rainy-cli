use std::net::IpAddr;

const SENSITIVE_QUERY_KEYS: &[&str] = &[
    "access_key",
    "apikey",
    "api_key",
    "authorization",
    "credential",
    "password",
    "secret",
    "signature",
    "token",
];

pub fn validate_http(value: &str, allow_loopback_http: bool) -> Result<(), &'static str> {
    let url = parse(value)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("embedded URL credentials are not allowed");
    }
    match url.scheme() {
        "https" => Ok(()),
        "http" if allow_loopback_http && loopback_host(&url) => Ok(()),
        "http" => Err("plain HTTP is allowed only for loopback development endpoints"),
        _ => Err("URL must use HTTPS"),
    }
}

pub fn validate_git(value: &str, allow_file: bool) -> Result<(), &'static str> {
    validate_git_with_private_http(value, allow_file, false)
}

pub fn validate_git_with_private_http(
    value: &str,
    allow_file: bool,
    allow_private_http: bool,
) -> Result<(), &'static str> {
    if let Some((user_host, path)) = value.split_once(':')
        && !value.contains("://")
    {
        let Some((user, host)) = user_host.split_once('@') else {
            return Err("SCP-style Git sources must use git@host:path");
        };
        if user == "git" && !host.is_empty() && !path.is_empty() && safe_text(value) {
            return Ok(());
        }
        return Err("SCP-style Git sources must use git@host:path");
    }
    let url = parse(value)?;
    if url.password().is_some() {
        return Err("embedded URL passwords are not allowed");
    }
    match url.scheme() {
        "https" if url.username().is_empty() => Ok(()),
        "ssh" if url.username().is_empty() || url.username() == "git" => Ok(()),
        "file" if allow_file && url.username().is_empty() => Ok(()),
        "http"
            if allow_private_http
                && url.username().is_empty()
                && private_or_loopback_host(&url) =>
        {
            Ok(())
        }
        "https" | "ssh" => Err("embedded URL credentials are not allowed"),
        "http" if !url.username().is_empty() => Err("embedded URL credentials are not allowed"),
        "http" => Err("plain HTTP Git sources require an explicit private-network opt-in"),
        _ => Err("Git source must use HTTPS or SSH"),
    }
}

fn parse(value: &str) -> Result<url::Url, &'static str> {
    if !safe_text(value) {
        return Err("URL contains unsafe characters");
    }
    let url = url::Url::parse(value).map_err(|_| "URL is invalid")?;
    if url.host_str().is_none() && url.scheme() != "file" {
        return Err("URL host is required");
    }
    if url.query_pairs().any(|(key, _)| {
        let key = key.to_ascii_lowercase();
        SENSITIVE_QUERY_KEYS
            .iter()
            .any(|sensitive| key == *sensitive || key.contains(sensitive))
    }) {
        return Err("sensitive authentication query parameters are not allowed");
    }
    Ok(url)
}

fn safe_text(value: &str) -> bool {
    !value.trim().is_empty() && !value.starts_with('-') && !value.chars().any(char::is_control)
}

fn loopback_host(url: &url::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn private_or_loopback_host(url: &url::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|address| match address {
            IpAddr::V4(address) => {
                address.is_private() || address.is_loopback() || address.is_link_local()
            }
            IpAddr::V6(address) => {
                address.is_unique_local()
                    || address.is_loopback()
                    || address.is_unicast_link_local()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_credentials_and_sensitive_queries() {
        assert!(validate_http("https://user:secret@example.com/file", false).is_err());
        assert!(validate_http("https://example.com/file?token=secret", false).is_err());
        assert!(validate_http("http://127.0.0.1:8080/file", true).is_ok());
        assert!(validate_git("git@example.com:company/packs.git", false).is_ok());
        assert!(validate_git("https://user@example.com/packs.git", false).is_err());
        assert!(validate_git("http://192.168.0.161/packs.git", false).is_err());
        assert!(
            validate_git_with_private_http("http://192.168.0.161/packs.git", false, true).is_ok()
        );
        assert!(
            validate_git_with_private_http("http://example.com/packs.git", false, true).is_err()
        );
    }
}
