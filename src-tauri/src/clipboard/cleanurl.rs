//! Clean URL (spec/clipboard/clean-url.md): strips tracking parameters from a lone link.

/// Parameters removed on every site. A trailing `*` matches any suffix.
const GLOBAL: &[&str] = &[
    "utm_*", "fbclid", "gclid", "gclsrc", "dclid", "gbraid", "wbraid", "msclkid", "mc_cid", "mc_eid", "igshid",
    "igsh", "yclid", "twclid", "ttclid", "li_fat_id", "_hsenc", "_hsmi", "__hssc", "__hstc", "__hsfp", "hsCtaTracking",
    "mkt_tok", "oly_anon_id", "oly_enc_id", "rb_clickid", "s_cid", "vero_conv", "vero_id", "wickedid", "_openstat",
    "ga_source", "ga_medium", "ga_campaign", "ga_content", "ga_term", "spm", "scm", "trk", "trkCampaign", "ref_src",
    "ref_url", "si_source", "pk_campaign", "pk_kwd", "pk_source", "pk_medium", "piwik_campaign", "piwik_kwd",
    "mtm_*", "matomo_*", "cmpid", "srsltid", "_ga", "_gl", "epik", "cvid", "oicd",
];

/// Site rules built into Notifi: a host (matching it and its subdomains) and the parameters to drop there.
const SITES: &[(&str, &[&str])] = &[
    ("amazon.*", &["ref", "ref_", "pd_rd_*", "pf_rd_*", "_encoding", "psc", "qid", "sr", "keywords", "crid", "sprefix", "content-id", "dib", "dib_tag", "th", "linkCode", "tag", "linkId", "camp", "creative"]),
    ("youtube.com", &["si", "feature", "pp", "ab_channel"]),
    ("youtu.be", &["si", "feature"]),
    ("music.youtube.com", &["si", "feature"]),
    ("twitter.com", &["s", "t", "ref_src", "ref_url"]),
    ("x.com", &["s", "t"]),
    ("instagram.com", &["igshid", "igsh", "img_index"]),
    ("facebook.com", &["mibextid", "sfnsn", "__tn__", "__cft__*", "hc_ref", "fref", "ref"]),
    ("tiktok.com", &["_r", "_t", "is_from_webapp", "sender_device", "is_copy_url", "share_app_id", "share_link_id", "u_code", "preview_pb", "timestamp", "user_id", "tt_from", "sec_user_id"]),
    ("spotify.com", &["si", "context", "nd", "dl_branch"]),
    ("linkedin.com", &["trackingId", "refId", "lipi", "midToken", "midSig", "trk", "trkEmail", "eid", "otpToken"]),
    ("reddit.com", &["share_id", "utm_name", "rdt", "correlation_id", "ref_campaign", "ref_source", "$deep_link", "$3p", "_branch_match_id", "_branch_referrer"]),
    ("google.*", &["ved", "ei", "sei", "gs_lcp", "gs_lcrp", "gs_lp", "sca_esv", "sca_upv", "sxsrf", "oq", "aqs", "uact", "sclient", "iflsig", "rlz", "sourceid", "client", "bih", "biw", "dpr", "source", "fbs", "sa", "usg"]),
    ("bing.com", &["form", "sp", "sc", "qs", "pq", "sk", "ghc", "lq", "ghsh", "ghacc", "ghpl"]),
    ("ebay.*", &["_trkparms", "_trksid", "hash", "mkcid", "mkevt", "mkrid", "campid", "toolid", "customid", "amdata", "var"]),
    ("aliexpress.com", &["spm", "scm", "pvid", "algo_pvid", "algo_exp_id", "btsid", "ws_ab_test", "gatewayAdapt", "_randl_currency", "_randl_shipto", "aff_fcid", "aff_fsk", "aff_platform", "aff_trace_key", "terminal_id", "sk"]),
    ("medium.com", &["source", "sk"]),
    ("imdb.com", &["ref_", "pf_rd_*"]),
    ("etsy.com", &["click_key", "click_sum", "ref", "pro", "frs", "sts", "plkey", "organic_search_click", "ga_order", "ga_search_type", "ga_view_type", "ga_search_query"]),
    ("walmart.com", &["athbdg", "athcpid", "athpgid", "athznid", "athieid", "athena", "from", "sid", "wl13"]),
    ("twitch.tv", &["tt_medium", "tt_content"]),
    ("netflix.com", &["trackId", "tctx", "jbv"]),
];

