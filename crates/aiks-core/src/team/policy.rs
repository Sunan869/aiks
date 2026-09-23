use super::Action;

/// All inputs must be derived from authenticated server state. A grant is never
/// allowed to override company, membership or directory validity.
pub fn allows(
    action: Action,
    same_company: bool,
    member_active: bool,
    directory_fresh: bool,
    owner: bool,
    directly_shared: bool,
    org_shared: bool,
) -> bool {
    same_company
        && member_active
        && directory_fresh
        && (owner || matches!(action, Action::Read) && (directly_shared || org_shared))
}
