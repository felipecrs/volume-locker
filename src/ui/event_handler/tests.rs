use super::{
    DeviceAction, DeviceId, DeviceType, PersistentState,
    device_settings_are_empty, handle_priority_event,
};
use crate::types::DeviceSettings;

#[test]
fn device_settings_empty_when_all_false() {
    let settings = DeviceSettings::new("Test".to_string(), DeviceType::Output);
    assert!(device_settings_are_empty(&settings));
}

#[test]
fn device_settings_not_empty_when_locked() {
    let mut settings = DeviceSettings::new("Test".to_string(), DeviceType::Output);
    settings.volume_lock.is_locked = true;
    assert!(!device_settings_are_empty(&settings));
}

fn make_state_with_device(device_id: &str, device_type: DeviceType) -> PersistentState {
    let mut state = PersistentState::default();
    state.devices.insert(
        DeviceId::from(device_id),
        DeviceSettings::new("Test Device".to_string(), device_type),
    );
    state
}

#[test]
fn priority_add_inserts_device() {
    let mut state = PersistentState::default();
    let changed = handle_priority_event(
        &DeviceAction::AddToPriority,
        &DeviceId::from("dev1"),
        DeviceType::Output,
        "Speaker",
        &mut state,
    );
    assert!(changed);
    assert_eq!(state.priority_list(DeviceType::Output), &["dev1"]);
    assert!(state.devices.contains_key("dev1"));
}

#[test]
fn priority_add_duplicate_no_op() {
    let mut state = PersistentState::default();
    state
        .priority_list_mut(DeviceType::Output)
        .push("dev1".into());
    let changed = handle_priority_event(
        &DeviceAction::AddToPriority,
        &DeviceId::from("dev1"),
        DeviceType::Output,
        "Speaker",
        &mut state,
    );
    assert!(!changed);
}

#[test]
fn priority_remove_cleans_empty_device() {
    let mut state = make_state_with_device("dev1", DeviceType::Output);
    state
        .priority_list_mut(DeviceType::Output)
        .push("dev1".into());
    let changed = handle_priority_event(
        &DeviceAction::RemoveFromPriority,
        &DeviceId::from("dev1"),
        DeviceType::Output,
        "Speaker",
        &mut state,
    );
    assert!(changed);
    assert!(state.priority_list(DeviceType::Output).is_empty());
    assert!(!state.devices.contains_key("dev1"));
}

#[test]
fn priority_move_up() {
    let mut state = PersistentState::default();
    *state.priority_list_mut(DeviceType::Output) =
        vec!["a".into(), "b".into(), "c".into()];
    let changed = handle_priority_event(
        &DeviceAction::MovePriorityUp,
        &DeviceId::from("b"),
        DeviceType::Output,
        "B",
        &mut state,
    );
    assert!(changed);
    assert_eq!(state.priority_list(DeviceType::Output), &["b", "a", "c"]);
}

#[test]
fn priority_move_up_already_top() {
    let mut state = PersistentState::default();
    *state.priority_list_mut(DeviceType::Output) = vec!["a".into(), "b".into()];
    let changed = handle_priority_event(
        &DeviceAction::MovePriorityUp,
        &DeviceId::from("a"),
        DeviceType::Output,
        "A",
        &mut state,
    );
    assert!(!changed);
}

#[test]
fn priority_move_down() {
    let mut state = PersistentState::default();
    *state.priority_list_mut(DeviceType::Output) =
        vec!["a".into(), "b".into(), "c".into()];
    let changed = handle_priority_event(
        &DeviceAction::MovePriorityDown,
        &DeviceId::from("b"),
        DeviceType::Output,
        "B",
        &mut state,
    );
    assert!(changed);
    assert_eq!(state.priority_list(DeviceType::Output), &["a", "c", "b"]);
}

#[test]
fn priority_move_to_top() {
    let mut state = PersistentState::default();
    *state.priority_list_mut(DeviceType::Output) =
        vec!["a".into(), "b".into(), "c".into()];
    let changed = handle_priority_event(
        &DeviceAction::MovePriorityToTop,
        &DeviceId::from("c"),
        DeviceType::Output,
        "C",
        &mut state,
    );
    assert!(changed);
    assert_eq!(state.priority_list(DeviceType::Output), &["c", "a", "b"]);
}

#[test]
fn priority_move_to_bottom() {
    let mut state = PersistentState::default();
    *state.priority_list_mut(DeviceType::Output) =
        vec!["a".into(), "b".into(), "c".into()];
    let changed = handle_priority_event(
        &DeviceAction::MovePriorityToBottom,
        &DeviceId::from("a"),
        DeviceType::Output,
        "A",
        &mut state,
    );
    assert!(changed);
    assert_eq!(state.priority_list(DeviceType::Output), &["b", "c", "a"]);
}

