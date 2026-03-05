use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{
    kCFRunLoopCommonModes, CFRunLoop, CFRunLoopSource, CFRunLoopSourceRef,
};
use objc2::msg_send;
use objc2::runtime::AnyObject;
use std::os::raw::c_void;
use std::sync::Mutex;

// CGEventTapCreate and related types from CoreGraphics.
// We declare our own FFI because the core-graphics crate's CGEventType enum
// doesn't include NX_SYSDEFINED (value 14) and transmuting to create it is UB.
type CGEventRef = *mut c_void;
type CGEventMask = u64;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,       // CGEventTapLocation
        place: u32,     // CGEventTapPlacement
        options: u32,   // CGEventTapOptions
        eventsOfInterest: CGEventMask,
        callback: unsafe extern "C" fn(
            proxy: *mut c_void,
            event_type: u32,
            event: CGEventRef,
            user_info: *mut c_void,
        ) -> CGEventRef,
        userInfo: *mut c_void,
    ) -> CFMachPortRef;

    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);

    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: CFMachPortRef,
        order: i64,
    ) -> CFRunLoopSourceRef;
}

// CGEventTapLocation::HID = 0
const K_CG_HID_EVENT_TAP: u32 = 0;
// CGEventTapPlacement::HeadInsertEventTap = 0
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
// CGEventTapOptions::Default = 0 (not listen-only, can modify/drop)
const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;

// NX_SYSDEFINED event type = 14
const NX_SYSDEFINED: u32 = 14;
// kCGEventTapDisabledByTimeout
const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;

// NX key type constants encoded in data1 of NX_SYSDEFINED events.
const NX_KEYTYPE_SOUND_UP: isize = 0;
const NX_KEYTYPE_SOUND_DOWN: isize = 1;
const NX_KEYTYPE_MUTE: isize = 7;
// NSEventSubtype value for auxiliary control buttons (media keys).
const NX_SUBTYPE_AUX_CONTROL_BUTTONS: i16 = 8;

/// A volume key event with optional shift modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeKeyEvent {
    pub key: VolumeKey,
    pub shift: bool,
}

/// Represents the type of volume key event intercepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKey {
    Up,
    Down,
    Mute,
}

/// Decode a volume key press from a raw NX_SYSDEFINED CGEvent.
///
/// Converts the CGEvent to an NSEvent to read `subtype` and `data1`, because
/// `CGEventGetIntegerValueField` does not expose these fields for system-defined
/// events.
///
/// Returns `Some(VolumeKeyEvent)` only on key-down events.
unsafe fn decode_volume_key(event: CGEventRef) -> Option<VolumeKeyEvent> {
    // Convert CGEventRef → NSEvent via [NSEvent eventWithCGEvent:]
    let cls = objc2::runtime::AnyClass::get(c"NSEvent")?;
    let ns_event: *mut AnyObject = msg_send![cls, eventWithCGEvent: event];
    if ns_event.is_null() {
        return None;
    }

    let subtype: i16 = msg_send![ns_event, subtype];
    if subtype != NX_SUBTYPE_AUX_CONTROL_BUTTONS {
        return None;
    }

    let data1: isize = msg_send![ns_event, data1];
    let key_code = (data1 >> 16) & 0xFF;
    let key_flags = (data1 >> 8) & 0xFF;
    let key_down = (key_flags & 0x01) != 0;

    if !key_down {
        return None;
    }

    const NS_EVENT_MODIFIER_FLAG_SHIFT: usize = 1 << 17;
    let modifier_flags: usize = msg_send![ns_event, modifierFlags];
    let shift = (modifier_flags & NS_EVENT_MODIFIER_FLAG_SHIFT) != 0;

    let key = match key_code {
        NX_KEYTYPE_SOUND_UP => VolumeKey::Up,
        NX_KEYTYPE_SOUND_DOWN => VolumeKey::Down,
        NX_KEYTYPE_MUTE => VolumeKey::Mute,
        _ => return None,
    };

    Some(VolumeKeyEvent { key, shift })
}

