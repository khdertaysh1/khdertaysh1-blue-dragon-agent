use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step {
    pub title: String,
    pub status: String,
}
impl Plan {
    pub fn replace(&mut self, steps: Vec<Step>) -> Result<()> {
        if steps.len() > 20
            || steps.iter().any(|s| {
                s.title.trim().is_empty()
                    || s.title.len() > 500
                    || !matches!(s.status.as_str(), "pending" | "in_progress" | "completed")
            })
        {
            bail!(
                "Invalid plan: at most 20 titled steps with pending/in_progress/completed status"
            );
        }
        if steps.iter().filter(|s| s.status == "in_progress").count() > 1 {
            bail!("Only one plan step can be in progress");
        }
        self.steps = steps;
        Ok(())
    }
    pub fn display(&self) -> String {
        if self.steps.is_empty() {
            return "No task plan yet.".into();
        }
        self.steps
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{} [{}] {}", i + 1, s.status, s.title))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ambiguous_progress_and_keeps_old_plan() {
        let mut p = Plan::default();
        p.replace(vec![Step {
            title: "Inspect".into(),
            status: "pending".into(),
        }])
        .unwrap();
        assert!(p
            .replace(vec![
                Step {
                    title: "A".into(),
                    status: "in_progress".into()
                },
                Step {
                    title: "B".into(),
                    status: "in_progress".into()
                }
            ])
            .is_err());
        assert_eq!(p.steps[0].title, "Inspect");
    }
}
