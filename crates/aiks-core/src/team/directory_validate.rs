use std::collections::{HashMap, HashSet};

use super::{DirectorySnapshot, TeamError};

pub(super) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn display_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

/// Returns external ancestor/descendant pairs, including the reflexive pair.
/// Each branch has a hard depth budget; malformed input cannot recurse forever.
pub(super) fn validate(
    input: &DirectorySnapshot,
    now: u64,
) -> Result<Vec<(String, String)>, TeamError> {
    if !input.complete
        || input.scope.is_empty()
        || input.scope.len() > 100
        || input.orgs.is_empty()
        || input.orgs.len() > 10_000
        || input.users.len() > 100_000
        || input.memberships.len() > 1_000_000
        || input.observed_at > now
        || now > i64::MAX as u64
        || now - input.observed_at > 3600
    {
        return Err(TeamError::InvalidInput);
    }
    let mut scope = HashSet::new();
    for root in &input.scope {
        if !identifier(root) || !scope.insert(root.as_str()) {
            return Err(TeamError::InvalidInput);
        }
    }
    let mut orgs = HashMap::new();
    for org in &input.orgs {
        if !identifier(&org.id)
            || !display_text(&org.name)
            || orgs.insert(org.id.as_str(), org).is_some()
        {
            return Err(TeamError::InvalidInput);
        }
    }
    for root in &scope {
        if !orgs.get(root).is_some_and(|org| org.parent_id.is_none()) {
            return Err(TeamError::InvalidInput);
        }
    }
    let mut closure = Vec::new();
    for org in &input.orgs {
        let mut current = org.id.as_str();
        let mut visited = HashSet::new();
        loop {
            if visited.len() >= 128 || !visited.insert(current) {
                return Err(TeamError::InvalidInput);
            }
            let entry = orgs.get(current).ok_or(TeamError::InvalidInput)?;
            closure.push((current.to_owned(), org.id.clone()));
            match &entry.parent_id {
                Some(parent) => current = parent,
                None if scope.contains(current) => break,
                None => return Err(TeamError::InvalidInput),
            }
        }
    }
    let mut users = HashMap::new();
    let mut unions = HashSet::new();
    for user in &input.users {
        if !identifier(&user.external_user_id)
            || !identifier(&user.union_id)
            || !display_text(&user.display_name)
            || users.insert(user.external_user_id.as_str(), user).is_some()
            || !unions.insert(user.union_id.as_str())
        {
            return Err(TeamError::InvalidInput);
        }
    }
    let mut pairs = HashSet::new();
    let mut assigned = HashSet::new();
    for membership in &input.memberships {
        if !users.contains_key(membership.user_id.as_str())
            || !orgs.contains_key(membership.org_id.as_str())
            || !pairs.insert((membership.user_id.as_str(), membership.org_id.as_str()))
        {
            return Err(TeamError::InvalidInput);
        }
        assigned.insert(membership.user_id.as_str());
    }
    if input
        .users
        .iter()
        .any(|user| user.active && !assigned.contains(user.external_user_id.as_str()))
    {
        return Err(TeamError::InvalidInput);
    }
    Ok(closure)
}
