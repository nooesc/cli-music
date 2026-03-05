use crate::config::HudStyle;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAnimationContext, NSBackingStoreType, NSColor, NSImageView, NSScreen, NSTextField, NSView,
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

/// Duration in seconds for the fade-out animation.
const FADE_DURATION: f64 = 0.3;

// ─── Legacy constants ────────────────────────────────────────────────────────

/// Legacy HUD window size in points.
const LEGACY_SIZE: f64 = 200.0;

/// Legacy corner radius.
const LEGACY_CORNER_RADIUS: f64 = 18.0;

/// Number of volume bar segments (legacy).
const BAR_COUNT: usize = 16;

/// Layout constants for the legacy volume indicator.
const BAR_ROW_WIDTH: f64 = 160.0;
const BAR_HEIGHT: f64 = 8.0;
const BAR_GAP: f64 = 2.5;
const BAR_Y: f64 = 30.0;
const BAR_RADIUS: f64 = 2.0;

/// Legacy SF Symbol icon layout.
const LEGACY_ICON_POINT_SIZE: f64 = 56.0;
const LEGACY_ICON_AREA_SIZE: f64 = 120.0;

// ─── Modern constants ────────────────────────────────────────────────────────

/// Modern pill dimensions.
const MODERN_WIDTH: f64 = 300.0;
const MODERN_HEIGHT: f64 = 50.0;

/// Modern corner radius (pill shape).
const MODERN_CORNER_RADIUS: f64 = 25.0;

/// Modern SF Symbol icon size.
const MODERN_ICON_POINT_SIZE: f64 = 20.0;
const MODERN_ICON_AREA_SIZE: f64 = 28.0;

/// Modern layout padding.
const MODERN_PADDING: f64 = 14.0;

/// Modern fill bar height.
const MODERN_BAR_HEIGHT: f64 = 6.0;
const MODERN_BAR_RADIUS: f64 = 3.0;

/// Modern percentage label width.
const MODERN_LABEL_WIDTH: f64 = 40.0;

/// Inner border opacity for glass edge.
const MODERN_BORDER_OPACITY: f64 = 0.15;
const MODERN_BORDER_WIDTH: f64 = 1.0;

/// Computed track bar geometry (derived from layout constants).
const MODERN_BAR_X: f64 = MODERN_PADDING + MODERN_ICON_AREA_SIZE + 10.0;
const MODERN_BAR_WIDTH: f64 =
    MODERN_WIDTH - MODERN_PADDING - MODERN_LABEL_WIDTH - MODERN_BAR_X - 10.0;

// ─── Shared types ────────────────────────────────────────────────────────────

/// Retained references for the legacy HUD indicator subviews.
struct LegacyIndicator {
    icon_view: Retained<NSImageView>,
    bars: Vec<Retained<NSView>>,
    unfilled_color: Retained<NSColor>,
}

/// Retained references for the modern HUD indicator subviews.
struct ModernIndicator {
    icon_view: Retained<NSImageView>,
    fill_bar: Retained<NSView>,
    _track_bar: Retained<NSView>,
    label: Retained<NSTextField>,
}

/// Which indicator is active for the current HUD.
enum Indicator {
    Legacy(LegacyIndicator),
    Modern(ModernIndicator),
}

