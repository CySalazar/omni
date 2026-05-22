//! Graphical desktop orchestrator — M5 milestone.
//!
//! Provides [`run_desktop`], which initialises the GOP framebuffer UI and
//! runs the PS/2 + RTC event loop until the user requests power-off. The
//! caller (`kmain`) is responsible for the serial banner and ACPI S5 after
//! this function returns.
//!
//! ## Layout
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │ System Info (W=484)       │ Terminal (W=480)                 │
//! │ • kernel version          │ OMNI OS Shell v0.1-alpha         │
//! │ • memory regions          │ hint text                        │
//! │ • display resolution      │ omni@kernel:~$ _                 │
//! │ • serial port             │                                  │
//! │ [ Power Off ]             │                                  │
//! ├──────────────────────────────────────────────────────────────┤
//! │ OMNI OS │     HH:MM:SS     │              ESC=off  Xs        │
//! └──────────────────────────────────────────────────────────────┘
//! ```

#![allow(
    clippy::doc_markdown,
    reason = "module doc references ASCII box drawing, register names, hex offsets without ticks"
)]
#![allow(
    clippy::similar_names,
    reason = "(cx0, cy0), (cpx, cpy) are pixel coord pairs by convention; renaming hurts clarity"
)]
#![allow(
    clippy::map_unwrap_or,
    reason = "Option::map(...).unwrap_or(...) reads more naturally for default-value chains"
)]
#![allow(
    clippy::too_many_lines,
    reason = "run_desktop is the orchestrator event loop; splitting hides the polling structure"
)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "framebuffer/echo-buf indices fit u32 by design"
)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "framebuffer ownership transfer is intentional for the run_desktop top-level"
)]

use super::cursor::Cursor;
use super::early_console;
use super::graphics::FrameBuffer;
use super::{arch, cpuinfo, font, graphics, input, vga, virtio_tablet, widget, wm};

// =============================================================================
// Constants
// =============================================================================

const TOTAL_SECS: usize = 300;

/// "Power Off" button in the System Info window content area.
const POWEROFF_BTN: widget::Button = widget::Button {
    rel_x: 8,
    rel_y: 254, // bumped from 148 to clear the 6 added CPU rows (84 px)
    width: 96,
    height: 22,
    label: "Power Off",
};

// =============================================================================
// Public entry point
// =============================================================================

