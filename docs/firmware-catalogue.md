# Apple's iPod firmware — the catalogue

**Apple still serves 66 of these 71, and every one of the 66 has been downloaded and
hashed here** — so any of them can be verified byte for byte. The remaining 5 return `403`.

They are Apple's own `secure-appldnld.apple.com`, not a mirror. That matters because
[ROADMAP](../ROADMAP.md) has said "Apple no longer serves anything" — true of **boot ROMs**, which
are per-unit and which Apple never served, and not true of firmware.

## Using it

    ipod-boot firmware list [filter]        # everything, or matching a model or filename
    ipod-boot firmware get 25 --dir .       # by UpdaterFamilyID — 25 is the 5.5G
    ipod-boot firmware get iPod_20.1.3.ipsw # or by name

Downloads are **verified**, not trusted: size and SHA-256, both. Nothing is renamed into place until
it verifies, so an interrupted download can never be mistaken for a finished one. A release Apple no
longer serves says exactly that rather than failing with a transport error — that is a fact about
Apple's servers, not about your network.

Supplying your own `.ipsw` still works and always will. This exists so that you do not have to.

## `UpdaterFamilyID` is the key, and `FamilyID` is a trap

The number in the filename is the **`UpdaterFamilyID`**, and it is stable. `FamilyID` is not:
`iPod_13.1.2.1` reports family `13` while `iPod_13.1.3` reports family **`6`**. Apple renumbered —
early firmware set `FamilyID == UpdaterFamilyID`, later firmware assigned real families. Anything
keying on `FamilyID` alone mis-sorts the early releases.

For the iPod with video this is the only thing separating the revisions, all three being `FamilyID 6`:

| `UpdaterFamilyID` | which |
|---|---|
| **13** | 5G, Initial (October 2005) |
| **20** | 5G, Rev A |
| **25** | **5.5G** — Late 2006, "Enhanced" |

## Provenance

