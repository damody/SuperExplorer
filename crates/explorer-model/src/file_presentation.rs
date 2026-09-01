//! Pure filename-based presentation shared by remote metadata and UI fallbacks.

/// Stable built-in icon families for ADB/SFTP files.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RemoteFileIconKind {
    Generic,
    Pdf,
    Text,
    Settings,
    Image,
    Archive,
    Audio,
    Video,
    Code,
    Script,
    Executable,
    AndroidPackage,
    Word,
    Spreadsheet,
    Presentation,
    Notebook,
    Database,
    Mail,
    Font,
    Certificate,
    DiskImage,
    Web,
    Data,
    Markup,
}

/// Deterministic presentation derived only from one remote filename.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteFilePresentation {
    pub type_label: String,
    pub icon_kind: RemoteFileIconKind,
}

const COMPOUND_EXTENSION_MAPPINGS: &[(&str, RemoteFileIconKind)] = &[
    ("tar.bz2", RemoteFileIconKind::Archive),
    ("tar.lz4", RemoteFileIconKind::Archive),
    ("tar.zst", RemoteFileIconKind::Archive),
    ("tar.gz", RemoteFileIconKind::Archive),
    ("tar.xz", RemoteFileIconKind::Archive),
    ("tar.lz", RemoteFileIconKind::Archive),
    ("tar.br", RemoteFileIconKind::Archive),
    ("json.gz", RemoteFileIconKind::Data),
    ("svg.gz", RemoteFileIconKind::Image),
];

