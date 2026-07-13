#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodePolicy {
    pub minimum_major: u64,
    pub accepted_lts_majors: &'static [u64],
    pub preferred_major: u64,
}

impl NodePolicy {
    pub const fn accepts_major(self, major: u64) -> bool {
        major >= self.minimum_major && self.accepted_lts_majors_contains(major)
    }

    const fn accepted_lts_majors_contains(self, major: u64) -> bool {
        let mut index = 0;
        while index < self.accepted_lts_majors.len() {
            if self.accepted_lts_majors[index] == major {
                return true;
            }
            index += 1;
        }
        false
    }
}

pub const fn node_policy() -> NodePolicy {
    NodePolicy {
        minimum_major: 22,
        accepted_lts_majors: &[22, 24],
        preferred_major: 24,
    }
}

#[cfg(test)]
mod tests {
    use super::node_policy;

    #[test]
    fn node_policy_accepts_supported_lts_and_rejects_old_node() {
        let policy = node_policy();
        assert!(policy.accepts_major(24));
        assert!(policy.accepts_major(22));
        assert!(!policy.accepts_major(18));
        assert_eq!(policy.preferred_major, 24);
    }
}