Transcribed from [theapplewiki's Firmware/iPod page](https://theapplewiki.com/wiki/Firmware/iPod),
read through the MediaWiki API rather than the HTML, which sits behind a challenge page. `FamilyID`
and `SHA-256` come from the files themselves.

The machine-readable copy is [`tools/eapp-loader/src/firmware.rs`](../tools/eapp-loader/src/firmware.rs).

## The table

| Upd | Fam | Model | Variant | File | Bytes | SHA-256 |
|---:|---:|---|---|---|---:|---|
| 1 | 1 | iPod (1st generation) and iPod (2nd generation) | — | [iPod_1.1.5.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2686.20060912.ipTsW/iPod_1.1.5.ipsw) | 2,092,355 | `0edbb2d512bc84d3…` |
| 2 | 2 | iPod with dock connector (3rd generation) | — | [iPod_2.2.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2687.20060912.IPwdC/iPod_2.2.3.ipsw) | 2,018,057 | `bec081f2bacd4099…` |
| 4 | 4 | iPod with Click Wheel (4th generation) | Initial (2004-07) | [iPod_4.3.1.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2691.20060912.ipDcw/iPod_4.3.1.1.ipsw) | 2,952,848 | `576e05def7800e28…` |
| 10 | 4 | iPod with Click Wheel (4th generation) | Rev A (?) | [iPod_10.3.1.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2692.20060912.pODcW/iPod_10.3.1.1.ipsw) | 2,952,859 | `b526ccf1c99406ee…` |
| 5 | 5 | iPod Photo (iPod with color display) | iPod Photo (2004-10) | [iPod_5.1.2.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2693.20060912.PdwCD/iPod_5.1.2.1.ipsw) | 3,831,893 | `03643928fd4b5d18…` |
| 11 | 5 | iPod Photo (iPod with color display) | iPod with color display (2005-06) | [iPod_11.1.2.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2694.20060912.ipDcD/iPod_11.1.2.1.ipsw) | 3,831,903 | `d8d566b7038b59cb…` |
| 13 | 13 | iPod with video (5th generation) | Initial (2005-10) | [iPod_13.1.2.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2788.20061206.nS1yA/iPod_13.1.2.1.ipsw) | 6,403,368 | `fab6508c546b715e…` |
| 13 | — | iPod with video (5th generation) | Initial (2005-10) | [iPod_13.1.2.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4093.20071126.7u8Jh/iPod_13.1.2.3.ipsw) | — | **gone (403)** |
| 13 | 6 | iPod with video (5th generation) | Initial (2005-10) | [iPod_13.1.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2965.20080313.R45jT/iPod_13.1.3.ipsw) | 6,526,351 | `66aad071f960061d…` |
| 20 | 20 | iPod with video (5th generation) | Rev A (?) | [iPod_20.1.2.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2789.20061206.9IIut/iPod_20.1.2.1.ipsw) | 6,403,352 | `84f193da71cc49d8…` |
| 20 | — | iPod with video (5th generation) | Rev A (?) | [iPod_20.1.2.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4094.20071126.87yhg/iPod_20.1.2.3.ipsw) | — | **gone (403)** |
| 20 | 6 | iPod with video (5th generation) | Rev A (?) | [iPod_20.1.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2966.20080313.2WqrT/iPod_20.1.3.ipsw) | 6,526,335 | `351b19ec7f3eb6e4…` |
| 25 | 25 | iPod with video (5th generation) | Late 2006 ("Enhanced"/"5.5th generation", 2006-09) | [iPod_25.1.2.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2790.20061206.iPr9t/iPod_25.1.2.1.ipsw) | 6,410,116 | `cc647affcca06681…` |
| 25 | 6 | iPod with video (5th generation) | Late 2006 ("Enhanced"/"5.5th generation", 2006-09) | [iPod_25.1.2.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4095.20071126.12bvn/iPod_25.1.2.3.ipsw) | 6,431,336 | `2af7eb2f6d98236c…` |
| 25 | 6 | iPod with video (5th generation) | Late 2006 ("Enhanced"/"5.5th generation", 2006-09) | [iPod_25.1.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2967.20080313.Cnvkg/iPod_25.1.3.ipsw) | 6,533,633 | `840b2480ad5b692c…` |
| 24 | — | iPod classic (6th generation) | Initial (80 GB/"Fat" 160 GB, 2007-09) | [iPod_24.1.0.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3619.20070905.iNq3b/iPod_24.1.0.ipsw) | — | **gone (403)** |
| 24 | 11 | iPod classic (6th generation) | Initial (80 GB/"Fat" 160 GB, 2007-09) | [iPod_24.1.0.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3877.20070914.n9gGb/iPod_24.1.0.1.ipsw) | 59,268,373 | `1f541660b2a79852…` |
| 24 | — | iPod classic (6th generation) | Initial (80 GB/"Fat" 160 GB, 2007-09) | [iPod_24.1.0.2.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3929.20071005.jGu6t/iPod_24.1.0.2.ipsw) | — | **gone (403)** |
| 24 | 11 | iPod classic (6th generation) | Initial (80 GB/"Fat" 160 GB, 2007-09) | [iPod_24.1.0.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3940.20071115.0Iun5/iPod_24.1.0.3.ipsw) | 60,004,790 | `7f0638426c0e44dd…` |
| 24 | 11 | iPod classic (6th generation) | Initial (80 GB/"Fat" 160 GB, 2007-09) | [iPod_24.1.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4010.20080115.Ad4rF/iPod_24.1.1.ipsw) | 60,383,109 | `e041487afb52e115…` |
| 24 | 11 | iPod classic (6th generation) | Initial (80 GB/"Fat" 160 GB, 2007-09) | [iPod_24.1.1.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4275.20080206.PdpOd/iPod_24.1.1.1.ipsw) | 60,383,154 | `7150f4597d64229d…` |
| 24 | 11 | iPod classic (6th generation) | Initial (80 GB/"Fat" 160 GB, 2007-09) | [iPod_24.1.1.2.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4306.20080430.Gtr54/iPod_24.1.1.2.ipsw) | 60,444,886 | `e753abfb11aaeaa6…` |
| 33 | 11 | iPod classic (6th generation) | Rev A (120 GB, 2008-09) | [iPod_33.2.0.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4962.20080909.Aaqs3/iPod_33.2.0.ipsw) | 61,028,317 | `b9b0fed169046372…` |
| 33 | 11 | iPod classic (6th generation) | Rev A (120 GB, 2008-09) | [iPod_33.2.0.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5740.20081111.ZaU7Y/iPod_33.2.0.1.ipsw) | 61,032,316 | `17b16ceb4e906cf5…` |
| 35 | 11 | iPod classic (6th generation) | Rev B ("Thin" 160 GB, 2009-09) | [iPod_35.2.0.2.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-6797.20090909.3uTfE/iPod_35.2.0.2.ipsw) | 61,033,067 | `a12f25067a821850…` |
| 35 | 11 | iPod classic (6th generation) | Rev B ("Thin" 160 GB, 2009-09) | [iPod_35.2.0.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7155.20090925.Ju879/iPod_35.2.0.3.ipsw) | 61,092,301 | `4366d2aaea16110f…` |
| 35 | 11 | iPod classic (6th generation) | Rev B ("Thin" 160 GB, 2009-09) | [iPod_35.2.0.4.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7299.20091217.Bghyt/iPod_35.2.0.4.ipsw) | 61,118,350 | `7ef835c74b08f0bd…` |
| 38 | 11 | iPod classic (6th generation) | Rev C ("Thin" 160 GB, 2012-09) | [iPod_38.2.0.5.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-8552.20121203.Bile3/iPod_38.2.0.5.ipsw) | 63,515,008 | `80f974edea54ae4c…` |
| 3 | 3 | iPod mini (1st generation) | Initial (2004-02) | [iPod_3.1.4.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2688.20060912.iDMni/iPod_3.1.4.1.ipsw) | 2,917,604 | `2fe8d980cb7d7d54…` |
| 6 | 3 | iPod mini (1st generation) | Rev A (?) | [iPod_6.1.4.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2689.20060912.ipDmn/iPod_6.1.4.1.ipsw) | 2,917,611 | `1db1cd67c939d22c…` |
| 7 | 3 | iPod mini (2nd generation) | — | [iPod_7.1.4.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2690.20060912.PdMin/iPod_7.1.4.1.ipsw) | 2,916,362 | `8811a6c77cd478c1…` |
| 14 | 14 | iPod nano (1st generation) | Initial (2005-09) | [iPod_14.1.3.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3190.20070315.p0oj7/iPod_14.1.3.1.ipsw) | 17,699,834 | `ec7f464fac1a6147…` |
| 17 | 17 | iPod nano (1st generation) | Rev A (2006-02) | [iPod_17.1.3.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3191.20070315.BgV6t/iPod_17.1.3.1.ipsw) | 17,699,818 | `34233805640b1c77…` |
| 19 | — | iPod nano (2nd generation) | Initial (2006-09) | [iPod_19.1.1.2.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2920.20070207.n89nY/iPod_19.1.1.2.ipsw) | — | **gone (403)** |
| 19 | 19 | iPod nano (2nd generation) | Initial (2006-09) | [iPod_19.1.1.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3325.20070507.KnB7v/iPod_19.1.1.3.ipsw) | 21,866,626 | `5de87a36f60923df…` |
| 29 | 29 | iPod nano (2nd generation) | Rev A (?) | [iPod_29.1.1.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3326.20070507.0Pm87/iPod_29.1.1.3.ipsw) | 21,866,613 | `a7317c697ee44983…` |
| 26 | 12 | iPod nano (3rd generation) | — | [iPod_26.1.0.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3878.20070914.P0omB/iPod_26.1.0.1.ipsw) | 60,068,899 | `07b2d35193ee6dd3…` |
| 26 | 12 | iPod nano (3rd generation) | — | [iPod_26.1.0.2.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3930.20071005.94rVg/iPod_26.1.0.2.ipsw) | 60,243,827 | `038dc1dd12377e44…` |
| 26 | 12 | iPod nano (3rd generation) | — | [iPod_26.1.0.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3941.20071115.Hngr4/iPod_26.1.0.3.ipsw) | 60,931,586 | `583eaf933a0f3743…` |
| 26 | 12 | iPod nano (3rd generation) | — | [iPod_26.1.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4011.20080115.Gh5yt/iPod_26.1.1.ipsw) | 61,314,505 | `2eaafe827b4cf44f…` |
| 26 | 12 | iPod nano (3rd generation) | — | [iPod_26.1.1.2.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4276.20080430.Gbjt5/iPod_26.1.1.2.ipsw) | 61,369,352 | `c6e1d0393802b455…` |
| 26 | 12 | iPod nano (3rd generation) | — | [iPod_26.1.1.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5164.20080722.hnt3A/iPod_26.1.1.3.ipsw) | 61,371,490 | `6d367d987d76fe0d…` |
| 31 | 15 | iPod nano (4th generation) | — | [iPod_31.1.0.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4637.20080909.vfH8i/iPod_31.1.0.ipsw) | 61,112,027 | `ba4c30cc0266e8e5…` |
| 31 | 15 | iPod nano (4th generation) | — | [iPod_31.1.0.2.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5529.20080915.3ngi4/iPod_31.1.0.2.ipsw) | 60,554,790 | `1ebdb6551d0a43e5…` |
| 31 | 15 | iPod nano (4th generation) | — | [iPod_31.1.0.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5583.20081111.Bhyui/iPod_31.1.0.3.ipsw) | 60,555,725 | `5c53d02517f8fd45…` |
| 31 | 15 | iPod nano (4th generation) | — | [iPod_31.1.0.4.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-5808.20090805.Fvgtr/iPod_31.1.0.4.ipsw) | 60,636,973 | `fc8da31398dc153d…` |
| 1 | 16 | iPod nano (5th generation) | — | [iPod_1.0.1_34A10006.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7165.20090909.AzPKm/iPod_1.0.1_34A10006.ipsw) | 78,809,411 | `1ec3d5ff1b1ae6a7…` |
| 1 | 16 | iPod nano (5th generation) | — | [iPod_1.0.2_34A20020.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-7408.20091109.Kef5t/iPod_1.0.2_34A20020.ipsw) | 90,622,159 | `d86f4e899ee42e94…` |
| 1 | 17 | iPod nano (6th generation) | — | [iPod_1.0_36A00403.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-9054.20100907.VKPt5/iPod_1.0_36A00403.ipsw) | 107,057,019 | `8c8f7c27d2f0c4e8…` |
| 1 | 17 | iPod nano (6th generation) | — | [iPod_1.1_36B00109.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-9358.20110221.9a5fF/iPod_1.1_36B00109.ipsw) | 107,163,190 | `5e2adefb31f1dee9…` |
| 1 | 17 | iPod nano (6th generation) | — | [iPod_1.2_36B10147.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-1920.20111004.CpeEw/iPod_1.2_36B10147.ipsw) | 112,554,060 | `84b27d59f376eaf5…` |
| 1 | 18 | iPod nano (7th generation) | Initial (2012-09) | [iPod_1.0.1_37A10002.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-7730.20121008.NvSxY/iPod_1.0.1_37A10002.ipsw) | 110,043,181 | `98c29168ad78affa…` |
| 1 | 18 | iPod nano (7th generation) | Initial (2012-09) | [iPod_1.0.2_37A20067.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-7265.20121212.WnBg0/iPod_1.0.2_37A20067.ipsw) | 115,299,463 | `98032007fc459875…` |
| 1 | 18 | iPod nano (7th generation) | Initial (2012-09) | [iPod_1.0.2_37A20090.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/091-8245.20130910.CP0D3/iPod_1.0.2_37A20090.ipsw) | 116,222,284 | `4a5dcd99e5b43f71…` |
| 1 | 18 | iPod nano (7th generation) | Initial (2012-09) | [iPod_1.0.3_37A30172.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-9962.20131211.Aqaqa/iPod_1.0.3_37A30172.ipsw) | 117,956,158 | `c34b5cb555a519f4…` |
| 1 | 18 | iPod nano (7th generation) | Initial (2012-09) | [iPod_1.0.4_37A40005.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/031-26260-201500810-D2BC269E-3FBC-11E5-885A-067B3A53DB92/iPod_1.0.4_37A40005.ipsw) | 117,989,754 | `cf26fb17fa37b685…` |
| 1 | 18 | iPod nano (7th generation) | Rev A (2015-07) | [iPod_1.1.1_39A00025.ipsw](https://secure-appldnld.apple.com/ipod/sbml/osx/bundles/031-25237-20150715-D737390E-1C1F-11E5-9274-0ACEBE268FF7/iPod_1.1.1_39A00025.ipsw) | 121,168,156 | `c83b816633f0b85d…` |
| 1 | 18 | iPod nano (7th generation) | Rev A (2015-07) | [iPod_1.1.2_39A10023.ipsw](https://secure-appldnld.apple.com/ipod/sbml/osx/bundles/031-59796-20160525-8E6A5D46-21FF-11E6-89D1-C5D3662719FC/iPod_1.1.2_39A10023.ipsw) | 121,168,449 | `960d570aa073f278…` |
| 128 | 128 | iPod shuffle (1st generation) | 512 MB (2005-01) | [iPod_128.1.1.5.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2975.20061218.in8Uq/iPod_128.1.1.5.ipsw) | 477,186 | `9ee98e0eea88ed1d…` |
| 129 | 128 | iPod shuffle (1st generation) | 1 GB (2006-02) | [iPod_129.1.1.5.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-2953.20061218.yRet5/iPod_129.1.1.5.ipsw) | 477,165 | `5e97a23d3ef4fe77…` |
| 130 | 130 | iPod shuffle (2nd generation) | Initial (2006-11) | [iPod_130.1.0.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3316.20070618.9n1bC/iPod_130.1.0.3.ipsw) | 750,455 | `6d4070ad1062a94b…` |
| 130 | 130 | iPod shuffle (2nd generation) | Initial (2006-11) | [iPod_130.1.0.4.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4376.20080303.Bi6T9/iPod_130.1.0.4.ipsw) | 750,458 | `601272a6533e6f32…` |
| 131 | 130 | iPod shuffle (2nd generation) | Rev A (?) | [iPod_131.1.0.3.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-3317.20070618.nBh6t/iPod_131.1.0.3.ipsw) | 750,441 | `a9ef80e1f0820d99…` |
| 131 | 130 | iPod shuffle (2nd generation) | Rev A (?) | [iPod_131.1.0.4.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4377.20080303.fk3ir/iPod_131.1.0.4.ipsw) | 750,444 | `aabb2542010e94bb…` |
| 133 | 130 | iPod shuffle (2nd generation) | Rev B (?) | [iPod_133.1.0.4.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-4378.20080303.G5T87/iPod_133.1.0.4.ipsw) | 750,444 | `bbdc92047cda2163…` |
| 132 | 132 | iPod shuffle (3rd generation) | — | [iPod_132.1.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-6315.20090526.AQS4R/iPod_132.1.1.ipsw) | 1,919,268 | `25ecd9c0bd908c13…` |
| 134 | 133 | iPod shuffle (4th generation) | Initial (2010-09) | [iPod_134.1.0.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-8479.20100811.Cdf87/iPod_134.1.0.ipsw) | 1,769,717 | `6ae5c2f6731923a7…` |
| 134 | 133 | iPod shuffle (4th generation) | Initial (2010-09) | [iPod_134.1.0.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/061-9471.20101102.NbU7y/iPod_134.1.0.1.ipsw) | 1,811,475 | `99e7cb085185f947…` |
| 135 | 133 | iPod shuffle (4th generation) | Rev A (?) | [iPod_135.1.0.1.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-3900.20120328.Efre4/iPod_135.1.0.1.ipsw) | 1,811,890 | `3cd400211da78177…` |
| 135 | 133 | iPod shuffle (4th generation) | Rev A (?) | [iPod_135.1.0.2.ipsw](https://secure-appldnld.apple.com/iPod/SBML/osx/bundles/041-6857.20121203.D0c4r/iPod_135.1.0.2.ipsw) | 1,813,224 | `efe260482e82d40e…` |
| 136 | 133 | iPod shuffle (4th generation) | Rev B (?) | [iPod_136.1.0.3.ipsw](https://secure-appldnld.apple.com/ipod/sbml/osx/bundles/031-17484-20150205-77E7B2BE-AC97-11E4-9C3C-8BC5C351B811/iPod_136.1.0.3.ipsw) | 1,813,485 | `8d36f4ad0dd825b2…` |

**gone (403)** marks the 5 Apple has stopped serving. Every other release in the same
updater family is still available and will do instead.
