//! The built-in extension -> category map and the resolution logic that merges
//! it with user overrides.

use std::collections::{HashMap, HashSet};

use crate::config::Config;

/// Built-in categories.
///
/// Format: one line per category, `Category|ext ext ext`. Category names may
/// contain `/` to create a sub-folder (e.g. `Applications/Windows`). Extensions
/// are matched case-insensitively; multi-dot extensions (e.g. `tar.gz`) are
/// supported and always win over single-dot matches.
pub const BUILTIN_CATEGORIES: &str = "\
Images|png jpg jpeg jpe jfif gif bmp dib tif tiff svg webp ico icns psd psb ai eps raw cr2 cr3 crw nef nrw arw sr2 srf dng orf rw2 raf pef srw heic heif hif avif jxl jp2 j2c j2k jpf jpx dds tga exr hdr pic pbm pgm ppm pnm pam mng apng kra ora xcf
Videos|mp4 m4v mkv mk3d avi mov qt wmv flv f4v webm mpg mpeg mpe m2v mpv m2p 3gp 3g2 3gpp ogv ogm vob ts mts m2ts mxf asf rm rmvb divx xvid amv roq y4m
Audio|mp3 mp2 mp1 flac wav wave aac ogg oga opus wma m4a m4b m4p m4r aiff aif aifc alac ape wv tta mpc mid midi kar mod s3m xm it mtm 669 amr ac3 eac3 dts dtshd caf au snd ra ram m3u m3u8 pls wpl
Documents|pdf doc docm docx dot dotm dotx xls xlsb xlsm xlsx xlt xltm xltx xlam odt ott ods ots odp otp odg otg odf odc txt text rtf csv tsv psv md markdown mdx tex latex ltx bib cls one onetoc2 vsd vsdx vss vst vssx vstx xps oxps wpd wps wri abw hwp hwpx sxw sxc sxi sxd sdc sdw rst adoc asciidoc org texinfo man nfo log
eBooks|epub mobi azw azw3 azw4 fb2 lit djvu djv cbz cbr cb7 cbt snb pdb
Compressed|zip zipx rar 7z gz tgz bz2 tbz tbz2 xz txz zst tzst lz4 tlz4 lzma lzo lz lzip z br brotli zpaq cpio cab arj ace zoo rpa sit sitx hqx tar tar.gz tar.bz2 tar.xz tar.zst tar.lz tar.lz4 tar.lzo tar.z jar war ear sar wim esd swm
Applications/Windows|exe msi msix msixbundle appx appxbundle msp mst cpl scr com
Applications/macOS|dmg pkg mpkg
Applications/Linux|appimage deb rpm flatpakref flatpakrepo flathub snap apk xbps ebuild pkg.tar.zst pkg.tar.xz pkg.tar.lz4
Applications/Android|apk aab apks xapk
Applications/Web|crx xpi vsix
Code|sh bash zsh fish csh tcsh ksh ash dash py py3 pyw pyi ipynb js mjs cjs jsx ts tsx mts cts c h cc cpp cxx c++ hh hpp hxx h++ inl ipp cs csx kt kts scala sc groovy gvy gradle rs go rb rake gemspec ru pl pm pod t php php3 php4 php5 php7 php8 phtml phps lua vim el elc lisp lsp cl clj cljs cljc edn hs lhs erl hrl ex exs eex leex heex nim nims zig dart swift m mm r jl rkt scm ss st sql psql plsql hql prql vue svelte astro html htm xhtml shtml css scss sass less styl stylus json json5 jsonc jsonl ndjson har xml xsl xslt xsd xjb dtd plist yaml yml toml ini cfg conf properties env dotenv editorconfig graphql gql proto protobuf thrift avdl asm s wat wasm teal sol cairo move ftl hbs handlebars mustache ejs pug jade haml slim twig razor cshtml vbhtml vb vbs ps1 psm1 psd1 bat cmd awk sed nix dhall cue reg
Fonts|ttf ttc otf otc woff woff2 eot pfa pfb pfm afm fon fnt bdf pcf dfont suit sfnt
Disk Images|iso img ima udf ccd sub cue bin mdf mds nrg toast cdr vhd vhdx vdi vmdk vpc qcow qcow2 qed hdd ova ovf sparseimage sparsebundle e01 ex01 dd flp
Virtual Machines|vbox vbox-prev pvm pvs vmwarevm vmx nvram vmsn vmsd vmss vmem
Torrents|torrent magnet
Databases|db db3 sqlite sqlite3 sqlitedb sql3 mdb accdb accde laccdb dbf frm myd myi ibd ndf ldf mdf bak dump realm
3D Models|obj mtl stl fbx blend blend1 dae 3ds max ma mb c4d lwo lws ply gltf glb usd usda usdc usdz abc alembic step stp iges igs dwg dxf dwf skp f3d f3z 3mf amf x3d x3dv wrl vrml pcd
Design|fig sketch xd indd idml indb afdesign afphoto afpub cdr cdrx cdt
Certificates|pem crt cer der p12 pfx key pub asc gpg sig csr
Subtitles|srt sub sbv vtt ass ssa aqt lrc smi sami ttml dfxp
Roms|rom gba gbc gb nds 3ds cia nsp xci wbfs wad nes sfc smc z64 n64 gcm
Disk Metadata|sfv md5 sha1 sha256 sha512 asc
";