/// Initialise and run the graphical desktop until the user requests power-off.
///
/// Blocks in the PS/2 + RTC event loop. Before returning it draws the
/// "Powering off…" overlay so the caller can proceed directly to ACPI S5.
///
/// # Parameters
///
/// - `framebuffer` — GOP framebuffer (GOP path). `None` falls back to VGA
///   text mode.
/// - `region_count` — number of physical memory regions from the bootloader;
///   displayed in the System Info window.
/// - `free_mib` — free physical RAM in MiB from the frame allocator.
/// - `total_mib` — total usable physical RAM in MiB from the frame allocator.
/// - `phys_offset` — bootloader's `physical_memory_offset`. Required by the
///   VirtIO tablet driver to translate `static mut` virtual addresses into
///   physical addresses via the page-table walker.
#[allow(
    clippy::cognitive_complexity,
    reason = "event loop orchestrator: branches mirror input sources (keyboard/mouse/RTC)"
)]
pub fn run_desktop(
    framebuffer: Option<graphics::FrameBuffer>,
    region_count: usize,
    free_mib: u32,
    total_mib: u32,
    phys_offset: u64,
    cpu_total: usize,
    cpu_bsp_apic_id: u32,
) {
    let fb_opt: Option<&FrameBuffer> = framebuffer.as_ref();

    // ── Window manager ────────────────────────────────────────────────────────
    let mut wm_state = wm::WindowManager::new();
    wm_state.add(wm::Window {
        x: 20,
        y: 20,
        width: 484,
        height: 308, // 224 + 84 (6 new CPU rows × 14)
        title: "System Info",
        bg_color: graphics::DARK_NAVY,
    });
    wm_state.add(wm::Window {
        x: 524,
        y: 20,
        width: 480,
        height: 300,
        title: "Terminal",
        bg_color: 0xFF_00_05_0E,
    });
    // Build Info window — full width below the existing two. Surfaces the
    // git commit, branch, build timestamp, and milestone status so the
    // running image is self-describing on Proxmox / VirtualBox / QEMU.
    wm_state.add(wm::Window {
        x: 20,
        y: 336,
        width: 984,
        height: 198,
        title: "Build Info",
        bg_color: graphics::DARK_NAVY,
    });

    // ── Terminal echo buffer ──────────────────────────────────────────────────
    let mut echo_buf = [0u8; 40];
    let mut echo_len: usize = 0;

    // ── Initial render ────────────────────────────────────────────────────────
    if let Some(fb) = fb_opt {
        fb.clear(graphics::DARK_NAVY);
        wm::draw_taskbar(fb, TOTAL_SECS);
        let (h, m, s) = arch::rtc_time();
        render_clock(fb, h, m, s);
        wm_state.draw_all(fb);
        render_sysinfo(
            fb,
            &wm_state,
            region_count,
            free_mib,
            total_mib,
            cpu_total,
            cpu_bsp_apic_id,
        );
        render_terminal_static(fb, &wm_state);
        render_echo(fb, &wm_state, &echo_buf, echo_len);
        render_buildinfo(fb, &wm_state);
    } else {
        render_vga_fallback(region_count, free_mib, total_mib);
    }

    // ── Mouse init: prefer VirtIO tablet (absolute), fall back to PS/2 ───────
    // SAFETY: `phys_offset` is the bootloader's `physical_memory_offset`
    // passed by `kmain`; ring 0; interrupts are masked for the whole demo.
    let mut vtablet = unsafe { virtio_tablet::VirtioTablet::try_init(phys_offset) };
    let use_virtio = vtablet.is_some();
    if use_virtio {
        early_console::write_str("[virtio] tablet ready\n");
    } else {
        early_console::write_str("[virtio] no legacy tablet found, using PS/2\n");
        input::ps2_mouse_init();
    }

    // ── Cursor ────────────────────────────────────────────────────────────────
    // Start at (0, 0) to match QEMU's initial virtual-cursor position. In VNC
    // mode QEMU converts absolute VNC coordinates to PS/2 relative deltas from
    // its own (0, 0) origin; starting here keeps the two in sync from the first
    // mouse event onward. The VirtIO tablet path overwrites it on the first
    // event anyway, so the starting position is harmless either way.
    let mut cursor_state: Option<Cursor> = fb_opt.map(|fb| Cursor::new(fb, 0, 0));

    // ── Event loop ────────────────────────────────────────────────────────────
    let mut remaining = TOTAL_SECS;
    let mut last_rtc = arch::rtc_seconds();

    'event: loop {
        // ── Mouse input: VirtIO tablet (absolute) when present, else PS/2 ──
        if let Some(ref mut tablet) = vtablet {
            if let Some(state) = tablet.poll() {
                if let Some(fb) = fb_opt {
                    // Scale absolute [0..=0x7FFF] coords into framebuffer px.
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "fb dimensions are u32; scaled product divided by 0x7FFF fits in u32"
                    )]
                    #[allow(
                        clippy::integer_division,
                        reason = "abs coord scaling truncates to pixel; sub-pixel accuracy unwanted"
                    )]
                    let px = (u64::from(state.abs_x) * u64::from(fb.width) / 0x7FFF) as u32;
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "fb dimensions are u32; scaled product divided by 0x7FFF fits in u32"
                    )]
                    #[allow(
                        clippy::integer_division,
                        reason = "abs coord scaling truncates to pixel; sub-pixel accuracy unwanted"
                    )]
                    let py = (u64::from(state.abs_y) * u64::from(fb.height) / 0x7FFF) as u32;
                    if let Some(c) = &mut cursor_state {
                        c.move_to(fb, px, py);
                    }
                    if state.buttons & 0x01 != 0 {
                        let (cpx, cpy) = cursor_state
                            .as_ref()
                            .map(|c| (c.cx, c.cy))
                            .unwrap_or((0, 0));
                        if let Some(c) = &cursor_state {
                            c.hide(fb);
                        }
                        wm_state.click_hit_test(fb, cpx, cpy);
                        let cx0 = wm_state.get(0).map_or(0, |w| w.x + 1);
                        let cy0 = wm_state.get(0).map_or(0, |w| w.y + wm::TITLEBAR_H);
                        if widget::button_hit_test(&POWEROFF_BTN, cx0, cy0, cpx, cpy) {
                            widget::draw_button(fb, cx0, cy0, &POWEROFF_BTN, true);
                            break 'event;
                        }
                        if let Some(c) = &mut cursor_state {
                            c.show(fb);
                        }
                    }
                }
            }
        } else if let Some(ev) = input::ps2_mouse_poll() {
            if let Some(fb) = fb_opt {
                // In QEMU/VNC mode PS/2 deltas are already in screen pixels
                // (QEMU converts absolute VNC coordinates to relative PS/2
                // events 1:1). Using ×1 keeps the software cursor in sync with
                // the VNC pointer; a real PS/2 mouse would need a higher factor.
                let dx = ev.dx;
                let dy = ev.dy;
                if dx != 0 || dy != 0 {
                    if let Some(c) = &mut cursor_state {
                        c.move_by(fb, dx, dy);
                    }
                }
                // Left button click → title-bar focus / power-off button.
                if ev.buttons & 0x01 != 0 {
                    let (px, py) = cursor_state
                        .as_ref()
                        .map(|c| (c.cx, c.cy))
                        .unwrap_or((0, 0));

                    if let Some(c) = &cursor_state {
                        c.hide(fb);
                    }

                    wm_state.click_hit_test(fb, px, py);

                    let cx0 = wm_state.get(0).map_or(0, |w| w.x + 1);
                    let cy0 = wm_state.get(0).map_or(0, |w| w.y + wm::TITLEBAR_H);
                    if widget::button_hit_test(&POWEROFF_BTN, cx0, cy0, px, py) {
                        widget::draw_button(fb, cx0, cy0, &POWEROFF_BTN, true);
                        break 'event;
                    }

                    if let Some(c) = &mut cursor_state {
                        c.show(fb);
                    }
                }
            }
        }

        if let Some(key) = input::ps2_poll() {
            match key {
                // ── Power off ────────────────────────────────────────────────
                input::Key::Escape => break 'event,

                // ── Window focus (Tab) ───────────────────────────────────────
                input::Key::Tab => {
                    if let Some(fb) = fb_opt {
                        if let Some(c) = &cursor_state {
                            c.hide(fb);
                        }
                        wm_state.tab_focus(fb);
                        if let Some(c) = &mut cursor_state {
                            c.show(fb);
                        }
                    }
                }

                // ── Enter: title-bar focus / button activation ───────────────
                input::Key::Enter => {
                    if let Some(fb) = fb_opt {
                        let (px, py) = cursor_state
                            .as_ref()
                            .map(|c| (c.cx, c.cy))
                            .unwrap_or((0, 0));

                        if let Some(c) = &cursor_state {
                            c.hide(fb);
                        }

                        // Title bar → shift keyboard focus.
                        wm_state.click_hit_test(fb, px, py);

                        // Power Off button → highlight + exit.
                        let cx0 = wm_state.get(0).map_or(0, |w| w.x + 1);
                        let cy0 = wm_state.get(0).map_or(0, |w| w.y + wm::TITLEBAR_H);
                        if widget::button_hit_test(&POWEROFF_BTN, cx0, cy0, px, py) {
                            widget::draw_button(fb, cx0, cy0, &POWEROFF_BTN, true);
                            break 'event;
                        }

                        // Terminal Enter → execute (clear echo line).
                        echo_len = 0;
                        render_echo(fb, &wm_state, &echo_buf, echo_len);

                        if let Some(c) = &mut cursor_state {
                            c.show(fb);
                        }
                    }
                }

                // ── Backspace ────────────────────────────────────────────────
                input::Key::Backspace => {
                    if let Some(fb) = fb_opt {
                        if let Some(c) = &cursor_state {
                            c.hide(fb);
                        }
                        #[allow(
                            clippy::implicit_saturating_sub,
                            reason = "explicit form documents intent for security audit"
                        )]
                        if echo_len > 0 {
                            echo_len -= 1;
                        }
                        render_echo(fb, &wm_state, &echo_buf, echo_len);
                        if let Some(c) = &mut cursor_state {
                            c.show(fb);
                        }
                    }
                }

                // ── Printable key → terminal echo ────────────────────────────
                input::Key::Char(byte) => {
                    if let Some(fb) = fb_opt {
                        if let Some(c) = &cursor_state {
                            c.hide(fb);
                        }
                        if echo_len < 40 {
                            #[allow(
                                clippy::indexing_slicing,
                                reason = "echo_len < 40 <= echo_buf.len() guarded above"
                            )]
                            {
                                echo_buf[echo_len] = byte;
                            }
                            echo_len += 1;
                        }
                        render_echo(fb, &wm_state, &echo_buf, echo_len);
                        if let Some(c) = &mut cursor_state {
                            c.show(fb);
                        }
                    }
                }

                // ── Arrow keys → cursor movement ─────────────────────────────
                input::Key::ArrowUp => {
                    if let Some(fb) = fb_opt {
                        if let Some(c) = &mut cursor_state {
                            c.move_by(fb, 0, -4);
                        }
                    }
                }
                input::Key::ArrowDown => {
                    if let Some(fb) = fb_opt {
                        if let Some(c) = &mut cursor_state {
                            c.move_by(fb, 0, 4);
                        }
                    }
                }
                input::Key::ArrowLeft => {
                    if let Some(fb) = fb_opt {
                        if let Some(c) = &mut cursor_state {
                            c.move_by(fb, -4, 0);
                        }
                    }
                }
                input::Key::ArrowRight => {
                    if let Some(fb) = fb_opt {
                        if let Some(c) = &mut cursor_state {
                            c.move_by(fb, 4, 0);
                        }
                    }
                }
            }
        }

        // ── RTC tick ──────────────────────────────────────────────────────────
        let curr = arch::rtc_seconds();
        let delta = if curr >= last_rtc {
            curr - last_rtc
        } else {
            60 - last_rtc + curr
        };
        if delta >= 1 {
            last_rtc = curr;
            remaining = remaining.saturating_sub(delta as usize);
            if let Some(fb) = fb_opt {
                if let Some(c) = &cursor_state {
                    c.hide(fb);
                }
                wm::update_taskbar_time(fb, remaining);
                let (h, m, s) = arch::rtc_time();
                render_clock(fb, h, m, s);
                if let Some(c) = &mut cursor_state {
                    c.show(fb);
                }
            } else {
                vga::write_at(16, 21, b"  ", vga::YELLOW, vga::BLACK);
                vga::write_usize_at(16, 21, remaining, vga::YELLOW, vga::BLACK);
            }
            if remaining == 0 {
                break 'event;
            }
        }

        core::hint::spin_loop();
    }

    // ── Power-off overlay ─────────────────────────────────────────────────────
    if let Some(fb) = fb_opt {
        if let Some(c) = &cursor_state {
            c.hide(fb);
        }
        wm::show_poweroff_overlay(fb);
    } else {
        vga::write_at(
            16,
            4,
            b"Powering off...              ",
            vga::YELLOW,
            vga::BLACK,
        );
    }
}

