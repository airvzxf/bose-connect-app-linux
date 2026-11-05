//! Party and stereo mode for two SoundLink Color II: the "Party mode"
//! group of the overview page, the join dialog and the countdown
//! before a join with this computer as the music source.
//!
//! With this computer as the source the speakers locked up unless the
//! computer sent no audio for a minute before the join (see "Party and
//! stereo mode" in `docs/protocol.md`), so the join waits for a
//! minute of silence first; the stream restart after every mode change
//! happens in the message handler.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use bose_connect::{AudioState, BdAddr, DeviceStatus, PartyMode};
use gtk::{glib, Box as GtkBox, Button, Image, Label, Orientation, StringList};

use crate::app::model::{AppModel, AppMsg};
use crate::app::widgets::{format_address, segmented_control, SenderExt};
use crate::services::audio::{audio_quiet, watch_audio, PARTY_SAMPLE_RATE, QUIET_BEFORE_JOIN};
use crate::t;

/// A speaker that can be joined to the one in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerChoice {
    pub address: BdAddr,
    pub name: String,
}

/// A music source the speaker can forward to the other one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChoice {
    /// `None`: this computer.
    pub address: Option<BdAddr>,
    pub name: String,
}

/// The speakers that can be joined: the other SoundLink Color II
/// paired with this computer, then the SoundLink Color II this
/// speaker has paired that are not connected to it now (the other
/// speaker only has to be paired with this one, not with the
/// computer). Phones and computers are never offered.
pub fn peer_choices(model: &AppModel) -> Vec<PeerChoice> {
    let Some(snapshot) = &model.snapshot else {
        return Vec::new();
    };
    let mut peers: Vec<PeerChoice> = model
        .host_devices
        .iter()
        .flatten()
        .filter(|d| bose_connect::has_party_mode(d.product_id))
        .filter_map(|d| {
            let address = BdAddr::from_canonical(&d.address)?;
            (address != snapshot.address).then(|| PeerChoice {
                address,
                name: d.name.clone(),
            })
        })
        .collect();
    // Only speakers that can join: the speaker would try to connect
    // to a phone or a computer as if it were a speaker.
    for dev in &snapshot.devices {
        let can_join = snapshot
            .products
            .get(&dev.address)
            .is_some_and(|&id| bose_connect::has_party_mode(id));
        if can_join
            && dev.status == DeviceStatus::Disconnected
            && dev.address != snapshot.address
            && !peers.iter().any(|p| p.address == dev.address)
        {
            peers.push(PeerChoice {
                address: dev.address,
                name: device_name(dev),
            });
        }
    }
    peers
}

/// The name of `peer`: as this computer knows it, else as this
/// speaker lists it (the other speaker does not have to be paired with
/// this computer), else its address.
fn peer_name(model: &AppModel, peer: BdAddr) -> String {
    let address = format_address(peer);
    if let Some(host) = model
        .host_devices
        .iter()
        .flatten()
        .find(|d| d.address.eq_ignore_ascii_case(&address))
    {
        return host.name.clone();
    }
    model
        .snapshot
        .iter()
        .flat_map(|s| &s.devices)
        .find(|d| d.address == peer)
        .map_or(address, device_name)
}

fn device_name(dev: &bose_connect::types::Device) -> String {
    std::str::from_utf8(&dev.name[..dev.name_len])
        .map(str::to_owned)
        .unwrap_or_else(|_| format_address(dev.address))
}

/// This computer, when it has an audio link to the speaker, then the
/// speaker's other connected sources (a phone).
pub fn source_choices(model: &AppModel) -> Vec<SourceChoice> {
    let Some(snapshot) = &model.snapshot else {
        return Vec::new();
    };
    let peers: Vec<BdAddr> = peer_choices(model).iter().map(|p| p.address).collect();
    let mut sources = Vec::new();
    if matches!(snapshot.audio, Some(AudioState::Idle | AudioState::Playing)) {
        sources.push(SourceChoice {
            address: None,
            name: t!("party-source-this"),
        });
    }
    for dev in &snapshot.devices {
        if dev.status == DeviceStatus::Connected && !peers.contains(&dev.address) {
            sources.push(SourceChoice {
                address: Some(dev.address),
                name: device_name(dev),
            });
        }
    }
    sources
}

