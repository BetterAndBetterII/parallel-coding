use proptest::prelude::*;

use pc_cli::agent_name::{derive_agent_name_from_branch, is_valid_agent_name, MAX_AGENT_NAME_LEN};

proptest! {
    #[test]
    fn derive_agent_name_output_is_always_valid(branch in any::<String>()) {
        let r = derive_agent_name_from_branch(&branch);
        if let Ok(name) = r {
            prop_assert!(is_valid_agent_name(&name));
            prop_assert!(name != ".");
            prop_assert!(name != "..");
            prop_assert!(name.len() <= MAX_AGENT_NAME_LEN);
        }
    }
}