thread_local! {
    static INDICATOR: RefCell<Option<Indicator>> = const { RefCell::new(None) };
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// Create an SF Symbol image with variable value (0.0–1.0) for wave intensity.
unsafe fn create_symbol_image(
    name: &NSString,
    variable_value: f64,
    point_size: f64,
) -> *mut AnyObject {
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

    let config_cls = AnyClass::get(c"NSImageSymbolConfiguration").unwrap();
    let config: *mut AnyObject = msg_send![config_cls,
        configurationWithPointSize: point_size as CGFloat,
        weight: -0.4 as CGFloat,
        scale: 3isize
    ];

    let configured: *mut AnyObject = msg_send![image, imageWithSymbolConfiguration: config];
    if !configured.is_null() {
        configured
    } else {
        image
    }
}

/// Set the SF Symbol image on an icon NSImageView.
unsafe fn set_icon_image(
    icon_view: &NSImageView,
    symbol_name: &NSString,
    variable_value: f64,
    point_size: f64,
) {
    let image = create_symbol_image(symbol_name, variable_value, point_size);
    if !image.is_null() {
        let _: () = msg_send![icon_view, setImage: image];
    }
}

/// Configure common window properties shared by both styles.
fn configure_window(window: &NSWindow, has_shadow: bool) {
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(has_shadow);
    window.setLevel(NSStatusWindowLevel);
    window.setIgnoresMouseEvents(true);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
}

/// Position the window centered horizontally on the screen containing the
/// mouse cursor, in the lower portion (15% from bottom).
fn center_on_active_screen(window: &NSWindow) {
    let Some(mtm) = MainThreadMarker::new() else { return };

    let mouse_loc = unsafe {
        let loc: NSPoint =
            msg_send![AnyClass::get(c"NSEvent").unwrap(), mouseLocation];
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
    let sf = screen.frame();
    let wf = window.frame();

    let x = sf.origin.x + (sf.size.width - wf.size.width) / 2.0;
    let y = sf.origin.y + (sf.size.height - wf.size.height) * 0.15;

    window.setFrameOrigin(NSPoint::new(x, y));
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Create the HUD window for the given style.
pub fn create_hud_window(mtm: MainThreadMarker, style: HudStyle) -> Retained<NSWindow> {
    match style {
        HudStyle::Legacy => create_legacy_window(mtm),
        HudStyle::Modern => create_modern_window(mtm),
    }
}

/// Show the HUD, updating the indicator for the current volume and mute state.
pub fn show_hud(window: &NSWindow, volume: f32, muted: bool) {
    INDICATOR.with(|cell| {
        let borrow = cell.borrow();
        let Some(indicator) = borrow.as_ref() else { return };
        match indicator {
            Indicator::Legacy(ind) => show_legacy(ind, volume, muted),
            Indicator::Modern(ind) => show_modern(ind, volume, muted),
        }
    });

    center_on_active_screen(window);
    window.setAlphaValue(1.0);
    window.orderFrontRegardless();
}

/// Fade the HUD window out.
pub fn hide_hud(window: &NSWindow) {
    NSAnimationContext::beginGrouping();
    let ctx = NSAnimationContext::currentContext();
    ctx.setDuration(FADE_DURATION);
    ctx.setAllowsImplicitAnimation(true);

    unsafe {
        let animator: Retained<AnyObject> = msg_send![window, animator];
        let _: () = msg_send![&*animator, setAlphaValue: 0.0 as CGFloat];
    }

    NSAnimationContext::endGrouping();
}

// ─── Legacy HUD ──────────────────────────────────────────────────────────────

fn create_legacy_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let content_rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(LEGACY_SIZE, LEGACY_SIZE),
    );
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            content_rect,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    configure_window(&window, false);

    let effect_view =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content_rect);
    effect_view.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect_view.setState(NSVisualEffectState::Active);
    effect_view.setWantsLayer(true);
    unsafe {
        let layer: *mut AnyObject = msg_send![&effect_view, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: LEGACY_CORNER_RADIUS as CGFloat];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
    }

    let (icon_view, bars) = create_legacy_indicator_views(mtm, &effect_view);
    let view: &NSView = &effect_view;
    window.setContentView(Some(view));

    let unfilled_color = NSColor::colorWithWhite_alpha(0.3, 1.0);
    INDICATOR.with(|cell| {
        *cell.borrow_mut() = Some(Indicator::Legacy(LegacyIndicator {
            icon_view,
            bars,
            unfilled_color,
        }));
    });

    window.setAlphaValue(0.0);
    center_on_active_screen(&window);
    window
}

fn create_legacy_indicator_views(
    mtm: MainThreadMarker,
    parent: &NSVisualEffectView,
) -> (Retained<NSImageView>, Vec<Retained<NSView>>) {
    // Speaker icon.
    let icon_x = (LEGACY_SIZE - LEGACY_ICON_AREA_SIZE) / 2.0;
    let icon_y = 55.0;
    let icon_frame = NSRect::new(
        NSPoint::new(icon_x, icon_y),
        NSSize::new(LEGACY_ICON_AREA_SIZE, LEGACY_ICON_AREA_SIZE),
    );

    let icon_view = NSImageView::initWithFrame(NSImageView::alloc(mtm), icon_frame);
    unsafe {
        set_icon_image(
            &icon_view,
            ns_string!("speaker.wave.3.fill"),
            1.0,
            LEGACY_ICON_POINT_SIZE,
        );
        let _: () = msg_send![&icon_view, setImageScaling: 3isize];
        let _: () = msg_send![&icon_view, setContentTintColor: &*NSColor::whiteColor()];
        let _: () = msg_send![&icon_view, setEditable: false];
    }
    parent.addSubview(&*icon_view as &NSView);

    // Volume bar segments.
    let total_gap = BAR_GAP * (BAR_COUNT as f64 - 1.0);
    let segment_width = (BAR_ROW_WIDTH - total_gap) / BAR_COUNT as f64;
    let bar_x_start = (LEGACY_SIZE - BAR_ROW_WIDTH) / 2.0;
    let unfilled_color = NSColor::colorWithWhite_alpha(0.3, 1.0);

    let mut bars = Vec::with_capacity(BAR_COUNT);
    for i in 0..BAR_COUNT {
        let x = bar_x_start + i as f64 * (segment_width + BAR_GAP);
        let frame = NSRect::new(NSPoint::new(x, BAR_Y), NSSize::new(segment_width, BAR_HEIGHT));
        let bar_view = NSView::initWithFrame(NSView::alloc(mtm), frame);
        bar_view.setWantsLayer(true);
        unsafe {
            let layer: *mut AnyObject = msg_send![&bar_view, layer];
            if !layer.is_null() {
                let cg_color: *const CGColor = msg_send![&*unfilled_color, CGColor];
                let _: () = msg_send![layer, setBackgroundColor: cg_color];
                let _: () = msg_send![layer, setCornerRadius: BAR_RADIUS as CGFloat];
            }
        }
        parent.addSubview(&*bar_view as &NSView);
        bars.push(bar_view);
    }

    (icon_view, bars)
}