#[derive(Debug, PartialEq, Eq)]
pub struct Cleaned {
    pub url: String,
    pub removed: Vec<String>,
}

/// One user rule line from the `cleanUrl.siteRules` list: `host: param, param` (`*` as the host
/// means every site). A line starting with `#` is switched off.
#[derive(Debug)]
struct Rule {
    host: String,
    params: Vec<String>,
}

fn parse_rules(lines: &[String]) -> Vec<Rule> {
    lines
        .iter()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| {
            let (host, params) = l.split_once(':')?;
            let host = host.trim().trim_start_matches("www.").to_ascii_lowercase();
            let params = params.split([',', ' ']).map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect::<Vec<_>>();
            (!host.is_empty() && !params.is_empty()).then_some(Rule { host, params })
        })
        .collect()
}

/// `host` matches `pattern` itself or a subdomain; a trailing `.*` matches any top-level part.
fn host_matches(host: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(stem) = pattern.strip_suffix(".*") {
        let labels: Vec<&str> = host.split('.').collect();
        let stem_labels: Vec<&str> = stem.split('.').collect();
        return labels.windows(stem_labels.len()).enumerate().any(|(i, w)| {
            w == stem_labels.as_slice() && labels.len() - i - stem_labels.len() <= 2 && labels.len() > i + stem_labels.len()
        });
    }
    host == pattern || host.ends_with(&format!(".{pattern}"))
}

fn param_matches(name: &str, pattern: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}

/// The copied text as a lone http(s) link, or `None` when it is anything more than that.
pub fn lone_link(text: &str) -> Option<&str> {
    let t = text.trim();
    if t.is_empty() || t.len() > 8192 || t.chars().any(char::is_whitespace) {
        return None;
    }
    let lower = t.get(..8)?.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")).then_some(t)
}

