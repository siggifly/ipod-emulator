/// **The Rockbox fetcher, actually fetching.** Network-touching, so `--ignored`.
#[test]
#[ignore]
fn rockbox_downloads_and_verifies() {
    let dir = std::env::temp_dir().join("ipod-rockbox-fetch-test");
    let _ = std::fs::remove_dir_all(&dir);
    for p in ipod_machine::rockbox::FULL_INSTALL {
        let got =
            ipod_machine::rockbox::download(p, &dir).unwrap_or_else(|e| panic!("{}: {e}", p.file));
        let bytes = std::fs::read(&got).unwrap();
        assert_eq!(bytes.len() as u64, p.bytes, "{}: wrong size", p.file);
        ipod_machine::rockbox::verify(p, &bytes).unwrap_or_else(|e| panic!("{}: {e}", p.file));
        println!("  {} {} bytes, verified", p.file, bytes.len());
    }
}