fn show_legacy(ind: &LegacyIndicator, volume: f32, muted: bool) {
    let (symbol, var_val) = if muted {
        (ns_string!("speaker.slash.fill"), 0.0)
    } else {
        (ns_string!("speaker.wave.3.fill"), volume as f64)
    };
    unsafe {
        set_icon_image(&ind.icon_view, symbol, var_val, LEGACY_ICON_POINT_SIZE);
    }

    let filled = if muted {
        0
    } else {
        ((volume * BAR_COUNT as f32).round() as usize).min(BAR_COUNT)
    };

    let white = NSColor::whiteColor();
    for (i, bar) in ind.bars.iter().enumerate() {
        let color = if i < filled { &*white } else { &*ind.unfilled_color };
        unsafe {
            let layer: *mut AnyObject = msg_send![bar, layer];
            if !layer.is_null() {
                let cg_color: *const CGColor = msg_send![color, CGColor];
                let _: () = msg_send![layer, setBackgroundColor: cg_color];
            }
        }
    }
}

// ─── Modern HUD ──────────────────────────────────────────────────────────────

fn create_modern_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let content_rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(MODERN_WIDTH, MODERN_HEIGHT),
    );
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            content_rect,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    configure_window(&window, true);

    // ── Visual effect view (deep glassmorphism) ──
    let effect_view =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content_rect);
    effect_view.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect_view.setState(NSVisualEffectState::Active);
    effect_view.setWantsLayer(true);

    unsafe {
        let layer: *mut AnyObject = msg_send![&effect_view, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: MODERN_CORNER_RADIUS as CGFloat];
            let _: () = msg_send![layer, setMasksToBounds: true];

            // Subtle inner border for glass edge definition.
            let border_color = NSColor::colorWithWhite_alpha(1.0, MODERN_BORDER_OPACITY);
            let cg_border: *const CGColor = msg_send![&*border_color, CGColor];
            let _: () = msg_send![layer, setBorderColor: cg_border];
            let _: () = msg_send![layer, setBorderWidth: MODERN_BORDER_WIDTH as CGFloat];
        }
    }

    let indicator = create_modern_indicator_views(mtm, &effect_view);

    let view: &NSView = &effect_view;
    window.setContentView(Some(view));

    INDICATOR.with(|cell| {
        *cell.borrow_mut() = Some(Indicator::Modern(indicator));
    });

    window.setAlphaValue(0.0);
    center_on_active_screen(&window);
    window
}

