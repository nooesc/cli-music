#![allow(dead_code)]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAnimationContext, NSBackingStoreType, NSColor, NSFont, NSScreen, NSTextField,
    NSTextAlignment, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowCollectionBehavior,
    NSWindowStyleMask, NSStatusWindowLevel,
};
use objc2_foundation::{ns_string, NSPoint, NSRect, NSSize};
use std::cell::RefCell;

// CGFloat is f64 on 64-bit macOS (the only supported target).
type CGFloat = f64;

/// HUD window size in points.
const HUD_SIZE: f64 = 200.0;

/// Corner radius for the HUD window.
const CORNER_RADIUS: f64 = 18.0;

/// Duration in seconds for the fade-out animation.
const FADE_DURATION: f64 = 0.3;

/// Number of volume bar segments.
const BAR_COUNT: usize = 16;

/// Layout constants for the volume indicator.
const BAR_ROW_WIDTH: f64 = 160.0;
const BAR_HEIGHT: f64 = 8.0;
const BAR_GAP: f64 = 2.5;
const BAR_Y: f64 = 30.0; // distance from bottom of HUD
const BAR_RADIUS: f64 = 2.0;
const ICON_FONT_SIZE: f64 = 68.0;

/// Retained references to the volume indicator subviews so we can update
/// them cheaply from `show_hud` without traversing the view hierarchy.
struct IndicatorViews {
    icon_label: Retained<NSTextField>,
    bars: Vec<Retained<NSView>>,
}

thread_local! {
    static INDICATOR: RefCell<Option<IndicatorViews>> = const { RefCell::new(None) };
}

/// Create a translucent, borderless, always-on-top HUD window.
///
/// The window is created hidden (alpha = 0) and must be shown explicitly
/// via [`show_hud`].
pub fn create_hud_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(HUD_SIZE, HUD_SIZE));

    // Borderless = 0, meaning no style mask bits set.
    let style = NSWindowStyleMask::Borderless;

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            content_rect,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };

    // Make the window transparent so the visual effect view shows through.
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(false);

    // Float above most windows (status window level = 25).
    window.setLevel(NSStatusWindowLevel);

    // Ignore all mouse interaction — the HUD is display-only.
    window.setIgnoresMouseEvents(true);

    // Transient + can join all spaces, so the HUD appears on every desktop.
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

    // --- Visual effect (blur) content view ---
    let effect_view =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content_rect);

    effect_view.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect_view.setState(NSVisualEffectState::Active);

    // Enable layer backing and set corner radius via Core Animation.
    effect_view.setWantsLayer(true);
    unsafe {
        // layer() requires the objc2-quartz-core feature which we don't want
        // to pull in, so we use raw msg_send! instead.
        let layer: *mut AnyObject = msg_send![&effect_view, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: CORNER_RADIUS as CGFloat];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
    }

    // --- Volume indicator subviews ---
    let (icon_label, bars) = create_indicator_views(mtm, &effect_view);

    // Install as the window's content view.  We upcast to &NSView because
    // setContentView expects Option<&NSView>.
    let view: &NSView = &effect_view;
    window.setContentView(Some(view));

    // Store indicator references for later use in show_hud.
    INDICATOR.with(|cell| {
        *cell.borrow_mut() = Some(IndicatorViews { icon_label, bars });
    });

    // Start fully transparent.
    window.setAlphaValue(0.0);

    // Center on screen now so the position is ready when first shown.
    center_on_screen(&window, mtm);

    window
}

