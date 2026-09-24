//! Trusted scope is internal, never accepted from a search request.
use super::{SearchCorpus, UnifiedSearchFilter};
use crate::service::RequestContext;
use rusqlite::types::Value;

#[derive(Clone)]
pub(super) struct ScopedFilter {
    pub filter: UnifiedSearchFilter,
    pub context: Option<RequestContext>,
    pub auth_now: u64,
}

pub(super) struct ScopeSql {
    pub clause: String,
    pub values: Vec<Value>,
}

impl std::ops::Deref for ScopedFilter {
    type Target = UnifiedSearchFilter;
    fn deref(&self) -> &Self::Target {
        &self.filter
    }
}

impl ScopedFilter {
    /// Build a pre-LIMIT visibility predicate with bound identity values. Team
    /// predicates also revalidate the opaque session and current directory in
    /// the same SQL statement that selects recall candidates.
    pub fn predicate(&self, corpus: SearchCorpus, first: usize) -> anyhow::Result<ScopeSql> {
        let Some(ctx) = &self.context else {
            return Ok(ScopeSql {
                clause: "1=1".into(),
                values: Vec::new(),
            });
        };
        if let Some(team) = ctx.team() {
            let now = i64::try_from(self.auth_now)?;
            let max_age = i64::try_from(team.directory_max_age())?;
            let company = first;
            let instance = first + 1;
            let session = first + 2;
            let user = first + 3;
            let space = first + 4;
            let access = first + 5;
            let at = first + 6;
            let age = first + 7;
            let auth = format!(
                "EXISTS(SELECT 1 FROM team_company c
                   JOIN team_auth_session a ON a.company_id=c.id
                   JOIN team_user u ON u.company_id=c.id AND u.id=a.user_id
                   JOIN team_user_state us ON us.company_id=c.id AND us.user_id=u.id
                   JOIN team_org_snapshot os ON os.company_id=c.id AND os.generation=c.directory_generation
                   WHERE c.singleton=1 AND c.id=?{company} AND c.instance_id=?{instance}
                     AND a.id=?{session} AND a.user_id=?{user} AND a.space_id=?{space}
                     AND a.access_hash=?{access} AND a.revoked_at IS NULL
                     AND a.issued_at<=?{at} AND a.access_expires_at>?{at}
                     AND u.active=1 AND u.private_space_id=a.space_id
                     AND us.auth_version=a.auth_version AND us.last_generation=c.directory_generation
                     AND ?{at}>=os.observed_at AND (?{at}-os.observed_at)<=?{age})"
            );
            let visibility = match corpus {
                SearchCorpus::Session => format!(
                    "EXISTS(SELECT 1 FROM service_session_binding b
                       JOIN service_derived_state d ON d.session_id=b.session_id
                       WHERE b.session_id=ss.id AND b.principal_id=?{user} AND b.space_id=?{space}
                         AND d.indexed_revision=b.current_revision)"
                ),
                SearchCorpus::Knowledge => format!(
                    "EXISTS(SELECT 1 FROM team_knowledge_owner o
                       WHERE o.company_id=?{company} AND o.knowledge_id=ki.id AND (
                           o.owner_user_id=?{user} OR EXISTS(
                               SELECT 1 FROM document_share_grant g
                               WHERE g.company_id=o.company_id AND g.knowledge_id=o.knowledge_id
                                 AND g.target_user_id=?{user}
                           ) OR EXISTS(
                               SELECT 1 FROM document_share_grant g
                               JOIN team_company gc ON gc.id=g.company_id
                               JOIN team_org_membership m ON m.company_id=g.company_id
                                    AND m.generation=gc.directory_generation AND m.user_id=?{user}
                               WHERE g.company_id=o.company_id AND g.knowledge_id=o.knowledge_id
                                 AND g.target_org_id IS NOT NULL
                                 AND (m.org_id=g.target_org_id OR (g.include_descendants=1 AND EXISTS(
                                     SELECT 1 FROM team_org_closure oc
                                     WHERE oc.company_id=m.company_id AND oc.generation=m.generation
                                       AND oc.ancestor_id=g.target_org_id AND oc.descendant_id=m.org_id)))
                           )
                       )) AND EXISTS(
                           SELECT 1 FROM service_session_binding b
                           JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id
                           WHERE b.session_id=ki.source_session_id AND d.revision=b.current_revision
                       )"
                ),
            };
            return Ok(ScopeSql {
                clause: format!("({auth}) AND ({visibility})"),
                values: vec![
                    Value::Text(team.company_id().to_owned()),
                    Value::Text(team.instance_id().to_owned()),
                    Value::Text(team.session_id().to_owned()),
                    Value::Text(team.user_id().to_owned()),
                    Value::Text(team.space_id().to_owned()),
                    Value::Text(team.access_hash().to_owned()),
                    Value::Integer(now),
                    Value::Integer(max_age),
                ],
            });
        }

        let principal = first;
        let space = first + 1;
        let clause = match corpus {
            SearchCorpus::Session => format!(
                "EXISTS(SELECT 1 FROM service_session_binding b
                   JOIN service_derived_state d ON d.session_id=b.session_id
                   WHERE b.session_id=ss.id AND b.principal_id=?{principal} AND b.space_id=?{space}
                     AND d.indexed_revision=b.current_revision)"
            ),
            SearchCorpus::Knowledge => format!(
                "EXISTS(SELECT 1 FROM service_session_binding b
                   JOIN service_knowledge_revision d ON d.session_id=b.session_id AND d.knowledge_id=ki.id
                   WHERE b.session_id=ki.source_session_id AND b.principal_id=?{principal}
                     AND b.space_id=?{space} AND d.revision=b.current_revision)"
            ),
        };
        Ok(ScopeSql {
            clause,
            values: vec![
                Value::Text(ctx.principal_id().to_owned()),
                Value::Text(ctx.space_id().to_owned()),
            ],
        })
    }
}
