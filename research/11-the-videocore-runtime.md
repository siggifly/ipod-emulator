# The VideoCore runtime API

> **This file is the co-processor side of the display.** Its counterpart is
> [research/12](12-how-retailos-draws.md), which describes the ARM side — paint, show, damage,
> flush, present, and the transport that carries a frame across the bus.


The iPod 5G's display and video work does not happen on the ARM. It happens on a **Broadcom BCM2722
— VideoCore II** hanging off the bus window at `0x30000000`, running its own operating system, with
its own filesystem, its own dynamic linker and its own service registry. RetailOS uploads that
firmware and then talks to it.

[research/10](10-the-resource-image.md) Addendum 28 established the surface: the `.VLL` codec
libraries in `rsrc` are ELF shared objects for `EM_VIDEOCORE`, and their undefined symbols name the
whole runtime API — 183 of them. This file is the **reference document for that API**: what each
group is, what the display model is, how much of it can be cross-checked against public
documentation, and what a host must do, in order, to put one image on the screen.

It exists so that the work of deriving the on-the-wire GENCMD registry format from RetailOS's own
reader has something authoritative to check itself against.

---

## 0. How to read this file — evidence tiers

Every claim below carries one of three markers. **This is the whole discipline of the document**, for
one reason:

> **VideoCore II is not VideoCore IV.**

Everything public about DispmanX and `vc_gencmd` describes the **Raspberry Pi's BCM2835, VideoCore
IV** — a later member of the same lineage, two generations and roughly seven years downstream of
ours. Reading Raspberry Pi documentation and writing a confident document about the BCM2722 is
exactly the failure mode this project has lost the most time to. So:

| tier | meaning |
|---|---|
| **(a) measured** | read out of our own binaries — the `rsrc` volume, the `.VLL` files, `vmcs.bin`, the NOR images. Reproducible with the commands given. |
| **(b) public** | documented outside this project. **Two sub-kinds, and the difference matters**: material about **our** part (Broadcom's `BCM2722` product brief, the Alphamosaic patents, Rockbox's 2009 notes) is direct evidence; material about **VideoCore IV** (the Raspberry Pi `userland` sources) is evidence about a *later chip* and says nothing about VideoCore II by itself. Each **(b)** claim below names its source so the two are never confused. |
| **(c) inference** | our reasoning. A guess. Marked as one. |

Where **(b)** and **(a)** disagree, the disagreement is stated as a finding rather than smoothed
over — see §4.4, where they disagree about the shape of the update transaction and the disagreement
is informative.

**No code was copied.** `userland` is BSD-3-Clause; the project's doctrine is *borrow freely to
learn, never to depend*. What follows documents facts, names and semantics. No source was taken and
none should be.

### Reproducing the measurements

```
ipod-boot rsrc resources/drives/ipod8g-retail.img --list
ipod-boot rsrc resources/drives/ipod8g-retail.img \
    --get RESOUR~1/VIDEOC~1/LIBRARY/MPLAYER.VLL -o MPLAYER.vll
ipod-boot rsrc resources/drives/ipod8g-retail.img \
    --get RESOUR~1/VIDEOC~1/BOOT/VMCS.BIN -o vmcs.bin
```

The `.VLL` files are 32-bit little-endian ELF; plain Python `struct` reads them. Sections of type
`2` (`SYMTAB`) and `11` (`DYNSYM`) hold symbols; those with `st_shndx == 0` are undefined, i.e.
imported. `vmcs.bin` is a flat image that loads at co-processor internal address 0, so **file offset
equals runtime address** and every address in this file is both.

### The lineage — how far apart the two chips actually are

**(b)** VideoCore was designed by **Alphamosaic Ltd** of Cambridge, a spin-out from Cambridge
Consultants, acquired by **Broadcom in September 2004** for ~$120 M in stock and 57 staff. The
generations, and where each landed:

| gen | part | primary evidence |
|---|---|---|
| VideoCore I | VC01 | Broadcom's acquisition release: *"Alphamosaic's first-generation VC01 multimedia co-processor"* |
| VideoCore II | BCM2702 | same release: *"Building on the success of the VC01, Alphamosaic is now sampling VC02"*; product brief 2702-PB01-R |
| **VideoCore II** | **BCM2722 — ours** | **product brief `2722-PB01-R`, dated 10/18/05** — see below |
| VideoCore III | BCM2727 | product brief 2727-PB01-R, 10/15/07: *"power optimized VideoCore® III architecture"*, block diagram *"Dual Vector Processing Units"* |
| VideoCore IV | BCM2763, BCM2835 | Raspberry Pi. Broadcom's VC4 3D guide §1: *"The second generation 3D system in VideoCore® IV is a major step on from the first generation 3D hardware in VideoCore III."* Everything in this document's **(b)** tier. |

**Two generations, and roughly seven years.** Close enough that API names survive — §5.3 shows
GENCMD's did, and §2.10 shows the `dl*` loader's did, 6 names for 6 — and far enough that structural
differences are expected. §4.3 and §4.4 find two.

⚠️ **Correction to a source it would be easy to trust.** The IEEE Computer Society's *Alphamosaic to
Pi to Doom* attributes the first video iPod to the **VC01**. That is wrong, and Broadcom's own brief
below contradicts it. Its architecture descriptions look sound; that attribution is not. Wikipedia's
prose likewise conflates "VC02" with "BCM2722" while its own table separates them.

#### Broadcom's product brief for our exact part — **(b)**, primary

