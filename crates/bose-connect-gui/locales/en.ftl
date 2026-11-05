# Bose Connect for Linux: English, the source language.
#
# Every other locales/<language>.ftl translates these same message
# ids; see CONTRIBUTING.md, "Translating the GUI". Brand and product
# names (Bose, Bose Connect, BlueZ, RFCOMM) stay untranslated.

## Window, header bar and main menu

page-overview-subtitle = Controls and settings
page-mybose-title = My Bose devices
page-mybose-subtitle = Bose devices paired with this computer
header-back = Back to the device
header-theme = Switch between light and dark
header-menu = Main menu
menu-refresh = Refresh
menu-mybose = My Bose devices
menu-about = About Bose Connect
menu-quit = Quit
about-comments =
    Control Bose headphones and speakers over Bluetooth RFCOMM on Linux.
    Protocol engine: bose-connect v{ $version }

## Device menu (top left)

device-button-connecting = Connecting…
device-button-none = No device
device-menu-title = Paired Bose devices
device-menu-empty = No Bose device is paired with this computer
device-menu-connected = { $address } · Connected
device-menu-disconnected = { $address } · Not connected
device-menu-manage = Manage devices…
bluez-down-title = BlueZ is not responding
bluez-down-subtitle = Could not read the list of paired devices

## Dialogs

dialog-cancel = Cancel
dialog-save = Save
rename-title = Rename device
rename-body = Enter the new Bluetooth name of the device (31 bytes at most):
rename-counter = { $length } / 31 bytes
forget-title = Forget this device?
forget-body = Remove “{ $name }” ({ $address }) from the memory of the Bose device? You will have to pair it again by hand.
forget-confirm = Forget

## Device card

hero-connecting = Connecting to { $name }…
hero-failed = Could not connect
hero-none = No device connected
hero-pick = Pick a device to control it
hero-rename = Rename device
hero-via-bd-addr = Connected over RFCOMM · BD_ADDR: { $address }
hero-via-mac = Connected over RFCOMM · MAC: { $address }
hero-history = last 32 readings
status-connected = ● Connected
status-connecting = ○ Connecting…
status-disconnected = ○ Disconnected

## Audio and playback

group-audio = Audio and playback
audio-row-title = Audio on this computer
audio-unknown = Not paired with this computer
audio-disconnected = No audio link with this computer
audio-without-audio = Connected, but without working audio from this computer
audio-idle = Audio ready (paused)
audio-playing = Playing audio from this computer
audio-reconnect-hint = Connected, but without working audio from this computer. If you expect sound from here, disconnect the device and connect it again from this computer: when the device starts the connection, BlueZ may not open the audio.
media-title = Playback
media-subtitle = Pause, resume or skip tracks
media-previous = Previous track
media-play = Play
media-pause = Pause
media-next = Next track
volume-title = Device volume
volume-subtitle = Bose volume level (0 to 75)
volume-value = { $level } / 75 ({ $percent }%)

## Headset controls

group-headset = Headset controls
anc-title = Active noise cancelling (ANC)
anc-subtitle = How much outside sound is attenuated
level-off = Off
level-low = Low
level-medium = Medium
level-high = High
self-voice-title = Self voice
self-voice-subtitle = Hear your own voice naturally during calls

## Voice prompts

group-voice = Voice and prompts
voice-prompts-title = Voice prompts
voice-prompts-subtitle = Spoken status and battery announcements
prompt-language-title = Prompt language
prompt-language-subtitle = Language of the voice prompts
prompt-language-unknown = Unknown ({ $code })
prompt-language-read-only = Change it in the Bose app on the phone: this computer cannot change it on this device yet

## Power

group-power = Power and battery
auto-off-title = Auto-off
auto-off-subtitle = Turns the device off after inactivity to save battery
auto-off-never = Never
auto-off-minutes = { $minutes } min

## Sources paired with the Bose device (multipoint)

group-multipoint = Sources paired with the Bose device (multipoint)
pairing-title = Visible for pairing
pairing-subtitle = Lets other phones or computers find this Bose device
sources-empty = No other paired devices
sources-empty-hint = Turn on “Visible for pairing” to connect new devices
source-this = This computer
source-active-audio = Active audio
source-unnamed = Device { $address }
action-connect = Connect
action-disconnect = Disconnect
action-forget = Forget device

