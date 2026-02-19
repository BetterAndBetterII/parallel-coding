pub mod agent_name {
    use anyhow::{bail, Result};

    pub const MAX_AGENT_NAME_LEN: usize = 64;

    pub fn is_valid_agent_name(name: &str) -> bool {
        !name.is_empty()
            && name != "."
            && name != ".."
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    }

    pub fn derive_agent_name_from_branch(branch_name: &str) -> Result<String> {
        if is_valid_agent_name(branch_name) {
            if branch_name.len() > MAX_AGENT_NAME_LEN {
                bail!(
                    "Derived agent name is too long (>{MAX_AGENT_NAME_LEN}). Use --agent-name to override."
                );
            }
            return Ok(branch_name.to_string());
        }

        let mut out = String::with_capacity(branch_name.len());
        let mut prev_underscore = false;
        for ch in branch_name.chars() {
            let ok = ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-';
            let mapped = if ok { ch } else { '_' };
            if mapped == '_' {
                if prev_underscore {
                    continue;
                }
                prev_underscore = true;
            } else {
                prev_underscore = false;
            }
            if out.len() >= MAX_AGENT_NAME_LEN {
                bail!(
                    "Derived agent name is too long (>{MAX_AGENT_NAME_LEN}). Use --agent-name to override."
                );
            }
            out.push(mapped);
        }

        let out = out.trim_matches('_').to_string();
        if out.is_empty() || out == "." || out == ".." {
            bail!(
                "Cannot derive a valid agent name from branch name: {branch_name:?}. Use --agent-name."
            );
        }
        Ok(out)
    }
}