/// Removes tracking parameters from `url`. Only the query and fragment parameters change; the
/// path keeps its case and everything else is left byte for byte. `None` when nothing was removed.
pub fn clean(url: &str, user_rules: &[String]) -> Option<Cleaned> {
    let link = lone_link(url)?;
    let rules = parse_rules(user_rules);
    let after_scheme = link.find("://")? + 3;
    let rest = &link[after_scheme..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host = authority.rsplit('@').next()?.split(':').next()?.trim_start_matches("www.").to_ascii_lowercase();

    let mut patterns: Vec<&str> = GLOBAL.to_vec();
    for (site, params) in SITES {
        if host_matches(&host, site) {
            patterns.extend_from_slice(params);
        }
    }
    for rule in &rules {
        if host_matches(&host, &rule.host) {
            patterns.extend(rule.params.iter().map(String::as_str));
        }
    }

    let (before_fragment, fragment) = match link.split_once('#') {
        Some((a, f)) => (a, Some(f)),
        None => (link, None),
    };
    let (base, query) = match before_fragment.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (before_fragment, None),
    };
    let mut removed = Vec::new();
    let mut strip = |q: &str| -> String {
        q.split('&')
            .filter(|pair| {
                if pair.is_empty() {
                    return false;
                }
                let name = pair.split('=').next().unwrap_or_default();
                let drop = patterns.iter().any(|p| param_matches(name, p));
                if drop {
                    removed.push(name.to_string());
                }
                !drop
            })
            .collect::<Vec<_>>()
            .join("&")
    };
    let query = query.map(&mut strip);
    // Only fragments shaped like a query (`#a=b&c=d`) carry tracking; others are anchors.
    let fragment = fragment.map(|f| if f.contains('=') && !f.contains('/') { strip(f) } else { f.to_string() });
    if removed.is_empty() {
        return None;
    }
    let mut out = base.to_string();
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        out.push('?');
        out.push_str(&q);
    }
    if let Some(f) = fragment.filter(|f| !f.is_empty()) {
        out.push('#');
        out.push_str(&f);
    }
    Some(Cleaned { url: out, removed })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(url: &str) -> Option<String> {
        clean(url, &[]).map(|c| c.url)
    }

    #[test]
    fn strips_global_trackers() {
        assert_eq!(
            c("https://example.com/Page?id=7&utm_source=news&utm_medium=mail&fbclid=abc"),
            Some("https://example.com/Page?id=7".into())
        );
        assert_eq!(c("https://example.com/?gclid=1"), Some("https://example.com/".into()));
    }

    #[test]
    fn leaves_clean_links_and_text_alone() {
        assert_eq!(c("https://example.com/a?id=1"), None);
        assert_eq!(c("see https://example.com/?utm_source=x"), None);
        assert_eq!(c("ftp://example.com/?utm_source=x"), None);
        assert_eq!(c("https://example.com/?utm_source=x\nmore"), None);
    }

    #[test]
    fn keeps_path_case_and_order() {
        let out = clean("https://Example.com/CaseSensitive/Path?b=2&utm_campaign=z&a=1#Top", &[]).unwrap();
        assert_eq!(out.url, "https://Example.com/CaseSensitive/Path?b=2&a=1#Top");
        assert_eq!(out.removed, vec!["utm_campaign"]);
    }

    #[test]
    fn site_rules_apply_only_on_their_site() {
        assert_eq!(c("https://www.youtube.com/watch?v=abc&si=XYZ"), Some("https://www.youtube.com/watch?v=abc".into()));
        assert_eq!(c("https://example.com/watch?v=abc&si=XYZ"), None);
        assert_eq!(c("https://www.amazon.co.uk/dp/B0?ref_=x&psc=1"), Some("https://www.amazon.co.uk/dp/B0".into()));
        assert_eq!(c("https://www.amazon.de/dp/B0?th=1"), Some("https://www.amazon.de/dp/B0".into()));
        assert_eq!(c("https://notamazon.example/dp?ref=1"), None);
    }

    #[test]
    fn user_rules_add_and_can_be_switched_off() {
        let rules = vec!["shop.example.com: aff, src".to_string(), "# example.org: id".to_string()];
        assert_eq!(
            clean("https://shop.example.com/x?aff=9&src=2&q=1", &rules).map(|c| c.url),
            Some("https://shop.example.com/x?q=1".into())
        );
        assert_eq!(clean("https://example.org/x?id=1", &rules), None);
        let all = vec!["*: session".to_string()];
        assert_eq!(clean("https://a.test/?session=1", &all).map(|c| c.url), Some("https://a.test/".into()));
    }

    #[test]
    fn query_shaped_fragments_are_cleaned() {
        assert_eq!(c("https://example.com/p#utm_source=x"), Some("https://example.com/p".into()));
        assert_eq!(c("https://example.com/p?utm_source=x#/route=a/b"), Some("https://example.com/p#/route=a/b".into()));
    }

    #[test]
    fn host_patterns() {
        assert!(host_matches("m.youtube.com", "youtube.com"));
        assert!(!host_matches("notyoutube.com", "youtube.com"));
        assert!(host_matches("amazon.co.uk", "amazon.*"));
        assert!(host_matches("smile.amazon.com", "amazon.*"));
        assert!(!host_matches("amazon.example.net.evil.org", "amazon.*"));
    }
}
