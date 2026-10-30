#[path = "../src/permissions.rs"]
mod permissions;
use permissions::*;
#[test]
fn restricted_profiles_block_legacy_new_and_indirect_mutation_paths() {
    for p in [Profile::ReadOnly, Profile::Isolated] {
        for n in [
            58,
            67,
            68,
            69,
            71,
            72,
            73,
            74,
            75,
            76,
            77,
            80,
            81,
            83,
            89,
            92,
            94,
            100,
            101,
            102,

            0xffff_ffff,
        ] {
            assert!(!p.allows(n, 0), "{p:?} allowed {n}");
        }
        assert!(p.allows(84, 0));
        assert!(!p.allows(84, 1));
        assert!(!p.allows(84, 2));
        for n in [
            1, 4, 20, 50, 51, 52, 53, 54, 55, 56, 59, 60, 61, 62, 63, 64, 65, 66, 70, 85, 86, 87,
            95,
        ] {
            assert!(p.allows(n, 0), "{p:?} denied {n}");
        }
    }
    for n in [57, 78, 79, 82, 88, 90, 91, 93] {
        assert!(Profile::ReadOnly.allows(n, 0));
        assert!(!Profile::Isolated.allows(n, 0));
    }
}
#[test]
fn launch_modes_parse_without_passing_flags_to_application_arguments() {
    for (s, name, args, profile) in [
        (
            "  --read-only   CHECK.ELF  hello world  ",
            "CHECK.ELF",
            "hello world",
            Profile::ReadOnly,
        ),
        (
            "--isolated /APPS/CHECK.ELF",
            "/APPS/CHECK.ELF",
            "",
            Profile::Isolated,
        ),
        ("notepad note.txt", "notepad", "note.txt", Profile::Trusted),
        (
            "/apps/viewer.bin a.bin",
            "/apps/viewer.bin",
            "a.bin",
            Profile::ReadOnly,
        ),
        ("--isolated viewer", "viewer", "", Profile::Isolated),
    ] {
        let r = parse(s).unwrap();
        assert_eq!((r.name, r.arguments, r.profile), (name, args, profile));
    }
    for s in [
        "",
        "--read-only",
        "--isolated",
        "--trusted x",
        "--read-only --isolated x",
    ] {
        assert!(parse(s).is_err(), "{s}");
    }
    assert!(parse(&"x".repeat(106)).is_err());
    assert!(parse(&format!("x {}", "a".repeat(129))).is_err());
}
#[test]
fn restrictions_never_grant_authority_and_profiles_are_queryable() {
    for p in [
        Profile::Trusted,
        Profile::Media,
        Profile::ReadOnly,
        Profile::Isolated,
    ] {
        for q in [
            Profile::Trusted,
            Profile::Media,
            Profile::ReadOnly,
            Profile::Isolated,
        ] {
            assert_eq!(p.restrict(q).mask(), p.mask() & q.mask());
        }
    }
    assert_eq!(
        Profile::Trusted.mask(),
        FILE_READ | FILE_WRITE | LAUNCH | SHARED | SHELL | AUDIO
    );
    assert_eq!(Profile::ReadOnly.mask(), FILE_READ);
    assert_eq!(Profile::Isolated.mask(), 0);
    assert_eq!(Profile::for_name("/APPS/VIEWER.BIN"), Profile::ReadOnly);
    assert_eq!(Profile::for_name("/EVIL/VIEWER.BIN"), Profile::ReadOnly);
    assert_eq!(Profile::for_name("other.bin"), Profile::Trusted);
    assert_eq!(Profile::for_name("/APPS/BIGDEMO.ELF"), Profile::Isolated);
}
#[test]
fn access_labels_are_appended_even_when_title_is_full() {
    for (p, suffix) in [
        (Profile::ReadOnly, b" [RO]" as &[u8]),
        (Profile::Isolated, b" [isolated]"),
    ] {
        let mut title = [b'X'; 48];
        let n = p.title(&mut title, 48);
        assert_eq!(n, 48);
        assert!(title.ends_with(suffix));
        assert_eq!(
            p.label(),
            if p == Profile::ReadOnly {
                "read-only"
            } else {
                "isolated"
            }
        );
        assert!(!p.prefix().is_empty());
    }
    assert_eq!(Profile::Trusted.title(&mut [0; 48], 4), 4);
}

#[test]
fn media_has_audio_and_read_access_but_no_write_or_shell_bypass() {
    let p = Profile::Media;
    assert_eq!(p.mask(), FILE_READ | AUDIO);
    assert_eq!(Profile::for_name("/APPS/PLAYER.ELF"), p);
    for n in [57, 78, 88, 90, 91, 93, 97, 98, 99] {
        assert!(p.allows(n, 0));
    }
    for n in [58, 67, 68, 71, 72, 74, 75, 80, 81, 83, 89, 92, 94] {
        assert!(!p.allows(n, 0));
    }
    assert!(p.allows(100,0));
    assert!(!p.allows(100,1));
    assert!(!p.allows(100,2));
    assert_eq!(parse("--media player").unwrap().profile, p);
    assert_eq!(
        parse("--isolated player").unwrap().profile,
        Profile::Isolated
    );
}

#[test]fn read_only_and_media_can_only_receive_explicit_drops(){for p in [Profile::ReadOnly,Profile::Media]{assert!(!p.allows(103,0));assert!(!p.allows(103,1));for op in [2,3,4]{assert!(p.allows(103,op));}}assert!(!Profile::Isolated.allows(103,4));}
