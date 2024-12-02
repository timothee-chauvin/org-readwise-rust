use reqwest::Url;

pub fn clean_url(url: &str) -> String {
    // Clean a URL of its query parameters.
    let parsed_url = Url::parse(url).unwrap();
    format!(
        "{}://{}{}",
        parsed_url.scheme(),
        parsed_url.host_str().unwrap_or(""),
        parsed_url.path()
    )
}