const EXTENSION_ICON_MAPPINGS: &[(&str, RemoteFileIconKind)] = &[
    ("pdf", RemoteFileIconKind::Pdf),
    ("xps", RemoteFileIconKind::Pdf),
    ("oxps", RemoteFileIconKind::Pdf),
    ("epub", RemoteFileIconKind::Pdf),
    ("mobi", RemoteFileIconKind::Pdf),
    ("azw", RemoteFileIconKind::Pdf),
    ("azw3", RemoteFileIconKind::Pdf),
    ("txt", RemoteFileIconKind::Text),
    ("text", RemoteFileIconKind::Text),
    ("log", RemoteFileIconKind::Text),
    ("md", RemoteFileIconKind::Text),
    ("markdown", RemoteFileIconKind::Text),
    ("rst", RemoteFileIconKind::Text),
    ("nfo", RemoteFileIconKind::Text),
    ("ini", RemoteFileIconKind::Settings),
    ("cfg", RemoteFileIconKind::Settings),
    ("conf", RemoteFileIconKind::Settings),
    ("config", RemoteFileIconKind::Settings),
    ("toml", RemoteFileIconKind::Settings),
    ("yaml", RemoteFileIconKind::Settings),
    ("yml", RemoteFileIconKind::Settings),
    ("properties", RemoteFileIconKind::Settings),
    ("prop", RemoteFileIconKind::Settings),
    ("rc", RemoteFileIconKind::Settings),
    ("policy", RemoteFileIconKind::Settings),
    ("cil", RemoteFileIconKind::Settings),
    ("service", RemoteFileIconKind::Settings),
    ("desktop", RemoteFileIconKind::Settings),
    ("env", RemoteFileIconKind::Settings),
    ("jpg", RemoteFileIconKind::Image),
    ("jpeg", RemoteFileIconKind::Image),
    ("jpe", RemoteFileIconKind::Image),
    ("png", RemoteFileIconKind::Image),
    ("gif", RemoteFileIconKind::Image),
    ("bmp", RemoteFileIconKind::Image),
    ("webp", RemoteFileIconKind::Image),
    ("tif", RemoteFileIconKind::Image),
    ("tiff", RemoteFileIconKind::Image),
    ("svg", RemoteFileIconKind::Image),
    ("ico", RemoteFileIconKind::Image),
    ("heic", RemoteFileIconKind::Image),
    ("heif", RemoteFileIconKind::Image),
    ("avif", RemoteFileIconKind::Image),
    ("raw", RemoteFileIconKind::Image),
    ("dng", RemoteFileIconKind::Image),
    ("psd", RemoteFileIconKind::Image),
    ("ai", RemoteFileIconKind::Image),
    ("zip", RemoteFileIconKind::Archive),
    ("7z", RemoteFileIconKind::Archive),
    ("rar", RemoteFileIconKind::Archive),
    ("tar", RemoteFileIconKind::Archive),
    ("gz", RemoteFileIconKind::Archive),
    ("tgz", RemoteFileIconKind::Archive),
    ("bz", RemoteFileIconKind::Archive),
    ("bz2", RemoteFileIconKind::Archive),
    ("tbz", RemoteFileIconKind::Archive),
    ("tbz2", RemoteFileIconKind::Archive),
    ("xz", RemoteFileIconKind::Archive),
    ("txz", RemoteFileIconKind::Archive),
    ("zst", RemoteFileIconKind::Archive),
    ("tzst", RemoteFileIconKind::Archive),
    ("lz", RemoteFileIconKind::Archive),
    ("lz4", RemoteFileIconKind::Archive),
    ("br", RemoteFileIconKind::Archive),
    ("cab", RemoteFileIconKind::Archive),
    ("arj", RemoteFileIconKind::Archive),
    ("cpio", RemoteFileIconKind::Archive),
    ("ab", RemoteFileIconKind::Archive),
    ("gzip", RemoteFileIconKind::Archive),
    ("rpm", RemoteFileIconKind::Archive),
    ("deb", RemoteFileIconKind::Archive),
    ("mp3", RemoteFileIconKind::Audio),
    ("wav", RemoteFileIconKind::Audio),
    ("flac", RemoteFileIconKind::Audio),
    ("aac", RemoteFileIconKind::Audio),
    ("m4a", RemoteFileIconKind::Audio),
    ("ogg", RemoteFileIconKind::Audio),
    ("oga", RemoteFileIconKind::Audio),
    ("opus", RemoteFileIconKind::Audio),
    ("wma", RemoteFileIconKind::Audio),
    ("mid", RemoteFileIconKind::Audio),
    ("midi", RemoteFileIconKind::Audio),
    ("aiff", RemoteFileIconKind::Audio),
    ("ape", RemoteFileIconKind::Audio),
    ("mp4", RemoteFileIconKind::Video),
    ("mkv", RemoteFileIconKind::Video),
    ("mov", RemoteFileIconKind::Video),
    ("avi", RemoteFileIconKind::Video),
    ("webm", RemoteFileIconKind::Video),
    ("wmv", RemoteFileIconKind::Video),
    ("m4v", RemoteFileIconKind::Video),
    ("mpeg", RemoteFileIconKind::Video),
    ("mpg", RemoteFileIconKind::Video),
    ("3gp", RemoteFileIconKind::Video),
    ("flv", RemoteFileIconKind::Video),
    ("m2ts", RemoteFileIconKind::Video),
    ("mts", RemoteFileIconKind::Video),
    ("rs", RemoteFileIconKind::Code),
    ("c", RemoteFileIconKind::Code),
    ("h", RemoteFileIconKind::Code),
    ("cpp", RemoteFileIconKind::Code),
    ("hpp", RemoteFileIconKind::Code),
    ("cc", RemoteFileIconKind::Code),
    ("cs", RemoteFileIconKind::Code),
    ("java", RemoteFileIconKind::Code),
    ("kt", RemoteFileIconKind::Code),
    ("kts", RemoteFileIconKind::Code),
    ("go", RemoteFileIconKind::Code),
    ("py", RemoteFileIconKind::Code),
    ("pyw", RemoteFileIconKind::Code),
    ("js", RemoteFileIconKind::Code),
    ("jsx", RemoteFileIconKind::Code),
    ("ts", RemoteFileIconKind::Code),
    ("tsx", RemoteFileIconKind::Code),
    ("php", RemoteFileIconKind::Code),
    ("rb", RemoteFileIconKind::Code),
    ("swift", RemoteFileIconKind::Code),
    ("lua", RemoteFileIconKind::Code),
    ("sql", RemoteFileIconKind::Code),
    ("wasm", RemoteFileIconKind::Code),
    ("dart", RemoteFileIconKind::Code),
    ("r", RemoteFileIconKind::Code),
    ("scala", RemoteFileIconKind::Code),
    ("ex", RemoteFileIconKind::Code),
    ("exs", RemoteFileIconKind::Code),
    ("fs", RemoteFileIconKind::Code),
    ("fsx", RemoteFileIconKind::Code),
    ("vb", RemoteFileIconKind::Code),
    ("sh", RemoteFileIconKind::Script),
    ("bash", RemoteFileIconKind::Script),
    ("zsh", RemoteFileIconKind::Script),
    ("fish", RemoteFileIconKind::Script),
    ("ps1", RemoteFileIconKind::Script),
    ("bat", RemoteFileIconKind::Script),
    ("cmd", RemoteFileIconKind::Script),
    ("vbs", RemoteFileIconKind::Script),
    ("awk", RemoteFileIconKind::Script),
    ("exe", RemoteFileIconKind::Executable),
    ("msi", RemoteFileIconKind::Executable),
    ("appx", RemoteFileIconKind::Executable),
    ("msix", RemoteFileIconKind::Executable),
    ("dll", RemoteFileIconKind::Executable),
    ("so", RemoteFileIconKind::Executable),
    ("dylib", RemoteFileIconKind::Executable),
    ("bin", RemoteFileIconKind::Executable),
    ("elf", RemoteFileIconKind::Executable),
    ("jar", RemoteFileIconKind::Executable),
    ("class", RemoteFileIconKind::Executable),
    ("o", RemoteFileIconKind::Executable),
    ("a", RemoteFileIconKind::Executable),
    ("bc", RemoteFileIconKind::Executable),
    ("bprof", RemoteFileIconKind::Data),
    ("ko", RemoteFileIconKind::Executable),
    ("apk", RemoteFileIconKind::AndroidPackage),
    ("aab", RemoteFileIconKind::AndroidPackage),
    ("apks", RemoteFileIconKind::AndroidPackage),
    ("xapk", RemoteFileIconKind::AndroidPackage),
    ("dex", RemoteFileIconKind::AndroidPackage),
    ("odex", RemoteFileIconKind::AndroidPackage),
    ("vdex", RemoteFileIconKind::AndroidPackage),
    ("art", RemoteFileIconKind::AndroidPackage),
    ("oat", RemoteFileIconKind::AndroidPackage),
    ("obb", RemoteFileIconKind::AndroidPackage),
    ("doc", RemoteFileIconKind::Word),
    ("docx", RemoteFileIconKind::Word),
    ("docm", RemoteFileIconKind::Word),
    ("dot", RemoteFileIconKind::Word),
    ("dotx", RemoteFileIconKind::Word),
    ("dotm", RemoteFileIconKind::Word),
    ("odt", RemoteFileIconKind::Word),
    ("ott", RemoteFileIconKind::Word),
    ("rtf", RemoteFileIconKind::Word),
    ("xls", RemoteFileIconKind::Spreadsheet),
    ("xlsx", RemoteFileIconKind::Spreadsheet),
    ("xlsm", RemoteFileIconKind::Spreadsheet),
    ("xlsb", RemoteFileIconKind::Spreadsheet),
    ("xlt", RemoteFileIconKind::Spreadsheet),
    ("xltx", RemoteFileIconKind::Spreadsheet),
    ("xltm", RemoteFileIconKind::Spreadsheet),
    ("ods", RemoteFileIconKind::Spreadsheet),
    ("ots", RemoteFileIconKind::Spreadsheet),
    ("csv", RemoteFileIconKind::Spreadsheet),
    ("tsv", RemoteFileIconKind::Spreadsheet),
    ("ppt", RemoteFileIconKind::Presentation),
    ("pptx", RemoteFileIconKind::Presentation),
    ("pptm", RemoteFileIconKind::Presentation),
    ("pps", RemoteFileIconKind::Presentation),
    ("ppsx", RemoteFileIconKind::Presentation),
    ("ppsm", RemoteFileIconKind::Presentation),
    ("pot", RemoteFileIconKind::Presentation),
    ("potx", RemoteFileIconKind::Presentation),
    ("potm", RemoteFileIconKind::Presentation),
    ("odp", RemoteFileIconKind::Presentation),
    ("otp", RemoteFileIconKind::Presentation),
    ("one", RemoteFileIconKind::Notebook),
    ("onetoc2", RemoteFileIconKind::Notebook),
    ("accdb", RemoteFileIconKind::Database),
    ("mdb", RemoteFileIconKind::Database),
    ("sqlite", RemoteFileIconKind::Database),
    ("sqlite3", RemoteFileIconKind::Database),
    ("db", RemoteFileIconKind::Database),
    ("db3", RemoteFileIconKind::Database),
    ("sdb", RemoteFileIconKind::Database),
    ("pst", RemoteFileIconKind::Mail),
    ("ost", RemoteFileIconKind::Mail),
    ("msg", RemoteFileIconKind::Mail),
    ("eml", RemoteFileIconKind::Mail),
    ("emlx", RemoteFileIconKind::Mail),
    ("mbox", RemoteFileIconKind::Mail),
    ("ttf", RemoteFileIconKind::Font),
    ("otf", RemoteFileIconKind::Font),
    ("woff", RemoteFileIconKind::Font),
    ("woff2", RemoteFileIconKind::Font),
    ("eot", RemoteFileIconKind::Font),
    ("fon", RemoteFileIconKind::Font),
    ("ttc", RemoteFileIconKind::Font),
    ("cer", RemoteFileIconKind::Certificate),
    ("crt", RemoteFileIconKind::Certificate),
    ("der", RemoteFileIconKind::Certificate),
    ("pem", RemoteFileIconKind::Certificate),
    ("pfx", RemoteFileIconKind::Certificate),
    ("p12", RemoteFileIconKind::Certificate),
    ("key", RemoteFileIconKind::Certificate),
    ("pub", RemoteFileIconKind::Certificate),
    ("asc", RemoteFileIconKind::Certificate),
    ("sig", RemoteFileIconKind::Certificate),
    ("img", RemoteFileIconKind::DiskImage),
    ("iso", RemoteFileIconKind::DiskImage),
    ("vhd", RemoteFileIconKind::DiskImage),
    ("vhdx", RemoteFileIconKind::DiskImage),
    ("vmdk", RemoteFileIconKind::DiskImage),
    ("qcow", RemoteFileIconKind::DiskImage),
    ("qcow2", RemoteFileIconKind::DiskImage),
    ("dmg", RemoteFileIconKind::DiskImage),
    ("sparseimage", RemoteFileIconKind::DiskImage),
    ("html", RemoteFileIconKind::Web),
    ("htm", RemoteFileIconKind::Web),
    ("css", RemoteFileIconKind::Web),
    ("scss", RemoteFileIconKind::Web),
    ("sass", RemoteFileIconKind::Web),
    ("less", RemoteFileIconKind::Web),
    ("url", RemoteFileIconKind::Web),
    ("webloc", RemoteFileIconKind::Web),
    ("json", RemoteFileIconKind::Data),
    ("jsonl", RemoteFileIconKind::Data),
    ("ndjson", RemoteFileIconKind::Data),
    ("dat", RemoteFileIconKind::Data),
    ("data", RemoteFileIconKind::Data),
    ("idx", RemoteFileIconKind::Data),
    ("trace", RemoteFileIconKind::Data),
    ("perfetto-trace", RemoteFileIconKind::Data),
    ("prof", RemoteFileIconKind::Data),
    ("pb", RemoteFileIconKind::Data),
    ("protobuf", RemoteFileIconKind::Data),
    ("avro", RemoteFileIconKind::Data),
    ("parquet", RemoteFileIconKind::Data),
    ("orc", RemoteFileIconKind::Data),
    ("feather", RemoteFileIconKind::Data),
    ("xml", RemoteFileIconKind::Markup),
    ("xsd", RemoteFileIconKind::Markup),
    ("xsl", RemoteFileIconKind::Markup),
    ("xslt", RemoteFileIconKind::Markup),
    ("kl", RemoteFileIconKind::Settings),
    ("kcm", RemoteFileIconKind::Settings),
    ("idc", RemoteFileIconKind::Settings),
];

