use super::{member, text, DingTalkClient};
use aiks_core::team::{DirectorySnapshot, DirectoryUser, Membership, OrgRecord, TeamError};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

impl DingTalkClient {
    pub async fn directory(&self, scope: &[String]) -> Result<DirectorySnapshot, TeamError> {
        let mut roots = HashSet::new();
        if scope.is_empty()
            || scope.len() > 100
            || scope.iter().any(|id| {
                !id.parse::<i64>()
                    .is_ok_and(|n| n > 0 && n.to_string() == *id)
                    || !roots.insert(id.clone())
            })
        {
            return Err(TeamError::InvalidInput);
        }
        tokio::time::timeout(Duration::from_secs(120), self.directory_inner(scope))
            .await
            .map_err(|_| TeamError::DirectoryUnavailable)?
    }

    async fn directory_inner(&self, scope: &[String]) -> Result<DirectorySnapshot, TeamError> {
        let observed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| TeamError::Unavailable)?
            .as_secs();
        let app_token = self.application_token().await?;
        let token = app_token.as_str();
        let mut queue = VecDeque::new();
        let mut discovered = HashSet::new();
        let mut orgs = Vec::new();
        for root in scope {
            let id = root.parse::<i64>().map_err(|_| TeamError::InvalidInput)?;
            let value = self
                .legacy(
                    "/topapi/v2/department/get",
                    token,
                    json!({"dept_id":id,"language":"zh_CN"}),
                )
                .await?;
            if department_id(&value, "dept_id")? != id {
                return Err(TeamError::DirectoryUnavailable);
            }
            orgs.push(OrgRecord {
                id: root.clone(),
                parent_id: None,
                name: text(&value, "name", 4096)?,
            });
            queue.push_back((id, 0));
            discovered.insert(id);
        }
        let mut users: HashMap<String, DirectoryUser> = HashMap::new();
        let mut pairs = HashSet::new();
        while let Some((dept, depth)) = queue.pop_front() {
            let children = self
                .legacy(
                    "/topapi/v2/department/listsub",
                    token,
                    json!({"dept_id":dept,"language":"zh_CN"}),
                )
                .await?;
            let children = children
                .as_array()
                .filter(|items| items.len() <= 10_000)
                .ok_or(TeamError::DirectoryUnavailable)?;
            for child in children {
                let id = department_id(child, "dept_id")?;
                if department_id(child, "parent_id")? != dept
                    || depth >= 127
                    || !discovered.insert(id)
                    || discovered.len() > 10_000
                {
                    // Overlapping configured roots are also rejected explicitly;
                    // silently cutting that edge would alter descendant grants.
                    return Err(TeamError::DirectoryUnavailable);
                }
                orgs.push(OrgRecord {
                    id: id.to_string(),
                    parent_id: Some(dept.to_string()),
                    name: text(child, "name", 4096)?,
                });
                queue.push_back((id, depth + 1));
            }
            let mut cursor = 0_u64;
            let mut cursors = HashSet::new();
            loop {
                if !cursors.insert(cursor) || cursors.len() > 100_000 {
                    return Err(TeamError::DirectoryUnavailable);
                }
                let page=self.legacy("/topapi/v2/user/list",token,json!({"dept_id":dept,"cursor":cursor,"size":100,"language":"zh_CN","contain_access_limit":false})).await?;
                let list = page
                    .get("list")
                    .and_then(Value::as_array)
                    .filter(|v| v.len() <= 100)
                    .ok_or(TeamError::DirectoryUnavailable)?;
                let more = page
                    .get("has_more")
                    .and_then(Value::as_bool)
                    .ok_or(TeamError::DirectoryUnavailable)?;
                if more && list.is_empty() {
                    return Err(TeamError::DirectoryUnavailable);
                }
                for entry in list {
                    let id = text(entry, "userid", 512)?;
                    let user = if entry.get("active").is_some() && entry.get("unionid").is_some() {
                        member(entry)?
                    } else {
                        // Missing active/union fields never mean an active employee.
                        self.read_member(token, &id).await?
                    };
                    if let Some(previous) = users.get(&id) {
                        if previous.union_id != user.union_id
                            || previous.active != user.active
                            || previous.display_name != user.display_name
                        {
                            return Err(TeamError::DirectoryUnavailable);
                        }
                    } else {
                        users.insert(id.clone(), user);
                    }
                    pairs.insert((id, dept.to_string()));
                    if users.len() > 100_000 || pairs.len() > 1_000_000 {
                        return Err(TeamError::DirectoryUnavailable);
                    }
                }
                if !more {
                    break;
                }
                cursor = page
                    .get("next_cursor")
                    .and_then(Value::as_u64)
                    .ok_or(TeamError::DirectoryUnavailable)?;
            }
        }
        let mut users: Vec<_> = users.into_values().collect();
        users.sort_by(|a, b| a.external_user_id.cmp(&b.external_user_id));
        let mut memberships: Vec<_> = pairs
            .into_iter()
            .map(|(user_id, org_id)| Membership { user_id, org_id })
            .collect();
        memberships.sort_by(|a, b| (&a.user_id, &a.org_id).cmp(&(&b.user_id, &b.org_id)));
        Ok(DirectorySnapshot {
            complete: true,
            scope: scope.to_vec(),
            users,
            orgs,
            memberships,
            observed_at,
        })
    }
}
fn department_id(value: &Value, field: &str) -> Result<i64, TeamError> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .ok_or(TeamError::DirectoryUnavailable)
}
