use coreaudio_sys::*;
use std::mem;
use std::os::raw::c_void;
use std::ptr;


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
