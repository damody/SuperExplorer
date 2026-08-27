//! Host-internal, use-time authorization envelope shared by extension adapters.

use std::{collections::BTreeMap, fmt, sync::Mutex};

use ring::{
    hmac,
    rand::{SecureRandom as _, SystemRandom},
};
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AuthorityClaimsV1 {
    pub package_id: String,
    pub feature_id: String,
    pub interface_id: String,
    pub incarnation: u64,
    pub capability: String,
    pub authorized_root_sha256: String,
    pub location_generation: u64,
    pub item_generation: u64,
    pub refresh_generation: u64,
    pub container_generation: u64,
    pub job_generation: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct AuthorityEnvelopeV1 {
    claims: AuthorityClaimsV1,
    tag: [u8; 32],
}

impl AuthorityEnvelopeV1 {
    pub(crate) const fn location_generation(&self) -> u64 {
        self.claims.location_generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthorityAdapterV1 {
    Stream,
    Tool,
    LockOwner,
    Navigation,
    OperationPlan,
    VirtualLocation,
    Renderer,
    Lua,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthorityErrorV1 {
    Invalid,
    Tampered,
    Revoked,
    Stale,
    CapabilityDenied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CurrentAuthorityV1 {
    claims: AuthorityClaimsV1,
    revoked: bool,
}

pub(crate) struct RuntimeAuthorityV1 {
    key: hmac::Key,
    current: Mutex<BTreeMap<(String, String, String, String), CurrentAuthorityV1>>,
}

impl fmt::Debug for RuntimeAuthorityV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeAuthorityV1")
            .field("key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl RuntimeAuthorityV1 {
    pub(crate) fn new() -> Result<Self, AuthorityErrorV1> {
        let mut secret = [0_u8; 32];
        SystemRandom::new()
            .fill(&mut secret)
            .map_err(|_| AuthorityErrorV1::Invalid)?;
        Ok(Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, &secret),
            current: Mutex::new(BTreeMap::new()),
        })
    }

    pub(crate) fn issue(
        &self,
        claims: AuthorityClaimsV1,
    ) -> Result<AuthorityEnvelopeV1, AuthorityErrorV1> {
        validate_claims(&claims)?;
        let tag = self.sign(&claims)?;
        let identity = identity(&claims);
        self.current
            .lock()
            .map_err(|_| AuthorityErrorV1::Revoked)?
            .insert(
                identity,
                CurrentAuthorityV1 {
                    claims: claims.clone(),
                    revoked: false,
                },
            );
        Ok(AuthorityEnvelopeV1 { claims, tag })
    }

    pub(crate) fn revoke_feature(
        &self,
        package: &str,
        feature: &str,
    ) -> Result<usize, AuthorityErrorV1> {
        let mut current = self.current.lock().map_err(|_| AuthorityErrorV1::Revoked)?;
        let mut revoked = 0;
        for ((entry_package, entry_feature, _, _), entry) in current.iter_mut() {
            if entry_package == package && entry_feature == feature && !entry.revoked {
                entry.revoked = true;
                revoked += 1;
            }
        }
        Ok(revoked)
    }

    pub(crate) fn revoke_feature_incarnation(
        &self,
        package: &str,
        feature: &str,
        incarnation: u64,
    ) -> Result<usize, AuthorityErrorV1> {
        let mut current = self.current.lock().map_err(|_| AuthorityErrorV1::Revoked)?;
        let mut revoked = 0;
        for ((entry_package, entry_feature, _, _), entry) in current.iter_mut() {
            if entry_package == package
                && entry_feature == feature
                && entry.claims.incarnation == incarnation
                && !entry.revoked
            {
                entry.revoked = true;
                revoked += 1;
            }
        }
        Ok(revoked)
    }

    pub(crate) fn revalidate<'a>(
        &self,
        envelope: &'a AuthorityEnvelopeV1,
        adapter: AuthorityAdapterV1,
    ) -> Result<&'a AuthorityClaimsV1, AuthorityErrorV1> {
        validate_claims(&envelope.claims)?;
        let expected = self.sign(&envelope.claims)?;
        if !constant_time_eq(&expected, &envelope.tag) {
            return Err(AuthorityErrorV1::Tampered);
        }
        if !adapter_accepts(adapter, &envelope.claims.capability) {
            return Err(AuthorityErrorV1::CapabilityDenied);
        }
        let current = self.current.lock().map_err(|_| AuthorityErrorV1::Revoked)?;
        let saved = current
            .get(&identity(&envelope.claims))
            .ok_or(AuthorityErrorV1::Revoked)?;
        if saved.revoked {
            return Err(AuthorityErrorV1::Revoked);
        }
        if saved.claims != envelope.claims {
            return Err(AuthorityErrorV1::Stale);
        }
        Ok(&envelope.claims)
    }

    fn sign(&self, claims: &AuthorityClaimsV1) -> Result<[u8; 32], AuthorityErrorV1> {
        let bytes = serde_json::to_vec(claims).map_err(|_| AuthorityErrorV1::Invalid)?;
        let mut output = [0_u8; 32];
        output.copy_from_slice(hmac::sign(&self.key, &bytes).as_ref());
        Ok(output)
    }
}

fn identity(claims: &AuthorityClaimsV1) -> (String, String, String, String) {
    (
        claims.package_id.clone(),
        claims.feature_id.clone(),
        claims.interface_id.clone(),
        claims.capability.clone(),
    )
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}
fn validate_claims(claims: &AuthorityClaimsV1) -> Result<(), AuthorityErrorV1> {
    if !valid_id(&claims.package_id)
        || !valid_id(&claims.feature_id)
        || !valid_id(&claims.interface_id)
        || !valid_id(&claims.capability)
        || claims.incarnation == 0
        || claims.authorized_root_sha256.len() != 64
        || !claims
            .authorized_root_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err(AuthorityErrorV1::Invalid);
    }
    Ok(())
}
fn adapter_accepts(adapter: AuthorityAdapterV1, capability: &str) -> bool {
    matches!(
        (adapter, capability),
        (AuthorityAdapterV1::Stream, "filesystem.read")
            | (AuthorityAdapterV1::Tool, "tools.execute_bundled")
            | (AuthorityAdapterV1::LockOwner, "lock_owner.query")
            | (AuthorityAdapterV1::Navigation, "navigation.request")
            | (AuthorityAdapterV1::OperationPlan, "operations.submit")
            | (AuthorityAdapterV1::VirtualLocation, "virtual_folder.read")
            | (AuthorityAdapterV1::Renderer, "gpui.render")
            | (AuthorityAdapterV1::Lua, "column.read")
            | (AuthorityAdapterV1::Lua, "filesystem.read")
            | (AuthorityAdapterV1::Lua, "filesystem.write")
            | (AuthorityAdapterV1::Lua, "commands.invoke")
            | (AuthorityAdapterV1::Lua, "forms.submit")
            | (AuthorityAdapterV1::Lua, "operations.submit")
            | (AuthorityAdapterV1::Lua, "tools.execute_bundled")
    )
}
fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn claims(capability: &str) -> AuthorityClaimsV1 {
        AuthorityClaimsV1 {
            package_id: "package".into(),
            feature_id: "feature".into(),
            interface_id: "interface".into(),
            incarnation: 1,
            capability: capability.into(),
            authorized_root_sha256: "a".repeat(64),
            location_generation: 1,
            item_generation: 2,
            refresh_generation: 3,
            container_generation: 4,
            job_generation: 5,
        }
    }
    #[test]
    fn every_adapter_consumes_only_its_capability_bound_envelope() {
        for (adapter, cap) in [
            (AuthorityAdapterV1::Stream, "filesystem.read"),
            (AuthorityAdapterV1::Tool, "tools.execute_bundled"),
            (AuthorityAdapterV1::LockOwner, "lock_owner.query"),
            (AuthorityAdapterV1::Navigation, "navigation.request"),
            (AuthorityAdapterV1::OperationPlan, "operations.submit"),
            (AuthorityAdapterV1::VirtualLocation, "virtual_folder.read"),
            (AuthorityAdapterV1::Renderer, "gpui.render"),
        ] {
            let authority = RuntimeAuthorityV1::new().unwrap();
            let envelope = authority.issue(claims(cap)).unwrap();
            assert!(authority.revalidate(&envelope, adapter).is_ok());
            assert_eq!(
                authority.revalidate(&envelope, AuthorityAdapterV1::Tool),
                if adapter == AuthorityAdapterV1::Tool {
                    Ok(&envelope.claims)
                } else {
                    Err(AuthorityErrorV1::CapabilityDenied)
                }
            );
        }
    }
    #[test]
    fn disable_and_package_update_revoke_or_stale_existing_grants() {
        let authority = RuntimeAuthorityV1::new().unwrap();
        let envelope = authority.issue(claims("filesystem.read")).unwrap();
        assert_eq!(authority.revoke_feature("package", "feature"), Ok(1));
        assert_eq!(
            authority.revalidate(&envelope, AuthorityAdapterV1::Stream),
            Err(AuthorityErrorV1::Revoked)
        );
        let mut next = claims("filesystem.read");
        next.incarnation = 2;
        authority.issue(next).unwrap();
        assert_eq!(
            authority.revalidate(&envelope, AuthorityAdapterV1::Stream),
            Err(AuthorityErrorV1::Stale)
        );
    }

    #[test]
    fn feature_revoke_closes_every_interface_grant_without_touching_siblings() {
        let authority = RuntimeAuthorityV1::new().unwrap();
        let first = authority.issue(claims("filesystem.read")).unwrap();
        let mut second_claims = claims("gpui.render");
        second_claims.interface_id = "renderer".into();
        let second = authority.issue(second_claims).unwrap();
        let mut sibling_claims = claims("filesystem.read");
        sibling_claims.feature_id = "sibling".into();
        sibling_claims.interface_id = "sibling-stream".into();
        let sibling = authority.issue(sibling_claims).unwrap();

        assert_eq!(authority.revoke_feature("package", "feature"), Ok(2));
        assert_eq!(
            authority.revalidate(&first, AuthorityAdapterV1::Stream),
            Err(AuthorityErrorV1::Revoked)
        );
        assert_eq!(
            authority.revalidate(&second, AuthorityAdapterV1::Renderer),
            Err(AuthorityErrorV1::Revoked)
        );
        assert!(
            authority
                .revalidate(&sibling, AuthorityAdapterV1::Stream)
                .is_ok()
        );
    }
    #[test]
    fn identity_race_and_every_tampered_field_fail_before_use() {
        let authority = RuntimeAuthorityV1::new().unwrap();
        let original = authority.issue(claims("filesystem.read")).unwrap();
        for mutate in 0..11 {
            let mut bad = original.clone();
            match mutate {
                0 => bad.claims.package_id = "other".into(),
                1 => bad.claims.feature_id = "other".into(),
                2 => bad.claims.interface_id = "other".into(),
                3 => bad.claims.incarnation += 1,
                4 => bad.claims.capability = "gpui.render".into(),
                5 => bad.claims.authorized_root_sha256 = "b".repeat(64),
                6 => bad.claims.location_generation += 1,
                7 => bad.claims.item_generation += 1,
                8 => bad.claims.refresh_generation += 1,
                9 => bad.claims.container_generation += 1,
                _ => bad.claims.job_generation += 1,
            };
            assert!(matches!(
                authority.revalidate(&bad, AuthorityAdapterV1::Stream),
                Err(AuthorityErrorV1::Tampered) | Err(AuthorityErrorV1::CapabilityDenied)
            ));
        }
        let mut replaced = claims("filesystem.read");
        replaced.item_generation += 1;
        authority.issue(replaced).unwrap();
        assert_eq!(
            authority.revalidate(&original, AuthorityAdapterV1::Stream),
            Err(AuthorityErrorV1::Stale)
        );
    }
}