fn create_modern_indicator_views(
    mtm: MainThreadMarker,
    parent: &NSVisualEffectView,
) -> ModernIndicator {
    let mid_y = (MODERN_HEIGHT - MODERN_BAR_HEIGHT) / 2.0;

    // ── Speaker icon (left side) ──
    let icon_x = MODERN_PADDING;
    let icon_y = (MODERN_HEIGHT - MODERN_ICON_AREA_SIZE) / 2.0;
    let icon_frame = NSRect::new(
        NSPoint::new(icon_x, icon_y),
        NSSize::new(MODERN_ICON_AREA_SIZE, MODERN_ICON_AREA_SIZE),
    );

    let icon_view = NSImageView::initWithFrame(NSImageView::alloc(mtm), icon_frame);
    unsafe {
        set_icon_image(
            &icon_view,
            ns_string!("speaker.wave.3.fill"),
            1.0,
            MODERN_ICON_POINT_SIZE,
        );
        let _: () = msg_send![&icon_view, setImageScaling: 3isize];
        let _: () = msg_send![&icon_view, setContentTintColor: &*NSColor::whiteColor()];
        let _: () = msg_send![&icon_view, setEditable: false];
    }
    parent.addSubview(&*icon_view as &NSView);

    // ── Percentage label (right side) ──
    // NSTextField draws text from the top, so we size it to the font's line
    // height and vertically center it within the pill.
    let label_font_size: f64 = 13.0;
    let label_height: f64 = 18.0; // approximate line height for 13pt font
    let label_x = MODERN_WIDTH - MODERN_PADDING - MODERN_LABEL_WIDTH;
    let label_y = (MODERN_HEIGHT - label_height) / 2.0;
    let label_frame = NSRect::new(
        NSPoint::new(label_x, label_y),
        NSSize::new(MODERN_LABEL_WIDTH, label_height),
    );
    let label = NSTextField::initWithFrame(NSTextField::alloc(mtm), label_frame);
    unsafe {
        let _: () = msg_send![&label, setStringValue: ns_string!("100%")];
        let _: () = msg_send![&label, setBezeled: false];
        let _: () = msg_send![&label, setDrawsBackground: false];
        let _: () = msg_send![&label, setEditable: false];
        let _: () = msg_send![&label, setSelectable: false];
        let _: () = msg_send![&label, setTextColor: &*NSColor::whiteColor()];
        let _: () = msg_send![&label, setAlignment: 2isize]; // NSTextAlignmentRight

        let font_cls = AnyClass::get(c"NSFont").unwrap();
        let font: *mut AnyObject = msg_send![font_cls,
            systemFontOfSize: label_font_size as CGFloat,
            weight: 0.23 as CGFloat // NSFontWeightMedium
        ];
        if !font.is_null() {
            let _: () = msg_send![&label, setFont: font];
        }
    }
    parent.addSubview(&*label as &NSView);

    // ── Volume track (unfilled background bar) ──
    let track_frame = NSRect::new(
        NSPoint::new(MODERN_BAR_X, mid_y),
        NSSize::new(MODERN_BAR_WIDTH, MODERN_BAR_HEIGHT),
    );

    let track_bar = NSView::initWithFrame(NSView::alloc(mtm), track_frame);
    track_bar.setWantsLayer(true);
    unsafe {
        let layer: *mut AnyObject = msg_send![&track_bar, layer];
        if !layer.is_null() {
            let track_color = NSColor::colorWithWhite_alpha(1.0, 0.15);
            let cg_color: *const CGColor = msg_send![&*track_color, CGColor];
            let _: () = msg_send![layer, setBackgroundColor: cg_color];
            let _: () = msg_send![layer, setCornerRadius: MODERN_BAR_RADIUS as CGFloat];
        }
    }
    parent.addSubview(&*track_bar as &NSView);

    // ── Volume fill bar (filled portion) ──
    let fill_frame = NSRect::new(
        NSPoint::new(MODERN_BAR_X, mid_y),
        NSSize::new(MODERN_BAR_WIDTH, MODERN_BAR_HEIGHT),
    );

    let fill_bar = NSView::initWithFrame(NSView::alloc(mtm), fill_frame);
    fill_bar.setWantsLayer(true);
    unsafe {
        let layer: *mut AnyObject = msg_send![&fill_bar, layer];
        if !layer.is_null() {
            let white = NSColor::whiteColor();
            let cg_color: *const CGColor = msg_send![&*white, CGColor];
            let _: () = msg_send![layer, setBackgroundColor: cg_color];
            let _: () = msg_send![layer, setCornerRadius: MODERN_BAR_RADIUS as CGFloat];
        }
    }
    parent.addSubview(&*fill_bar as &NSView);

    ModernIndicator {
        icon_view,
        fill_bar,
        _track_bar: track_bar,
        label,
    }
}

fn show_modern(ind: &ModernIndicator, volume: f32, muted: bool) {
    // Update icon.
    let (symbol, var_val) = if muted {
        (ns_string!("speaker.slash.fill"), 0.0)
    } else {
        (ns_string!("speaker.wave.3.fill"), volume as f64)
    };
    unsafe {
        set_icon_image(&ind.icon_view, symbol, var_val, MODERN_ICON_POINT_SIZE);
    }

    // Update percentage label.
    let pct = if muted {
        0
    } else {
        (volume * 100.0).round() as u32
    };
    let pct_string = format!("{pct}%");
    unsafe {
        let ns_str = objc2_foundation::NSString::from_str(&pct_string);
        let _: () = msg_send![&ind.label, setStringValue: &*ns_str];
    }

    // Update fill bar width using computed constants (not live frame which
    // can be zero before first display). Hide the bar entirely at zero width
    // to avoid CALayerInvalidGeometry from cornerRadius on a zero-width layer.
    let fill_fraction = if muted { 0.0 } else { volume as f64 };
    let fill_width = MODERN_BAR_WIDTH * fill_fraction;

    if fill_width < 1.0 {
        ind.fill_bar.setHidden(true);
    } else {
        ind.fill_bar.setHidden(false);
        let mid_y = (MODERN_HEIGHT - MODERN_BAR_HEIGHT) / 2.0;
        let new_fill_frame = NSRect::new(
            NSPoint::new(MODERN_BAR_X, mid_y),
            NSSize::new(fill_width, MODERN_BAR_HEIGHT),
        );
        ind.fill_bar.setFrame(new_fill_frame);
    }
}
