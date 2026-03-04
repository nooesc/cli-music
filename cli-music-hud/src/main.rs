mod audio;

fn main() {
    let device = audio::default_output_device().expect("No output device");
    let vol = audio::get_volume(device).expect("Can't read volume");
    let muted = audio::is_muted(device).unwrap_or(false);
    println!(
        "Device: {device}, Volume: {:.0}%, Muted: {muted}",
        vol * 100.0
    );
}
