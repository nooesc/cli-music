use coreaudio_sys::*;
use std::mem;
use std::os::raw::c_void;
use std::ptr;
use std::sync::mpsc;

/// Get the default output audio device ID.
pub fn default_output_device() -> Option<AudioObjectID> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyDefaultOutputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut device_id: AudioObjectID = 0;
    let mut size = mem::size_of::<AudioObjectID>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut device_id as *mut _ as *mut c_void,
        )
    };
    if status == 0 {
        Some(device_id)
    } else {
        None
    }
}

/// Find the element that supports volume for this device.
/// Tries master element (0) first, then falls back to channel 1.
fn volume_element(device_id: AudioObjectID) -> Option<u32> {
    for element in [kAudioObjectPropertyElementMain, 1] {
        let address = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyVolumeScalar,
            mScope: kAudioObjectPropertyScopeOutput,
            mElement: element,
        };
        let has = unsafe { AudioObjectHasProperty(device_id, &address) };
        if has != 0 {
            return Some(element);
        }
    }
    None
}

/// Get the current volume (0.0 to 1.0) for a device.
pub fn get_volume(device_id: AudioObjectID) -> Option<f32> {
    let element = volume_element(device_id)?;
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyVolumeScalar,
        mScope: kAudioObjectPropertyScopeOutput,
        mElement: element,
    };
    let mut volume: f32 = 0.0;
    let mut size = mem::size_of::<f32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut volume as *mut _ as *mut c_void,
        )
    };
    if status == 0 {
        Some(volume)
    } else {
        None
    }
}

/// Set the volume (0.0 to 1.0) for a device.
/// Sets all available channels (master, then channels 1 and 2).
pub fn set_volume(device_id: AudioObjectID, volume: f32) -> bool {
    let volume = volume.clamp(0.0, 1.0);
    let size = mem::size_of::<f32>() as u32;
    let mut any_set = false;
    // Try master element and both stereo channels
    for element in [kAudioObjectPropertyElementMain, 1, 2] {
        let address = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyVolumeScalar,
            mScope: kAudioObjectPropertyScopeOutput,
            mElement: element,
        };
        let has = unsafe { AudioObjectHasProperty(device_id, &address) };
        if has != 0 {
            let status = unsafe {
                AudioObjectSetPropertyData(
                    device_id,
                    &address,
                    0,
                    ptr::null(),
                    size,
                    &volume as *const _ as *const c_void,
                )
            };
            if status == 0 {
                any_set = true;
            }
        }
    }
    any_set
}

/// Find the element that supports mute for this device.
fn mute_element(device_id: AudioObjectID) -> Option<u32> {
    for element in [kAudioObjectPropertyElementMain, 1] {
        let address = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyMute,
            mScope: kAudioObjectPropertyScopeOutput,
            mElement: element,
        };
        let has = unsafe { AudioObjectHasProperty(device_id, &address) };
        if has != 0 {
            return Some(element);
        }
    }
    None
}

/// Check if a device is muted.
pub fn is_muted(device_id: AudioObjectID) -> Option<bool> {
    let element = mute_element(device_id)?;
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyMute,
        mScope: kAudioObjectPropertyScopeOutput,
        mElement: element,
    };
    let mut muted: u32 = 0;
    let mut size = mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut muted as *mut _ as *mut c_void,
        )
    };
    if status == 0 {
        Some(muted != 0)
    } else {
        None
    }
}

/// Toggle mute on a device.
pub fn set_mute(device_id: AudioObjectID, mute: bool) -> bool {
    let element = match mute_element(device_id) {
        Some(e) => e,
        None => return false,
    };
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyMute,
        mScope: kAudioObjectPropertyScopeOutput,
        mElement: element,
    };
    let value: u32 = if mute { 1 } else { 0 };
    let size = mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectSetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            size,
            &value as *const _ as *const c_void,
        )
    };
    status == 0
}

/// The step size for volume changes (matches macOS default: 1/16 = 0.0625).
pub const VOLUME_STEP: f32 = 1.0 / 16.0;

