//! Trusted scope is internal, never accepted from a search request.
use super::{SearchCorpus, UnifiedSearchFilter};
use crate::service::LocalContext;

#[derive(Clone)]
pub(super) struct ScopedFilter {
    pub filter: UnifiedSearchFilter,
    pub context: Option<LocalContext>,
}

impl std::ops::Deref for ScopedFilter {
    type Target = UnifiedSearchFilter;
    fn deref(&self) -> &Self::Target { &self.filter }
}

impl ScopedFilter {
    /// Only persistent server-issued identity is quoted here. Query, project and
    /// source inputs remain bound parameters in every existing search path.
    pub fn predicate(&self, corpus: SearchCorpus) -> String {
        let Some(ctx) = &self.context else { return "1=1".into(); };
        let revision = match corpus {
            SearchCorpus::Session => "d.indexed_revision",
            SearchCorpus::Knowledge => "d.knowledge_revision",
        };
        format!(
            "EXISTS (SELECT 1 FROM service_session_binding b
             JOIN service_derived_state d ON d.session_id=b.session_id
             WHERE b.session_id=ss.id AND b.principal_id='{}' AND b.space_id='{}'
               AND {revision}=b.current_revision)",
            ctx.principal_id().replace('\'', "''"),
            ctx.space_id().replace('\'', "''"),
        )
    }
}
