use aiks_core::team::{policy::allows, Action};

#[test]
fn owner_is_the_only_writer_and_reader_grants_form_a_union() {
    for action in [
        Action::Read,
        Action::Edit,
        Action::ManageShares,
        Action::Archive,
    ] {
        assert!(allows(action, true, true, true, true, false, false));
        for (person, department) in [(false, false), (false, true), (true, false), (true, true)] {
            let expected = matches!(action, Action::Read) && (person || department);
            assert_eq!(
                allows(action, true, true, true, false, person, department),
                expected
            );
        }
    }
}

#[test]
fn company_membership_and_directory_freshness_are_required_even_for_owners() {
    for same_company in [false, true] {
        for active in [false, true] {
            for fresh in [false, true] {
                if same_company && active && fresh {
                    continue;
                }
                for action in [
                    Action::Read,
                    Action::Edit,
                    Action::ManageShares,
                    Action::Archive,
                ] {
                    assert!(!allows(
                        action,
                        same_company,
                        active,
                        fresh,
                        true,
                        true,
                        true
                    ));
                }
            }
        }
    }
}
