use std::collections::HashMap;
use std::time::{Duration, Instant};

use irc::proto::command::Numeric::*;
use irc::proto::Command;

use crate::isupport;

/// Default time-to-live for cached WHOIS entries.
const DEFAULT_TTL: Duration = Duration::from_secs(300);

/// Structured WHOIS data for a single user, accumulated from server responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhoisData {
    pub nickname: String,
    pub username: Option<String>,
    pub hostname: Option<String>,
    pub real_name: Option<String>,
    pub server: Option<String>,
    pub server_info: Option<String>,
    pub channels: Vec<String>,
    pub account: Option<String>,
    pub idle_secs: Option<u64>,
    pub sign_on_secs: Option<u64>,
    pub secure: bool,
    pub away_message: Option<String>,
}

impl WhoisData {
    pub fn new(nickname: impl Into<String>) -> Self {
        Self {
            nickname: nickname.into(),
            username: None,
            hostname: None,
            real_name: None,
            server: None,
            server_info: None,
            channels: Vec::new(),
            account: None,
            idle_secs: None,
            sign_on_secs: None,
            secure: false,
            away_message: None,
        }
    }

    /// Apply a single WHOIS-related numeric to this entry.
    /// Returns `true` if the command was recognized and applied.
    pub fn apply(&mut self, command: &Command) -> bool {
        match command {
            Command::Numeric(RPL_WHOISUSER, params) => {
                // params: <client> <nick> <username> <hostname> * <realname>
                if let (Some(user), Some(host), Some(real)) =
                    (params.get(2), params.get(3), params.get(5))
                {
                    self.username = Some(user.clone());
                    self.hostname = Some(host.clone());
                    self.real_name = Some(real.clone());
                }
                true
            }
            Command::Numeric(RPL_WHOISSERVER, params) => {
                // params: <client> <nick> <server> <server_info>
                if let (Some(srv), Some(info)) = (params.get(2), params.get(3))
                {
                    self.server = Some(srv.clone());
                    self.server_info = Some(info.clone());
                }
                true
            }
            Command::Numeric(RPL_WHOISCHANNELS, params) => {
                // params: <client> <nick> <channels>
                if let Some(chans) = params.get(2) {
                    self.channels
                        .extend(chans.split_whitespace().map(String::from));
                }
                true
            }
            Command::Numeric(RPL_WHOISIDLE, params) => {
                // params: <client> <nick> <idle> <signon> :seconds idle, signon time
                if let Some(idle) = params.get(2).and_then(|s| s.parse().ok()) {
                    self.idle_secs = Some(idle);
                }
                if let Some(signon) =
                    params.get(3).and_then(|s| s.parse().ok())
                {
                    self.sign_on_secs = Some(signon);
                }
                true
            }
            Command::Numeric(RPL_WHOISACCOUNT, params) => {
                // params: <client> <nick> <account> :is logged in as
                if let Some(acct) = params.get(2) {
                    self.account = Some(acct.clone());
                }
                true
            }
            Command::Numeric(RPL_WHOISSECURE, _) => {
                self.secure = true;
                true
            }
            Command::Numeric(RPL_AWAY, params) => {
                // params: <client> <nick> <message>
                if let Some(msg) = params.get(2) {
                    self.away_message = Some(msg.clone());
                }
                true
            }
            Command::Numeric(
                RPL_WHOISCERTFP | RPL_WHOISREGNICK | RPL_WHOISOPERATOR
                | RPL_WHOISSPECIAL | RPL_WHOISACTUALLY | RPL_WHOISHOST
                | RPL_WHOISMODES,
                _,
            ) => {
                // Recognized but not stored in structured fields.
                true
            }
            _ => false,
        }
    }
}

/// A time-stamped cache entry.
#[derive(Debug, Clone)]
struct Entry {
    data: WhoisData,
    fetched_at: Instant,
}

/// Cache of WHOIS data keyed by normalized nickname.
#[derive(Debug)]
pub struct WhoisCache {
    entries: HashMap<String, Entry>,
    ttl: Duration,
}

impl Default for WhoisCache {
    fn default() -> Self {
        Self::new(DEFAULT_TTL)
    }
}