/// Classifies a filename without filesystem access, content inspection, or platform associations.
pub fn classify_remote_file_name(name: &str) -> RemoteFilePresentation {
    if let Some(remainder) = name.strip_prefix('.')
        && !remainder.contains('.')
    {
        return dotfile_setting_name(remainder).map_or_else(
            generic_file_presentation,
            |setting_name| RemoteFilePresentation {
                type_label: format!("{setting_name} Setting File"),
                icon_kind: RemoteFileIconKind::Settings,
            },
        );
    }

    let lowercase = name.to_ascii_lowercase();
    if let Some((extension, icon_kind)) = COMPOUND_EXTENSION_MAPPINGS
        .iter()
        .find(|(extension, _)| lowercase.ends_with(&format!(".{extension}")))
    {
        return RemoteFilePresentation {
            type_label: format!("{} File", extension.to_ascii_uppercase()),
            icon_kind: *icon_kind,
        };
    }

    let Some((_, extension)) = name.rsplit_once('.') else {
        return generic_file_presentation();
    };
    if extension.is_empty() {
        return generic_file_presentation();
    }

    RemoteFilePresentation {
        type_label: format!("{} File", extension.to_uppercase()),
        icon_kind: icon_kind_for_extension(extension),
    }
}

fn generic_file_presentation() -> RemoteFilePresentation {
    RemoteFilePresentation {
        type_label: "File".to_owned(),
        icon_kind: RemoteFileIconKind::Generic,
    }
}

