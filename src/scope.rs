//! User-managed request boundaries. This is not a browser network sandbox.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Policy {
    pub enabled: bool,
    pub grants: Vec<Grant>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grant {
    origin: String,
    prefix: String,
    level: String,
    account_label: String,
}
fn rank(level: &str) -> Option<u8> {
    match level {
        "read" => Some(0),
        "interact" => Some(1),
        "mutate" => Some(2),
        _ => None,
    }
}
impl Policy {
    pub fn command(&mut self, text: &str) -> Result<Value> {
        let words = text.split_whitespace().collect::<Vec<_>>();
        match words.as_slice() {
            []=>{},
            ["clear"]=>{self.enabled=true;self.grants.clear();},
            ["allow",url,level] | ["allow",url,level,_]=>{
                crate::browser::validate_url(url)?;
                let parsed=reqwest::Url::parse(url)?;
                if parsed.query().is_some() || parsed.fragment().is_some() {bail!("Scope uses an origin/path prefix without query or fragment");}
                rank(level).context("Use read/interact/mutate")?;
                let label=words.get(3).copied().unwrap_or("anonymous");
                if label.len()>80 || self.grants.len()>=32 {bail!("Scope/account limit exceeded");}
                self.enabled=true;
                let grant=Grant {origin:parsed.origin().ascii_serialization(),prefix:parsed.path().trim_end_matches('/').to_owned(),level:(*level).into(),account_label:label.into()};
                self.grants.retain(|g|g.origin!=grant.origin || g.prefix!=grant.prefix);
                self.grants.push(grant);
            }
            _=>bail!("Use /scope, /scope allow URL read|interact|mutate [account-label], or /scope clear. Only user commands change scope."),
        }
        Ok(
            json!({"scope":self,"account_verification":"Labels are user-declared, not automatic authentication identity verification. Reconfigure scope when changing login.","limits":"Enforces requested URLs and observed page location before direct actions. Not a network firewall: redirects, page scripts, links and subresources can contact other origins. Arbitrary commands/MCP are outside this browser scope and still require approval."}),
        )
    }
    pub(crate) fn declared_account(&self, raw: &str, level: &str) -> Option<String> {
        if !self.enabled || self.check(raw, level).is_err() {
            return None;
        }
        let u = reqwest::Url::parse(raw).ok()?;
        let labels = self
            .grants
            .iter()
            .filter(|g| {
                g.origin == u.origin().ascii_serialization()
                    && (u.path() == g.prefix
                        || u.path()
                            .strip_prefix(&g.prefix)
                            .is_some_and(|s| s.starts_with('/')))
                    && rank(&g.level).zip(rank(level)).is_some_and(|(a, b)| a >= b)
            })
            .map(|g| g.account_label.clone())
            .collect::<std::collections::BTreeSet<_>>();
        if labels.len() == 1 {
            labels.into_iter().next()
        } else {
            None
        }
    }

    pub fn check(&self, raw: &str, level: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let parsed = reqwest::Url::parse(raw)?;
        let path = parsed.path();
        // Avoid interpreting ambiguous encoded path separators/dot traversal
        // differently from an application server or reverse proxy.
        let lower = path.to_ascii_lowercase();
        if ["%2f", "%5c", "%2e", "%25"]
            .iter()
            .any(|p| lower.contains(p))
        {
            bail!("Ambiguous encoded path is outside enforceable scope");
        }
        let needed = rank(level).context("Unknown scope operation class")?;
        if self.grants.iter().any(|g| {
            g.origin == parsed.origin().ascii_serialization()
                && (path == g.prefix
                    || path
                        .strip_prefix(&g.prefix)
                        .is_some_and(|rest| rest.starts_with('/')))
                && rank(&g.level).is_some_and(|n| n >= needed)
        }) {
            return Ok(());
        }
        bail!(
            "Scope denied {level} request to {}{}; user must configure /scope explicitly",
            parsed.origin().ascii_serialization(),
            path
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_persist_and_reject_host_path_encoding_and_action_escalation() {
        let mut p = Policy::default();
        p.command("allow https://example.test/app read account-a")
            .unwrap();
        let p: Policy = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert!(p.check("https://example.test/app/page?q=x", "read").is_ok());
        for url in [
            "https://example.test/application",
            "https://example.test/other",
            "https://other.test/app",
            "https://example.test/app/%2fadmin",
            "https://example.test/app/%252e%252e/admin",
        ] {
            assert!(p.check(url, "read").is_err(), "{url}");
        }
        assert!(p.check("https://example.test/app", "mutate").is_err());
        assert!(p.check("https://example.test/app", "interact").is_err());
    }
}
