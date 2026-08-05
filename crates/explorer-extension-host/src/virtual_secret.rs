//! One-shot host-to-plugin secret transport for encrypted virtual containers.

use std::{
    hint::black_box,
    sync::{Arc, Mutex},
};

use abi_stable::std_types::RVec;
use explorer_extension_api::{
    AbiVirtualSecretServicesV1, MAX_VIRTUAL_SECRET_UTF16_V1, VirtualSecretMaterialV1,
    VirtualSecretStatusV1, VirtualSecretV1,
};

fn wipe(secret: &mut [u16]) {
    secret.fill(0);
    black_box(secret);
}

#[derive(Clone)]
struct OneShotSecretServicesV1 {
    value: Arc<Mutex<Option<Vec<u16>>>>,
}

impl AbiVirtualSecretServicesV1 for OneShotSecretServicesV1 {
    fn take(&self) -> VirtualSecretMaterialV1 {
        let Ok(mut value) = self.value.lock() else {
            return material(VirtualSecretStatusV1::INVALID, RVec::new());
        };
        let Some(mut secret) = value.take() else {
            return material(VirtualSecretStatusV1::CONSUMED, RVec::new());
        };
        let transported = RVec::from(secret.clone());
        wipe(&mut secret);
        material(VirtualSecretStatusV1::READY, transported)
    }
}

impl Drop for OneShotSecretServicesV1 {
    fn drop(&mut self) {
        if Arc::strong_count(&self.value) != 1 {
            return;
        }
        if let Ok(mut value) = self.value.lock()
            && let Some(secret) = value.as_mut()
        {
            wipe(secret);
        }
    }
}

fn material(status: VirtualSecretStatusV1, utf16: RVec<u16>) -> VirtualSecretMaterialV1 {
    VirtualSecretMaterialV1 {
        status,
        reserved: 0,
        utf16,
    }
}

#[must_use]
pub fn mint_virtual_secret_v1(utf16: Vec<u16>) -> Option<VirtualSecretV1> {
    if utf16.is_empty() || utf16.len() > MAX_VIRTUAL_SECRET_UTF16_V1 {
        return None;
    }
    Some(VirtualSecretV1::from_host(OneShotSecretServicesV1 {
        value: Arc::new(Mutex::new(Some(utf16))),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_is_bounded_and_one_shot() {
        assert!(mint_virtual_secret_v1(Vec::new()).is_none());
        assert!(mint_virtual_secret_v1(vec![1; MAX_VIRTUAL_SECRET_UTF16_V1 + 1]).is_none());

        let secret =
            mint_virtual_secret_v1("password".encode_utf16().collect()).expect("bounded secret");
        let first = secret.take();
        assert_eq!(first.status, VirtualSecretStatusV1::READY);
        assert_eq!(first.utf16.len(), 8);
        let second = secret.take();
        assert_eq!(second.status, VirtualSecretStatusV1::CONSUMED);
        assert!(second.utf16.is_empty());
    }
}
