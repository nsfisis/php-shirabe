use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_class_map_generator::class_map_generator::ClassMapGenerator;
use shirabe_class_map_generator::file_list::FileList;
use shirabe_php_shim::{bconcat, display_to_bytes, path_from_bytes, path_to_bytes};
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, PartialEq)]
struct ScanOutcome {
    map: IndexMap<BString, BString>,
    ambiguous_classes: IndexMap<BString, Vec<BString>>,
    psr_violations: Vec<BString>,
    errors: Vec<BString>,
}

fn write(path: &BStr, contents: &BStr) {
    let path = path_from_bytes(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn build_tree(root: &BStr) {
    for i in 0..40usize {
        write(
            bconcat!(root, "/classmap/dir", i % 4, "/File", i, ".php").as_ref(),
            bconcat!(
                "<?php\nnamespace Foo;\nclass Unique",
                i,
                " {}\nclass Shared",
                i % 3,
                " {}\ninterface Common {}\n"
            )
            .as_ref(),
        );
    }
    write(
        bconcat!(root, "/classmap/empty.php").as_ref(),
        "  \n".into(),
    );
    write(
        bconcat!(root, "/classmap/readme.txt").as_ref(),
        "class NotScanned {}".into(),
    );

    for i in 0..20usize {
        let namespace = if i % 5 == 0 { 9 } else { i % 2 };
        write(
            bconcat!(root, "/psr4/Sub", i % 2, "/Klass", i, ".php").as_ref(),
            bconcat!(
                "<?php\nnamespace Acme\\Sub",
                namespace,
                ";\nclass Klass",
                i,
                " {}\n"
            )
            .as_ref(),
        );
    }
    std::os::unix::fs::symlink(
        path_from_bytes(&bconcat!(root, "/psr4/Sub0")),
        path_from_bytes(&bconcat!(root, "/psr4/Alias")),
    )
    .unwrap();
}

fn scan(root: &BStr, max_parse_threads: usize) -> ScanOutcome {
    let mut generator = ClassMapGenerator::new_default();
    generator.__set_max_parse_threads(max_parse_threads);
    generator.avoid_duplicate_scans(Some(FileList::new()));

    let mut errors = vec![];
    for (path, autoload_type, namespace) in [
        ("classmap", "classmap", None),
        ("psr4", "psr-4", Some("Acme\\")),
        ("classmap", "classmap", None),
        ("unreadable", "classmap", None),
    ] {
        if let Err(e) = generator.scan_paths(
            bconcat!(root, "/", path),
            None,
            autoload_type.into(),
            namespace.map(BString::from),
            vec![],
        ) {
            errors.push(display_to_bytes(e));
        }
    }

    let class_map = generator.take_class_map();
    ScanOutcome {
        map: class_map.get_map().clone(),
        ambiguous_classes: class_map.get_ambiguous_classes(None).unwrap(),
        psr_violations: class_map.get_psr_violations(),
        errors,
    }
}

#[test]
fn parallel_scan_matches_sequential_scan() {
    let dir = tempfile::tempdir().unwrap();
    let root = BString::from(path_to_bytes(&std::fs::canonicalize(dir.path()).unwrap()));
    let root: &BStr = root.as_ref();
    build_tree(root);
    let mut locked = true;
    for i in 0..4usize {
        let path = bconcat!(root, "/unreadable/Locked", i, ".php");
        write(path.as_ref(), "<?php class Locked {}".into());
        std::fs::set_permissions(path_from_bytes(&path), PermissionsExt::from_mode(0o000)).unwrap();
        // Permissions do not restrict a privileged user.
        locked &= std::fs::read(path_from_bytes(&path)).is_err();
    }

    let sequential = scan(root, 1);
    assert!(!sequential.ambiguous_classes.is_empty());
    assert!(!sequential.psr_violations.is_empty());
    assert_eq!(usize::from(locked), sequential.errors.len());

    for max_parse_threads in [2, 3, 8] {
        assert_eq!(sequential, scan(root, max_parse_threads));
    }
}
