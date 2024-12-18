use crate::settings::SETTINGS;
use reqwest::Url;

pub fn clean_url(url: &str) -> String {
    // Clean the URL of its query parameters, except for those that are in the SETTINGS.keep_query_params list for this domain.
    let mut parsed_url = Url::parse(url).unwrap();
    let host = parsed_url.host_str().unwrap_or("");

    // Check if we have rules for this domain
    let matching_domain = SETTINGS
        .keep_query_params
        .keys()
        .find(|&domain| host.ends_with(domain));

    if let Some(domain) = matching_domain {
        if let Some(allowed_params) = SETTINGS.keep_query_params.get(domain) {
            // Build new query string with only allowed parameters
            let params: Vec<(String, String)> = parsed_url
                .query_pairs()
                .filter(|(k, _)| allowed_params.contains(&k.to_string()))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            parsed_url.query_pairs_mut().clear();
            for (key, value) in params {
                parsed_url.query_pairs_mut().append_pair(&key, &value);
            }
        }
    } else {
        // No rules for this domain - strip all query params
        parsed_url.set_query(None);
    }
    parsed_url.set_fragment(None);
    parsed_url.to_string()
}