/// A resolved category table ready for lookups.
#[derive(Debug, Clone)]
pub struct CategorySet {
    /// Ordered list of directory -> extensions.
    pub categories: Vec<(String, Vec<String>)>,
    /// Single (and full multi-dot) extension -> directory.
    ext_map: HashMap<String, String>,
    /// Multi-dot extensions, longest first: (ext, directory).
    multi_exts: Vec<(String, String)>,
    /// Lower-cased top level folder names we own and must never move.
    pub known_top_level: HashSet<String>,
    /// Directory used for loose folders.
    pub folders_category: String,
    /// Directory used for unknown files.
    pub unknown_category: String,
    /// Filenames without a useful extension that map to a category.
    exact_names: HashMap<String, String>,
}

/// Parse the built-in category table.
pub fn parse_builtins() -> Vec<(String, Vec<String>)> {
    BUILTIN_CATEGORIES
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, exts) = line.split_once('|')?;
            let exts = split_exts(exts);
            Some((name.trim().to_string(), exts))
        })
        .collect()
}

fn split_exts(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
}

/// Merge built-in categories with the user configuration.
pub fn resolve(config: &Config) -> CategorySet {
    let mut categories = if config.replace_categories {
        Vec::new()
    } else {
        parse_builtins()
    };

    if !config.disable_categories.is_empty() {
        let disabled: HashSet<String> = config
            .disable_categories
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .collect();
        categories.retain(|(name, _)| !disabled.contains(&name.to_ascii_lowercase()));
    }

    for (name, exts) in &config.categories {
        let name = name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let normalised = split_exts(&exts.join(" "));
        match categories
            .iter_mut()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(&name))
        {
            Some(entry) => entry.1 = normalised,
            None => categories.push((name, normalised)),
        }
    }

    let mut ext_map: HashMap<String, String> = HashMap::new();
    let mut multi_exts: Vec<(String, String)> = Vec::new();
    let mut known_top_level: HashSet<String> = HashSet::new();

    for (category, exts) in &categories {
        if let Some(top) = category.split('/').next() {
            known_top_level.insert(top.to_ascii_lowercase());
        }
        for ext in exts {
            if ext.contains('.') {
                multi_exts.push((ext.clone(), category.clone()));
            }
            ext_map
                .entry(ext.clone())
                .or_insert_with(|| category.clone());
        }
    }

    known_top_level.insert(config.folders_category.to_ascii_lowercase());
    known_top_level.insert(config.unknown_category.to_ascii_lowercase());

    // Longest multi-dot extension first so `tar.gz` beats `gz`.
    multi_exts.sort_by_key(|entry| std::cmp::Reverse(entry.0.len()));
    multi_exts.dedup_by(|a, b| a.0 == b.0);

    CategorySet {
        categories,
        ext_map,
        multi_exts,
        known_top_level,
        folders_category: config.folders_category.clone(),
        unknown_category: config.unknown_category.clone(),
        exact_names: exact_name_map(),
    }
}

impl CategorySet {
    /// Determine the category for a file name, if it is known.
    pub fn classify(&self, file_name: &str) -> Option<String> {
        let lower = file_name.to_ascii_lowercase();

        if let Some(category) = self.exact_names.get(&lower) {
            return Some(category.clone());
        }

        for (ext, category) in &self.multi_exts {
            if suffix_matches(&lower, ext) {
                return Some(category.clone());
            }
        }

        if let Some((_, ext)) = lower.rsplit_once('.') {
            if !ext.is_empty() {
                if let Some(category) = self.ext_map.get(ext) {
                    return Some(category.clone());
                }
            }
        }

        None
    }

    /// The matched extension of `file_name` (without the leading dot), using
    /// the original casing. Multi-dot extensions are preferred.
    pub fn extension_of(&self, file_name: &str) -> Option<String> {
        let lower = file_name.to_ascii_lowercase();
        for (ext, _) in &self.multi_exts {
            if suffix_matches(&lower, ext) && lower.len() == file_name.len() {
                let start = file_name.len() - ext.len();
                return Some(file_name[start..].to_string());
            }
        }
        let (stem, ext) = file_name.rsplit_once('.')?;
        if ext.is_empty() || stem.is_empty() {
            None
        } else {
            Some(ext.to_string())
        }
    }

