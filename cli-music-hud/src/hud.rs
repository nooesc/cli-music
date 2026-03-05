use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAnimationContext, NSBackingStoreType, NSColor, NSImageView, NSScreen, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
    NSStatusWindowLevel,
};
use objc2_foundation::{ns_string, NSPoint, NSRect, NSSize, NSString};
use std::cell::RefCell;

// CGFloat is f64 on 64-bit macOS (the only supported target).
type CGFloat = f64;

// Opaque CGColor struct for msg_send! type encoding (`^{CGColor=}`).
// CoreGraphics' CGColorRef is `*const CGColor` — we only pass it through.
#[repr(C)]
struct CGColor {
    _priv: [u8; 0],
}

unsafe impl objc2::encode::RefEncode for CGColor {
    const ENCODING_REF: objc2::encode::Encoding = objc2::encode::Encoding::Pointer(
        &objc2::encode::Encoding::Struct("CGColor", &[]),
    );
}

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

/// SF Symbol icon layout.
const ICON_POINT_SIZE: f64 = 56.0;
const ICON_AREA_SIZE: f64 = 120.0;

/// Retained references to the volume indicator subviews so we can update
/// them cheaply from `show_hud` without traversing the view hierarchy.
struct IndicatorViews {
    icon_view: Retained<NSImageView>,
    bars: Vec<Retained<NSView>>,
    unfilled_color: Retained<NSColor>,
}

thread_local! {
    static INDICATOR: RefCell<Option<IndicatorViews>> = const { RefCell::new(None) };
}

/// Create an SF Symbol image with variable value (0.0–1.0) for wave intensity.
///
/// Uses `[NSImage imageWithSystemSymbolName:variableValue:accessibilityDescription:]`
/// so the speaker body stays the same size while wave arcs fade in/out.
///
/// Returns a raw pointer to an autoreleased NSImage, or null on failure.
unsafe fn create_symbol_image(name: &NSString, variable_value: f64) -> *mut AnyObject {
    let cls = AnyClass::get(c"NSImage").unwrap();
    let none: Option<&NSString> = None;
    let image: *mut AnyObject = msg_send![cls,
        imageWithSystemSymbolName: name,
        variableValue: variable_value as CGFloat,
        accessibilityDescription: none
    ];
    if image.is_null() {
        return std::ptr::null_mut();
    }

    // Apply symbol configuration: light weight for thin sound waves.
    let config_cls = AnyClass::get(c"NSImageSymbolConfiguration").unwrap();
    let config: *mut AnyObject = msg_send![config_cls,
        configurationWithPointSize: ICON_POINT_SIZE as CGFloat,
        weight: -0.4 as CGFloat, // NSFontWeightLight
        scale: 3isize            // NSImageSymbolScaleLarge
    ];

    let configured: *mut AnyObject = msg_send![image, imageWithSymbolConfiguration: config];
    if !configured.is_null() {
        configured
    } else {
        image
    }
}

/// Set the SF Symbol image on the icon NSImageView.
unsafe fn set_icon_image(icon_view: &NSImageView, symbol_name: &NSString, variable_value: f64) {
    let image = create_symbol_image(symbol_name, variable_value);
    if !image.is_null() {
        let _: () = msg_send![icon_view, setImage: image];
    }
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
        let layer: *mut AnyObject = msg_send![&effect_view, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: CORNER_RADIUS as CGFloat];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
    }

    // --- Volume indicator subviews ---
    let (icon_view, bars) = create_indicator_views(mtm, &effect_view);

    // Install as the window's content view.
    let view: &NSView = &effect_view;
    window.setContentView(Some(view));

    // Store indicator references for later use in show_hud.
    let unfilled_color = NSColor::colorWithWhite_alpha(0.3, 1.0);
    INDICATOR.with(|cell| {
        *cell.borrow_mut() = Some(IndicatorViews { icon_view, bars, unfilled_color });
    });

    // Start fully transparent.
    window.setAlphaValue(0.0);

    // Center on the active screen so the position is ready when first shown.
    center_on_active_screen(&window);

    window
}

