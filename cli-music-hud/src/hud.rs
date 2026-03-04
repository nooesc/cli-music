#![allow(dead_code)]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAnimationContext, NSBackingStoreType, NSColor, NSScreen, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSStatusWindowLevel,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

// CGFloat is f64 on 64-bit macOS (the only supported target).
type CGFloat = f64;

/// HUD window size in points.
const HUD_SIZE: f64 = 200.0;

/// Corner radius for the HUD window.
const CORNER_RADIUS: f64 = 18.0;

/// Duration in seconds for the fade-out animation.
const FADE_DURATION: f64 = 0.3;

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

    // Install as the window's content view.  We upcast to &NSView because
    // setContentView expects Option<&NSView>.
    let view: &NSView = &effect_view;
    window.setContentView(Some(view));

    // Start fully transparent.
    window.setAlphaValue(0.0);

    // Center on screen now so the position is ready when first shown.
    center_on_screen(&window, mtm);

    window
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
/// `volume` (0.0..=1.0) and `muted` are provided for future use by the
/// drawing layer (Task 6) — for now this function only manages visibility.
pub fn show_hud(window: &NSWindow, _volume: f32, _muted: bool) {
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
