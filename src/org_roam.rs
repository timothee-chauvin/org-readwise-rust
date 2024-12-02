use reqwest::Url;

pub fn org_roam_ref(url: &str) -> String {
    // Return the org-roam ref for a given URL.
    // It's defined as the URL starting with a double slash, without the scheme.
    // org-roam also stores URLs as UTF-8, not as percent-encoded.

    let parsed_url = Url::parse(url).unwrap();
    let clean_url = format!(
        "//{}{}",
        parsed_url.host_str().unwrap_or(""),
        parsed_url.path()
    );
    urlencoding::decode(&clean_url).expect("UTF-8").to_string()
}