/// The speaker "Separate" unlinks: the one joined last from here,
/// else the only other SoundLink Color II paired with this computer.
/// The speaker does not report which one is joined.
pub fn joined_peer(model: &AppModel) -> Option<BdAddr> {
    let this = model.snapshot.as_ref().map(|s| s.address);
    let remembered = model
        .persisted
        .party_peer
        .as_deref()
        .and_then(BdAddr::from_canonical)
        .filter(|p| Some(*p) != this);
    remembered.or_else(|| {
        let speakers: Vec<BdAddr> = model
            .host_devices
            .iter()
            .flatten()
            .filter(|d| bose_connect::has_party_mode(d.product_id))
            .filter_map(|d| BdAddr::from_canonical(&d.address))
            .filter(|a| Some(*a) != this)
            .collect();
        (speakers.len() == 1).then(|| speakers[0])
    })
}

/// The "Party mode" group; `None` for devices without party mode.
pub fn party_group(model: &AppModel, sender: &flume::Sender<AppMsg>) -> Option<gtk::Widget> {
    let snapshot = model.snapshot.as_ref()?;
    if !bose_connect::has_party_mode(snapshot.device_id) {
        return None;
    }
    if !model.simulator {
        // The join needs to know how long the audio has been quiet.
        watch_audio(&format_address(snapshot.address));
    }
    let group = adw::PreferencesGroup::new();
    group.set_title(&t!("group-party"));
    group.set_description(Some(&t!("party-description")));

    let peer = joined_peer(model);
    let peer_name = peer.map(|p| peer_name(model, p));
    let mode_row = adw::ActionRow::new();
    mode_row.set_title(&t!("party-title"));
    mode_row.set_subtitle_lines(0);
    mode_row.add_prefix(&Image::from_icon_name("audio-speakers-symbolic"));
    let other = peer_name
        .clone()
        .unwrap_or_else(|| t!("party-other-speaker"));
    let busy = model.party_busy;
    mode_row.set_subtitle(&match snapshot.party_mode {
        _ if busy => t!("party-busy"),
        None => t!("party-mode-unknown"),
        Some(PartyMode::Off) => t!("party-mode-off"),
        Some(PartyMode::Party) => t!("party-mode-party", peer = other.as_str()),
        Some(PartyMode::Stereo) => t!("party-mode-stereo", peer = other.as_str()),
    });
    match snapshot.party_mode {
        Some(PartyMode::Party | PartyMode::Stereo) => {
            let current = if snapshot.party_mode == Some(PartyMode::Stereo) {
                "stereo"
            } else {
                "party"
            };
            let s_mode = sender.clone();
            let seg = segmented_control(
                &[
                    ("party", &t!("party-choice-party")),
                    ("stereo", &t!("party-choice-stereo")),
                ],
                current,
                move |v| {
                    let mode = if v == "stereo" {
                        PartyMode::Stereo
                    } else {
                        PartyMode::Party
                    };
                    s_mode.send_app(AppMsg::SetPartyMode(mode));
                },
            );
            seg.set_valign(gtk::Align::Center);
            seg.set_sensitive(!busy);
            mode_row.add_suffix(&seg);

            let leave = Button::with_label(&t!("party-leave"));
            leave.set_valign(gtk::Align::Center);
            match peer {
                Some(peer) => {
                    let s = sender.clone();
                    leave.connect_clicked(move |_| s.send_app(AppMsg::LeaveParty(peer)));
                }
                None => {
                    leave.set_sensitive(false);
                    leave.set_tooltip_text(Some(&t!("party-leave-unknown")));
                }
            }
            if busy {
                leave.set_sensitive(false);
            }
            mode_row.add_suffix(&leave);
        }
        _ => {
            let join = Button::with_label(&t!("party-join"));
            join.add_css_class("suggested-action");
            join.set_valign(gtk::Align::Center);
            let peers = peer_choices(model);
            let sources = source_choices(model);
            let address = model.active_address().unwrap_or_default();
            let s = sender.clone();
            join.connect_clicked(move |b| {
                open_join_dialog(b, &peers, &sources, &address, &s);
            });
            join.set_sensitive(!busy);
            mode_row.add_suffix(&join);
        }
    }
    if busy {
        let spinner = adw::Spinner::new();
        spinner.set_valign(gtk::Align::Center);
        mode_row.add_suffix(&spinner);
    }
    group.add(&mode_row);

    if let Some(error) = &model.party_error {
        let row = adw::ActionRow::new();
        row.set_title(&t!("party-error"));
        row.set_subtitle(error);
        row.set_subtitle_lines(0);
        let icon = Image::from_icon_name("dialog-error-symbolic");
        icon.add_css_class("error");
        row.add_prefix(&icon);
        group.add(&row);
    }

    if let Some(rate) = snapshot.audio_rate {
        group.add(&rate_row(rate, sender));
    }

    let warn = adw::ActionRow::new();
    warn.set_title(&t!("party-lockup-title"));
    warn.set_subtitle(&t!("party-lockup-body"));
    warn.set_subtitle_lines(0);
    let icon = Image::from_icon_name("dialog-warning-symbolic");
    icon.add_css_class("warning");
    warn.add_prefix(&icon);
    group.add(&warn);

    Some(group.upcast())
}