// =============================================================================
// Private rendering helpers
// =============================================================================

/// Render the RTC clock (HH:MM:SS) centred in the taskbar.
#[allow(
    clippy::integer_division,
    reason = "pixel and clock digit math; integer truncation is intentional"
)]
fn render_clock(fb: &FrameBuffer, h: u8, m: u8, s: u8) {
    let y0 = fb.height.saturating_sub(wm::TASKBAR_H);
    let time_w = 64_u32; // 8 chars × 8px
    let tx = (fb.width / 2).saturating_sub(time_w / 2);
    let ty = y0 + 10;

    // Clear the clock area (slightly wider to erase the previous value).
    fb.draw_rect_filled(
        tx.saturating_sub(4),
        y0 + 1,
        tx + time_w + 4,
        fb.height,
        graphics::DARK_NAVY,
    );

    let buf: [u8; 8] = [
        b'0' + h / 10,
        b'0' + h % 10,
        b':',
        b'0' + m / 10,
        b'0' + m % 10,
        b':',
        b'0' + s / 10,
        b'0' + s % 10,
    ];
    if let Ok(time_str) = core::str::from_utf8(&buf) {
        font::render_str(fb, tx, ty, time_str, graphics::WHITE, graphics::DARK_NAVY);
    }
}

/// Render the System Info window content (labels + Power Off button).
#[allow(
    clippy::too_many_arguments,
    reason = "render_sysinfo paints a fixed-format panel; every argument is a distinct visible field"
)]
#[allow(
    clippy::too_many_lines,
    reason = "linear sequence of font::render_str calls — one per displayed row"
)]
fn render_sysinfo(
    fb: &FrameBuffer,
    wm_state: &wm::WindowManager,
    region_count: usize,
    free_mib: u32,
    total_mib: u32,
    cpu_total: usize,
    cpu_bsp_apic_id: u32,
) {
    let Some(w) = wm_state.get(0) else { return };
    let cx = w.x + 8;
    let cy = w.y + wm::TITLEBAR_H + 8;
    let step: u32 = 14;

    font::render_str(
        fb,
        cx,
        cy,
        "Kernel  : omni-kernel v",
        graphics::CYAN,
        w.bg_color,
    );
    font::render_str(
        fb,
        cx + 23 * 8,
        cy,
        env!("CARGO_PKG_VERSION"),
        graphics::WHITE,
        w.bg_color,
    );
    font::render_str(
        fb,
        cx,
        cy + step,
        "Boot    : UEFI + GOP Framebuffer",
        graphics::CYAN,
        w.bg_color,
    );
    font::render_str(
        fb,
        cx,
        cy + step * 2,
        "Memory  : ",
        graphics::CYAN,
        w.bg_color,
    );
    font::render_usize_scaled(
        fb,
        cx + 10 * 8,
        cy + step * 2,
        region_count,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    let rw = font::digit_width(region_count, 1);
    font::render_str(
        fb,
        cx + 10 * 8 + rw + 4,
        cy + step * 2,
        "regions",
        graphics::WHITE,
        w.bg_color,
    );
    font::render_str(
        fb,
        cx,
        cy + step * 3,
        "Display : ",
        graphics::CYAN,
        w.bg_color,
    );
    font::render_usize_scaled(
        fb,
        cx + 10 * 8,
        cy + step * 3,
        fb.width as usize,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    let dw = font::digit_width(fb.width as usize, 1);
    font::render_str(
        fb,
        cx + 10 * 8 + dw,
        cy + step * 3,
        " x ",
        graphics::WHITE,
        w.bg_color,
    );
    let xh = cx + 10 * 8 + dw + 3 * 8;
    font::render_usize_scaled(
        fb,
        xh,
        cy + step * 3,
        fb.height as usize,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    let hw = font::digit_width(fb.height as usize, 1);
    font::render_str(
        fb,
        xh + hw,
        cy + step * 3,
        " @ 32bpp",
        graphics::WHITE,
        w.bg_color,
    );
    font::render_str(
        fb,
        cx,
        cy + step * 4,
        "Serial  : COM1 @ 115200 8N1",
        graphics::CYAN,
        w.bg_color,
    );
    // RAM stats from BitmapFrameAllocator (Track B MB1).
    font::render_str(
        fb,
        cx,
        cy + step * 5,
        "Ram     : ",
        graphics::CYAN,
        w.bg_color,
    );
    font::render_usize_scaled(
        fb,
        cx + 10 * 8,
        cy + step * 5,
        free_mib as usize,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    let fw = font::digit_width(free_mib as usize, 1);
    font::render_str(
        fb,
        cx + 10 * 8 + fw,
        cy + step * 5,
        " /",
        graphics::DARK_GRAY,
        w.bg_color,
    );
    font::render_usize_scaled(
        fb,
        cx + 10 * 8 + fw + 2 * 8 + 4,
        cy + step * 5,
        total_mib as usize,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    let tw = font::digit_width(total_mib as usize, 1);
    font::render_str(
        fb,
        cx + 10 * 8 + fw + 2 * 8 + 4 + tw,
        cy + step * 5,
        " MiB free",
        graphics::LIGHT_CYAN,
        w.bg_color,
    );
    // Track B MB2: page-table walker.
    font::render_str(
        fb,
        cx,
        cy + step * 6,
        "Paging  : mapper ready (MB2)",
        graphics::CYAN,
        w.bg_color,
    );
    // Track B MB3: IDT loaded.
    font::render_str(
        fb,
        cx,
        cy + step * 7,
        "IDT     : loaded #DE #DF #GP #PF",
        graphics::CYAN,
        w.bg_color,
    );
    // Track B MB4: syscall dispatcher.
    font::render_str(
        fb,
        cx,
        cy + step * 8,
        "Syscall : LSTAR+INT80 ready (MB4)",
        graphics::CYAN,
        w.bg_color,
    );
    // Track B MB5: ELF64 loader.
    font::render_str(
        fb,
        cx,
        cy + step * 9,
        "ELF     : parser ready (MB5)",
        graphics::CYAN,
        w.bg_color,
    );

    // ── CPU rows (MB14.a/c.1 + CPUID snapshot) ────────────────────────
    let snap = cpuinfo::snapshot();

    // Row 10 — CPU brand (CPUID 0x80000002..4).
    font::render_str(
        fb,
        cx,
        cy + step * 10,
        "CPU     : ",
        graphics::CYAN,
        w.bg_color,
    );
    {
        let brand = cpuinfo::trim_brand(&snap.brand);
        let brand_str = core::str::from_utf8(brand).unwrap_or("(non-ASCII)");
        font::render_str(
            fb,
            cx + 10 * 8,
            cy + step * 10,
            brand_str,
            graphics::WHITE,
            w.bg_color,
        );
    }

    // Row 11 — CPU vendor (CPUID 0).
    font::render_str(
        fb,
        cx,
        cy + step * 11,
        "Vendor  : ",
        graphics::CYAN,
        w.bg_color,
    );
    {
        let vendor_str = core::str::from_utf8(&snap.vendor).unwrap_or("(non-ASCII)");
        font::render_str(
            fb,
            cx + 10 * 8,
            cy + step * 11,
            vendor_str,
            graphics::WHITE,
            w.bg_color,
        );
    }

    // Row 12 — Family / Model / Stepping (CPUID 1 EAX decoded).
    font::render_str(
        fb,
        cx,
        cy + step * 12,
        "CPUID   : Family ",
        graphics::CYAN,
        w.bg_color,
    );
    let fm = snap.family_model;
    let mut x_cursor = cx + 17 * 8;
    font::render_usize_scaled(
        fb,
        x_cursor,
        cy + step * 12,
        fm.family as usize,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    x_cursor += font::digit_width(fm.family as usize, 1);
    font::render_str(
        fb,
        x_cursor,
        cy + step * 12,
        " Model ",
        graphics::CYAN,
        w.bg_color,
    );
    x_cursor += 7 * 8;
    font::render_usize_scaled(
        fb,
        x_cursor,
        cy + step * 12,
        fm.model as usize,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    x_cursor += font::digit_width(fm.model as usize, 1);
    font::render_str(
        fb,
        x_cursor,
        cy + step * 12,
        " Step ",
        graphics::CYAN,
        w.bg_color,
    );
    x_cursor += 6 * 8;
    font::render_usize_scaled(
        fb,
        x_cursor,
        cy + step * 12,
        fm.stepping as usize,
        graphics::WHITE,
        w.bg_color,
        1,
    );

    // Row 13 — logical cores enumerated via the MADT.
    font::render_str(
        fb,
        cx,
        cy + step * 13,
        "Cores   : ",
        graphics::CYAN,
        w.bg_color,
    );
    let mut xc = cx + 10 * 8;
    font::render_usize_scaled(
        fb,
        xc,
        cy + step * 13,
        cpu_total,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    xc += font::digit_width(cpu_total, 1);
    font::render_str(
        fb,
        xc,
        cy + step * 13,
        " logical (BSP+",
        graphics::DARK_GRAY,
        w.bg_color,
    );
    xc += 14 * 8;
    let ap_count = cpu_total.saturating_sub(1);
    font::render_usize_scaled(
        fb,
        xc,
        cy + step * 13,
        ap_count,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    xc += font::digit_width(ap_count, 1);
    font::render_str(
        fb,
        xc,
        cy + step * 13,
        " AP)",
        graphics::DARK_GRAY,
        w.bg_color,
    );

    // Row 14 — BSP LAPIC ID + APIC mode.
    font::render_str(
        fb,
        cx,
        cy + step * 14,
        "APIC    : BSP id=",
        graphics::CYAN,
        w.bg_color,
    );
    let mut xa = cx + 17 * 8;
    font::render_usize_scaled(
        fb,
        xa,
        cy + step * 14,
        cpu_bsp_apic_id as usize,
        graphics::WHITE,
        w.bg_color,
        1,
    );
    xa += font::digit_width(cpu_bsp_apic_id as usize, 1);
    font::render_str(
        fb,
        xa,
        cy + step * 14,
        " mode=xAPIC",
        graphics::DARK_GRAY,
        w.bg_color,
    );

    // Row 15 — feature flags (selected subset of CPUID 1/7).
    font::render_str(
        fb,
        cx,
        cy + step * 15,
        "Feats   : ",
        graphics::CYAN,
        w.bg_color,
    );
    {
        let feats = cpuinfo::trim_feature_summary(&snap.feature_summary);
        let feats_str = core::str::from_utf8(feats).unwrap_or("(non-ASCII)");
        font::render_str(
            fb,
            cx + 10 * 8,
            cy + step * 15,
            feats_str,
            graphics::LIGHT_CYAN,
            w.bg_color,
        );
    }

    widget::draw_button(fb, w.x + 1, w.y + wm::TITLEBAR_H, &POWEROFF_BTN, false);
}

/// Render the static content of the Terminal window (title + hint).
fn render_terminal_static(fb: &FrameBuffer, wm_state: &wm::WindowManager) {
    let Some(w) = wm_state.get(1) else { return };
    let tx = w.x + 8;
    let ty = w.y + wm::TITLEBAR_H + 8;
    font::render_str(
        fb,
        tx,
        ty,
        "OMNI OS Shell  v0.1-alpha",
        graphics::LIGHT_CYAN,
        w.bg_color,
    );
    font::render_str(
        fb,
        tx,
        ty + 14,
        "Tab/Enter: focus | Mouse/Arrows: cursor | ESC: off",
        graphics::DARK_GRAY,
        w.bg_color,
    );
}

/// Render the Build Info window content.
///
/// Surfaces build-time metadata (git commit, branch, build timestamp) injected
/// by `crates/omni-kernel/build.rs`, plus static milestone status so the
/// running image is self-describing without consulting the host repo.
fn render_buildinfo(fb: &FrameBuffer, wm_state: &wm::WindowManager) {
    let Some(w) = wm_state.get(2) else { return };
    let cx_l = w.x + 8;
    #[allow(
        clippy::integer_division,
        reason = "half-width column split; sub-pixel precision is meaningless on a pixel grid"
    )]
    let cx_r = w.x + w.width / 2 + 8;
    let cy = w.y + wm::TITLEBAR_H + 8;
    let step: u32 = 14;

    // Helper closure: "Label  : value" with cyan label + white value.
    let row =
        |row_idx: u32, col_x: u32, label: &str, label_w: u32, value: &str, value_color: u32| {
            font::render_str(
                fb,
                col_x,
                cy + step * row_idx,
                label,
                graphics::CYAN,
                w.bg_color,
            );
            font::render_str(
                fb,
                col_x + label_w * 8,
                cy + step * row_idx,
                value,
                value_color,
                w.bg_color,
            );
        };

    // ── Left column: identity + provenance ─────────────────────────────
    row(
        0,
        cx_l,
        "Version  : omni-kernel v",
        23,
        env!("CARGO_PKG_VERSION"),
        graphics::WHITE,
    );
    row(
        1,
        cx_l,
        "Branch   : ",
        11,
        env!("OMNI_GIT_BRANCH"),
        graphics::WHITE,
    );
    row(
        2,
        cx_l,
        "Commit   : ",
        11,
        env!("OMNI_GIT_HASH"),
        graphics::LIGHT_CYAN,
    );
    row(
        3,
        cx_l,
        "Built    : ",
        11,
        env!("OMNI_BUILD_DATE"),
        graphics::WHITE,
    );
    row(
        4,
        cx_l,
        "Rustc    : ",
        11,
        "1.85 stable + x86_64-unknown-none",
        graphics::WHITE,
    );
    row(5, cx_l, "License  : ", 11, "AGPL-3.0-only", graphics::WHITE);
    row(
        6,
        cx_l,
        "Repo     : ",
        11,
        "github.com/CySalazar/omni",
        graphics::WHITE,
    );

    // ── Right column: implementation status ────────────────────────────
    row(
        0,
        cx_r,
        "Phase    : ",
        11,
        "1 - Microkernel POC  (~99.95%)",
        graphics::WHITE,
    );
    row(
        1,
        cx_r,
        "Track A  : ",
        11,
        "Desktop M1-M5  OK",
        graphics::WHITE,
    );
    row(
        2,
        cx_r,
        "Track B  : ",
        11,
        "MB1-MB14 OK (cycle closed)",
        graphics::WHITE,
    );
    row(
        3,
        cx_r,
        "Active   : ",
        11,
        "P6.7.9-pre.10 PT wire DriverLoad",
        graphics::LIGHT_CYAN,
    );
    row(
        4,
        cx_r,
        "Next     : ",
        11,
        "P6.7.9-pre.11 PT install DTE",
        graphics::WHITE,
    );
    row(
        5,
        cx_r,
        "Tests    : ",
        11,
        "1192 workspace pass",
        graphics::WHITE,
    );
    row(6, cx_r, "Author   : ", 11, "cySalazar", graphics::WHITE);
}

/// Render the terminal input line: prompt + echo buffer + cursor indicator.
fn render_echo(fb: &FrameBuffer, wm_state: &wm::WindowManager, echo: &[u8; 40], len: usize) {
    let Some(w) = wm_state.get(1) else { return };
    let tx = w.x + 8;
    let ty = w.y + wm::TITLEBAR_H + 8 + 32;
    let x_end = w.x + w.width - 1;

    // Clear input row.
    fb.draw_rect_filled(tx, ty, x_end, ty + 8, w.bg_color);

    // Prompt.
    font::render_str(fb, tx, ty, "omni@kernel:~$ ", graphics::CYAN, w.bg_color);

    // Typed chars.
    let echo_x = tx + 15 * 8;
    let visible = echo.get(..len).unwrap_or(&[]);
    if let Ok(echo_str) = core::str::from_utf8(visible) {
        font::render_str(fb, echo_x, ty, echo_str, graphics::WHITE, w.bg_color);
    }

    // Cursor indicator.
    if len < 40 {
        font::render_str(
            fb,
            echo_x + len as u32 * 8,
            ty,
            "|",
            graphics::LIGHT_CYAN,
            w.bg_color,
        );
    }
}

/// VGA text-mode fallback banner (no GOP framebuffer).
fn render_vga_fallback(region_count: usize, free_mib: u32, total_mib: u32) {
    vga::clear(vga::WHITE, vga::BLACK);
    vga::write_at(3, 2, &[0xCD_u8; 76], vga::CYAN, vga::BLACK);
    vga::write_at(7, 2, &[0xCD_u8; 76], vga::CYAN, vga::BLACK);
    vga::write_at(4, 27, b"  O M N I   O S  ", vga::YELLOW, vga::BLACK);
    vga::write_at(5, 27, b"  Boot Demo       ", vga::WHITE, vga::BLACK);
    vga::write_at(6, 27, b"  v", vga::WHITE, vga::BLACK);
    vga::write_at(
        6,
        30,
        env!("CARGO_PKG_VERSION").as_bytes(),
        vga::LIGHT_CYAN,
        vga::BLACK,
    );
    vga::write_at(9, 4, b"Kernel:", vga::LIGHT_CYAN, vga::BLACK);
    vga::write_at(9, 20, b"omni-kernel v", vga::WHITE, vga::BLACK);
    vga::write_at(
        9,
        33,
        env!("CARGO_PKG_VERSION").as_bytes(),
        vga::WHITE,
        vga::BLACK,
    );
    vga::write_at(10, 4, b"Boot mode:", vga::LIGHT_CYAN, vga::BLACK);
    vga::write_at(
        10,
        20,
        b"UEFI / bootloader 0.11 (OVMF)",
        vga::WHITE,
        vga::BLACK,
    );
    vga::write_at(11, 4, b"Memory:", vga::LIGHT_CYAN, vga::BLACK);
    vga::write_usize_at(11, 20, region_count, vga::WHITE, vga::BLACK);
    vga::write_at(11, 23, b"physical regions mapped", vga::WHITE, vga::BLACK);
    vga::write_at(12, 4, b"Ram:", vga::LIGHT_CYAN, vga::BLACK);
    vga::write_usize_at(12, 20, free_mib as usize, vga::WHITE, vga::BLACK);
    vga::write_at(12, 23, b"/", vga::WHITE, vga::BLACK);
    vga::write_usize_at(12, 25, total_mib as usize, vga::WHITE, vga::BLACK);
    vga::write_at(12, 28, b"MiB free", vga::LIGHT_CYAN, vga::BLACK);
    vga::write_at(13, 4, b"Serial:", vga::LIGHT_CYAN, vga::BLACK);
    vga::write_at(13, 20, b"COM1 @ 115200 8N1", vga::WHITE, vga::BLACK);
    vga::write_at(
        14,
        4,
        b"Syscall: LSTAR+INT80 ready (MB4)",
        vga::LIGHT_CYAN,
        vga::BLACK,
    );
    vga::write_at(
        15,
        4,
        b"ELF  : parser ready (MB5)     ",
        vga::LIGHT_CYAN,
        vga::BLACK,
    );
    vga::write_at(
        16,
        4,
        b"Powering off in:    seconds  ",
        vga::YELLOW,
        vga::BLACK,
    );
}