[`BCM2722 Product Brief`, `2722-PB01-R`, 10/18/05](https://www.curiousdog.org/Steve/assets/pdf/BCM2722_PB.pdf).
Read against §§2–6 of this document it stops being a datasheet and becomes a **checklist of things we
inferred from strings**:

| the brief says | what we measured |
|---|---|
| *"Based on the **VideoCore®II** processing engine … software-compatible with the established VideoCore family"* | settles the generation, and states backward compatibility as a design goal — which is the licence for reading VC IV documentation at all |
| *"150-MHz dual-ALU VideoCore II, 128-Kbit instruction and data caches"* | the `vclib_save_high_registers` / VRF checkout idiom (§2.7) is what a dual-ALU vector machine needs |
| *"**1.25-MB SRAM** + 4-MB SDRAM"*, *"10 Mbits on-chip SRAM + 32-Mbit stacked SDRAM"* | `vmcs.bin` is `0x312A0` bytes and loads at 0; its export table sits at `0x2160C`. Both comfortably inside `0x140000`. Consistent with the flat-image-at-zero model |
| *"**64-polyphony MIDI player** and music synthesizer"* | §5.5's `rg_load` / `rg_play` / `/mfs/temp.mid`. Marked **(c)** there — **now confirmed as a shipped feature.** |
| *"**High-quality graphics acceleration for 3D games**"* | `render.bin`'s `ShaderMachine`. §8 called it anomalous; it is a headline product feature |
| *"**DRM-ready** … CPRM encryption for SD card, **AES**, 3DES, RSA"*, block diagram *"VideoCore II **with DRM**"* | `PASSTHROUGH.VLL`'s `aes_init` / `aes_decipher` / `vcaes_decipher` / `stream_crypto_info` |
| *"2-megapixel **JPEG** encode and decode"* | `SLIDESHOW`'s `.jpg` / `.jpeg` handling and its `vc_image_get_u` / `_get_v` YUV path |
| *"Support for **multiple displays**"*, block diagram *"Main LCD"* + *"**Sub-LCD**"* | RetailOS's `'Sub-LCD'` / `'Sub-TV'` strings (Addendum 26 §2) |
| *"**PAL/NTSC** analog TV output as **S-Video or composite**"*, *"Macrovision support"* | the `display_control` parameter table (§5.1): `dac`, `svideo`, `encoding`, `wide`, `mode` |
| *"Appears as memory-mapped peripheral to host … **Dual software channel**"* | the `0x30000000` register bank **and** its `0x30040000` alternate — the `BCM_ALT_*` set in Rockbox's `lcd-video.c` is the second channel, by name |
| *"**Fully programmable** architecture … the full range of multimedia codecs implemented in software … without hardware accelerators"* | why there are six `.VLL` codec libraries on the disk at all |

⚠️ **What is still not public at our generation: the software interface.** Public VideoCore II
material covers the silicon. Nothing found describes `vmcs.bin`'s format, the `.VLL` container, or
VideoCore II's display API. **Our own binaries remain the only source for those.**

But **this document is not the first to look.** Rockbox's `dreamlayers` got there in **2009**:

> *"In `Resources/VideoCore/Boot` you have a `vmcs.bin` which may be the firmware loaded into the BCM
> once the OF boots from disk. In `Resources/VideoCore/Library` you have vll files. **Those are ELF
> DLLs which get loaded into the BCM.** GNU objdump and nm can provide some info (eg. try `nm -D` or
> `objdump -x`). Unfortunately, details of the architecture and instruction encodings aren't known so
> you cannot disassemble those files."*
> — [Rockbox FS#9787](https://web.archive.org/web/20240228164605/https://www.rockbox.org/tracker/task/9787), 2009-02-11

⚠️ **An earlier revision of this file claimed its (a) tier was "the only written description" of these
formats. Retracted.** The ELF identification, the extraction recipe and the `nm -D` route are
seventeen years older than this project. What is new here is the *content* — the 183-symbol API, the
export table, the GENCMD vocabulary, the mapping — and dreamlayers' closing sentence is precisely
Addendum 27 §4's conclusion, reached independently and much earlier.

#### One trap in the ELF machine number

**(b)** The gABI registry assigns `EM_VIDEOCORE = 95`, `EM_VIDEOCORE3 = 137`, `EM_VIDEOCORE5 = 198`.
There is **no `EM_VIDEOCORE4`**, so VideoCore IV firmware reuses **137** — the Raspberry Pi's
`start.elf` (VC IV) and `start4.elf` (VC VI) both carry it.

**So `137` means "some VideoCore VPU", not "VideoCore III".** Our `.VLL` files carry **95**, the
original Alphamosaic-era number, which is what a VideoCore I/II object should carry and is one more
consistency check on the generation.

---

## 1. The runtime, in one page

**(a)** Six libraries, all `e_machine = 0x5f` = `EM_VIDEOCORE` (95), `e_type = 3` (`ET_DYN`),
`e_flags = 0x2`, `.comment` = **`MetaWare Universal Linker v5.3h`**:

| library | bytes | imports | exports | what it is |
|---|---|---|---|---|
| `AACDEC.VLL` | 52 664 | **5** | 120 | AAC audio decoder |
| `H264DEC.VLL` | 106 960 | 22 | 360 | H.264 video decoder |
| `MPG4DEC.VLL` | 147 232 | 22 | 358 | MPEG-4 video decoder |
| `MPLAYER.VLL` | 51 620 | **125** | 68 | the media player — the orchestrator |
| `PASSTH~1.VLL` | 6 528 | 12 | 11 | "passthrough" — and it is the **AES** unit (`aes_init`, `aes_decipher`, `aes_sbox`, `stream_crypto_info`) |
| `SLIDES~1.VLL` | 47 940 | **107** | 53 | the photo slideshow |
| | | **183 distinct** | | |

**The spread is the finding.** `AACDEC` imports five symbols — `malloc_256bit`, `free_256bit`,
`memset`, `vclib_get_tmp_buf`, `vclib_memcpy`. It is a pure function of its input buffer. `MPLAYER`
imports 125 and `SLIDESHOW` 107, including every `dispman_*`, every `gencmd_*` and every
`hostreq_*`. **The codecs are leaves; the players are the applications.** A codec cannot draw, cannot
register a command, cannot call the host. Only the two player libraries can, and they are the only
two that link against Nucleus PLUS task and semaphore services as well.

That layering is worth stating plainly because it bounds what a host has to emulate to make a codec
run versus to make a *screen* work: `AACDEC` needs **five** allocator and memcpy primitives and
nothing else; the screen needs the display stack, the registry and the host-callback channel.

### The export table — `vmcs.bin` names its own API

**(a)** ⚠️ **This corrects [research/10](10-the-resource-image.md) Addendum 28 §3.** That section
recorded, as a limit, that *"none of these names appear in `vmcs.bin` … confirmed by direct search
for `gencmd_register`, `dispman_object_create`, `vmcs_queue_message`, `hostreq_rendertext`: **zero
hits each**"*, and concluded the linkage must be resolved by ordinal or hash.

**All 183 names are in `vmcs.bin`, in an export table, and every one of them carries its address.**

The likely cause of the error is the ambiguity Addendum 27 §1 itself flagged and this document must
therefore repeat every time: **there are two different `vmcs.bin`.**

| copy | bytes | where | strings |
|---|---|---|---|
| **`rsrc`** — the one **RetailOS** uploads | 201 376 | `RESOUR~1/VIDEOC~1/BOOT/VMCS.BIN` | `dispman`, `gencmd`, `hostreq`, `NUCLEUS`, `error_msg`, the export table — all present |
| **NOR** — the one the **bootloader** uploads | 101 728 | `resources/derived/fw/flsh/vmcs.bin` | `dispman` **zero**, `gencmd` **zero**, `hostreq` **zero**, `error_msg` **zero**. Carries `M25 Diagnostics`, `Audio SCBs`, `Present SCB`, `Interrupt VRF`, `vc_image_malloc` |

A search of the NOR copy returns exactly the zero hits Addendum 28 reported. The two are different
builds of different scope; anything measured against "the" vmcs must say which.

**The table, measured:**

```
base            0x2160C     (preceded by a run of zero words)
record          8 bytes:  u32 code_addr,  u32 name_ptr
count           183        exactly the union of the six libraries' imports
terminator      an all-zero record at 0x21BC4
name strings    two NUL-terminated runs, 0x1FCBC..0x1FDAA and 0x202A0..0x20DC4
                (merged into one sorted sequence by the table, not two)
```

The field order — address first, name second — is **not** obvious from a byte dump, because the
opposite reading is equally self-consistent. Two independent tests settle it:

1. **Module clustering.** Under `(addr, name)`, 18 of 19 prefix groups occupy a *perfectly
   contiguous* address range with zero foreign symbols interleaved — `dispman_` is 0x6a58..0x774c
   and nothing else lives there, `gencmd_` is 0xb958..0xc49a, `str*` is 0x12dd4..0x1313e. Under
   `(name, addr)` four groups shatter: `dma_` acquires 54 foreign symbols, `str*` 102, `EVC_` 64.
   Totals: **155 vs 375**.
2. **Nucleus monotonicity.** The 25 Nucleus PLUS symbols are *strictly ascending* in address under
   `(addr, name)` — all 25 of them, 0x91a to 0x2810, one contiguous kernel linked at the bottom of
   the image. Under `(name, addr)` the longest ascending run is 16.

The one outlier under the winning reading is `vclib_dcache_flush` at **0x25a**, far below the rest of
`vclib_` at 0x1d0xx — which is exactly where a low-level cache primitive belongs, beside the startup
code at 0x200.

**The names are sorted, and the collation is measurable.** Zero inversions under *lowercase, with
`_` collating below alphanumerics*; one inversion under plain lowercase (`dma_transfer_setup_memcpy_uncached`
before `dma_transfer_setup_memcpy2d_uncached`); four under raw ASCII. So the table is
**binary-searchable by name**, which is what `dlsym` and `vll_cache_sym` want — and **(c)** that is
almost certainly what they do with it.

The practical consequence for this project is large and immediate: **every function named in this
document has a known address inside a 201 376-byte image we hold.** A `dispman_object_create` call
is a branch to `0x713a`. That is a breakpoint, not a mystery.

---

## 2. The 183-symbol runtime API

Addresses are file offsets in the **`rsrc`** `vmcs.bin`, which equal co-processor addresses. The
"used by" column abbreviates: **A** = `AACDEC`, **H** = `H264DEC`, **M4** = `MPG4DEC`,
**MP** = `MPLAYER`, **P** = `PASSTHROUGH`, **S** = `SLIDESHOW`.

Where a count appears it is the number of **relocations** against that symbol — i.e. call sites, not
merely "is imported". A symbol referenced 33 times is load-bearing in a way one referenced once is
not.

### 2.1 `dispman_*` — the display stack (12)

| symbol | addr | used by | call sites |
|---|---|---|---|
| `dispman_display` | `0x006a58` | MP,S | 6 |
| `dispman_object_create` | `0x00713a` | MP,S | 2 |
| `dispman_object_add` | `0x006fda` | MP,S | 2 |
| `dispman_object_remove` | `0x007212` | MP,S | 3 |
| `dispman_object_delete` | `0x00717e` | MP | 1 |
| `dispman_resource_create` | `0x00752e` | MP,S | 3 |
| `dispman_resource_delete` | `0x007566` | MP,S | 3 |
| `dispman_update_create` | `0x00760c` | MP | 1 |
| `dispman_update_start` | `0x00774c` | MP,S | 3 |
| `dispman_update_end` | `0x00762a` | MP,S | 3 |
| `dispman_update_delete` | `0x00761e` | MP | 1 |
| `dispman_rect_set` | `0x007462` | MP,S | 7 |

### 2.2 `gencmd_*` — the service registry (6)

| symbol | addr | used by | call sites |
|---|---|---|---|
| `gencmd_register` | `0x00c49a` | MP,S | **33** (MP 24, S 9) |
| `gencmd_deregister` | `0x00ba40` | MP,S | **33** (MP 24, S 9) |
| `gencmd_execute` | `0x00bbee` | MP,S | 2 |
| `gencmd_param` | `0x00c362` | MP,S | 15 |
| `gencmd_decode_int` | `0x00b970` | MP,S | 21 |
| `gencmd_decode_fourcc` | `0x00b958` | MP | 1 |

### 2.3 `hostreq_*` — the co-processor calls the ARM (3)

| symbol | addr | used by | call sites |
|---|---|---|---|
| `hostreq_notify` | `0x00cf52` | MP | 17 |
| `hostreq_read_iphoto_block` | `0x00cf80` | S | 2 |
| `hostreq_rendertext` | `0x00d05e` | MP | 1 |

### 2.4 `vmcs_*` — the system service (10)

| symbol | addr | used by | call sites |
|---|---|---|---|
| `vmcs_queue_message` | `0x01dc1e` | MP,P,S | 28 |
| `vmcs_get_cookie` | `0x01daba` | MP,S | 39 |
| `vmcs_set_cookie` | `0x01dd0c` | MP,S | 2 |
| `vmcs_create_task` | `0x01d7a6` | MP | 1 |
| `vmcs_end_task` | `0x01da4a` | MP | 1 |
| `vmcs_create_timer` | `0x01d912` | MP,S | 3 |
| `vmcs_schedule_timer` | `0x01dcd6` | MP | 2 |
| `vmcs_end_timer` | `0x01daa4` | MP,S | 3 |
| `vmcs_display` | `0x01d938` | MP | 1 |
| `vmcs_clear_displays` | `0x01d774` | MP | 1 |

`vmcs_get_cookie` at 39 call sites is the most-used service call in the whole runtime after the
allocator. **(c)** A "cookie" here is almost certainly the per-instance opaque token a registered
service stashes and retrieves — the moral equivalent of a `void *user_data`.

### 2.5 `vc_image_*` — image surfaces (21)

| symbol | addr | used by | call sites |
|---|---|---|---|
| `vc_image_set_type` | `0x0188f2` | MP,S | 12 |
| `vc_image_set_dimensions` | `0x0187e6` | MP,S | 13 |
| `vc_image_set_pitch` | `0x0188da` | MP,S | 15 |
| `vc_image_set_image_data` | `0x0187ec` | MP,S | 7 |
| `vc_image_set_image_data_yuv` | `0x01882c` | S | 14 |
| `vc_image_required_size` | `0x017c48` | S | 27 |
| `vc_image_prioritymalloc` | `0x017846` | S | 6 |
| `vc_image_free` | `0x01701c` | S | 9 |
| `vc_image_blt` | `0x016542` | S | 1 |
| `vc_image_copy` | `0x016c78` | S | 6 |
| `vc_image_convert` | `0x016800` | S | 1 |
| `vc_image_reshape` | `0x017c76` | S | 10 |
| `vc_image_resize` | `0x017cb6` | S | 1 |
| `vc_image_resize_yuv` | `0x018460` | S | 1 |
| `vc_image_transpose` | `0x018b60` | S | 4 |
| `vc_image_hflip` | `0x017050` | S | 1 |
| `vc_image_hflip_in_place` | `0x0170ac` | S | 2 |
| `vc_image_combine_transforms` | `0x0167b6` | S | 2 |
| `vc_image_get_u` | `0x017048` | S | 19 |
| `vc_image_get_v` | `0x01704c` | S | 19 |
| `vc_image_const_0_15` | `0x0167f0` | S | 1 |

The set/`required_size`/`prioritymalloc`/`free` shape says a `VC_IMAGE_T` is a **descriptor struct
the caller owns** — type, dimensions, pitch, data pointer are each set by a separate call — and the
pixel buffer is allocated separately at a size the library computes. `get_u`/`get_v` returning the
chroma planes of a YUV image, used 19 times each, is the slideshow's JPEG path.

### 2.6 `dma_*` — transfer queues (14)

`dma_get_transfer_queue`, `dma_transfer_queue_post`, `dma_transfer_queue_release`,
`dma_transfer_chain`, `dma_transfer_wait`, `dma_transfer_has_finished`, `dma_transfer_set_callback`,
`dma_transfer_setup_memcpy`, `dma_transfer_setup_memcpy_uncached`,
`dma_transfer_setup_memcpy2d_uncached`, `dma_memcpy`, `dma_memcpy2d_uncached`,
`dma_subchan_request`, `dma_subchan_free` — `0x007af6`..`0x00841a`.

The idiom is explicit: **obtain a queue, set up a transfer, chain it, post it, wait, release.** The
decoders (H, M4) live on this; the slideshow uses the 2D variants to move image stripes.

### 2.7 `vclib_*` — the low-level library (13)

`vclib_obtain_VRF` / `vclib_release_VRF` / `vclib_check_VRF` (43/39/1 call sites),
`vclib_memcpy`, `vclib_memset`, `vclib_memset2`, `vclib_dcache_flush`,
`vclib_save_high_registers` / `vclib_restore_high_registers`, `vclib_get_tmp_buf`,
`vclib_timer_init` / `vclib_timer_reset` / `vclib_timer_cancel`.

**VRF** appears in five of the six libraries and is obtained/released around every heavy operation.
**(c)** It is a scarce hardware resource — a vector register file or scratchpad — that the SIMD units
share and must be checked out. `vclib_save_high_registers`/`restore` being imported only by `MPG4DEC`
supports that: a decoder that needs more of it than the ABI grants.

### 2.8 Nucleus PLUS (25)

⚠️ Addendum 28's table said 21. Recount from the same data: **25**.

| family | n | symbols |
|---|---|---|
| `TCC_` | 9 | `Create_Task` `Delete_Task` `Terminate_Task` `Reset_Task` `Task_Sleep` `Relinquish` `Resume_Service` `Current_Task_Pointer` `Delete_HISR` |
| `TCT_` | 3 | `Activate_HISR` `Control_Interrupts` `Local_Control_Interrupts` |
| `TCS_` | 2 | `Change_Priority` `Change_Preemption` |
| `TCF_` | 1 | `Task_Information` |
| `SMC_` | 4 | `Create_Semaphore` `Delete_Semaphore` `Obtain_Semaphore` `Release_Semaphore` |
| `SMS_` | 1 | `Reset_Semaphore` |
| `EVC_` | 4 | `Create_Event_Group` `Delete_Event_Group` `Set_Events` `Retrieve_Events` |
| `TMT_` | 1 | `Retrieve_Clock` |

They occupy `0x91a`..`0x2810` — one contiguous kernel at the bottom of the image, linked first. This
confirms Addendum 27's Nucleus PLUS identification by a completely independent route: that section
read magic constants out of a byte dump, this one reads the kernel's public entry points out of a
linker's export table.

### 2.9 Services and C library (the remaining 79)

12 + 6 + 3 + 10 + 21 + 14 + 13 + 25 = 104 above; 79 here; 183 in total.

| group | n | what |
|---|---|---|
| `audio_*` | 8 | `playopen` `play` `playstart` `playpause` `playclose` `playgetclock` `playgetendclock` `playgetlatency` |
| `univ_*` | 9 | board-level: audio record position/present/pause/restart, `setleds`/`clearleds`, `getmicrosecs`, `getstcfreq`, `create_shared_stack_hisr` |
| `filesys_*` | 4 | `register` `deregister` `remove` `size` — a **pluggable filesystem**, see §6.3 |
| `powerman_*` | 3 | `register_user` `deregister_user` `set_user_request` |
| `pds_*` | 5 | `apps_and_handler_task_id`, `m_mp_{get_frame,seek,selectplay,stop}` — imported **only** by `PASSTHROUGH` |
| `vll_*` | 4 | `cache_open` `cache_close` `cache_done` `cache_sym` — the loadable-library cache |
| `dl*` | 6 | `dlopen` `dlsym` `dlclose` `dlerror` `dlcheck` `dldone` — a POSIX-shaped dynamic linker |
| `malloc*` | 4 | `malloc`, `malloc_priority` (53 call sites — the real allocator), `malloc_256bit`, `malloc_external` |
| C library | 36 | `memcpy` `memset` `strcmp` `sprintf` `sscanf` `qsort` `fopen` `fread` `fseek` `fwrite` `fclose` … plus `__ldivs` `__lmul` `__modu` `__rem` (no hardware divide) |

Five names swept into that last bucket are not C library at all and matter:
`mp_disp_compute_rects` (`0x00e030`, used by **both** players — a shared source/destination rectangle
calculator, which is why both a video player and a slideshow need it), `resource_local_funcs`
(`0x010cca`), `execute_command_local` (`0x009554`, `MPLAYER` only — see §5.5),
`audioplus_register_callback` (`0x0049e6`) and `hflip_block` (`0x00cb84`, `SLIDESHOW`).

One oddity, recorded rather than explained: `vsprintf` is the last record in the export table and its
address is `0x01dd74`, the highest in the image. **(c)** It is probably the tail of the print module;
nothing suggests it is special.

### 2.10 The `.VLL` loader contract

**(a)** The runtime loads these libraries itself, and both ends of that contract are measurable.

The libraries' `.dynamic` sections are minimal and identical in shape across all six:

```
DT_STRTAB  DT_STRSZ  DT_SYMTAB  DT_SYMENT=0x10  DT_HASH  DT_RELA  DT_RELASZ  DT_RELAENT=0xc
DT_TEXTREL  DT_NULL
```

**No `DT_NEEDED`, no `DT_SONAME`, no `DT_INIT`/`DT_FINI`.** A `.VLL` declares no dependencies and no
constructors: it links against the firmware's export table and nothing else, and it has no
initialiser to run. Three program headers — two `PT_LOAD` (RX at vaddr `0xfff60`, RW) and one
`PT_DYNAMIC` — with `.text` linked at `0x100000`.

`vmcs.bin`'s loader validates exactly those fields, in its own words:

```
%s is not a DLL.                    %s has the wrong endian.
Can't read ELF header of %s         %s is not a valid ELF file.
%s does not contain %s code.        %s: unable to read section headers.
%s: can't find mapping of .dynamic segment.
%s: section header size appears to be garbage.
%s: can't find symbol, string, or hash table in DYNAMIC table.
Bad name in symbol #%d of %s        %s: symbol %s is not defined.
%s: fixup #%d (@0x%x) has unrecognized type 0x%x
```

`"symbol, string, or hash table"` is `DT_SYMTAB` / `DT_STRTAB` / `DT_HASH`, named in that order — the
loader's requirements and the libraries' `.dynamic` agree exactly.

**Relocations.** `Elf32_Rela`, 12 bytes. Six types appear: `0x01`, `0x0b`, `0x10`, `0x11`, `0x12`,
`0x17`. Split by whether the target symbol is defined or imported:

| type | against defined symbols | against imports |
|---|---|---|
| `0x17` (23) | 0 | **1445 — all of them** |
| `0x0b` (11) | 1075 | 1 (`SLIDESHOW`) |
| `0x12` (18) | 473 | 0 |
| `0x01` (1) | 291 | 0 |
| `0x11` (17) | 16 | 0 |
| `0x10` (16) | 2 | 0 |

**Type `0x17` is the external-symbol fixup**, used exclusively for imports in all six libraries (one
`SLIDESHOW` outlier aside). Anyone writing a `.VLL` loader needs to resolve `0x17` against the export
table at `0x2160C` and can apply the rest locally.

#### The other side of this contract is public Broadcom code

**(b)** [`middleware/dlloader/dlfcn.h`](https://raw.githubusercontent.com/raspberrypi/userland/master/middleware/dlloader/dlfcn.h),
© 2012 Broadcom Europe Ltd, BSD-licensed. It declares:

```
dlloader_init  dlopen  dlopen_at  dlopen_pmm  dlsym  dlclose  dlerror  dldone  dlcheck
dlshared_vll_load(vll_name, symbols, pmm_alloc, pmm_free, pmm_priv, vll_init_required)
dlshared_vll_init_done   dlshared_get_vll_symbol   dlshared_vll_closing   dlshared_vll_unload
dlgetsegmentsizes(vll, nrows, segdata)
struct dlsegmentsizedata { int size; int align; enum dlpoolflags flags; }
enum dlpoolflags { DL_POOLFLAGS_EXECUTABLE, _WRITABLE, _TEMPORARY, _DEBUGINFO }
#ifdef FOR_VMCS  ->  dl_set_vll_dir(const char *dir_name)
```

**Six of those names are our runtime's six, exactly:** `dlopen` · `dlsym` · `dlclose` · `dlerror` ·
`dldone` · `dlcheck` (§2.9). Not five, not seven — the same six, including the two nobody else has
(`dldone`, `dlcheck`). Seven years and two generations apart. Together with GENCMD (§5.3) that is the
**second** interface in this document to survive the gap intact, and the more surprising of the two,
because a loader is the kind of thing that normally gets rewritten.

Four more measured/documented pairs, each independent:

| ours **(a)** | theirs **(b)** |
|---|---|
| `set_vll_dir` is a registered GENCMD in `vmcs.bin` (§5.1), and `/mfs/vlls/` is a literal at `0x295a0` | `dl_set_vll_dir` exists **only** under `#ifdef FOR_VMCS`; `bcm_host.c` issues the gencmd `set_vll_dir /sd/vlls`. **Same command name, same purpose, same subsystem gate.** |
| `MPLAYER` builds `%s_vis.vll`; `SLIDESHOW` opens `transitions.vll` | `OMX_Broadcom.h` documents `_vis.vll` / `_tran.vll` suffix conventions |
the `get_*_func_table` pattern below — measured, and it is a **pair** per library | a third-party VideoCore III RE effort reports each VLL exporting a `get_<type>_func_table` / `get_<specific>_func_table` pair pointing at one vtable, resolved by `dlsym`. **Weak source — and our exports match its description exactly**, two generations earlier |
| `vll_cache_open` / `_sym` / `_close` / `_done` (§2.9) | a VCOS comment: *"we intentionally call `dldone` here to encourage VLLs with only a single entry-point!"* |

**The plugin ABI, measured (a).** Every `.VLL` that provides something exports **two** entry points —
a generic name for the *interface* and a specific one for the *implementation* — and imports none:

| library | generic | specific |
|---|---|---|
| `AACDEC` | `get_aud_dec_func_table` | `get_aac_dec_func_table` |
| `H264DEC` | `get_vid_dec_func_table` | `get_h264_dec_func_table` |
| `MPG4DEC` | `get_vid_dec_func_table` | `get_mpeg4_dec_func_table` |
| `SLIDESHOW` | `get_tran_func_table` | `get_tran_cube_` / `_fade_` / `_jumbo_` / `_page_func_table` |
| `MPLAYER` | *(none — it is the consumer)* | carries `get_vid_dec_func_table`, `get_aud_dec_func_table`, `get_vis_func_table` in `.text` as **`dlsym` argument strings** |

So the whole codec system is: `MPLAYER` picks a file by extension (`.h264` → `stream`, `.mp4v`,
`.amr`, `.qcp` → `qcelp`, …), `dlopen`s the matching `.VLL`, `dlsym`s the **generic** name, and gets a
vtable. `H264DEC` and `MPG4DEC` both exporting `get_vid_dec_func_table` is the point — that is
polymorphism by symbol name, and it is why `MPLAYER` imports `dlopen`/`dlsym` at all.

And one link that matters for §3.5 and §5: `vchi.h` carries
`const char * const vll_filename; /* VLL to load to start this service */`. **(b)** On VideoCore IV, a
VCHI service is *started by loading a VLL*. **(c)** If that held at our generation it would tie the
`dl*` loader, the `0x1f0` channel directory and the GENCMD registry into one mechanism — a service is
a loaded library that registers command names on a channel. Nothing measured supports it on VideoCore
II, and it is exactly the kind of tidy story that has been wrong before in this project.

**What "VLL" expands to is not attested anywhere.** Broadcom's own headers use the bare token. The
common gloss "VideoCore Loadable Library" has no source. **VMCS** does have one: Steve Allen, who ran
the group that wrote it, gives *"VideoCore Multimedia **Coprocessor Software**"* — not "Subsystem",
which appears to be folklore.

---

## 3. The DispmanX model

**(b) — this entire section is VideoCore IV documentation.** Read it as the *later* generation's
version of the interface, then read §4, which is where ours differs.

Sources, all primary, all from the Raspberry Pi `userland` tree:
[`interface/vmcs_host/vc_dispmanx.h`](https://raw.githubusercontent.com/raspberrypi/userland/master/interface/vmcs_host/vc_dispmanx.h) ·
[`interface/vmcs_host/vc_dispmanx_types.h`](https://raw.githubusercontent.com/raspberrypi/userland/master/interface/vmcs_host/vc_dispmanx_types.h) ·
[`interface/vmcs_host/vc_vchi_gencmd.h`](https://raw.githubusercontent.com/raspberrypi/userland/master/interface/vmcs_host/vc_vchi_gencmd.h) ·
[`host_applications/linux/apps/hello_pi/hello_dispmanx/dispmanx.c`](https://raw.githubusercontent.com/raspberrypi/userland/master/host_applications/linux/apps/hello_pi/hello_dispmanx/dispmanx.c).

DispmanX is Broadcom's display compositor interface. It has exactly four object kinds, and the whole
API is those four plus a rectangle helper. **All four handle types are `uint32_t`, and
`DISPMANX_NO_HANDLE` is `0`** — so every one of them is an opaque token, and zero is the failure
value.

### 3.1 Display

A **display** is a physical output. It is opened by numeric ID —
`vc_dispmanx_display_open(uint32_t device)` — and yields a handle. The IDs are named constants:

```
DISPMANX_ID_MAIN_LCD  0      DISPMANX_ID_SDTV       3      DISPMANX_ID_FORCE_OTHER  6
DISPMANX_ID_AUX_LCD   1      DISPMANX_ID_FORCE_LCD  4      DISPMANX_ID_HDMI1        7
DISPMANX_ID_HDMI0     2      DISPMANX_ID_FORCE_TV   5      DISPMANX_ID_FORCE_TV2    8
```

`vc_dispmanx_display_get_info` fills a `DISPMANX_MODEINFO_T` — width, height, frame rate, aspect
ratio. There are also `display_open_mode`, `display_open_offscreen` (render into a resource instead
of a panel), `display_reconfigure`, `display_set_destination`, `display_set_background` and
`display_close`.

A display owns nothing. It is a compositing target.

### 3.2 Resource

A **resource** is off-CPU pixel memory owned by the co-processor.
`vc_dispmanx_resource_create(VC_IMAGE_TYPE_T type, uint32_t width, uint32_t height, uint32_t *native_image_handle)`
takes a pixel *type* and *dimensions* and hands back a handle. The host does not get a pointer; it
cannot write to it directly. It pushes pixels in with
`vc_dispmanx_resource_write_data(res, src_type, src_pitch, src_address, rect)` — source pointer,
source pitch, destination rectangle — and can read them back with `resource_read_data`. There is also
`resource_set_palette` for 4/8 bpp, and `resource_get_image_handle`.

Two things about resources are load-bearing and both are places a host implementation goes wrong:

- **Rows are padded to an aligned pitch, not `width × bpp`.** The `hello_dispmanx` sample computes
  its pitch as `ALIGN_UP(width × bytes_per_pixel, 32)` — the macro is `(x + (y)-1) & ~((y)-1)` and
  the alignment is **32 bytes**. For its 200-pixel-wide 16 bpp image that is
  `ALIGN_UP(400, 32) = 416`, and the same 416 is passed to `resource_write_data`. Passing an
  unpadded pitch produces a sheared image — the classic first-attempt bug.
- **A resource is not a surface on a display.** Creating one puts nothing on screen. It is a texture
  waiting for an element to reference it.

### 3.3 Element

An **element** is the binding, and it is where all the compositing parameters live.
`vc_dispmanx_element_add` takes ten arguments:

| parameter | meaning |
|---|---|
| `update` | the transaction this add belongs to |
| `display` | which output it appears on |
| `layer` (`int32_t`) | Z order. Higher layers composite over lower ones. |
| `dest_rect` | where on the display it lands, **in display pixels** |
| `src` | the resource handle — which pixels |
| `src_rect` | which part of the resource is used, **in 16.16 fixed point**. The sample builds it as `0, 0, width << 16, height << 16`; this is how arbitrary scaling is expressed, since a src_rect smaller than the dest_rect scales up. *(The `<< 16` convention is visible in the sample, not stated in the header.)* |
| `protection` | `DISPMANX_PROTECTION_T`, a `uint32_t` |
| `alpha` | `VC_DISPMANX_ALPHA_T` — mode (`FROM_SOURCE`, `FIXED_ALL_PIXELS`, `FIXED_NON_ZERO`, `FIXED_EXCEED_0X07`) plus `PREMULT` / `MIX` / `DISCARD_LOWER_LAYERS` flags |
| `clamp` | `DISPMANX_CLAMP_T` |
| `transform` | rotation in bits 0–1 (`NO_ROTATE` 0, `ROTATE_90` 1, `ROTATE_180` 2, `ROTATE_270` 3), plus `FLIP_HRIZ` (1<<16) and `FLIP_VERT` (1<<17) |

Once added, an element can be mutated inside a later update: `element_change_source`,
`element_change_layer`, `element_change_attributes`, `element_modified` (signal that the underlying
bitmap changed), `element_remove`.

An element is what "one image on screen" *is*. Everything else is bookkeeping around it.

### 3.4 Update — the transaction bracket

This is the part of DispmanX that is most often misread as ceremony, and is not.

Display changes are **transactional**. You do not add an element and see it. You:

1. **start an update**, which returns an update handle and takes a *priority*;
2. perform any number of add / remove / change-attributes / change-source operations, each tagged
   with that update handle — none of which have any visible effect;
3. **submit the update**, at which point every queued operation becomes visible **together, on one
   vertical blank**.

The guarantee is atomicity against the scan-out. Without it, moving two elements produces a frame
where one has moved and the other has not — visible tearing between *objects*, not merely within one.
With it, a composition change is all-or-nothing.

`vc_dispmanx_update_start(int32_t priority)` returns a `DISPMANX_UPDATE_HANDLE_T`, or
`DISPMANX_NO_HANDLE` (= 0) on error. There are two submit forms:
`vc_dispmanx_update_submit(update, cb_func, cb_arg)` is asynchronous and takes a completion callback;
`vc_dispmanx_update_submit_sync(update)` blocks until the update has been applied. The asynchronous
form is what a video player wants (it must not stall its decode loop); the synchronous form is what a
"draw one thing and stop" host wants, because it removes the question of when it is safe to reuse or
free the resource. The sample uses the synchronous form.

**A resource must not be deleted while an element still references it, and an element must be removed
inside an update like any other change.** Teardown is therefore also transactional.

### 3.5 The transport, and why it matters here

**(b)** DispmanX on VideoCore IV is a **VCHI service**. The preferred initialiser is
`vc_vchi_dispmanx_init(VCHI_INSTANCE_T instance, VCHI_CONNECTION_T **connections, uint32_t num_connections)`,
and `vc_vchi_gencmd_init` has the identical shape. So on the later part, both the display API and the
command registry are clients of one host↔VideoCore channel abstraction, and a host opens *connections*
before it opens a display.

**(c)** That is the strongest available hint about what RetailOS is doing when it reads a 16-byte
block at co-processor `0x1f0`, follows a pointer to an 8-entry `u16` table, and matches 16-byte
descriptors by numeric tag 1 / 2 / 7 ([research/10](10-the-resource-image.md) Addendum 26 §4). A
directory of numbered channels, discovered before any display call, is structurally what a
VCHI-shaped connection table looks like. **It is a shape match and nothing more** — no public
material ties VCHI to VideoCore II, and the tag values are ours, not Broadcom's.

---

## 4. Mapping our names to the documented API

Rows are marked:

- **confirmed** — the correspondence is forced by more than the name: call-site structure, argument
  count where visible, or the role the symbol plays in a measured sequence.
- **probable** — name and position match and nothing contradicts it, but only the name is evidence.
- **unknown** — we are guessing, or the documented API has no counterpart.

| ours (VideoCore II, **(a)**) | documented (VideoCore IV, **(b)**) | verdict | note |
|---|---|---|---|
| `dispman_display` | `vc_dispmanx_display_open` / `_open_mode` / `_open_offscreen` / `_reconfigure` / `_set_destination` / `_set_background` / `_get_info` / `_close` | **probable** | ours is **one** symbol used 6 times where VC IV has **eight**. Either it is multiplexed on an opcode argument, or the II-era display is not an object with a lifetime at all — just a name a call takes. |
| `dispman_resource_create` | `vc_dispmanx_resource_create` | **confirmed** | same name, same position in the sequence, paired 1:1 with a delete. |
| `dispman_resource_delete` | `vc_dispmanx_resource_delete` | **confirmed** | |
| *(none)* | `vc_dispmanx_resource_write_data` | **unknown** ⚠ | **We have no resource-write symbol at all.** See §4.2 — this is the most important gap in the table. |
| `dispman_object_create` | *(no counterpart)* | **unknown** | VC IV has no element *create*; `element_add` both creates and binds. Ours splits them. |
| `dispman_object_add` | `vc_dispmanx_element_add` | **probable** | "object" is the II-era word for what IV calls an "element". Sequence position matches exactly. |
| `dispman_object_remove` | `vc_dispmanx_element_remove` | **probable** | |
| `dispman_object_delete` | *(no counterpart)* | **unknown** | mirrors `object_create`. |
| `dispman_update_start` | `vc_dispmanx_update_start` | **confirmed** | bracketed with `update_end` at every call site. |
| `dispman_update_end` | `vc_dispmanx_update_submit` / `_submit_sync` | **probable** | name differs; role is the same closing bracket. Which of the two submit semantics it has is **unknown**. |
| `dispman_update_create` | *(no counterpart)* | **unknown** | see §4.4. |
| `dispman_update_delete` | *(no counterpart)* | **unknown** | see §4.4. |
| `dispman_rect_set` | `vc_dispmanx_rect_set` | **confirmed** | same name, 7 call sites, used to build both src and dest rects. |
| `vc_image_*` (21) | `VC_IMAGE_TYPE_T` (the type, not the functions) | **probable** | `VC_IMAGE_TYPE_T` is a live parameter of `vc_dispmanx_resource_create` on VC IV, so the image-descriptor vocabulary survives. **The 21 `vc_image_*` functions themselves were not found in any public header** — on VC IV they are internal to the firmware, exactly as they are here. Name-level match only. |
| `gencmd_register` / `_deregister` | *(no host counterpart)* | **unknown** ⚠ | see §5 — the **VC-side** half of gencmd is not public. Only the host side is. |
| `gencmd_execute` | `vc_gencmd` / `vc_gencmd_send` | **probable** | ours is the VC-side dispatcher; the documented one is the host-side sender. Same protocol, opposite ends. |
| `gencmd_param` | *(no counterpart)* | **probable** | argument accessor for a handler. VC IV's host side parses the response string instead. |
| `gencmd_decode_int` | `vc_gencmd_number_property` | **probable** | both pull an integer out of the command text. |
| `gencmd_decode_fourcc` | *(no counterpart)* | **unknown** | |
| `hostreq_*` (3) | *(no counterpart)* | **unknown** ⚠ | VC IV has no equivalent public "VideoCore calls the ARM" API. See §6. |

### 4.1 What the mapping is worth

The **confirmed** rows are five: `resource_create`, `resource_delete`, `update_start`, `rect_set`,
and the pairing of `update_end` with `update_start`. That is enough to say **this is DispmanX**, and
not enough to say it is *the same* DispmanX.

The size difference is itself informative. VideoCore IV's host header declares roughly **30**
functions (8 display, 6 resource, 3 update, 6 element, 7 utility); we have **12**. Everything VC IV has that we do not is a later addition or a host-side
convenience — `element_change_source`, `element_change_layer`, `element_change_attributes`,
`element_modified`, `resource_read_data`, `resource_set_palette`, `snapshot`, `vsync_callback`,
`query_image_formats`, the six extra display calls. Ours is a **smaller, earlier** interface with the
same four object kinds, and that is consistent with a common ancestor rather than with a subset of
the modern API.

⚠️ **The direction of the comparison must not be forgotten.** VideoCore IV's header is the host API,
compiled on the ARM. Our 183 names are what runs **on the co-processor**. They are two sides of the
same interface only if VideoCore II's host API mirrored its VC-side one, and nothing establishes
that. Where the names are identical (`resource_create`, `update_start`, `rect_set`) that is real
evidence; where only the *role* matches, it is weaker than it looks.

### 4.2 The missing write path — the gap that matters most

**(a)** There is no `dispman_resource_write_data`, and no symbol of any name that obviously moves
host pixels into a resource. The 183 imports contain no candidate.

Two readings, and we cannot currently choose:

1. **(c)** The libraries never write a resource from the host side because they are *already* on the
   co-processor — they fill resource memory with `vc_image_*` and `dma_*` directly, and the
   host-facing write path exists but is only used by the **host**, which does not link against
   `vmcs.bin`'s export table at all. Under this reading the write path is an RPC, not a symbol, and
   we would find it in RetailOS rather than here.
2. **(c)** The II-era resource is written by handing it an image descriptor
   (`vc_image_set_image_data`) rather than by a copy call, and "creating a resource" wraps memory the
   caller already owns.

Reading 1 is the better fit with what [research/10](10-the-resource-image.md) Addendum 26 measured on
the ARM side: RetailOS allocates a *surface* on the co-processor through `FUN_00286ca8`, gets back a
co-processor-side address, and then uploads dirty scanlines to that address with a bulk block write
(`FUN_00287be8`) — **not** through any named call. That is exactly "the write path is an RPC and an
address, not an API symbol". **It is still marked (c)**, because nothing measured connects
`dispman_resource_create` to the thing `FUN_00286ca8` allocates.

### 4.3 A structural difference: create/delete around add/remove

**(a)** Ours has *four* object calls where VC IV has two:

```
ours:      object_create   object_add     object_remove   object_delete
VC IV:                     element_add    element_remove
```

`object_create`, `object_add` and `object_remove` are used by **both** players; `object_delete` is
imported **only by MPLAYER**, once. **(c)** The likeliest reading is that the II-era element has an
explicit lifetime independent of its membership in a display's composition list — you create it once,
add and remove it from the display many times, delete it at shutdown. VC IV collapsed that into
add/remove with the element existing exactly as long as it is added. That `SLIDESHOW` creates objects
and never deletes them fits: a slideshow that runs until the device sleeps has no shutdown path.

If that reading is right it has a direct consequence for the bring-up sequence in §7, and it is
marked as a guess for exactly that reason.

### 4.4 Where (a) and (b) disagree: the update lifecycle

**This is the clearest disagreement in the document, and it is a finding.**

**(b)** VideoCore IV's update lifecycle is two calls: `update_start(priority)` returns a handle,
`update_submit_sync(handle)` consumes it. There is no create and no delete; the handle's lifetime is
the transaction.

**(a)** VideoCore II has **four**: `update_create`, `update_start`, `update_end`, `update_delete`.
Measured call sites: `update_start` 3, `update_end` 3, `update_create` 1, `update_delete` 1 —
`MPLAYER` only for the latter two, `SLIDESHOW` never touching them.

**(c)** The reading that fits the counts: an update object is a **reusable, allocated thing**. You
create it once at startup, then `start`/`end` it repeatedly around each batch of changes, and delete
it at shutdown. VC IV's `update_start` allocates from a pool internally and hides that.

The consequence for a host is concrete: on VideoCore II, **`update_start` may require an update
handle that already exists**, and a host that calls `update_start` cold — the VC IV idiom — may get
nothing. A host implementation that assumes the documented two-call shape has a real chance of being
wrong here, and this is the single place in the mapping where following the Raspberry Pi
documentation is most likely to produce a plausible-and-broken result.

`SLIDESHOW` using `start`/`end` without ever calling `create` is evidence **against** the strict
reading — either the update object is created elsewhere and shared, or `start` can allocate one.
Recorded as a contradiction rather than resolved.

---

## 5. GENCMD — a name-dispatched service registry

### 5.1 What our binaries show

**(a)** Six symbols, and the two with 33 call sites each are `register` and `deregister`. That
asymmetry is the whole architecture: the libraries are overwhelmingly **providers**, not callers.

The registration counts are exact, and they cross-validate against string data perfectly:

| library | `gencmd_register` sites | `gencmd_deregister` sites | command-name strings found in `.text` |
|---|---|---|---|
| `MPLAYER` | 24 | 24 | **24** `mp_*` names |
| `SLIDESHOW` | 9 | 9 | **9** `ss_*` names |

`MPLAYER` even exports the two functions that do it — `mp_register_gencmds` and
`mp_deregister_gencmds` — and `SLIDESHOW` exports `ss_register_gencmds` / `ss_deregister_gencmds`.

**One `gencmd_register` call per command name.** That is measured, not assumed.

The 24 `MPLAYER` commands:

```
mp_play          mp_stop        mp_pause       mp_step        mp_seek        mp_ff
mp_rewind        mp_paint       mp_region      mp_control     mp_restore     mp_suspend
mp_set_ap        mp_vistype     mp_visprop     mp_selectplay  mp_playrecord  mp_get_stats
mp_get_status    mp_set_transform              mp_get_file_props
mp_screen_capture                mp_playrecord_stop            mp_playrecord_pause
```

The 9 `SLIDESHOW` commands:

```
ss_stop      ss_region     ss_transet    ss_trantype   ss_tranprop
ss_trantime  ss_selectplay ss_get_status ss_set_transform
```

And **(a)** `vmcs.bin` itself carries its own command vocabulary as a contiguous run of NUL-padded
string literals at `0x00be2c`..`0x00bf50`:

```
version            commands           power_down         set_vll_dir        led_control
disk_notify        usb_inserted       audio_enable       inuse_notify       power_control
audio_control      motor_control      camera_control     display_control    end_application
power_management   load_application   vmcs_display_clear vmcs_display_enable
```

plus a power-manager set at `0x00f98c`: `pm_set_policy` `pm_get_status` `pm_show_stats`
`pm_start_logging` `pm_stop_logging`.

**No word in the image points at any of those strings.** Searched every 4-aligned u32: zero hits for
all 26. **(c)** The registrations are `gencmd_register("version", handler)` calls with the string
address computed PC-relative in code — normal for this architecture, and the reason the (name →
handler) binding is not recoverable by scanning for pointer tables the way §1's export table was.

Two *sub-command* tables **are** recoverable, and they use the opposite field order from the export
table — `(name_ptr, handler_addr)`, forced because the word before each table is not a valid address:

```
0x0220b4  display_control parameters      0x022e6c  power_management parameters
  power      -> 0x005aae                    power      -> 0x00f672
  dac        -> 0x005aae                    getpower   -> 0x00c874
  svideo     -> 0x005b7c                    powersave  -> 0x012a22
  mode       -> 0x005a32                    freeze     -> 0x00ade2
  encoding   -> 0x005b9e                    model      -> 0x012a02
  width      -> 0x005e62
  height     -> 0x005a00
  backlight  -> 0x0058b8
  wide       -> 0x005e38
```

**(c)** So a GENCMD is probably `display_control width 320`, not `display_width 320`: **one
registered name, then keyword parameters**. `gencmd_param` (15 call sites) and `gencmd_decode_int`
(21) are how a handler walks them. The two-level structure is **(a)**; the exact surface syntax is a
guess, and `display_control` could equally take positional arguments the handler matches against
those names.

⚠️ **What is measured and what is not, precisely.** That 24 = 24 = 24 and 9 = 9 = 9 hold is measured,
and it is strong evidence that registration is one call per name. **Which handler each name binds to
is not measured** — the relocation counts give call-site totals, not arguments, and the name strings
are PC-relative literals. `SLIDESHOW` exporting `ss_cmd_zoom` while registering no `ss_zoom` is a
live reminder that not every `*_cmd_*` export is a top-level command.

### 5.2 The response format, measured

**(a)** The `.text` of `vmcs.bin` and of both player libraries is full of `printf` templates that are
plainly the reply wire format:

```
error=%d error_msg="Command not registered"      error=%d error_msg="odd number of arguments"
error=%d error_msg="missing argument"            error=%d error_msg="Invalid arguments"
error=%d error_msg="bad display"                 error=%d error_msg="bad argument"
commands="                                       tasks="
version=%s%s          version=%s %s%s            value=%d    result=%d    task=%d
state=%s mode=%s substate=%s paused=%s buffering=%d duration=%lu elapsed=%lu
```

**A GENCMD response is a flat `key=value` text string.** Errors are `error=<n> error_msg="<text>"`.
That is not inferred — those are the format strings.

### 5.3 What is public, and what it confirms

**(b)** On VideoCore IV the same service is reached from the ARM through the host API in
[`vc_vchi_gencmd.h`](https://raw.githubusercontent.com/raspberrypi/userland/master/interface/vmcs_host/vc_vchi_gencmd.h):

```
vc_gencmd_init()                                    vc_gencmd_stop()
vc_vchi_gencmd_init(instance, connections, num)     use_gencmd_service() / release_gencmd_service()
vc_gencmd_send(const char *format, ...)             ** the command is a printf-formatted string **
vc_gencmd_read_response(char *response, int maxlen) ** the reply is text **
vc_gencmd(char *response, int maxlen, const char *format, ...)
vc_gencmd_string_property(char *text, const char *property, char **value, int *length)
vc_gencmd_number_property(char *text, const char *property, int *number)
vc_gencmd_until(char *cmd, const char *property, char *value, const char *error_string, int timeout)
```

Read the signatures rather than the names and the protocol falls out of them: **a command is a
formatted text string; a response is a text buffer; and the only two accessors provided parse a
named `property` out of that text as a string or as a number.** The header's own wording is
"property=value type" pairs. It is exposed to userspace as the `vcgencmd` tool.

Three things the header does **not** say, recorded so they are not assumed: there are no maximum
command/response length constants in it, no formal grammar for the response, and no description of
the error format. Those come from the tool's observed behaviour, not from this primary source.

**The properties that can be checked, check out.** Command-by-name dispatch, text request, text
response, `key=value` reply pairs, a `property` accessor for ints — all four are visible in the
VideoCore IV header **and** measured in our 2005 image in §5.1–5.2 (`commands="` at `0x00b94c`,
`version=%s`, `value=%d`, `gencmd_decode_int`). The `error=<n> error_msg="<text>"` shape is a
**(a) measured** fact about our image and a widely-observed property of `vcgencmd`, but is **not** in
the header — so it is a match against a weaker source.

**This is the strongest cross-generation result in the document.** Where the DispmanX mapping has
five confirmed rows out of twenty, GENCMD's semantics match on every property checkable from both
sides. The protocol is recognisably the same protocol, seven years and two generations apart.

The one **asymmetry** to keep in view: the public API is entirely the **host** side. There is no
public `gencmd_register` — nothing documents how a service *on* the VideoCore publishes a command
name. Our six symbols are the other half, and it is the half nobody has written down.

What remains **unknown** is everything below the text layer: the transport. `vc_vchi_gencmd_init`
puts VC IV's gencmd on VCHI (§3.5). There is no evidence either way that VideoCore II does.

### 5.4 What GENCMD is *not*

⚠️ A distinction this document exists partly to make, because conflating the two would waste the next
agent's time.

The GENCMD registry is **not** the structure RetailOS fails to read at co-processor offset `0x1f0`.

[research/10](10-the-resource-image.md) Addendum 26 measured RetailOS reading 16 bytes at `0x1f0`,
requiring `word[2] == 1` and `word[3]` to be a non-null 4-aligned pointer, then following that
pointer to an 8-entry `u16` table, then reading a 16-byte descriptor per entry and matching a **tag**
— 1, 2 or 7. That is a **channel/service directory** for an RPC transport, matched by *numeric tag*.
GENCMD is matched by *name*, in text, and lives above whatever transport that directory describes.

Addendum 28 §2 said the registry "*is* the service directory Addendum 26 found RetailOS failing to
read". **That is not right.** They are two different structures at two different levels: a numeric
channel directory (what `0x1f0` describes) and a name registry (what `gencmd_register` fills). The
`0x1f0` block being runtime-populated is still the reason RetailOS gets zeros — Addendum 27 §3
confirmed that block is zero in the file — but a GENCMD registry is not what a tag-2 descriptor is.

### 5.5 `execute_command_local`

**(a)** `MPLAYER` alone imports `execute_command_local` (`0x009554`) and carries these literals:

```
rg_load 0        rg_play 0        rg_stop 0        rg_seek 0 %d
rg_load 0 file %s num 0 d        /mfs/temp.mid
```

So a service on the co-processor can **issue** a GENCMD to another service on the same
co-processor, as a formatted text string, without going through the host. `/mfs/temp.mid` names the
co-processor's own filesystem (`\mfs` also appears at `0x1fc9d`, and `/mfs/vlls/` at `0x295a0` is
where `set_vll_dir` points the loader). **(c)** `rg_*` is a MIDI/ringtone generator service.

---

## 6. `hostreq_*` — the co-processor calls back

**(a)** Three symbols, and their direction is the point. Everything else in this document is the ARM
asking the VideoCore to do something. These are the **VideoCore asking the ARM**.

| symbol | addr | caller | sites | what it must mean |
|---|---|---|---|---|
| `hostreq_notify` | `0x00cf52` | `MPLAYER` | 17 | asynchronous event upcall — "frame displayed", "stream ended", "state changed". 17 sites in a player is exactly the shape of a state machine reporting transitions. |
| `hostreq_read_iphoto_block` | `0x00cf80` | `SLIDESHOW` | 2 | the co-processor pulling photo data from the host. Named for **iPhoto**, in Apple's own firmware. |
| `hostreq_rendertext` | `0x00d05e` | `MPLAYER` | 1 | the co-processor asking the ARM to render a string. |

**(b)** There is no public counterpart. VideoCore IV's host-facing API has no documented "VideoCore
calls the ARM" surface of this kind. This is either a II-era design that later disappeared, or an
Apple-specific extension, and we cannot tell which.

### 6.1 `hostreq_read_iphoto_block` — the architecture it implies

**(a)** `SLIDESHOW` imports `filesys_register`, `filesys_deregister`, `filesys_size`, `fopen`,
`fread`, `fseek`, `fclose` **and** `hostreq_read_iphoto_block`; `vmcs.bin` carries the strings
`FS_NUCLEUS_RESPS`, **`Host Filesystem`**, `HR_COMND`, `HR_RESPS`, `HOSTIFACE`.

So the co-processor has a **pluggable filesystem layer whose backing store is the ARM**. It calls
`fopen`/`fread` locally; the filesystem driver turns those into host requests; the ARM services them
off the disk. `hostreq_read_iphoto_block` is a specialised fast path for the photo database.

`HR_COMND` / `HR_RESPS` are 8-character Nucleus object names — **(c)** a command queue and a response
queue, which is the natural shape for a host-request channel.

### 6.2 `hostreq_rendertext` and the 319 font lookups — an open question

[research/10](10-the-resource-image.md) measures that RetailOS performs **337 font-registry lookups,
319 of them for the key `("Podium Sans", 18, 1)`**, gets a null every time because Podium Sans is
registered at 14/16/22/28 and not 18, and **never reads a font file** — because in our emulator the
`rsrc` volume that holds `PodiumSans18.ttf` is never mounted.

A co-processor primitive named `hostreq_rendertext` is an obvious thing to connect to that. **It is
an open question, not a claim, and the honest position is that the two cannot currently be the same
event:**

- **Against.** `hostreq_rendertext` is called *by the co-processor firmware*. In our emulator the
  co-processor firmware **never executes** — Addendum 26 established that the BCM model is a memory
  and a protocol, not a CPU. So no `hostreq_rendertext` has ever been issued in any run this project
  has made. The 319 lookups are therefore host-originated, from RetailOS's own text layout, and are
  not co-processor callbacks.
- **Against, second.** [research/10](10-the-resource-image.md) Addendum 15 §8 already killed the font
  lead as the cause of the boot wall, on three independent measurements.
- **For, on real hardware.** If the co-processor renders subtitles or on-screen text during video
  playback, and the fonts live on the host's disk, then `hostreq_rendertext` must reach *some* ARM
  text renderer — plausibly the same font registry. On a real iPod the two paths could share a
  bottom.
- **A measured negative that constrains it.** `vmcs.bin` contains **no font name of any kind**:
  searches for `font`, `Font`, `FONT`, `Podium`, `podium` return zero. Whatever text the
  co-processor asks for, it does not name the typeface itself — the font must be identified by the
  host, or passed as a parameter, or already chosen.

**Status: unresolved, and untestable until the co-processor firmware actually runs.** It goes on the
list of things a working BCM model would settle in one measurement, not on the list of things we
believe.

### 6.3 What `hostreq_*` means for a host implementation

**(c)** A host that implements only the ARM→VC direction gets a co-processor that can composite but
cannot read a photo, cannot report that a frame was displayed, and cannot ask for text. For the
narrow goal of *one static image on screen* that is probably sufficient. For video playback it is
not: 17 `hostreq_notify` call sites in `MPLAYER` say the player's state machine expects the host to
be listening.

---

## 7. Minimal display bring-up

**This is the deliverable the GENCMD-derivation work should check itself against.**

### 7.0 ⚠️ Two sequences, two sides of the bus — do not merge them

The single easiest mistake to make with this material, and the one that would cost the most, is to
treat "the DispmanX bring-up sequence" as one thing. It is two, on opposite sides of the bus, and
only one of them is what a *host* does.

| | who runs it | what it calls |
|---|---|---|
| **VC-side** | code executing **on the co-processor** — `MPLAYER`, `SLIDESHOW`, and any service inside `vmcs.bin` | the 183 symbols in §2, resolved by the firmware's own linker against the export table at `0x2160C` |
| **host-side** | code executing **on the ARM** — RetailOS | an RPC over a channel it must first discover; the calls are `FUN_00286ca8`, `FUN_00164450`, `FUN_001649ac`, `FUN_00164878`, `FUN_00164f44` |

**Our 183 names are the VC side.** VideoCore IV's `vc_dispmanx_*` header is the **host** side. They
are comparable only if VideoCore II's host proxy mirrored its VC-side API name-for-name the way VC
IV's does — which is likely, since that is how such proxies are generated, and is **not
established**.

So §7.1 is a host-side sequence for the wrong generation; §7.2 is a VC-side sequence for the right
one; and §7.3 is the only host-side sequence for the right chip that we have any measurements of at
all. **§7.3 is the one to check a derivation against.**

### 7.1 The documented DispmanX sequence — **(b)**, VideoCore IV, **host side**

The canonical "one image on screen" ordering, as the `hello_dispmanx` sample actually performs it:

```
 1  bcm_host_init()                                    bring up the host-side interface
 2  display = vc_dispmanx_display_open(0)              0 = DISPMANX_ID_MAIN_LCD
 3  vc_dispmanx_display_get_info(display, &info)       learn the panel's width/height
 4  pitch = ALIGN_UP(width * bpp, 32)                  ** 32-byte alignment, not width*bpp **
    image = malloc(pitch * height)                     ALIGN_UP(x,y) = (x + (y)-1) & ~((y)-1)
 5  resource = vc_dispmanx_resource_create(            pixel type + dimensions;
        type, width, height, &native_handle)           returns an opaque uint32 handle
 6  vc_dispmanx_rect_set(&write_rect, 0,0, width,height)   region of the resource to fill
 7  vc_dispmanx_resource_write_data(                   push pixels in — NOT transactional
        resource, type, pitch, image, &write_rect)
 8  update = vc_dispmanx_update_start(priority)        0 = DISPMANX_NO_HANDLE means failure
 9  vc_dispmanx_rect_set(&src, 0,0, width<<16, height<<16)   src rect is 16.16 fixed point
    vc_dispmanx_rect_set(&dst, x, y, w, h)                   dst rect is display pixels
10  element = vc_dispmanx_element_add(                 bind resource -> display
        update, display, layer, &dst, resource, &src,
        protection, &alpha, clamp, transform)
11  vc_dispmanx_update_submit_sync(update)             ** the image appears here **
```

and teardown, itself a transaction, in the sample's own order:

```
12  update = vc_dispmanx_update_start(priority)
13  vc_dispmanx_element_remove(update, element)
14  vc_dispmanx_update_submit_sync(update)
15  vc_dispmanx_resource_delete(resource)              only after the element is gone
16  vc_dispmanx_display_close(display)
```

The ordering constraints that actually matter:

- **5 before 7** — you cannot write to a resource that does not exist.
- **7 before 11, and it does not need step 8** — writing pixels is *not* transactional; only the
  composition change is. You may write a resource that is already on screen, and the change appears
  when the scan-out reaches it, with no update bracket at all. This is how video playback works and it
  is the single most commonly misunderstood part of the API.
- **8 before 10 before 11** — every element operation needs a live update handle.
- **13 before 15** — deleting a resource an element still references is a use-after-free on the
  co-processor.

### 7.2 The same sequence in our names — **(a)** names, **(c)** ordering, **VC side**

This is what a service *running on the co-processor* does — `MPLAYER` putting a decoded frame up, or
a hypothetical program of ours running there. **It is not what a host does**; for that, see §7.3.

```
 1  (transport bring-up — not a dispman_ call; see §7.3)
 2  dispman_display(...)                    ** signature unknown: one symbol, not open/close **
 3  dispman_resource_create(...)
 4  (write path unknown — see §4.2. Probably an RPC to a co-processor address,
     not a dispman_ symbol at all.)
 5  dispman_update_create(...)              ** may be required once, before any update_start **
 6  dispman_update_start(...)
 7  dispman_rect_set(...) x2                src and dest
 8  dispman_object_create(...)              ** no VC IV counterpart **
 9  dispman_object_add(update, ...)
10  dispman_update_end(update)              ** the image appears here **
```

teardown:

```
11  dispman_update_start
12  dispman_object_remove
13  dispman_update_end
14  dispman_object_delete
15  dispman_resource_delete
16  dispman_update_delete
```

**Every ordering claim in §7.2 is (c).** The names and the addresses are (a); the sequence is
transposed from (b) and adjusted for the four-call update lifecycle and the create/add split. The two
places it is most likely to be wrong are the two places §4.3 and §4.4 flagged: whether
`update_create` is required before `update_start`, and whether `object_create` is required before
`object_add`. A third, quieter risk: `SLIDESHOW` imports **neither** `update_create`,
`update_delete` **nor** `object_delete` — it does `object_create` / `object_add` / `object_remove`
and `update_start` / `update_end`, and never tears an object down. So at least one real service on
this chip performs a shorter sequence than the one written above, and steps 5, 14 and 16 are the
optional ones.

### 7.3 The host-side sequence for *our* chip — **(a)**, and the one to check against

This is the most valuable calibration in the document, because it is *our* hardware and *our*
firmware. It is also the only host-side ordering here that is measured rather than transposed.

**The ordered steps a host makes, as RetailOS actually makes them:**

```
 1  power the BCM and handshake              GPO32_VAL bit 0x4000, 50 ms, then FUN_00287998
 2  upload vmcs.bin to co-processor 0        201 376 bytes, DMA, to internal address 0
 3  read 16 bytes at co-processor 0x1f0      FUN_00288058
       require  word[2] == 1
       require  word[3] != 0  and  (word[3] & 3) == 0      <- the service-directory base
 4  read 8 u16 offsets at word[3]            -> a local copy at 0x108d3bd4
 5  for each of the 8 slots:                 FUN_00286aa8 / FUN_00287194 / FUN_00288978
       read a 16-byte descriptor at base + offset
       match a numeric tag: 2 (surfaces), 1, 7
 6  bind the channel                         the matched slot index; -1 means "no service"
 7  allocate a surface on the co-processor   FUN_00286ca8 -> handle + co-processor address
 8  create a layer                           FUN_001649ac
 9  bind the layer to the surface            FUN_00164878
10  upload dirty scanlines                   FUN_00287be8, straight to the surface address
11  present                                  FUN_00286b6c, then flip front/back
```

Steps 1–2 succeed today. **Step 3 is where every run of this project has stopped**: the words come
back zero because our BCM model is a memory and a protocol, not a CPU, so `vmcs.bin` never executes
and never fills `0x1f0`. Steps 4–11 have never run.

Set that beside §7.2:

| RetailOS's ARM-side step | the runtime call it must reach |
|---|---|
| discover a tag-2 service at `0x1f0` | *(transport — below dispman entirely)* |
| `FUN_00286ca8` "allocate on the co-processor" | `dispman_resource_create` — **probable** |
| `FUN_00164450` "create surface" | wrapper around the above |
| `FUN_001649ac` "create layer" | `dispman_object_create` — **probable** |
| `FUN_00164878` "bind layer" | `dispman_object_add` — **probable** |
| `FUN_00164f44` upload dirty scanlines, then present | the missing write path (§4.2), then `update_start`/`update_end` |

The two vocabularies line up term for term: **surface = resource, layer = object/element, present =
update bracket.** Addendum 26 described this chain from the ARM side without knowing the API's names;
this document has the names without the ARM side. They are the same mechanism seen from two ends, and
the fact that they match at every step is the best evidence in this file that the DispmanX reading is
correct.

**It also localises the remaining work exactly.** RetailOS never reaches `dispman_*` at all. It stops
four levels lower, at the transport: a 16-byte block at `0x1f0`, an 8-entry `u16` directory, a
16-byte descriptor per service tagged 1 / 2 / 7, and the ring protocol behind `FUN_0028861c` /
`FUN_00288434`. **None of that is DispmanX and none of it is GENCMD.** The API this document
describes sits above a channel that has not been opened yet.

---

## 8. What surprised us, and what could not be established

### Surprises

- **The export table exists.** Addendum 28 recorded, as a hard limit, that the symbol names appear
  nowhere in `vmcs.bin` and are therefore "a specification of the API and **not** a way to locate
  anything inside the firmware image". The opposite is true for the `rsrc` copy: 183 records, name
  and address, sorted for binary search. Every function in this document has an address.
- **The two `vmcs.bin` really are different programs**, not two revisions. The NOR one is
  `M25 Diagnostics` and has no display stack at all.
- **`render.bin` is a shader-capable 3D renderer.** Not ELF, so no symbols — but its strings are
  `ShaderMachine: Attempt to set uniform when no shader is bound`,
  `ShaderMachine: Invalid shader type found`,
  `Length is less than data described in texture sub data header`, plus `gldCalloc` / `gldVecMalloc`
  / `gldMallocSlow` and `FrontBufferA` / `display map` / `TV buffer`. *(Surprising until the product
  brief turned up: it lists "high-quality graphics acceleration for 3D games" as a headline feature.
  This is a shipped capability Apple did not use, not an oddity.)*
- **`PASSTHROUGH.VLL` is the crypto unit.** It exports `aes_init`, `aes_decipher`, `aes_sbox`,
  `aes_sbox_inv`, `aes_rcon`, `vcaes_decipher`, `stream_crypto_info` — decryption running on the video
  co-processor, not the ARM. *(Also in the brief: "DRM-ready … AES, 3DES, RSA", and the block diagram
  labels the core "VideoCore II **with DRM**".)*
- **The dynamic loader's six names survived two generations unchanged** — `dlopen` `dlsym` `dlclose`
  `dlerror` `dldone` `dlcheck`, ours in 2005 and Broadcom's BSD-licensed `dlfcn.h` in 2012, the same
  six including the two that are not POSIX. Along with `set_vll_dir` being the same GENCMD name for
  the same job. §2.10.
- **The codec plugin ABI is polymorphism by symbol name.** `H264DEC` and `MPG4DEC` both export
  `get_vid_dec_func_table`; `MPLAYER` `dlsym`s that one string and gets whichever decoder it opened.
- **The GENCMD command vocabulary is legible in plain text**, and the registration counts
  cross-validate exactly: 24 `gencmd_register` sites in `MPLAYER`, 24 `mp_*` names; 9 and 9 in
  `SLIDESHOW`. Two entirely independent measurements agreeing to the unit is rare in this project.
- **GENCMD's wire semantics survived two generations intact.** `commands="`, `error=%d
  error_msg="…"`, `version=%s`, name dispatch — the 2005 image and the public Raspberry Pi API agree
  on every property checkable from strings.
- **The co-processor's filesystem is the ARM's disk.** `filesys_register` + `Host Filesystem` +
  `hostreq_read_iphoto_block` + `HR_COMND`/`HR_RESPS`.
- **`MetaWare Universal Linker v5.3h`** — a real toolchain identification, in `.comment`, in all six
  libraries.

### Could not be established

- **How a resource gets written.** No symbol does it (§4.2). The best hypothesis is that it is an RPC
  to a co-processor address rather than an API call, which is consistent with what RetailOS does, but
  nothing measured connects the two.
- **Whether `dispman_update_create` is required before `dispman_update_start`.** `MPLAYER` calls it
  once, `SLIDESHOW` never does. §4.4 records the contradiction unresolved.
- **Any function signature.** The ELF gives names, addresses and call-site counts. It gives no
  argument counts and no types. Ghidra ships no VideoCore II processor module (Addendum 27 §4), so
  the code cannot be read — but see the note below, which bounds that problem rather than removing it.
- **The (name → handler) binding for `vmcs.bin`'s own 19 GENCMDs.** The names are in `.text` as
  PC-relative literals with no pointer table; only the two sub-parameter tables at `0x220b4` and
  `0x022e6c` are recoverable by scanning.
- **The transport under GENCMD.** VC IV uses VCHI/VCHIQ; whether VideoCore II does, or uses something
  earlier, is not established from our binaries and was not found in public material.
- **Whether `hostreq_rendertext` and the 319 Podium Sans 18 lookups are connected** (§6.2). Untestable
  until the co-processor firmware executes.
- **Whether DispmanX on VideoCore II and on VideoCore IV are the same API or a redesign.** Five
  confirmed rows, seven unknowns, and two structural differences (§4.3, §4.4) that say "related, not
  identical".
- **Whether the *name* "DispmanX" was used at the VideoCore II generation at all.** Ours says
  `dispman_`, not `dispmanx_`. The API is DispmanX by shape; calling it that is our word, not a sourced
  one. ⚠️ **A lead, recorded unverified:** delegated research reported that DispmanX is VideoCore
  III-era and succeeded a VideoCore II **`DISPMAN2`**. That would fit our `dispman_` prefix and would
  explain §4.3/§4.4's structural differences neatly — which is exactly why it should not be adopted
  without a source. **A targeted search for `DISPMAN2` found nothing**; the only public comment on the
  naming is a Raspberry Pi forum answer saying "Dispman" is lazy shorthand for "DispmanX", which is a
  different claim. Treat as unconfirmed.
- **Whether VideoCore II has VCHI.** §3.5's shape match between RetailOS's `0x1f0` service directory
  and a VCHI connection table is an inference from one generation to another, with nothing under it.
- **What "VLL" stands for.** Broadcom's own headers use the bare token and never expand it.

### A bounded path to the instruction set, for later

Addendum 27 §4 concluded that `vmcs.bin`'s code cannot be read: no VideoCore II processor module
exists for Ghidra, and Broadcom's only public architecture document covers VideoCore **IV**'s 3D
system — explicitly *"the 3D system"*, not the VPU, and nothing about earlier generations. That
stands. But the problem is **bounded rather than opaque**, and this is worth recording because the
next person to try should not start from zero:

- **(b)** The **Alphamosaic patents** disclose the architecture. `US7036001B2` *"Vector processing
  system"* (priority 2001-10-31, GB, Alphamosaic Ltd; inventors Barlow, Bailey, Ramsdale, Plowman,
  Swann) describes sixteen 16-bit pixel processing units in parallel, a scalar file of thirty-two
  32-bit registers, and a 2-D vector register file addressed in groups of sixteen contiguous pixels.
  On encodings: *"Each instruction type has an **80-bit full encoding, and a compact 48-bit
  encoding**"*, with scalar instructions *"a standard encoding of **16 bits, with 32 bit and 48 bit
  variants**"*. The VideoCore IV RE project notes the patents are an invaluable reference *"whilst the
  instruction encodings are different"* — i.e. structurally right, numerically wrong for VC IV.
- **(a)-adjacent corroboration** from someone holding our exact chip's code: Rockbox's `dreamlayers`,
  2009, poking at iPod BCM2722 code — *"Data certainly isn't encrypted, and I don't think the code is
  either; it's just weird… Several [Alphamosaic patents] show 48 and 80 bit instruction encodings."*
- **(a)** The toolchain is named in our own binaries: `.comment` = `MetaWare Universal Linker v5.3h`.
  The Raspberry Pi's shipped `start.elf` carries `MetaWare Linker v5.6.19`. Same vendor, VideoCore I
  through VI. Broadcom's archived toolchain page confirms MetaWare/ARC as the official chain and that
  it *"supports ELF object module format"* — which is why our `.VLL` files are ELF at all.

None of that yields a disassembler. It does mean a variable-length 48/80-bit vector and 16/32/48-bit
scalar encoding is the shape to expect, which is a far better starting point than "it's just weird".

---

## 9. Settled when

This file is a reference, not an experiment, so it has no measurement that closes it. The claim it
makes that most deserves to be falsified is §7.2's ordering — and the thing that would falsify it is
a co-processor model that answers the `0x1f0` handshake, at which point `dispman_object_create` at
`0x713a` becomes a breakpoint and the sequence becomes observable rather than transposed.

What has firmed up since the first revision is the *licence to read forward at all*. Broadcom's own
brief for our part says the BCM2722 is *"software-compatible with the established VideoCore family …
backward compatibility for applications software"*, and two whole interfaces are now measured to have
survived to VideoCore IV unchanged — GENCMD's semantics (§5.3) and the `dl*` loader's six names
(§2.10). That does not make DispmanX safe to assume; it makes the *method* sound.

Until then: **five confirmed mappings, seven unknowns, and two places where following the Raspberry
Pi documentation is more likely to be wrong than right.**