/// Create the speaker icon (NSImageView with SF Symbol) and 16 volume bar
/// segment views, adding them as subviews of `parent`. Returns retained
/// references for later updates.
fn create_indicator_views(
    mtm: MainThreadMarker,
    parent: &NSVisualEffectView,
) -> (Retained<NSImageView>, Vec<Retained<NSView>>) {
    // --- Speaker icon (SF Symbol via NSImageView) ---
    let icon_x = (HUD_SIZE - ICON_AREA_SIZE) / 2.0;
    let icon_y = 55.0; // bottom of icon area, leaving room for bars below
    let icon_frame = NSRect::new(
        NSPoint::new(icon_x, icon_y),
        NSSize::new(ICON_AREA_SIZE, ICON_AREA_SIZE),
    );

    let icon_view = NSImageView::initWithFrame(NSImageView::alloc(mtm), icon_frame);

    unsafe {
        // Set the initial speaker icon at full volume.
        set_icon_image(&icon_view, ns_string!("speaker.wave.3.fill"), 1.0);

        // Scale the symbol to fill the image view proportionally.
        let _: () = msg_send![&icon_view, setImageScaling: 3isize]; // NSImageScaleProportionallyUpOrDown

        // White tint for visibility on the dark HUD background.
        let _: () = msg_send![&icon_view, setContentTintColor: &*NSColor::whiteColor()];

        // Disable editing (no drag-and-drop).
        let _: () = msg_send![&icon_view, setEditable: false];
    }

    let icon_as_view: &NSView = &icon_view;
    parent.addSubview(icon_as_view);

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
                let cg_color: *const CGColor = msg_send![&*unfilled_color, CGColor];
                let _: () = msg_send![layer, setBackgroundColor: cg_color];
                let _: () = msg_send![layer, setCornerRadius: BAR_RADIUS as CGFloat];
            }
        }

        let view_ref: &NSView = &bar_view;
        parent.addSubview(view_ref);
        bars.push(bar_view);
    }

    (icon_view, bars)
}

/// Position the window centered horizontally on the screen that currently has
/// keyboard focus, in the lower portion (15% from bottom).
///
/// Uses the mouse location to find the active screen so the HUD follows the
/// user across displays.
fn center_on_active_screen(window: &NSWindow) {
    let Some(mtm) = MainThreadMarker::new() else { return };

    // Find the screen containing the mouse cursor.
    let mouse_loc = unsafe {
        let loc: NSPoint = msg_send![objc2::runtime::AnyClass::get(c"NSEvent").unwrap(), mouseLocation];
        loc
    };

    let screens = NSScreen::screens(mtm);
    let screen = screens
        .iter()
        .find(|s| {
            let f = s.frame();
            mouse_loc.x >= f.origin.x
                && mouse_loc.x < f.origin.x + f.size.width
                && mouse_loc.y >= f.origin.y
                && mouse_loc.y < f.origin.y + f.size.height
        })
        .or_else(|| screens.iter().next());

    let Some(screen) = screen else { return };
    let screen_frame = screen.frame();
    let window_frame = window.frame();

    let x = screen_frame.origin.x + (screen_frame.size.width - window_frame.size.width) / 2.0;
    let y = screen_frame.origin.y + (screen_frame.size.height - window_frame.size.height) * 0.15;

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

        // Always use the 3-wave icon with variable value so the speaker body
        // stays a consistent size while wave arcs fade with volume.
        let (symbol, var_val) = if muted {
            (ns_string!("speaker.slash.fill"), 0.0)
        } else {
            (ns_string!("speaker.wave.3.fill"), volume as f64)
        };
        unsafe {
            set_icon_image(&indicator.icon_view, symbol, var_val);
        }

        // Determine how many bars are filled.
        let filled = if muted {
            0
        } else {
            ((volume * BAR_COUNT as f32).round() as usize).min(BAR_COUNT)
        };

        let white = NSColor::whiteColor();

        for (i, bar) in indicator.bars.iter().enumerate() {
            let color = if i < filled { &*white } else { &*indicator.unfilled_color };
            unsafe {
                let layer: *mut AnyObject = msg_send![bar, layer];
                if !layer.is_null() {
                    let cg_color: *const CGColor = msg_send![color, CGColor];
                    let _: () = msg_send![layer, setBackgroundColor: cg_color];
                }
            }
        }
    });

    // Re-center on whichever screen has the mouse so the HUD follows the user.
    center_on_active_screen(window);

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
