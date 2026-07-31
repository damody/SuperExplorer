//! Apartment-neutral contracts for Windows locked-delete recovery.

use explorer_common::ExplorerError;

use crate::LocationDescriptor;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteLockKind {
    SharingViolation,
    LockViolation,
}

impl DeleteLockKind {
    /// Classifies raw Win32 codes and `HRESULT_FROM_WIN32` values without localized text parsing.
    pub const fn from_native_code(code: i32) -> Option<Self> {
        let bits = u32::from_ne_bytes(code.to_ne_bytes());
        if matches!(bits, 0x8027_0027 | 0x8027_0028) {
            return Some(Self::SharingViolation);
        }
        let win32 = if bits & 0xffff_0000 == 0x8007_0000 {
            bits & 0xffff
        } else {
            bits
        };
        match win32 {
            32 => Some(Self::SharingViolation),
            33 => Some(Self::LockViolation),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LockOwnerIdentity {
    pub process_id: u32,
    /// Windows `FILETIME` ticks from the process creation identity.
    pub creation_time_100ns: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockOwnerApplicationType {
    Unknown,
    MainWindow,
    OtherWindow,
    Service,
    Explorer,
    Console,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockOwnerEligibility {
    Eligible,
    ThisApplication,
    System,
    Critical,
    Service,
    Protected,
    Elevated,
    IdentityUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockOwner {
    pub identity: LockOwnerIdentity,
    pub display_name: String,
    pub application_type: LockOwnerApplicationType,
    pub restartable: bool,
    pub eligibility: LockOwnerEligibility,
}

impl LockOwner {
    pub const fn can_close(&self) -> bool {
        matches!(self.eligibility, LockOwnerEligibility::Eligible)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockOwnerDiscoveryRequest {
    pub resources: Vec<LocationDescriptor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockOwnerCloseResult {
    Closed,
    AlreadyExited,
    StaleIdentity,
    Denied,
    Protected,
    Refused,
    Timeout,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockOwnerCloseOutcome {
    pub identity: LockOwnerIdentity,
    pub result: LockOwnerCloseResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockOwnerCloseRequest {
    pub resources: Vec<LocationDescriptor>,
    pub owners: Vec<LockOwnerIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LockOwnerDiscoveryTerminal {
    Ready(Vec<LockOwner>),
    Empty,
    Cancelled,
    Unavailable(ExplorerError),
    Failed(ExplorerError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LockOwnerCloseTerminal {
    Closed(Vec<LockOwnerCloseOutcome>),
    Partial(Vec<LockOwnerCloseOutcome>),
    Cancelled,
    Failed(ExplorerError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_sharing_and_lock_codes_are_recoverable_locks() {
        assert_eq!(
            DeleteLockKind::from_native_code(32),
            Some(DeleteLockKind::SharingViolation)
        );
        assert_eq!(
            DeleteLockKind::from_native_code(i32::from_ne_bytes(0x8007_0021_u32.to_ne_bytes())),
            Some(DeleteLockKind::LockViolation)
        );
        assert_eq!(
            DeleteLockKind::from_native_code(i32::from_ne_bytes(0x8027_0027_u32.to_ne_bytes())),
            Some(DeleteLockKind::SharingViolation)
        );
        assert_eq!(
            DeleteLockKind::from_native_code(i32::from_ne_bytes(0x8027_0028_u32.to_ne_bytes())),
            Some(DeleteLockKind::SharingViolation)
        );
        assert_eq!(DeleteLockKind::from_native_code(5), None);
        assert_eq!(
            DeleteLockKind::from_native_code(i32::from_ne_bytes(0x8007_0005_u32.to_ne_bytes())),
            None
        );
    }

    #[test]
    fn only_explicitly_eligible_owner_can_close() {
        let identity = LockOwnerIdentity {
            process_id: 42,
            creation_time_100ns: 99,
        };
        for (eligibility, expected) in [
            (LockOwnerEligibility::Eligible, true),
            (LockOwnerEligibility::ThisApplication, false),
            (LockOwnerEligibility::System, false),
            (LockOwnerEligibility::Critical, false),
            (LockOwnerEligibility::Service, false),
            (LockOwnerEligibility::Protected, false),
            (LockOwnerEligibility::Elevated, false),
            (LockOwnerEligibility::IdentityUnavailable, false),
        ] {
            assert_eq!(
                LockOwner {
                    identity,
                    display_name: "owner".to_owned(),
                    application_type: LockOwnerApplicationType::Unknown,
                    restartable: false,
                    eligibility,
                }
                .can_close(),
                expected
            );
        }
    }
}
