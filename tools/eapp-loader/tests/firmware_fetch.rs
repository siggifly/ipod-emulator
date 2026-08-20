/// The exact choice the wizard makes: the last served, verifiable release of the model's family.
#[test]
#[ignore]
fn the_wizard_can_fetch_apple_firmware_for_a_video() {
    for (label, families) in [("5G", vec![13u32, 20]), ("5.5G", vec![25u32])] {
        let rel = eapp_loader::firmware::CATALOGUE
            .iter()
            .filter(|r| families.contains(&(r.updater_family as u32)))
            .filter(|r| r.served && r.is_verifiable())
            .next_back()
            .unwrap_or_else(|| panic!("{label}: no served, verifiable release"));
        println!("  {label} -> {} ({} bytes)", rel.file, rel.bytes);
        let got = eapp_loader::firmware::download(rel, &eapp_loader::firmware::cache_dir())
            .unwrap_or_else(|e| panic!("{label}: {} failed: {e}", rel.file));
        let n = std::fs::metadata(&got).unwrap().len();
        assert_eq!(n, rel.bytes, "{label}: wrong size");
        // And it has to be usable: inspected, and the right family.
        match eapp_loader::ipsw::inspect(&got) {
            eapp_loader::ipsw::Ipsw::Good(what, fw) => {
                println!("    {what} — firmware {} bytes", fw.len())
            }
            eapp_loader::ipsw::Ipsw::Wrong(w) | eapp_loader::ipsw::Ipsw::Bad(w) => {
                panic!("{label}: fetched but unusable: {w}")
            }
        }
    }
}