/// The sample rate of the audio link from this computer; at any rate
/// but 44.1 kHz, the advice and a button to make PipeWire use it.
fn rate_row(rate: u32, sender: &flume::Sender<AppMsg>) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&t!("party-rate-title"));
    row.set_subtitle_lines(0);
    let khz = format!("{:.1}", f64::from(rate) / 1000.0);
    if rate == PARTY_SAMPLE_RATE {
        row.set_subtitle(&t!("party-rate-ok", rate = khz.as_str()));
        row.add_prefix(&Image::from_icon_name("emblem-ok-symbolic"));
        return row;
    }
    let icon = Image::from_icon_name("dialog-warning-symbolic");
    icon.add_css_class("warning");
    row.add_prefix(&icon);
    let configured = crate::services::audio::wireplumber_rate_file().is_some_and(|p| p.exists());
    if configured {
        row.set_subtitle(&t!("party-rate-reconnect", rate = khz.as_str()));
    } else {
        row.set_subtitle(&t!("party-rate-wrong", rate = khz.as_str()));
        let button = Button::with_label(&t!("party-rate-fix"));
        button.set_valign(gtk::Align::Center);
        let s = sender.clone();
        button.connect_clicked(move |b| {
            let root = b.root().and_then(|r| r.downcast::<gtk::Window>().ok());
            let dialog = adw::AlertDialog::new(
                Some(&t!("party-rate-fix-title")),
                Some(&t!("party-rate-fix-body")),
            );
            dialog.add_response("cancel", &t!("dialog-cancel"));
            dialog.add_response("apply", &t!("party-rate-fix"));
            dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
            let s = s.clone();
            dialog.choose(root.as_ref(), None::<&gtk::gio::Cancellable>, move |r| {
                if r == "apply" {
                    s.send_app(AppMsg::Use44100);
                }
            });
        });
        row.add_suffix(&button);
    }
    row
}

