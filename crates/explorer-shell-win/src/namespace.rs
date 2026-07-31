//! Public-Shell-attribute projection into deny-by-default owned namespace metadata.
#![allow(
    unsafe_code,
    reason = "IShellItem public attribute queries require one audited COM vtable call on the owning STA"
)]

use explorer_common::{ExplorerError, ExplorerErrorKind};
use explorer_model::{
    LocationDescriptor, NamespaceCapabilities, NamespaceItem, PropertyKey, PropertyValue,
    ShellIdentity, ShellItemId,
};
use windows::Win32::{
    Foundation::PROPERTYKEY,
    System::Com::{
        CoTaskMemFree,
        StructuredStorage::{PropVariantToFileTime, PropVariantToStringAlloc, PropVariantToUInt64},
    },
    System::SystemServices::{
        SFGAO_BROWSABLE, SFGAO_CANCOPY, SFGAO_CANDELETE, SFGAO_CANMOVE, SFGAO_CANRENAME,
        SFGAO_DROPTARGET, SFGAO_FLAGS, SFGAO_FOLDER, SFGAO_HASPROPSHEET,
    },
    UI::Shell::{IShellItem2, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY},
};
use windows::core::Interface as _;

const PROPERTY_FORMAT: windows::core::GUID =
    windows::core::GUID::from_u128(0xb725f130_47ef_101a_a5f1_02608c9eebac);
const PKEY_ITEM_TYPE_TEXT: PROPERTYKEY = PROPERTYKEY {
    fmtid: PROPERTY_FORMAT,
    pid: 4,
};
const PKEY_SIZE: PROPERTYKEY = PROPERTYKEY {
    fmtid: PROPERTY_FORMAT,
    pid: 12,
};
const PKEY_DATE_MODIFIED: PROPERTYKEY = PROPERTYKEY {
    fmtid: PROPERTY_FORMAT,
    pid: 14,
};

/// Resolves public Shell attributes and converts them to owned model capability bits.
///
/// # Errors
///
/// Returns a typed Shell availability or identity error when resolution fails.
pub fn inspect_namespace_item(
    location: &LocationDescriptor,
) -> Result<NamespaceItem, ExplorerError> {
    let item = crate::navigation::shell_item(location)?;
    let display_name = crate::navigation::shell_item_name(&item, SIGDN_NORMALDISPLAY)?;
    let parsing_name = crate::navigation::shell_item_name(&item, SIGDN_DESKTOPABSOLUTEPARSING).ok();
    let mask = SFGAO_BROWSABLE
        | SFGAO_CANCOPY
        | SFGAO_CANMOVE
        | SFGAO_CANRENAME
        | SFGAO_CANDELETE
        | SFGAO_DROPTARGET
        | SFGAO_FOLDER
        | SFGAO_HASPROPSHEET;
    // SAFETY: the Shell item remains on its owning STA and the mask contains public SFGAO flags.
    let attributes = unsafe { item.GetAttributes(mask) }.map_err(|error| {
        ExplorerError::new(
            ExplorerErrorKind::Availability,
            "read Shell namespace attributes",
            true,
            "This Shell item is temporarily unavailable.",
            format!("HRESULT {:#010x}", error.code().0),
        )
    })?;
    let has = |flag: SFGAO_FLAGS| attributes.0 & flag.0 != 0;
    let is_container = has(SFGAO_FOLDER) || has(SFGAO_BROWSABLE);
    let mut bits = NamespaceCapabilities::OPEN | NamespaceCapabilities::CONTEXT_MENU;
    if is_container {
        bits |= NamespaceCapabilities::ENUMERATE | NamespaceCapabilities::SEARCH;
    } else {
        bits |= NamespaceCapabilities::THUMBNAIL | NamespaceCapabilities::PREVIEW;
    }
    if has(SFGAO_CANCOPY) {
        bits |= NamespaceCapabilities::COPY;
    }
    if has(SFGAO_CANMOVE) || has(SFGAO_DROPTARGET) {
        bits |= NamespaceCapabilities::DROP;
    }
    if is_container && has(SFGAO_DROPTARGET) {
        bits |= NamespaceCapabilities::PASTE;
    }
    if has(SFGAO_CANRENAME) {
        bits |= NamespaceCapabilities::RENAME;
    }
    if has(SFGAO_CANDELETE) {
        bits |= NamespaceCapabilities::DELETE;
    }
    if has(SFGAO_HASPROPSHEET) {
        bits |= NamespaceCapabilities::PROPERTIES;
    }
    let serializable = location.validate().is_ok();
    if serializable {
        bits |= NamespaceCapabilities::PIN;
    }
    let stable_bytes = parsing_name.as_deref().unwrap_or(&display_name).as_bytes();
    let stable_id = ShellItemId::from_provider_bytes(stable_bytes).ok_or_else(|| {
        ExplorerError::new(
            ExplorerErrorKind::Input,
            "construct Shell namespace identity",
            false,
            "This Shell item has an invalid identity.",
            "empty or excessive provider identity",
        )
    })?;
    Ok(NamespaceItem {
        identity: ShellIdentity {
            stable_id,
            descriptor: location.clone(),
            display_name,
            parsing_name,
            serializable,
            nonserializable_reason: (!serializable)
                .then(|| "Shell descriptor cannot be reconstructed".to_owned()),
        },
        is_container,
        capabilities: NamespaceCapabilities::from_public_bits(bits),
        properties: read_namespace_properties(&item),
        unavailable_reason: None,
    })
}

