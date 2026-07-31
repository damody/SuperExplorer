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
        assert!(parse_address("name:report type:pdf").is_err());
        assert!(parse_address("relative folder").is_err());

        assert!(parse(r"C:\Users\fixture").is_err());
        assert!(parse("name:report type:pdf").is_ok());
    }
}