    /// True when `name` is one of the folders we manage and therefore must not
    /// itself be treated as a user folder.
    pub fn is_managed_folder(&self, name: &str) -> bool {
        self.known_top_level.contains(&name.to_ascii_lowercase())
    }

    /// The list of category directory names (including nested ones).
    pub fn category_names(&self) -> Vec<String> {
        self.categories
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }
}

fn suffix_matches(lower_name: &str, ext: &str) -> bool {
    if !lower_name.ends_with(ext) {
        return false;
    }
    let boundary = lower_name.len() - ext.len();
    boundary > 0 && lower_name.as_bytes()[boundary - 1] == b'.'
}

/// Files that have no (or a meaningless) extension but still belong to a
/// category.
fn exact_name_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let pairs = [
        ("makefile", "Code"),
        ("gnumakefile", "Code"),
        ("cmakelists.txt", "Code"),
        ("dockerfile", "Code"),
        ("containerfile", "Code"),
        ("pkgbuild", "Code"),
        ("justfile", "Code"),
        ("rakefile", "Code"),
        ("gemfile", "Code"),
        ("vagrantfile", "Code"),
        ("procfile", "Code"),
        ("brewfile", "Code"),
        ("meson.build", "Code"),
        ("build.ninja", "Code"),
        ("sconstruct", "Code"),
        ("sconscript", "Code"),
        ("cargo.lock", "Code"),
        ("docker-compose.yml", "Code"),
        ("docker-compose.yaml", "Code"),
        ("license", "Documents"),
        ("licence", "Documents"),
        ("copying", "Documents"),
        ("readme", "Documents"),
        ("changelog", "Documents"),
        ("authors", "Documents"),
        ("notice", "Documents"),
    ];
    for (name, category) in pairs {
        map.insert(name.to_string(), category.to_string());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> CategorySet {
        resolve(&Config::default())
    }

    #[test]
    fn classifies_common_files() {
        let s = set();
        assert_eq!(s.classify("photo.PNG").as_deref(), Some("Images"));
        assert_eq!(s.classify("archive.tar.gz").as_deref(), Some("Compressed"));
        assert_eq!(s.classify("archive.tar").as_deref(), Some("Compressed"));
        assert_eq!(
            s.classify("setup.exe").as_deref(),
            Some("Applications/Windows")
        );
        assert_eq!(s.classify("video.mkv").as_deref(), Some("Videos"));
        assert_eq!(s.classify("Makefile").as_deref(), Some("Code"));
        assert_eq!(s.classify("mystery.qqq"), None);
    }

    #[test]
    fn multi_dot_wins_over_single_dot() {
        let s = set();
        assert_eq!(s.classify("backup.tar.gz").as_deref(), Some("Compressed"));
        assert_eq!(s.classify("backup.tar.xz").as_deref(), Some("Compressed"));
        // `.zst` alone is compressed too, but `tar.zst` must resolve to the same.
        assert_eq!(s.classify("x.tar.zst").as_deref(), Some("Compressed"));
    }

    #[test]
    fn extension_of_preserves_casing() {
        let s = set();
        assert_eq!(s.extension_of("Photo.PNG").as_deref(), Some("PNG"));
        assert_eq!(s.extension_of("backup.TAR.GZ").as_deref(), Some("TAR.GZ"));
    }

    #[test]
    fn does_not_match_bare_extension_without_dot() {
        let s = set();
        assert_eq!(s.classify("png"), None);
    }

    #[test]
    fn user_overrides_replace_named_category() {
        let mut config = Config::default();
        config
            .categories
            .insert("Images".to_string(), vec!["foo".to_string()]);
        let s = resolve(&config);
        assert_eq!(s.classify("x.foo").as_deref(), Some("Images"));
        // The override replaced the built-in list, so png is no longer Images.
        assert_eq!(s.classify("x.png"), None);
    }

    #[test]
    fn disabled_categories_are_removed() {
        let mut config = Config::default();
        config.disable_categories.push("Images".to_string());
        let s = resolve(&config);
        assert_eq!(s.classify("x.png"), None);
    }

    #[test]
    fn replace_categories_ignores_builtins() {
        let mut config = Config {
            replace_categories: true,
            ..Config::default()
        };
        config
            .categories
            .insert("Mine".to_string(), vec!["xyz".to_string()]);
        let s = resolve(&config);
        assert_eq!(s.classify("a.xyz").as_deref(), Some("Mine"));
        assert_eq!(s.classify("a.png"), None);
    }
}
