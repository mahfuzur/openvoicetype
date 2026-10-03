//! Feedback sounds (follows `AppDelegate.play`: Tink, Pop, Funk and Basso on the Mac), from the Windows sound scheme
//! so they follow the user's choices in Settings ▸ Sound. An event the scheme leaves silent stays silent.

use windows::core::HSTRING;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// Recording started (Tink).
    Start,
    /// Recording stopped (Pop).
    Stop,
    /// No speech, cancelled, refused (Funk).
    Notice,
    /// Something failed (Basso).
    Error,
}

impl Sound {
    /// Event names under `HKCU\AppEvents\Schemes\Apps\.Default`.
    fn alias(self) -> &'static str {
        match self {
            Sound::Start => "DeviceConnect",
            Sound::Stop => "DeviceDisconnect",
            Sound::Notice => "SystemNotification",
            Sound::Error => "SystemHand",
        }
    }
}

pub fn play(sound: Sound, enabled: bool) {
    if !enabled {
        return;
    }
    // SAFETY: an alias name and no module; asynchronous, so it never blocks the caller.
    unsafe {
        let _ = PlaySoundW(&HSTRING::from(sound.alias()), None, SND_ALIAS | SND_ASYNC | SND_NODEFAULT);
    }
}
