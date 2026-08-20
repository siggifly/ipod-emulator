use arm7tdmi::Bus as _;
#[test]
fn a_byte_written_to_the_iram_lock_reads_back() {
    let app = eapp_loader::EApp::none();
    let mut m = eapp_loader::Machine::new(&app, 0x1000_0000, 0x1_0000);
    eapp_loader::map_hardware(&mut m, true);
    const LOCK: u32 = 0x4000_0fac;
    for v in [1u8, 0, 0xff, 0] {
        m.mem.write8(LOCK, v);
        assert_eq!(
            m.mem.read8(LOCK),
            v,
            "byte {v:#04x} at {LOCK:#010x} did not read back"
        );
    }
    // And through a word store, which is how a compiler may spell it.
    m.mem.write32(LOCK & !3, 0);
    assert_eq!(m.mem.read8(LOCK), 0, "a word store did not clear the byte");
}