#[test]
fn priority_input_type_uses_input_list() {
    let mut state = PersistentState::default();
    handle_priority_event(
        &DeviceAction::AddToPriority,
        &DeviceId::from("mic1"),
        DeviceType::Input,
        "Mic",
        &mut state,
    );
    assert!(state.priority_list(DeviceType::Output).is_empty());
    assert_eq!(state.priority_list(DeviceType::Input), &["mic1"]);
}

// --- apply_device_lock_toggle tests ---

use crate::audio::tests::MockAudioBackend;
use crate::audio::tests::MockDevice;
use super::apply_device_lock_toggle;

fn make_backend_with_device(id: &str, name: &str) -> MockAudioBackend {
    let mut dev = MockDevice::new(id, name, true);
    dev.device_type = DeviceType::Output;
    MockAudioBackend::new(vec![dev])
}

#[test]
fn volume_lock_enable_captures_current_volume() {
    let backend = make_backend_with_device("dev1", "Speaker");
    let mut state = PersistentState::default();

    apply_device_lock_toggle(
        &DeviceAction::VolumeLock,
        true,
        &DeviceId::from("dev1"),
        "Speaker",
        DeviceType::Output,
        &mut state,
        &backend,
    );

    let settings = state.devices.get("dev1").unwrap();
    assert!(settings.volume_lock.is_locked);
    // MockDevice::new creates devices with volume 1.0 (100%)
    assert_eq!(settings.volume_lock.target_percent, 100.0);
}

#[test]
fn volume_lock_disable_clears_locked_state() {
    let backend = make_backend_with_device("dev1", "Speaker");
    let mut state = PersistentState::default();

    // Enable first
    apply_device_lock_toggle(
        &DeviceAction::VolumeLock,
        true,
        &DeviceId::from("dev1"),
        "Speaker",
        DeviceType::Output,
        &mut state,
        &backend,
    );
    assert!(state.devices.get("dev1").unwrap().volume_lock.is_locked);

    // Disable
    apply_device_lock_toggle(
        &DeviceAction::VolumeLock,
        false,
        &DeviceId::from("dev1"),
        "Speaker",
        DeviceType::Output,
        &mut state,
        &backend,
    );
    assert!(!state.devices.get("dev1").unwrap().volume_lock.is_locked);
}

#[test]
fn volume_lock_fails_when_device_not_found() {
    // Empty backend — device lookup will fail
    let backend = MockAudioBackend::new(vec![]);
    let mut state = PersistentState::default();

    apply_device_lock_toggle(
        &DeviceAction::VolumeLock,
        true,
        &DeviceId::from("missing"),
        "Ghost",
        DeviceType::Output,
        &mut state,
        &backend,
    );

    let settings = state.devices.get("missing").unwrap();
    assert!(!settings.volume_lock.is_locked);
}

#[test]
fn unmute_lock_toggle() {
    let backend = make_backend_with_device("dev1", "Speaker");
    let mut state = PersistentState::default();

    apply_device_lock_toggle(
        &DeviceAction::UnmuteLock,
        true,
        &DeviceId::from("dev1"),
        "Speaker",
        DeviceType::Output,
        &mut state,
        &backend,
    );
    assert!(state.devices.get("dev1").unwrap().unmute_lock.is_locked);

    apply_device_lock_toggle(
        &DeviceAction::UnmuteLock,
        false,
        &DeviceId::from("dev1"),
        "Speaker",
        DeviceType::Output,
        &mut state,
        &backend,
    );
    assert!(!state.devices.get("dev1").unwrap().unmute_lock.is_locked);
}

#[test]
fn notify_toggles_independent_of_lock() {
    let backend = make_backend_with_device("dev1", "Speaker");
    let mut state = PersistentState::default();

    apply_device_lock_toggle(
        &DeviceAction::VolumeLockNotify,
        true,
        &DeviceId::from("dev1"),
        "Speaker",
        DeviceType::Output,
        &mut state,
        &backend,
    );
    let settings = state.devices.get("dev1").unwrap();
    assert!(settings.volume_lock.notify);
    assert!(!settings.volume_lock.is_locked);
}

#[test]
fn empty_settings_detected_after_all_unlocked() {
    let backend = make_backend_with_device("dev1", "Speaker");
    let mut state = PersistentState::default();

    // Lock then unlock — settings should be empty
    apply_device_lock_toggle(
        &DeviceAction::VolumeLock,
        true,
        &DeviceId::from("dev1"),
        "Speaker",
        DeviceType::Output,
        &mut state,
        &backend,
    );
    apply_device_lock_toggle(
        &DeviceAction::VolumeLock,
        false,
        &DeviceId::from("dev1"),
        "Speaker",
        DeviceType::Output,
        &mut state,
        &backend,
    );

    let settings = state.devices.get("dev1").unwrap();
    assert!(device_settings_are_empty(settings));
}