/// Create the speaker icon label and 16 volume bar segment views, adding
/// them as subviews of `parent`.  Returns retained references for later
/// updates.
fn create_indicator_views(
    mtm: MainThreadMarker,
    parent: &NSVisualEffectView,
) -> (Retained<NSTextField>, Vec<Retained<NSView>>) {
    // --- Speaker icon ---
    let icon_label = NSTextField::labelWithString(ns_string!("\u{1F50A}"), mtm);
    icon_label.setEditable(false);
    icon_label.setSelectable(false);
    icon_label.setBordered(false);
    icon_label.setDrawsBackground(false);
    icon_label.setTextColor(Some(&NSColor::whiteColor()));
    icon_label.setFont(Some(&NSFont::systemFontOfSize(ICON_FONT_SIZE)));
    icon_label.setAlignment(NSTextAlignment(2)); // Center = 2

    // Position the icon label centered horizontally, in the upper portion.
    // The label needs enough room for a large emoji; give it a generous frame.
    let icon_width = HUD_SIZE;
    let icon_height = ICON_FONT_SIZE + 20.0;
    let icon_y = HUD_SIZE - icon_height - 20.0; // 20pt from top
    let icon_frame = NSRect::new(NSPoint::new(0.0, icon_y), NSSize::new(icon_width, icon_height));
    icon_label.setFrame(icon_frame);

    let icon_view: &NSView = &icon_label;
    parent.addSubview(icon_view);

    // --- Volume bar segments ---
    let total_gap = BAR_GAP * (BAR_COUNT as f64 - 1.0);
    let segment_width = (BAR_ROW_WIDTH - total_gap) / BAR_COUNT as f64;
    let bar_x_start = (HUD_SIZE - BAR_ROW_WIDTH) / 2.0;

    let unfilled_color = NSColor::colorWithWhite_alpha(0.3, 1.0);

    let mut bars = Vec::with_capacity(BAR_COUNT);
    for i in 0..BAR_COUNT {
        let x = bar_x_start + i as f64 * (segment_width + BAR_GAP);
        let frame = NSRect::new(NSPoint::new(x, BAR_Y), NSSize::new(segment_width, BAR_HEIGHT));

        let bar_view = NSView::initWithFrame(NSView::alloc(mtm), frame);
        bar_view.setWantsLayer(true);

        // Set initial color to unfilled (dark gray).
        unsafe {
            let layer: *mut AnyObject = msg_send![&bar_view, layer];
            if !layer.is_null() {
                let cg_color: *mut AnyObject = msg_send![&*unfilled_color, CGColor];
                let _: () = msg_send![layer, setBackgroundColor: cg_color];
                let _: () = msg_send![layer, setCornerRadius: BAR_RADIUS as CGFloat];
            }
        }

        let view_ref: &NSView = &bar_view;
        parent.addSubview(view_ref);
        bars.push(bar_view);
    }

    (icon_label, bars)
}

/// Position the window at the centre of the main screen, shifted slightly
/// downward (30 % from the top).
pub fn center_on_screen(window: &NSWindow, mtm: MainThreadMarker) {
    let Some(screen) = NSScreen::mainScreen(mtm) else {
        return;
    };
    let screen_frame = screen.frame();
    let window_frame = window.frame();

    let x = screen_frame.origin.x + (screen_frame.size.width - window_frame.size.width) / 2.0;
    // Place the window slightly below vertical centre (30% from top).
    let y = screen_frame.origin.y + (screen_frame.size.height - window_frame.size.height) * 0.40;

    window.setFrameOrigin(NSPoint::new(x, y));
}

/// Show the HUD window immediately (alpha = 1, ordered front).
///
/// Updates the speaker icon and volume bar segments to reflect the current
/// `volume` (0.0..=1.0) and `muted` state.
pub fn show_hud(window: &NSWindow, volume: f32, muted: bool) {
    // Update the volume indicator before showing.
    INDICATOR.with(|cell| {
        let borrow = cell.borrow();
        let Some(indicator) = borrow.as_ref() else {
            return;
        };

        // Update speaker icon.
        let icon = if muted {
            ns_string!("\u{1F507}") // muted speaker
        } else if volume < 0.01 {
            ns_string!("\u{1F508}") // speaker low (no waves)
        } else if volume < 0.34 {
            ns_string!("\u{1F509}") // speaker medium (one wave)
        } else {
            ns_string!("\u{1F50A}") // speaker high (three waves)
        };
        indicator.icon_label.setStringValue(icon);

        // Determine how many bars are filled.
        let filled = if muted {
            0
        } else {
            ((volume * BAR_COUNT as f32).round() as usize).min(BAR_COUNT)
        };

        let white = NSColor::whiteColor();
        let gray = NSColor::colorWithWhite_alpha(0.3, 1.0);

        for (i, bar) in indicator.bars.iter().enumerate() {
            let color = if i < filled { &*white } else { &*gray };
            unsafe {
                let layer: *mut AnyObject = msg_send![bar, layer];
                if !layer.is_null() {
                    let cg_color: *mut AnyObject = msg_send![color, CGColor];
                    let _: () = msg_send![layer, setBackgroundColor: cg_color];
                }
            }
        }
    });

    window.setAlphaValue(1.0);
    window.orderFrontRegardless();
}

/// Fade the HUD window out over [`FADE_DURATION`] seconds using
/// `NSAnimationContext`.
pub fn hide_hud(window: &NSWindow) {
    NSAnimationContext::beginGrouping();
    let ctx = NSAnimationContext::currentContext();
    ctx.setDuration(FADE_DURATION);
    ctx.setAllowsImplicitAnimation(true);

    // The animator proxy forwards setAlphaValue through Core Animation so
    // that the change is animated over `duration` seconds.
    unsafe {
        let animator: Retained<AnyObject> = msg_send![window, animator];
        let _: () = msg_send![&*animator, setAlphaValue: 0.0 as CGFloat];
    }

    NSAnimationContext::endGrouping();
}