## Device information

group-info = Device information
info-device-id = Hardware id
info-bd-addr = Physical Bluetooth address (BD_ADDR)
info-bd-addr-subtitle = Hardware MAC address of the Bose module
info-firmware = Firmware version
info-serial = Serial number
info-serial-show = Show serial number
info-serial-hide = Hide serial number
group-advanced = Advanced
info-versions = Versions

## Not connected, no paired device, My Bose page

pairing-steps =
    1. Put the Bose device in pairing mode: hold its Bluetooth button until the light blinks blue.
    2. Pair it in the desktop's Bluetooth settings (or with bluetoothctl).
    3. It appears here and in the device menu.
no-devices-title = No Bose device paired
no-devices-body = This computer has no Bose device paired.
no-devices-again = Check again
device-connected = Connected
device-not-connected = Not connected
device-in-use = In use
action-use = Use
action-connecting = Connecting…
connect-group-title = Connect to a device
connect-group-description = Pick one of the Bose devices paired with this computer.
mybose-group-title = Bose devices paired with this computer
mybose-reload = Read the BlueZ list again
mybose-pair-title = Pair a new device

## Desktop notifications and tray icon

notify-battery-critical = Battery critical — last 5 %. Plug in now.
notify-battery-low = Battery low — plug in soon.
notify-battery-threshold = Battery dropped below the threshold.
tray-battery = Bose Connect ({ $percent }%)
tray-disconnected = { $name } — disconnected
tray-battery-low = Bose Connect — battery is low

# Party mode (SoundLink Color II)
group-party = Party mode
party-description = Two SoundLink Color II play the same music: this speaker receives it and forwards it to the other one.
party-title = Second speaker
party-other-speaker = the other speaker
party-mode-unknown = Unknown: the speaker did not answer
party-mode-off = Not joined to another speaker
party-mode-party = Party: this speaker and { $peer } play the same music
party-mode-stereo = Stereo: this speaker plays the left channel, { $peer } the right one
party-choice-party = Party
party-choice-stereo = Stereo
party-join = Join with…
party-leave = Separate
party-leave-unknown = Join the other speaker from this app to separate it here
party-busy = Waiting for the speaker…
party-error = The speaker did not do it
party-lockup-title = These speakers lock up now and then
party-lockup-body = In party mode a SoundLink Color II sometimes stops responding, also with a phone and the Bose app. Switch it off and on again.
party-rate-title = Audio link from this computer
party-rate-ok = SBC at { $rate } kHz, as party mode needs
party-rate-wrong = SBC at { $rate } kHz: party mode locked up the speakers at this rate; it needs 44.1 kHz. Switch the speaker off and on and let it connect by itself, or make this computer use 44.1 kHz.
party-rate-reconnect = SBC at { $rate } kHz: this computer now uses 44.1 kHz for new links. Disconnect the speaker and connect it again.
party-rate-fix = Use 44.1 kHz
party-rate-fix-title = Use 44.1 kHz for Bluetooth audio?
party-rate-fix-body = This writes a WirePlumber setting so the Bluetooth audio links this computer opens use 44.1 kHz instead of 48 kHz, for every Bluetooth device, and restarts WirePlumber: the sound stops for a moment.
party-join-title = Join another speaker
party-join-body = Both speakers must be on. Pick the other speaker (not a phone or a computer: the speaker tries to connect to it as a speaker) and the device that plays the music.
party-join-no-peer = Pair the other SoundLink Color II with this computer first, so its address is known here.
party-join-no-source = No music source is connected to this speaker. Connect this computer's audio or a phone first.
party-join-peer = Other speaker
party-join-source = Music from
party-join-confirm = Join
party-source-this = This computer
party-wait-title = Waiting for a minute of silence
party-wait-checking = Checking the audio from this computer…
party-wait-counting = Joining in { $seconds } s.
party-wait-playing = This computer is playing audio to the speaker: pause it. The countdown starts again when it stops.
party-wait-no-link = This computer has no audio link to the speaker. Connect it again, or cancel and pick another source.
party-wait-hint = The speakers lock up when they are joined while this computer sends audio, or shortly after it stopped. Play nothing until the join is done.
