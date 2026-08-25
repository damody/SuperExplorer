use std::path::PathBuf;

use explorer_model::LocationDescriptor;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressParseError {
    pub message: String,
}

/// Parses only explicit Windows locations; it never falls through to search semantics.
///
/// # Errors
///
/// Returns an address-specific error for relative text or search-property syntax.
pub fn parse_address(input: &str) -> Result<LocationDescriptor, AddressParseError> {
    let value = input.trim();
    if value.starts_with("shell:") {
        return Ok(LocationDescriptor::ParsingName(value.to_owned()));
    }
    if let Some((provider, remainder)) = value.split_once("://") {
        if matches!(provider.to_ascii_lowercase().as_str(), "adb" | "sftp") {
            return explorer_model::RemoteAddress::parse(value)
                .and_then(|address| {
                    address
                        .to_deterministic_location(1)
                        .map_err(|_| explorer_model::RemoteAddressError::InvalidComponent)
                })
                .map_err(|error| AddressParseError {
                    message: error.to_string(),
                });
        }
        let valid_provider = !provider.is_empty()
            && provider.len() <= 64
            && provider.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            });
        let mut parts = remainder.split('/');
        let identity_text = parts.next().unwrap_or_default();
        let generation_text = parts.next().unwrap_or_default();
        let mut identity = [0_u8; 16];
        let valid_identity = identity_text.len() == 32
            && identity_text
                .as_bytes()
                .chunks_exact(2)
                .enumerate()
                .all(|(index, pair)| {
                    let Ok(text) = std::str::from_utf8(pair) else {
                        return false;
                    };
                    let Ok(byte) = u8::from_str_radix(text, 16) else {
                        return false;
                    };
                    identity[index] = byte;
                    true
                });
        let generation = generation_text
            .parse::<u64>()
            .ok()
            .filter(|value| *value != 0);
        let components = parts.map(str::to_owned).collect::<Vec<_>>();
        if valid_provider
            && valid_identity
            && let Some(generation) = generation
        {
            return LocationDescriptor::try_virtual(
                provider, identity, generation, None, components,
            )
            .map_err(|_| AddressParseError {
                message: "Invalid virtual location".to_owned(),
            });
        }
        return Err(AddressParseError {
            message: "Invalid virtual location".to_owned(),
        });
    }
    let bytes = value.as_bytes();
    let drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let unc = value.starts_with(r"\\")
        && value[2..]
            .split(['\\', '/'])
            .filter(|part| !part.is_empty())
            .take(2)
            .count()
            == 2;
    if drive_absolute || unc {
        Ok(LocationDescriptor::FileSystem(PathBuf::from(value)))
    } else {
        Err(AddressParseError {
            message: "地址必須是磁碟機絕對路徑、UNC 路徑或 shell: parsing name".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn address_and_search_parsers_never_fall_through_into_each_other() {
        assert!(parse_address(r"C:\Users\fixture").is_ok());
        assert!(parse_address(r"\\server\share\folder").is_ok());
        assert!(parse_address("shell:Downloads").is_ok());
        assert_eq!(
            parse_address("adb://device-123/sdcard/Download").unwrap(),
            explorer_model::RemoteAddress::parse("adb://device-123/sdcard/Download")
                .unwrap()
                .to_deterministic_location(1)
                .unwrap()
        );
        assert!(parse_address("sftp://production/root").is_ok());
        assert_eq!(
            parse_address("rust-7z://09090909090909090909090909090909/5/src/nested").unwrap(),
            LocationDescriptor::try_virtual(
                "rust-7z",
                [9; 16],
                5,
                None,
                vec!["src".to_owned(), "nested".to_owned()],
            )
            .unwrap()
        );
        assert!(parse_address("rust-7z://0909/5/src").is_err());
        assert!(parse_address("rust-7z://09090909090909090909090909090909/0/src").is_err());
        assert!(parse_address("rust-7z://09090909090909090909090909090909/5/../src").is_err());
        assert!(parse_address("name:report type:pdf").is_err());
        assert!(parse_address("relative folder").is_err());

        assert!(parse(r"C:\Users\fixture").is_err());
        assert!(parse("name:report type:pdf").is_ok());
    }
}
