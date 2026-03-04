mod audio;
mod event_tap;

use std::sync::mpsc;

fn main() {
    let device = audio::default_output_device().expect("No output device");
    let vol = audio::get_volume(device).expect("Can't read volume");
    println!("Volume: {:.0}%", vol * 100.0);

    let (tx, rx) = mpsc::channel();
    let _listener = audio::listen_volume(device, &tx).expect("Can't listen");

    println!("Listening for volume changes... (change volume to test, Ctrl+C to quit)");
    for event in rx {
        println!("Event: {event:?}");
    }
}
