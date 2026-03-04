#![allow(dead_code)]

use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType, CallbackResult,
};
use std::sync::{Arc, Mutex};

// NX_SYSDEFINED event type (value 14) is not in the CGEventType enum,
// so we transmute it. This is the event type used for media key events
// on macOS (volume, brightness, etc.).
const NX_SYSDEFINED: u32 = 14;

// NX key type constants encoded in the data1 field of NX_SYSDEFINED events.
const NX_KEYTYPE_SOUND_UP: i64 = 0;
const NX_KEYTYPE_SOUND_DOWN: i64 = 1;
const NX_KEYTYPE_MUTE: i64 = 7;

// Subtype indicating auxiliary control button events (media keys).
const NX_SUBTYPE_AUX_CONTROL_BUTTONS: i64 = 8;

// CGEventField values for NX_SYSDEFINED event data.
// These are not in the EventField struct from core-graphics.
const EVENT_FIELD_SUBTYPE: u32 = 110; // 0x6E
const EVENT_FIELD_DATA1: u32 = 111; // 0x6F

// Raw FFI binding to re-enable an event tap from the callback.
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

/// Represents the type of volume key event intercepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKey {
    Up,
    Down,
    Mute,
}

/// A wrapper around a raw `CFMachPortRef` that is safe to send between threads.
///
/// We only use this to call `CGEventTapEnable`, which is documented as safe
/// for this purpose. The mach port lifetime is managed by the `CGEventTap`
/// struct on the owning thread.
#[derive(Clone, Copy)]
struct SendableMachPortRef(CFMachPortRef);

// SAFETY: CFMachPortRef is a Core Foundation reference type. We only use it
// to call CGEventTapEnable, which is thread-safe for re-enabling an event tap.
// The underlying mach port is kept alive by the CGEventTap on the run loop thread.
unsafe impl Send for SendableMachPortRef {}
unsafe impl Sync for SendableMachPortRef {}

/// Attempts to decode a volume key press from an NX_SYSDEFINED CGEvent.
///
/// Volume keys arrive as NX_SYSDEFINED events with:
/// - subtype == 8 (NX_SUBTYPE_AUX_CONTROL_BUTTONS)
/// - data1 encodes: key_code = (data1 >> 16) & 0xFF
///                   key_flags = (data1 >> 8) & 0xFF
///                   key_down = (key_flags & 0x01) != 0
///
/// Returns `Some(VolumeKey)` only on key-down events.
fn decode_volume_key(event: &CGEvent) -> Option<VolumeKey> {
    let subtype = event.get_integer_value_field(EVENT_FIELD_SUBTYPE);
    if subtype != NX_SUBTYPE_AUX_CONTROL_BUTTONS {
        return None;
    }

    let data1 = event.get_integer_value_field(EVENT_FIELD_DATA1);
    let key_code = (data1 >> 16) & 0xFF;
    let key_flags = (data1 >> 8) & 0xFF;
    let key_down = (key_flags & 0x01) != 0;

    if !key_down {
        return None;
    }

    match key_code {
        NX_KEYTYPE_SOUND_UP => Some(VolumeKey::Up),
        NX_KEYTYPE_SOUND_DOWN => Some(VolumeKey::Down),
        NX_KEYTYPE_MUTE => Some(VolumeKey::Mute),
        _ => None,
    }
}

/// Starts an event tap that intercepts system volume key events.
///
/// The provided callback `on_volume_key` is invoked for each volume key-down
/// event. Volume key events are swallowed (not passed to the system) so that
/// the caller can handle volume changes directly.
///
/// This function blocks the current thread by running a CFRunLoop. It should
/// be called from a dedicated thread.
///
/// # Errors
///
/// Returns `Err` if the event tap cannot be created. This typically happens
/// when the process lacks Accessibility permissions (System Settings >
/// Privacy & Security > Accessibility).
///
/// # Safety considerations
///
/// The event tap requires the `kCGEventTapOptionDefault` permission level,
/// which allows modifying or dropping events. The process must be trusted
/// for Accessibility access.
pub fn run_event_tap<F>(on_volume_key: F) -> Result<(), String>
where
    F: FnMut(VolumeKey) + Send + 'static,
{
    // Wrap the mutable callback in a Mutex so the closure passed to CGEventTap
    // can be Fn (not FnMut). The Mutex is uncontended since the CFRunLoop
    // callback is single-threaded.
    let callback = Mutex::new(on_volume_key);

    // NX_SYSDEFINED (14) is not in the CGEventType enum, so we transmute.
    // SAFETY: CGEventType is #[repr(u32)] and value 14 is a valid macOS
    // event type (NX_SYSDEFINED). The core-graphics crate itself uses
    // similar out-of-range values (e.g., TapDisabledByTimeout = 0xFFFFFFFE).
    let nx_sysdefined: CGEventType = unsafe { std::mem::transmute(NX_SYSDEFINED) };

    let events_of_interest = vec![nx_sysdefined, CGEventType::TapDisabledByTimeout];

    // Shared holder for the mach port reference. We populate it after creating
    // the event tap, and the callback reads it to re-enable the tap on timeout.
    let port_holder: Arc<Mutex<Option<SendableMachPortRef>>> = Arc::new(Mutex::new(None));
    let port_holder_cb = Arc::clone(&port_holder);

    let event_tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        events_of_interest,
        move |_proxy: CGEventTapProxy, event_type: CGEventType, event: &CGEvent| {
            // Re-enable the tap if it was disabled by timeout.
            // macOS disables event taps that take too long to process events.
            if matches!(event_type, CGEventType::TapDisabledByTimeout) {
                if let Ok(guard) = port_holder_cb.lock() {
                    if let Some(port) = *guard {
                        unsafe { CGEventTapEnable(port.0, true) };
                    }
                }
                return CallbackResult::Keep;
            }

            // Check if this is a volume key event.
            if let Some(volume_key) = decode_volume_key(event) {
                if let Ok(mut cb) = callback.lock() {
                    cb(volume_key);
                }
                // Swallow the event so the system doesn't also handle it.
                return CallbackResult::Drop;
            }

            // Pass through any other NX_SYSDEFINED events (brightness, etc.).
            CallbackResult::Keep
        },
    )
    .map_err(|()| {
        "Failed to create event tap. \
         Ensure this application has Accessibility permissions \
         (System Settings > Privacy & Security > Accessibility)."
            .to_string()
    })?;

    // Store the mach port reference so the callback can re-enable the tap.
    {
        let raw_port = event_tap.mach_port().as_concrete_TypeRef();
        let mut guard = port_holder.lock().unwrap();
        *guard = Some(SendableMachPortRef(raw_port));
    }

    // Create a run loop source from the tap's mach port and add it to the
    // current thread's run loop.
    let loop_source = event_tap
        .mach_port()
        .create_runloop_source(0)
        .map_err(|_| "Failed to create run loop source from event tap mach port.")?;

    let run_loop = CFRunLoop::get_current();
    run_loop.add_source(&loop_source, unsafe { kCFRunLoopCommonModes });

    event_tap.enable();

    // Block the current thread, processing events via the run loop.
    // This never returns under normal operation.
    CFRunLoop::run_current();

    Ok(())
}