/// State shared between the event tap callback and the run loop.
struct TapState {
    callback: Box<dyn FnMut(VolumeKeyEvent) + Send>,
    port: CFMachPortRef,
}

/// Raw C callback for CGEventTapCreate.
///
/// Returns NULL to swallow the event, or the original event to pass it through.
unsafe extern "C" fn tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    // Re-enable tap if it was disabled by timeout.
    if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
        let state = &*(user_info as *const Mutex<TapState>);
        if let Ok(guard) = state.lock() {
            CGEventTapEnable(guard.port, true);
        }
        return event;
    }

    // Only process NX_SYSDEFINED events.
    if event_type != NX_SYSDEFINED {
        return event;
    }

    let volume_event = decode_volume_key(event);

    match volume_event {
        Some(evt) => {
            let state = &*(user_info as *const Mutex<TapState>);
            if let Ok(mut guard) = state.lock() {
                (guard.callback)(evt);
            }
            // Return NULL to swallow the event (suppress native HUD).
            std::ptr::null_mut()
        }
        None => event, // Pass through non-volume NX_SYSDEFINED events.
    }
}

/// Starts an event tap that intercepts system volume key events.
///
/// `on_volume_key` is called for each volume key-down event. The native volume
/// HUD is suppressed by swallowing the key event.
///
/// This function blocks the current thread (runs a CFRunLoop). Call it from a
/// dedicated background thread.
///
/// Returns `Err` if the event tap cannot be created (typically means the app
/// lacks Accessibility permissions).
pub fn run_event_tap<F>(on_volume_key: F) -> Result<(), String>
where
    F: FnMut(VolumeKeyEvent) + Send + 'static,
{
    // Event mask: NX_SYSDEFINED (bit 14) only. The timeout event is delivered
    // automatically regardless of the mask.
    let event_mask: CGEventMask = 1u64 << NX_SYSDEFINED;

    // State shared with the callback via user_info pointer.
    // The Mutex protects the FnMut callback.
    let state = Box::new(Mutex::new(TapState {
        callback: Box::new(on_volume_key),
        port: std::ptr::null_mut(),
    }));
    let state_ptr = Box::into_raw(state);

    let port = unsafe {
        CGEventTapCreate(
            K_CG_HID_EVENT_TAP,
            K_CG_HEAD_INSERT_EVENT_TAP,
            K_CG_EVENT_TAP_OPTION_DEFAULT,
            event_mask,
            tap_callback,
            state_ptr as *mut c_void,
        )
    };

    if port.is_null() {
        // Clean up the leaked state.
        unsafe { drop(Box::from_raw(state_ptr)) };
        return Err(
            "Failed to create event tap. \
             Ensure this application has Accessibility permissions \
             (System Settings > Privacy & Security > Accessibility)."
                .to_string(),
        );
    }

    // Store the port in the state so the callback can re-enable it on timeout.
    unsafe {
        if let Ok(mut guard) = (*state_ptr).lock() {
            guard.port = port;
        }
    }

    // Create a run loop source from the mach port and add it to this thread's
    // run loop.
    let source = unsafe { CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0) };
    if source.is_null() {
        unsafe { drop(Box::from_raw(state_ptr)) };
        return Err("Failed to create run loop source from event tap.".to_string());
    }

    unsafe {
        let run_loop = CFRunLoop::get_current();
        let source_ref = CFRunLoopSource::wrap_under_create_rule(source);
        run_loop.add_source(&source_ref, kCFRunLoopCommonModes);
    }

    // Enable the tap and block on the run loop.
    unsafe { CGEventTapEnable(port, true) };
    CFRunLoop::run_current();

    // Clean up (unreachable in practice — the run loop runs forever).
    unsafe { drop(Box::from_raw(state_ptr)) };
    Ok(())
}
