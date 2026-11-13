//! In-memory limits for the anonymous public demo; no visitor identifiers are logged.
use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, Ipv6Addr},
    time::{Duration, Instant},
};
const QUESTION_WINDOW: Duration = Duration::from_secs(3600);
const SESSION_WINDOW: Duration = Duration::from_secs(600);
const COOLDOWN: Duration = Duration::from_secs(5);
const MAX_CLIENTS: usize = 4096;

#[derive(Debug, PartialEq)]
pub struct Blocked {
    pub retry_after_seconds: u64,
    pub code: &'static str,
    pub message: &'static str,
}
impl Blocked {
    fn until(code: &'static str, message: &'static str, remaining: Duration) -> Self {
        Self {
            retry_after_seconds: remaining
                .as_secs()
                .saturating_add(u64::from(remaining.subsec_nanos() > 0))
                .max(1),
            code,
            message,
        }
    }
}
#[derive(Default)]
struct Visitor {
    questions: VecDeque<Instant>,
    sessions: VecDeque<Instant>,
    last_question: HashMap<String, Instant>,
}
#[derive(Default)]
pub struct DemoLimits {
    visitors: HashMap<IpAddr, Visitor>,
}
impl DemoLimits {
    fn cleanup(&mut self, now: Instant) {
        self.visitors.retain(|_, visitor| {
            visitor
                .questions
                .retain(|at| now.duration_since(*at) < QUESTION_WINDOW);
            visitor
                .sessions
                .retain(|at| now.duration_since(*at) < SESSION_WINDOW);
            visitor
                .last_question
                .retain(|_, at| now.duration_since(*at) < COOLDOWN);
            !visitor.questions.is_empty() || !visitor.sessions.is_empty()
        });
    }
    fn check_capacity(&mut self, ip: IpAddr, now: Instant) -> Result<(), Blocked> {
        self.cleanup(now);
        if !self.visitors.contains_key(&ip) && self.visitors.len() >= MAX_CLIENTS {
            return Err(Blocked::until(
                "rate_limit",
                "De demo ontvangt veel bezoekers. Probeer het later opnieuw.",
                QUESTION_WINDOW,
            ));
        }
        Ok(())
    }
    pub fn check_run(&mut self, ip: IpAddr, session: &str, now: Instant) -> Result<(), Blocked> {
        self.check_capacity(ip, now)?;
        if let Some(visitor) = self.visitors.get(&ip) {
            if visitor.questions.len() >= 10 {
                let remaining =
                    QUESTION_WINDOW.saturating_sub(now.duration_since(visitor.questions[0]));
                return Err(Blocked::until(
                    "rate_limit",
                    "Je kunt maximaal tien vragen per uur stellen. Wacht even voordat je opnieuw probeert.",
                    remaining,
                ));
            }
            if let Some(at) = visitor.last_question.get(session) {
                return Err(Blocked::until(
                    "session_cooldown",
                    "Wacht vijf seconden tussen vragen.",
                    COOLDOWN.saturating_sub(now.duration_since(*at)),
                ));
            }
        }
        Ok(())
    }
    pub fn record_run(&mut self, ip: IpAddr, session: &str, now: Instant) {
        let visitor = self.visitors.entry(ip).or_default();
        visitor.questions.push_back(now);
        visitor.last_question.insert(session.to_owned(), now);
    }
    pub fn check_session(&mut self, ip: IpAddr, now: Instant) -> Result<(), Blocked> {
        self.check_capacity(ip, now)?;
        if let Some(visitor) = self.visitors.get(&ip)
            && visitor.sessions.len() >= 30
        {
            return Err(Blocked::until(
                "session_limit",
                "Je hebt te vaak opnieuw verbinding gemaakt. Probeer het later opnieuw.",
                SESSION_WINDOW.saturating_sub(now.duration_since(visitor.sessions[0])),
            ));
        }
        Ok(())
    }
    pub fn record_session(&mut self, ip: IpAddr, now: Instant) {
        self.visitors.entry(ip).or_default().sessions.push_back(now);
    }
}
/// Ignore all forwarded identities unless the real TCP peer is the configured proxy.
/// That proxy must overwrite X-Real-IP rather than pass through client input.
pub fn client_ip(
    peer: IpAddr,
    trusted_proxy: Option<IpAddr>,
    real_ip: Option<&str>,
) -> Result<IpAddr, &'static str> {
    let ip = if trusted_proxy == Some(peer) {
        real_ip
            .and_then(|value| value.parse::<IpAddr>().ok())
            .ok_or("Ongeldig bezoekersadres van de proxy.")?
    } else {
        peer
    };
    Ok(match ip {
        IpAddr::V6(address) => match address.to_ipv4_mapped() {
            Some(ipv4) => IpAddr::V4(ipv4),
            None => IpAddr::V6(Ipv6Addr::from((u128::from(address) >> 64) << 64)),
        },
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn ip() -> IpAddr {
        "192.0.2.1".parse().unwrap()
    }
    #[test]
    fn new_sessions_cannot_bypass_ten_questions_per_hour() {
        let mut limits = DemoLimits::default();
        let now = Instant::now();
        for n in 0..10 {
            let session = format!("visitor-{n}");
            assert_eq!(limits.check_run(ip(), &session, now), Ok(()));
            limits.record_run(ip(), &session, now);
        }
        assert!(limits.check_run(ip(), "new-session", now).is_err());
        assert_eq!(
            limits.check_run(ip(), "new-session", now + Duration::from_secs(3600)),
            Ok(())
        );
    }
    #[test]
    fn same_session_has_a_five_second_cooldown_but_another_visitor_can_start() {
        let mut limits = DemoLimits::default();
        let now = Instant::now();
        limits.record_run(ip(), "one", now);
        assert!(
            limits
                .check_run(ip(), "one", now + Duration::from_secs(4))
                .is_err()
        );
        assert_eq!(limits.check_run(ip(), "two", now), Ok(()));
        assert_eq!(
            limits.check_run(ip(), "one", now + Duration::from_secs(5)),
            Ok(())
        );
    }
    #[test]
    fn session_creation_is_limited_independently_of_questions() {
        let mut limits = DemoLimits::default();
        let now = Instant::now();
        for _ in 0..30 {
            assert_eq!(limits.check_session(ip(), now), Ok(()));
            limits.record_session(ip(), now);
        }
        assert!(limits.check_session(ip(), now).is_err());
        assert_eq!(limits.check_run(ip(), "one", now), Ok(()));
        assert_eq!(
            limits.check_session(ip(), now + Duration::from_secs(600)),
            Ok(())
        );
    }
    #[test]
    fn only_the_explicit_proxy_can_supply_one_real_ip() {
        let proxy = "127.0.0.1".parse().unwrap();
        assert_eq!(client_ip(ip(), None, Some("203.0.113.9")), Ok(ip()));
        assert_eq!(client_ip(ip(), Some(proxy), Some("203.0.113.9")), Ok(ip()));
        assert_eq!(
            client_ip(proxy, Some(proxy), Some("203.0.113.9")),
            Ok("203.0.113.9".parse().unwrap())
        );
        assert!(client_ip(proxy, Some(proxy), Some("203.0.113.9, 192.0.2.4")).is_err());
        assert!(client_ip(proxy, Some(proxy), None).is_err());
    }
    #[test]
    fn ipv6_privacy_addresses_and_ipv4_mapped_addresses_share_limits() {
        let one = "2001:db8:1:2::1".parse().unwrap();
        let two = "2001:db8:1:2::beef".parse().unwrap();
        assert_eq!(client_ip(one, None, None), client_ip(two, None, None));
        assert_eq!(
            client_ip("::ffff:192.0.2.1".parse().unwrap(), None, None),
            Ok(ip())
        );
    }
    #[test]
    fn full_memory_does_not_evict_existing_quota_and_expires_after_the_window() {
        let now = Instant::now();
        let mut limits = DemoLimits::default();
        for n in 0..MAX_CLIENTS {
            let address = IpAddr::V4(std::net::Ipv4Addr::from(n as u32));
            limits.record_run(address, "one", now);
        }
        let original = IpAddr::V4(std::net::Ipv4Addr::from(0));
        for _ in 1..10 {
            limits.record_run(original, "one", now);
        }
        assert!(limits.check_run(ip(), "new", now).is_err());
        assert!(limits.check_run(original, "new", now).is_err());
        assert_eq!(
            limits.check_run(ip(), "new", now + Duration::from_secs(3600)),
            Ok(())
        );
    }
}