fn dotfile_setting_name(remainder: &str) -> Option<String> {
    if remainder.is_empty() || remainder.contains('.') {
        return None;
    }
    let words = remainder
        .split(['_', '-'])
        .filter(|word| !word.is_empty())
        .map(title_case_first)
        .collect::<Vec<_>>();
    (!words.is_empty()).then(|| words.join(" "))
}

fn title_case_first(word: &str) -> String {
    let mut characters = word.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_uppercase().chain(characters).collect()
}

fn icon_kind_for_extension(extension: &str) -> RemoteFileIconKind {
    if !extension.is_ascii() {
        return RemoteFileIconKind::Generic;
    }
    let lowercase = extension.to_ascii_lowercase();
    EXTENSION_ICON_MAPPINGS
        .iter()
        .find_map(|(candidate, kind)| (*candidate == lowercase).then_some(*kind))
        .unwrap_or(RemoteFileIconKind::Generic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_type_labels_follow_ordered_grammar() {
        let cases = [
            ("report.txt", "TXT File", RemoteFileIconKind::Text),
            ("PHOTO.JpG", "JPG File", RemoteFileIconKind::Image),
            ("backup.TAR.Gz", "TAR.GZ File", RemoteFileIconKind::Archive),
            ("firmware.bin.gz", "GZ File", RemoteFileIconKind::Archive),
            ("bundle.tgz", "TGZ File", RemoteFileIconKind::Archive),
            ("README", "File", RemoteFileIconKind::Generic),
            (".", "File", RemoteFileIconKind::Generic),
            ("trailing.", "File", RemoteFileIconKind::Generic),
            ("", "File", RemoteFileIconKind::Generic),
            ("report.資料", "資料 File", RemoteFileIconKind::Generic),
        ];
        for (name, type_label, icon_kind) in cases {
            let presentation = classify_remote_file_name(name);
            assert_eq!(presentation.type_label, type_label, "{name}");
            assert_eq!(presentation.icon_kind, icon_kind, "{name}");
        }
    }

    #[test]
    fn dotfiles_are_setting_files_and_format_separated_words() {
        let cases = [
            (".bashrc", "Bashrc Setting File"),
            (".bash_logout", "Bash Logout Setting File"),
            (".profile", "Profile Setting File"),
            (".gitignore", "Gitignore Setting File"),
            (".my--mixed_name", "My Mixed Name Setting File"),
            (".設定", "設定 Setting File"),
        ];
        for (name, type_label) in cases {
            let presentation = classify_remote_file_name(name);
            assert_eq!(presentation.type_label, type_label, "{name}");
            assert_eq!(
                presentation.icon_kind,
                RemoteFileIconKind::Settings,
                "{name}"
            );
        }
        assert_eq!(
            classify_remote_file_name(".env.local").type_label,
            "LOCAL File"
        );
        assert_eq!(
            classify_remote_file_name("..hidden").type_label,
            "HIDDEN File"
        );
        assert_eq!(classify_remote_file_name(".---").type_label, "File");
    }

    #[test]
    fn every_icon_family_has_representative_extensions() {
        let cases = [
            (RemoteFileIconKind::Pdf, &["pdf"][..]),
            (RemoteFileIconKind::Text, &["txt", "log", "markdown", "rst"]),
            (RemoteFileIconKind::Settings, &["conf", "prop", "cil", "rc"]),
            (RemoteFileIconKind::Image, &["jpg", "png", "svg", "avif"]),
            (RemoteFileIconKind::Archive, &["zip", "7z", "tgz", "gz"]),
            (RemoteFileIconKind::Audio, &["mp3", "flac", "opus", "midi"]),
            (RemoteFileIconKind::Video, &["mp4", "mkv", "webm", "mpeg"]),
            (RemoteFileIconKind::Code, &["rs", "py", "wasm", "sql"]),
            (RemoteFileIconKind::Script, &["sh", "ps1", "bat", "awk"]),
            (RemoteFileIconKind::Executable, &["exe", "bin", "so", "o"]),
            (RemoteFileIconKind::AndroidPackage, &["apk", "aab", "dex"]),
            (RemoteFileIconKind::Word, &["docx", "docm", "odt", "rtf"]),
            (
                RemoteFileIconKind::Spreadsheet,
                &["xlsx", "xlsm", "ods", "csv"],
            ),
            (RemoteFileIconKind::Presentation, &["pptx", "ppsx", "odp"]),
            (RemoteFileIconKind::Notebook, &["one", "onetoc2"]),
            (RemoteFileIconKind::Database, &["accdb", "sqlite", "db"]),
            (RemoteFileIconKind::Mail, &["pst", "msg", "eml"]),
            (RemoteFileIconKind::Font, &["ttf", "otf", "woff2"]),
            (RemoteFileIconKind::Certificate, &["cer", "pem", "p12"]),
            (RemoteFileIconKind::DiskImage, &["iso", "vhdx", "qcow2"]),
            (RemoteFileIconKind::Web, &["html", "css", "webloc"]),
            (RemoteFileIconKind::Data, &["json", "pb", "parquet"]),
            (RemoteFileIconKind::Markup, &["xml", "xsd", "xslt"]),
            (RemoteFileIconKind::Generic, &["unknown"]),
        ];
        for (kind, extensions) in cases {
            for extension in extensions {
                assert_eq!(
                    classify_remote_file_name(&format!("sample.{extension}")).icon_kind,
                    kind,
                    "{extension}"
                );
            }
        }
    }

    #[test]
    fn all_declared_compounds_use_longest_match() {
        for (extension, expected_kind) in COMPOUND_EXTENSION_MAPPINGS {
            let presentation = classify_remote_file_name(&format!("backup.{extension}"));
            assert_eq!(
                presentation.type_label,
                format!("{} File", extension.to_ascii_uppercase())
            );
            assert_eq!(presentation.icon_kind, *expected_kind);
        }
    }

    #[test]
    fn every_declared_final_extension_mapping_is_case_insensitive() {
        let mut seen = std::collections::HashSet::new();
        for (extension, expected_kind) in EXTENSION_ICON_MAPPINGS {
            assert!(seen.insert(*extension), "duplicate extension: {extension}");
            for candidate in [extension.to_string(), extension.to_ascii_uppercase()] {
                let presentation = classify_remote_file_name(&format!("sample.{candidate}"));
                assert_eq!(presentation.icon_kind, *expected_kind, "{candidate}");
                assert_eq!(
                    presentation.type_label,
                    format!("{} File", candidate.to_uppercase())
                );
            }
        }
    }

    #[test]
    fn representative_adb_system_filenames_use_expected_families() {
        let cases = [
            ("mke2fs.conf", RemoteFileIconKind::Settings),
            ("default-permissions.xml", RemoteFileIconKind::Markup),
            ("protolog.conf.json.gz", RemoteFileIconKind::Data),
            ("systemserverclasspath.pb", RemoteFileIconKind::Data),
            ("plat_sepolicy.cil", RemoteFileIconKind::Settings),
            ("crash_dump.x86_64.policy", RemoteFileIconKind::Settings),
            ("init.rc", RemoteFileIconKind::Settings),
            ("init.sh", RemoteFileIconKind::Script),
            ("libcrypto.so", RemoteFileIconKind::Executable),
            ("netd.o", RemoteFileIconKind::Executable),
            ("libclcore.bc", RemoteFileIconKind::Executable),
            ("boot-image.prof", RemoteFileIconKind::Data),
            ("boot-image.bprof", RemoteFileIconKind::Data),
        ];
        for (name, expected_kind) in cases {
            assert_eq!(
                classify_remote_file_name(name).icon_kind,
                expected_kind,
                "{name}"
            );
        }
        assert_eq!(
            classify_remote_file_name("payload.not-json.gz").icon_kind,
            RemoteFileIconKind::Archive
        );
    }
}