impl WhoisCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
        }
    }

    /// Insert or replace a completed WHOIS entry.
    pub fn insert(&mut self, data: WhoisData, casemapping: isupport::CaseMap) {
        let key = casemapping.normalize(&data.nickname);
        self.entries.insert(
            key,
            Entry {
                data,
                fetched_at: Instant::now(),
            },
        );
    }

    /// Look up cached WHOIS data. Returns `None` if absent or expired.
    pub fn get(
        &self,
        nick: NickRef<'_>,
        casemapping: isupport::CaseMap,
    ) -> Option<&WhoisData> {
        let key = casemapping.normalize(nick.as_str());
        self.entries.get(&key).and_then(|entry| {
            if entry.fetched_at.elapsed() < self.ttl {
                Some(&entry.data)
            } else {
                None
            }
        })
    }

    /// Remove expired entries.
    pub fn evict_expired(&mut self) {
        self.entries
            .retain(|_, entry| entry.fetched_at.elapsed() < self.ttl);
    }

    /// Number of (possibly expired) entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

use crate::user::NickRef;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isupport;
    use crate::user::Nick;
    use irc::proto::command::Numeric;

    fn numeric(num: Numeric, params: Vec<&str>) -> Command {
        Command::Numeric(num, params.into_iter().map(String::from).collect())
    }

    // ── WhoisData::apply ──────────────────────────────────────────

    #[test]
    fn apply_whoisuser() {
        let mut data = WhoisData::new("alice");
        let cmd = numeric(
            RPL_WHOISUSER,
            vec!["me", "alice", "alice", "example.com", "*", "Alice Smith"],
        );
        assert!(data.apply(&cmd));
        assert_eq!(data.username.as_deref(), Some("alice"));
        assert_eq!(data.hostname.as_deref(), Some("example.com"));
        assert_eq!(data.real_name.as_deref(), Some("Alice Smith"));
    }

    #[test]
    fn apply_whoisserver() {
        let mut data = WhoisData::new("alice");
        let cmd = numeric(
            RPL_WHOISSERVER,
            vec!["me", "alice", "irc.libera.chat", "Stockholm, SE"],
        );
        assert!(data.apply(&cmd));
        assert_eq!(data.server.as_deref(), Some("irc.libera.chat"));
        assert_eq!(data.server_info.as_deref(), Some("Stockholm, SE"));
    }

    #[test]
    fn apply_whoischannels() {
        let mut data = WhoisData::new("alice");
        let cmd = numeric(
            RPL_WHOISCHANNELS,
            vec!["me", "alice", "#rust @#halloy +#general"],
        );
        assert!(data.apply(&cmd));
        assert_eq!(
            data.channels,
            vec!["#rust", "@#halloy", "+#general"]
        );
    }

    #[test]
    fn apply_whoischannels_accumulates() {
        let mut data = WhoisData::new("alice");
        // Servers may split channels across multiple 319 replies.
        data.apply(&numeric(
            RPL_WHOISCHANNELS,
            vec!["me", "alice", "#one #two"],
        ));
        data.apply(&numeric(
            RPL_WHOISCHANNELS,
            vec!["me", "alice", "#three"],
        ));
        assert_eq!(data.channels, vec!["#one", "#two", "#three"]);
    }

    #[test]
    fn apply_whoisidle() {
        let mut data = WhoisData::new("alice");
        let cmd = numeric(
            RPL_WHOISIDLE,
            vec!["me", "alice", "120", "1700000000", "seconds idle, signon time"],
        );
        assert!(data.apply(&cmd));
        assert_eq!(data.idle_secs, Some(120));
        assert_eq!(data.sign_on_secs, Some(1_700_000_000));
    }

    #[test]
    fn apply_whoisaccount() {
        let mut data = WhoisData::new("alice");
        let cmd = numeric(
            RPL_WHOISACCOUNT,
            vec!["me", "alice", "alice_account", "is logged in as"],
        );
        assert!(data.apply(&cmd));
        assert_eq!(data.account.as_deref(), Some("alice_account"));
    }

    #[test]
    fn apply_whoissecure() {
        let mut data = WhoisData::new("alice");
        let cmd = numeric(RPL_WHOISSECURE, vec!["me", "alice", "is using a secure connection"]);
        assert!(data.apply(&cmd));
        assert!(data.secure);
    }

    #[test]
    fn apply_away() {
        let mut data = WhoisData::new("alice");
        let cmd = numeric(RPL_AWAY, vec!["me", "alice", "Gone fishing"]);
        assert!(data.apply(&cmd));
        assert_eq!(data.away_message.as_deref(), Some("Gone fishing"));
    }

    #[test]
    fn apply_unrelated_command_returns_false() {
        let mut data = WhoisData::new("alice");
        let cmd = Command::PRIVMSG("alice".into(), "hello".into());
        assert!(!data.apply(&cmd));
    }

    #[test]
    fn full_whois_session() {
        let mut data = WhoisData::new("alice");

        // Simulate a realistic sequence of server replies.
        let commands = vec![
            numeric(
                RPL_WHOISUSER,
                vec!["me", "alice", "alice", "example.com", "*", "Alice"],
            ),
            numeric(
                RPL_WHOISACCOUNT,
                vec!["me", "alice", "alice_acct", "is logged in as"],
            ),
            numeric(
                RPL_WHOISSERVER,
                vec!["me", "alice", "irc.libera.chat", "Stockholm"],
            ),
            numeric(
                RPL_WHOISCHANNELS,
                vec!["me", "alice", "#rust #halloy"],
            ),
            numeric(RPL_WHOISSECURE, vec!["me", "alice", "is using a secure connection"]),
            numeric(
                RPL_WHOISIDLE,
                vec!["me", "alice", "60", "1700000000", "seconds idle"],
            ),
        ];

        for cmd in &commands {
            assert!(data.apply(cmd));
        }

        assert_eq!(data.nickname, "alice");
        assert_eq!(data.username.as_deref(), Some("alice"));
        assert_eq!(data.hostname.as_deref(), Some("example.com"));
        assert_eq!(data.real_name.as_deref(), Some("Alice"));
        assert_eq!(data.account.as_deref(), Some("alice_acct"));
        assert_eq!(data.server.as_deref(), Some("irc.libera.chat"));
        assert_eq!(data.server_info.as_deref(), Some("Stockholm"));
        assert_eq!(data.channels, vec!["#rust", "#halloy"]);
        assert!(data.secure);
        assert_eq!(data.idle_secs, Some(60));
        assert_eq!(data.sign_on_secs, Some(1_700_000_000));
        assert!(data.away_message.is_none());
    }

    // ── WhoisCache ────────────────────────────────────────────────

    fn casemap() -> isupport::CaseMap {
        isupport::CaseMap::default()
    }

    fn nick(s: &str) -> Nick {
        Nick::from_str(s, casemap())
    }

    #[test]
    fn cache_insert_and_get() {
        let mut cache = WhoisCache::default();
        let data = WhoisData::new("alice");
        cache.insert(data.clone(), casemap());

        let result = cache.get(nick("alice").as_nickref(), casemap());
        assert_eq!(result, Some(&data));
    }

    #[test]
    fn cache_case_insensitive_lookup() {
        let mut cache = WhoisCache::default();
        cache.insert(WhoisData::new("Alice"), casemap());

        // RFC 1459 casemapping: alice == ALICE == Alice
        assert!(
            cache
                .get(nick("alice").as_nickref(), casemap())
                .is_some()
        );
        assert!(
            cache
                .get(nick("ALICE").as_nickref(), casemap())
                .is_some()
        );
    }

    #[test]
    fn cache_expired_entry_returns_none() {
        let mut cache = WhoisCache::new(Duration::from_millis(0));
        cache.insert(WhoisData::new("alice"), casemap());

        // Entry is already expired (TTL = 0).
        let result = cache.get(nick("alice").as_nickref(), casemap());
        assert!(result.is_none());
    }

    #[test]
    fn cache_evict_expired() {
        let mut cache = WhoisCache::new(Duration::from_millis(0));
        cache.insert(WhoisData::new("alice"), casemap());
        cache.insert(WhoisData::new("bob"), casemap());
        assert_eq!(cache.len(), 2);

        cache.evict_expired();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn cache_replace_existing() {
        let mut cache = WhoisCache::default();

        let mut data1 = WhoisData::new("alice");
        data1.real_name = Some("Old Name".into());
        cache.insert(data1, casemap());

        let mut data2 = WhoisData::new("alice");
        data2.real_name = Some("New Name".into());
        cache.insert(data2, casemap());

        let result = cache.get(nick("alice").as_nickref(), casemap());
        assert_eq!(
            result.and_then(|d| d.real_name.as_deref()),
            Some("New Name")
        );
    }

    #[test]
    fn cache_different_users() {
        let mut cache = WhoisCache::default();
        cache.insert(WhoisData::new("alice"), casemap());
        cache.insert(WhoisData::new("bob"), casemap());

        assert!(
            cache
                .get(nick("alice").as_nickref(), casemap())
                .is_some()
        );
        assert!(
            cache
                .get(nick("bob").as_nickref(), casemap())
                .is_some()
        );
        assert!(
            cache
                .get(nick("charlie").as_nickref(), casemap())
                .is_none()
        );
    }

    #[test]
    fn cache_is_empty() {
        let cache = WhoisCache::default();
        assert!(cache.is_empty());
    }
}