/// "Join another speaker": pick the speaker and the music source.
fn open_join_dialog(
    parent: &Button,
    peers: &[PeerChoice],
    sources: &[SourceChoice],
    address: &str,
    sender: &flume::Sender<AppMsg>,
) {
    let root = parent.root().and_then(|r| r.downcast::<gtk::Window>().ok());
    let body = if peers.is_empty() {
        t!("party-join-no-peer")
    } else if sources.is_empty() {
        t!("party-join-no-source")
    } else {
        t!("party-join-body")
    };
    let dialog = adw::AlertDialog::new(Some(&t!("party-join-title")), Some(&body));
    dialog.add_response("cancel", &t!("dialog-cancel"));
    dialog.add_response("join", &t!("party-join-confirm"));
    dialog.set_response_appearance("join", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("join"));
    dialog.set_response_enabled("join", !peers.is_empty() && !sources.is_empty());

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    let peer_row = adw::ComboRow::new();
    peer_row.set_title(&t!("party-join-peer"));
    let peer_names: Vec<String> = peers
        .iter()
        .map(|p| format!("{} ({})", p.name, format_address(p.address)))
        .collect();
    let peer_refs: Vec<&str> = peer_names.iter().map(String::as_str).collect();
    peer_row.set_model(Some(&StringList::new(&peer_refs)));
    let source_row = adw::ComboRow::new();
    source_row.set_title(&t!("party-join-source"));
    let source_names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
    source_row.set_model(Some(&StringList::new(&source_names)));
    if !peers.is_empty() {
        list.append(&peer_row);
    }
    if !sources.is_empty() {
        list.append(&source_row);
    }
    dialog.set_extra_child(Some(&list));

    let peers = peers.to_vec();
    let sources = sources.to_vec();
    let address = address.to_owned();
    let s = sender.clone();
    let parent = parent.clone();
    dialog.choose(root.as_ref(), None::<&gtk::gio::Cancellable>, move |r| {
        if r != "join" {
            return;
        }
        let (Some(peer), Some(source)) = (
            peers.get(peer_row.selected() as usize),
            sources.get(source_row.selected() as usize),
        ) else {
            return;
        };
        match source.address {
            Some(source) => s.send_app(AppMsg::JoinParty {
                peer: peer.address,
                source: Some(source),
            }),
            None => open_countdown_dialog(&parent, peer.address, &address, &s),
        }
    });
}