// ---------------------------------------------------------------------------
// Volume change listener
// ---------------------------------------------------------------------------

/// Events emitted when the audio state changes.
#[derive(Debug)]
pub enum AudioEvent {
    VolumeChanged(f32),
    MuteChanged(bool),
    DeviceChanged(AudioObjectID),
}

/// Holds the listener registration so it can be removed on drop.
pub struct VolumeListener {
    device_id: AudioObjectID,
    tx: *const mpsc::Sender<AudioEvent>,
}

// The raw pointer is only used as an opaque handle passed to CoreAudio; the
// Sender it points to lives as long as the caller's channel and is only read
// (never mutated) from the callback.
unsafe impl Send for VolumeListener {}

impl Drop for VolumeListener {
    fn drop(&mut self) {
        let element = volume_element(self.device_id).unwrap_or(kAudioObjectPropertyElementMain);
        let vol_address = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyVolumeScalar,
            mScope: kAudioObjectPropertyScopeOutput,
            mElement: element,
        };
        unsafe {
            AudioObjectRemovePropertyListener(
                self.device_id,
                &vol_address,
                Some(volume_changed_callback),
                self.tx as *mut c_void,
            );
        }

        let mute_elem = mute_element(self.device_id).unwrap_or(kAudioObjectPropertyElementMain);
        let mute_address = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyMute,
            mScope: kAudioObjectPropertyScopeOutput,
            mElement: mute_elem,
        };
        unsafe {
            AudioObjectRemovePropertyListener(
                self.device_id,
                &mute_address,
                Some(volume_changed_callback),
                self.tx as *mut c_void,
            );
        }
    }
}

/// CoreAudio property-listener callback.
///
/// Called on an internal CoreAudio thread whenever a watched property changes.
/// We re-read the current volume and mute state and send them through the
/// channel whose `Sender` was stashed in `client_data`.
unsafe extern "C" fn volume_changed_callback(
    _id: AudioObjectID,
    _num_addresses: UInt32,
    _addresses: *const AudioObjectPropertyAddress,
    client_data: *mut c_void,
) -> OSStatus {
    let tx = &*(client_data as *const mpsc::Sender<AudioEvent>);

    if let Some(device) = default_output_device() {
        if let Some(vol) = get_volume(device) {
            let _ = tx.send(AudioEvent::VolumeChanged(vol));
        }
        if let Some(muted) = is_muted(device) {
            let _ = tx.send(AudioEvent::MuteChanged(muted));
        }
    }
    0
}

/// Register a CoreAudio property listener for volume and mute changes on
/// `device_id`.  Events are sent through `tx`.
///
/// Returns a [`VolumeListener`] guard — dropping it removes the listener.
/// `tx` must remain valid for the lifetime of the returned guard.
pub fn listen_volume(
    device_id: AudioObjectID,
    tx: *const mpsc::Sender<AudioEvent>,
) -> Option<VolumeListener> {
    let element = volume_element(device_id)?;
    let vol_address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyVolumeScalar,
        mScope: kAudioObjectPropertyScopeOutput,
        mElement: element,
    };
    let status = unsafe {
        AudioObjectAddPropertyListener(
            device_id,
            &vol_address,
            Some(volume_changed_callback),
            tx as *mut c_void,
        )
    };
    if status != 0 {
        return None;
    }

    // Also listen for mute changes.
    let mute_elem = mute_element(device_id).unwrap_or(kAudioObjectPropertyElementMain);
    let mute_address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyMute,
        mScope: kAudioObjectPropertyScopeOutput,
        mElement: mute_elem,
    };
    let status = unsafe {
        AudioObjectAddPropertyListener(
            device_id,
            &mute_address,
            Some(volume_changed_callback),
            tx as *mut c_void,
        )
    };
    // If mute listener fails we still have the volume listener, so we don't
    // bail out — but we log to stderr.
    if status != 0 {
        eprintln!("warning: failed to add mute property listener (status={status})");
    }

    Some(VolumeListener { device_id, tx })
}