/// Retrieves a small viewport-oriented public property set and converts every PROPVARIANT before
/// returning to the UI/model boundary. Individual provider failures become `Unsupported` values.
fn read_namespace_properties(
    item: &windows::Win32::UI::Shell::IShellItem,
) -> Vec<(PropertyKey, PropertyValue)> {
    let Ok(item): Result<IShellItem2, _> = item.cast() else {
        return Vec::new();
    };
    [
        (PKEY_ITEM_TYPE_TEXT, PropertyKind::Text),
        (PKEY_SIZE, PropertyKind::Unsigned),
        (PKEY_DATE_MODIFIED, PropertyKind::FileTime),
    ]
    .into_iter()
    .map(|(key, kind)| {
        let model_key = PropertyKey {
            format_id: key.fmtid.to_u128().to_be_bytes(),
            property_id: key.pid,
        };
        // SAFETY: item is apartment-owned, key is a valid PROPERTYKEY, and PROPVARIANT owns its data.
        let value = unsafe { item.GetProperty(&raw const key) }
            .ok()
            .and_then(|variant| property_value(&variant, kind))
            .unwrap_or(PropertyValue::Unsupported);
        (model_key, value)
    })
    .collect()
}

#[derive(Clone, Copy)]
enum PropertyKind {
    Text,
    Unsigned,
    FileTime,
}

#[allow(
    unsafe_code,
    reason = "PROPVARIANT conversion APIs return owned values copied before leaving the STA"
)]
fn property_value(
    variant: &windows::Win32::System::Com::StructuredStorage::PROPVARIANT,
    kind: PropertyKind,
) -> Option<PropertyValue> {
    unsafe {
        match kind {
            PropertyKind::Text => {
                let text = PropVariantToStringAlloc(variant).ok()?;
                let value = text.to_string().ok().map(PropertyValue::Text);
                CoTaskMemFree(Some(text.0.cast()));
                value
            }
            PropertyKind::Unsigned => PropVariantToUInt64(variant)
                .ok()
                .map(PropertyValue::Unsigned),
            PropertyKind::FileTime => {
                PropVariantToFileTime(variant, windows::Win32::System::Variant::PSTF_UTC)
                    .ok()
                    .map(|time| {
                        PropertyValue::FileTime(
                            (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime),
                        )
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_this_pc_and_recycle_bin_capabilities_are_public_and_owned() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        for parsing_name in ["shell:MyComputerFolder", "shell:RecycleBinFolder"] {
            let item =
                inspect_namespace_item(&LocationDescriptor::ParsingName(parsing_name.to_owned()))
                    .expect("namespace item");
            assert!(item.is_container);
            assert!(item.capabilities.contains(NamespaceCapabilities::OPEN));
            assert!(item.identity.validate().is_ok());
            assert_eq!(item.properties.len(), 3);
        }
    }
}
