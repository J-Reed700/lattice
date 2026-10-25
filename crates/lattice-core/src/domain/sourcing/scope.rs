//! Search scopes: the one place a search names the site it keeps to, and
//! where that site lives now.
//!
//! A scope is a need's own field ([`super::SourceNeed::scope`]). Planners
//! that learned search from search boxes also write it into the query as a
//! `site:` operator, sometimes several joined by `OR`; such a query is one
//! search per site with its scope in the wrong place, and [`lift_sites`]
//! moves it back.
//!
//! A scope names a site as the caller knows it, and callers (models most
//! of all) know sites by the names they had. Sites move: `docs.anthropic.com`
//! serves its pages from `platform.claude.com`, `reactjs.org` from
//! `react.dev`, `docs.microsoft.com` from `learn.microsoft.com`. Searching,
//! listing and filtering under the old name finds nothing, or discards
//! exactly the pages the engines return from the new one. The site itself
//! says where it went, through its redirects, and [`SiteHome`] reads them.

use url::Url;

use super::need::Origin;

/// `query` without its `site:` operators, and the sites they named, each
/// once, in the order written. The `OR` or `|` that joined an operator to
/// its neighbour goes with it, so `site:a OR site:b tool use` is the terms
/// `tool use` on two sites. An exclusion (`-site:`) is a term, not a scope,
/// and stays; an operator naming no site is dropped.
pub fn lift_sites(query: &str) -> (String, Vec<Origin>) {
    let tokens: Vec<(&str, Token)> = query
        .split_whitespace()
        .map(|text| (text, Token::of(text)))
        .collect();
    let is_operator = |index: Option<usize>| {
        index
            .and_then(|index| tokens.get(index))
            .is_some_and(|(_, token)| matches!(token, Token::Site(_)))
    };
    let mut sites: Vec<Origin> = Vec::new();
    let mut terms: Vec<&str> = Vec::new();
    for (index, (text, token)) in tokens.iter().enumerate() {
        match token {
            Token::Site(Some(site)) if !sites.contains(site) => sites.push(site.clone()),
            Token::Site(_) => {}
            Token::Connector
                if is_operator(index.checked_sub(1)) || is_operator(index.checked_add(1)) => {}
            Token::Connector | Token::Term => terms.push(text),
        }
    }
    (terms.join(" "), sites)
}

/// One whitespace-separated token of a query, as [`lift_sites`] reads it.
enum Token {
    /// A `site:` operator, and the site it names when it names one.
    Site(Option<Origin>),
    /// `OR` or `|`: joins two alternatives in the engines' query syntax.
    Connector,
    Term,
}

impl Token {
    fn of(text: &str) -> Self {
        let bare = text.trim_matches(['(', ')', '"', '\'', ',', ';']);
        match bare.get(..5) {
            Some(head) if head.eq_ignore_ascii_case("site:") => {
                Self::Site(bare.get(5..).and_then(Origin::parse))
            }
            _ if text == "OR" || text == "|" => Self::Connector,
            _ => Self::Term,
        }
    }
}

/// One redirect a site answered with: its status and where it pointed,
/// resolved against the address that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    pub status: u16,
    pub to: Url,
}

impl Hop {
    /// 301 Moved Permanently or 308 Permanent Redirect: "the target resource
    /// has been assigned a new permanent URI" (RFC 9110 §15.4.2, §15.4.9).
    /// 302, 303 and 307 are a sign-in wall, a locale or a campaign; they say
    /// nothing about where the site's pages live.
    pub fn is_permanent(&self) -> bool {
        matches!(self.status, 301 | 308)
    }
}

/// A scope and the home its site answers from now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteHome {
    asked: Origin,
    home: Origin,
}

impl SiteHome {
    /// A scope whose site has not moved, or whose move could not be read.
    pub fn unmoved(asked: Origin) -> Self {
        Self {
            home: asked.clone(),
            asked,
        }
    }

    /// The home `asked` reaches through `hops`, the redirects its address
    /// answered with, in order.
    ///
    /// A hop the current home still covers is the site arranging its own
    /// pages (a scheme, a locale, an index) and is passed over. The first hop
    /// that leaves it decides: a permanent one moves the home there and the
    /// walk goes on from it; any other ends the walk where it stands. A move
    /// keeps the scope's breadth: the home is as many path segments deep on
    /// the new address as the scope was on the old, so a whole site moves to
    /// a whole host and `github.com/dotnet` to the one account it became.
    pub fn after(asked: Origin, hops: &[Hop]) -> Self {
        let depth = asked
            .path_prefix()
            .map_or(0, |prefix| segments(prefix).count());
        let mut home = asked.clone();
        for hop in hops {
            if home.covers(&hop.to) {
                continue;
            }
            let Some(host) = hop.to.host_str().filter(|_| hop.is_permanent()) else {
                break;
            };
            let prefix: Vec<&str> = segments(hop.to.path()).take(depth).collect();
            if prefix.len() < depth {
                break;
            }
            home = Origin::new(host, Some(&prefix.join("/")));
        }
        Self { asked, home }
    }

    /// The scope as the caller wrote it.
    pub fn asked(&self) -> &Origin {
        &self.asked
    }

    /// Where the site answers from now: what a search keeps to.
    pub fn home(&self) -> &Origin {
        &self.home
    }

    pub fn moved(&self) -> bool {
        self.home != self.asked
    }

    /// Whether `url` is this site's page: on its home, or under the name the
    /// scope was asked by, which the engines still index and which redirects
    /// home when read.
    pub fn covers(&self, url: &Url) -> bool {
        self.home.covers(url) || self.asked.covers(url)
    }
}

/// A path's non-empty segments.
fn segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|segment| !segment.is_empty())
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