/// Join with this computer as the source once its audio link has
/// been quiet for [`QUIET_BEFORE_JOIN`]: at once when it already has
/// been, else after a countdown that "Join now" cuts short.
fn open_countdown_dialog(
    parent: &Button,
    peer: BdAddr,
    address: &str,
    sender: &flume::Sender<AppMsg>,
) {
    let total = QUIET_BEFORE_JOIN.as_secs() as u32;
    let quiet_secs = move |address: &str| {
        audio_quiet(address)
            .and_then(|(_, quiet)| quiet)
            .map_or(0, |q| q.as_secs().min(u64::from(total)) as u32)
    };
    if quiet_secs(address) >= total {
        tracing::info!(target: "party", "the audio link has been quiet for a minute: joining");
        sender.send_app(AppMsg::JoinParty { peer, source: None });
        return;
    }

    let root = parent.root().and_then(|r| r.downcast::<gtk::Window>().ok());
    let dialog = adw::AlertDialog::new(
        Some(&t!("party-wait-title")),
        Some(&t!("party-wait-checking")),
    );
    dialog.add_response("cancel", &t!("dialog-cancel"));
    dialog.add_response("join", &t!("party-wait-join-now"));
    dialog.set_response_appearance("join", adw::ResponseAppearance::Destructive);
    dialog.set_close_response("cancel");
    let level = gtk::LevelBar::for_interval(0.0, f64::from(total));
    level.set_value(0.0);
    let hint = Label::new(Some(&t!("party-wait-hint")));
    hint.set_wrap(true);
    hint.add_css_class("dim-label");
    let extra = GtkBox::new(Orientation::Vertical, 12);
    extra.append(&level);
    extra.append(&hint);
    dialog.set_extra_child(Some(&extra));

    let finished = Rc::new(Cell::new(false));
    {
        let finished = finished.clone();
        let sender = sender.clone();
        dialog
            .clone()
            .choose(root.as_ref(), None::<&gtk::gio::Cancellable>, move |r| {
                // Cancel, Escape, "Join now", or the close after the
                // countdown (which has already joined).
                if r == "join" && !finished.get() {
                    tracing::info!(target: "party", "joining before a minute of quiet, as asked");
                    sender.send_app(AppMsg::JoinParty { peer, source: None });
                }
                finished.set(true);
            });
    }

    // Pause the players once; the countdown waits for any that
    // did not pause.
    glib::MainContext::default().spawn_local(async {
        let paused = gtk::gio::spawn_blocking(crate::services::audio::pause_media_players)
            .await
            .unwrap_or(0);
        tracing::info!(target: "party", "paused {paused} media players before the join");
    });

    let address = address.to_owned();
    let sender = sender.clone();
    glib::timeout_add_seconds_local(1, move || {
        if finished.get() {
            return glib::ControlFlow::Break;
        }
        let state = audio_quiet(&address).and_then(|(state, _)| state);
        let quiet = quiet_secs(&address);
        level.set_value(f64::from(quiet));
        match state {
            Some(AudioState::Idle) => {
                let left = total.saturating_sub(quiet);
                dialog.set_body(&t!("party-wait-counting", seconds = left));
                if left == 0 {
                    finished.set(true);
                    sender.send_app(AppMsg::JoinParty { peer, source: None });
                    dialog.force_close();
                    return glib::ControlFlow::Break;
                }
            }
            Some(AudioState::Playing) => dialog.set_body(&t!("party-wait-playing")),
            None => {}
            Some(_) => dialog.set_body(&t!("party-wait-no-link")),
        }
        glib::ControlFlow::Continue
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::bluetooth::PairedBose;
    use crate::services::device::{DeviceService, MockScenario, MockService};
    use crate::transport::{self, demo, MockSeed};
    use std::sync::Arc;

    fn speaker_model() -> AppModel {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let (handle, ticker) =
            rt.block_on(async { transport::spawn(Some(MockSeed::speaker()), None) });
        drop(ticker);
        let svc = Arc::new(MockService::with_scenario(handle, MockScenario::Speaker));
        let mut model = AppModel::new(svc.clone());
        model.persisted = Default::default();
        model.host_devices = Some(svc.known_devices().expect("devices"));
        model.snapshot = Some(svc.connect(demo::DEVICE).expect("connect"));
        model
    }

    #[test]
    fn the_peer_is_the_other_speaker() {
        let mut model = speaker_model();
        let peers = peer_choices(&model);
        // The speaker paired with this computer first, then what the
        // speaker itself has paired but not connected.
        // The tablet is not a speaker: never offered.
        let addresses: Vec<String> = peers.iter().map(|p| format_address(p.address)).collect();
        assert_eq!(addresses, [demo::SPEAKER], "{peers:?}");
        // The only other speaker paired with this computer.
        assert_eq!(joined_peer(&model), Some(peers[0].address));

        // A SoundLink Color II only this speaker has paired.
        let other = BdAddr::from_canonical("02:00:00:00:00:07").unwrap();
        let snapshot = model.snapshot.as_mut().unwrap();
        snapshot.devices.push(transport::make_device_info(
            other,
            "Speaker 2",
            DeviceStatus::Disconnected,
        ));
        snapshot.products.insert(other, 0x400d);
        let addresses: Vec<String> = peer_choices(&model)
            .iter()
            .map(|p| format_address(p.address))
            .collect();
        assert_eq!(addresses, [demo::SPEAKER, "02:00:00:00:00:07"]);
        // Its name comes from this speaker's list.
        assert_eq!(peer_name(&model, other), "Speaker 2");
        assert_eq!(
            peer_name(&model, BdAddr::from_canonical(demo::SPEAKER).unwrap()),
            "Bose SoundLink Color II"
        );

        // A second one: "Separate" needs the one joined last.
        model.host_devices.as_mut().unwrap().push(PairedBose {
            address: "02:00:00:00:00:06".into(),
            name: "Speaker 3".into(),
            connected: true,
            product_id: 0x400d,
        });
        assert_eq!(joined_peer(&model), None);
        model.persisted.party_peer = Some("02:00:00:00:00:06".into());
        assert_eq!(
            format_address(joined_peer(&model).unwrap()),
            "02:00:00:00:00:06"
        );
        // Never this speaker itself.
        model.persisted.party_peer = Some(demo::DEVICE.into());
        assert_eq!(joined_peer(&model), None);
    }

    #[test]
    fn sources_are_this_computer_and_the_connected_phone() {
        let mut model = speaker_model();
        // The simulator reports no BlueZ audio link: only the phone.
        let sources = source_choices(&model);
        assert_eq!(sources.len(), 1, "{sources:?}");
        assert_eq!(sources[0].address, BdAddr::from_canonical(demo::PHONE));

        model.snapshot.as_mut().unwrap().audio = Some(AudioState::Idle);
        let sources = source_choices(&model);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].address, None, "this computer first");
    }
}
