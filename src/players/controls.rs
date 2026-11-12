//! Key bindings: every Player action and the key or mouse button it's on. The Player rebinds them
//! on the Settings screen's Controls page, and they're saved with the Settings. Esc (pause) and
//! the debug keys aren't rebindable, and the mouse wheel always switches Weapons.

use std::collections::BTreeMap;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::storage::settings::Settings;
use crate::players::player::LocalPlayer;
use crate::state::{Choosing, Pause};

/// Something the Player does with a key or mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Sprint,
    /// The Crouch key: lowers into the Crouched Stance, by the Toggle or Hold setting.
    Crouch,
    /// The Prone key: lies the Player into the Prone Stance, by the Toggle or Hold setting.
    Prone,
    Fire,
    Aim,
    Reload,
    /// The Use key: buys Buyables, flips the Power Switch, rebuilds Barriers.
    Use,
    Knife,
    SwitchWeapon,
    Weapon1,
    Weapon2,
    Weapon3,
    Lethal,
    MonkeyBomb,
    Claymore,
    Underbarrel,
}

impl Action {
    pub const ALL: [Action; 21] = [
        Action::Forward,
        Action::Back,
        Action::Left,
        Action::Right,
        Action::Jump,
        Action::Sprint,
        Action::Crouch,
        Action::Prone,
        Action::Fire,
        Action::Aim,
        Action::Reload,
        Action::Use,
        Action::Knife,
        Action::SwitchWeapon,
        Action::Weapon1,
        Action::Weapon2,
        Action::Weapon3,
        Action::Lethal,
        Action::MonkeyBomb,
        Action::Claymore,
        Action::Underbarrel,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Forward => "Move Forward",
            Action::Back => "Move Back",
            Action::Left => "Move Left",
            Action::Right => "Move Right",
            Action::Jump => "Jump",
            Action::Sprint => "Sprint",
            Action::Crouch => "Crouch",
            Action::Prone => "Prone",
            Action::Fire => "Fire",
            Action::Aim => "Aim Down Sights",
            Action::Reload => "Reload",
            Action::Use => "Use",
            Action::Knife => "Knife",
            Action::SwitchWeapon => "Switch Weapon",
            Action::Weapon1 => "Weapon 1",
            Action::Weapon2 => "Weapon 2",
            Action::Weapon3 => "Weapon 3",
            Action::Lethal => "Lethal",
            Action::MonkeyBomb => "Monkey Bomb",
            Action::Claymore => "Claymore",
            Action::Underbarrel => "Underbarrel",
        }
    }

    /// Its stable name in the Settings file.
    fn id(self) -> &'static str {
        match self {
            Action::Forward => "forward",
            Action::Back => "back",
            Action::Left => "left",
            Action::Right => "right",
            Action::Jump => "jump",
            Action::Sprint => "sprint",
            Action::Crouch => "crouch",
            Action::Prone => "prone",
            Action::Fire => "fire",
            Action::Aim => "aim",
            Action::Reload => "reload",
            Action::Use => "use",
            Action::Knife => "knife",
            Action::SwitchWeapon => "switch_weapon",
            Action::Weapon1 => "weapon_1",
            Action::Weapon2 => "weapon_2",
            Action::Weapon3 => "weapon_3",
            Action::Lethal => "lethal",
            Action::MonkeyBomb => "monkey_bomb",
            Action::Claymore => "claymore",
            Action::Underbarrel => "underbarrel",
        }
    }

    pub fn default_binding(self) -> Binding {
        use Binding::{Key, Mouse};
        match self {
            Action::Forward => Key(KeyCode::KeyW),
            Action::Back => Key(KeyCode::KeyS),
            Action::Left => Key(KeyCode::KeyA),
            Action::Right => Key(KeyCode::KeyD),
            Action::Jump => Key(KeyCode::Space),
            Action::Sprint => Key(KeyCode::ShiftLeft),
            Action::Crouch => Key(KeyCode::KeyC),
            Action::Prone => Key(KeyCode::KeyZ),
            Action::Fire => Mouse(MouseButton::Left),
            Action::Aim => Mouse(MouseButton::Right),
            Action::Reload => Key(KeyCode::KeyR),
            Action::Use => Key(KeyCode::KeyF),
            Action::Knife => Key(KeyCode::KeyV),
            Action::SwitchWeapon => Key(KeyCode::KeyQ),
            Action::Weapon1 => Key(KeyCode::Digit1),
            Action::Weapon2 => Key(KeyCode::Digit2),
            Action::Weapon3 => Key(KeyCode::Digit3),
            Action::Lethal => Key(KeyCode::KeyG),
            Action::MonkeyBomb => Key(KeyCode::Digit4),
            Action::Claymore => Key(KeyCode::Digit5),
            Action::Underbarrel => Key(KeyCode::KeyX),
        }
    }
}

/// A key or mouse button an Action is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// Every key that can be bound, and the name shown for it. Esc (pause) and the debug keys (F1–F3,
/// F6–F10) are left out.
const KEYS: &[(KeyCode, &str)] = &[
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyD, "D"),
    (KeyCode::KeyE, "E"),
    (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"),
    (KeyCode::KeyH, "H"),
    (KeyCode::KeyI, "I"),
    (KeyCode::KeyJ, "J"),
    (KeyCode::KeyK, "K"),
    (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"),
    (KeyCode::KeyN, "N"),
    (KeyCode::KeyO, "O"),
    (KeyCode::KeyP, "P"),
    (KeyCode::KeyQ, "Q"),
    (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyT, "T"),
    (KeyCode::KeyU, "U"),
    (KeyCode::KeyV, "V"),
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"),
    (KeyCode::KeyZ, "Z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::F4, "F4"),
    (KeyCode::F5, "F5"),
    (KeyCode::F11, "F11"),
    (KeyCode::F12, "F12"),
    (KeyCode::Space, "Space"),
    (KeyCode::ShiftLeft, "Left Shift"),
    (KeyCode::ShiftRight, "Right Shift"),
    (KeyCode::ControlLeft, "Left Ctrl"),
    (KeyCode::ControlRight, "Right Ctrl"),
    (KeyCode::AltLeft, "Left Alt"),
    (KeyCode::AltRight, "Right Alt"),
    (KeyCode::Tab, "Tab"),
    (KeyCode::CapsLock, "Caps Lock"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::Backspace, "Backspace"),
    (KeyCode::Backquote, "`"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
    (KeyCode::BracketLeft, "["),
    (KeyCode::BracketRight, "]"),
    (KeyCode::Backslash, "\\"),
    (KeyCode::Semicolon, ";"),
    (KeyCode::Quote, "'"),
    (KeyCode::Comma, ","),
    (KeyCode::Period, "."),
    (KeyCode::Slash, "/"),
    (KeyCode::ArrowUp, "Up"),
    (KeyCode::ArrowDown, "Down"),
    (KeyCode::ArrowLeft, "Left"),
    (KeyCode::ArrowRight, "Right"),
    (KeyCode::Insert, "Insert"),
    (KeyCode::Delete, "Delete"),
    (KeyCode::Home, "Home"),
    (KeyCode::End, "End"),
    (KeyCode::PageUp, "Page Up"),
    (KeyCode::PageDown, "Page Down"),
    (KeyCode::Numpad0, "Num 0"),
    (KeyCode::Numpad1, "Num 1"),
    (KeyCode::Numpad2, "Num 2"),
    (KeyCode::Numpad3, "Num 3"),
    (KeyCode::Numpad4, "Num 4"),
    (KeyCode::Numpad5, "Num 5"),
    (KeyCode::Numpad6, "Num 6"),
    (KeyCode::Numpad7, "Num 7"),
    (KeyCode::Numpad8, "Num 8"),
    (KeyCode::Numpad9, "Num 9"),
];

/// Every mouse button that can be bound, and its name.
const MOUSE_BUTTONS: &[(MouseButton, &str)] = &[
    (MouseButton::Left, "Mouse 1"),
    (MouseButton::Right, "Mouse 2"),
    (MouseButton::Middle, "Mouse 3"),
    (MouseButton::Back, "Mouse 4"),
    (MouseButton::Forward, "Mouse 5"),
];

impl Binding {
    /// Its name, as the Controls page and the Prompts show it, and as it's saved.
    pub fn name(self) -> &'static str {
        match self {
            Binding::Key(key) => KEYS.iter().find(|(k, _)| *k == key).map_or("?", |(_, name)| name),
            Binding::Mouse(button) => MOUSE_BUTTONS.iter().find(|(b, _)| *b == button).map_or("?", |(_, name)| name),
        }
    }

    /// The binding with this name, if it's one that can be bound.
    pub fn from_name(name: &str) -> Option<Binding> {
        KEYS.iter()
            .find(|(_, n)| *n == name)
            .map(|(key, _)| Binding::Key(*key))
            .or_else(|| MOUSE_BUTTONS.iter().find(|(_, n)| *n == name).map(|(button, _)| Binding::Mouse(*button)))
    }

    /// Whether a key can be bound.
    pub fn bindable_key(key: KeyCode) -> bool {
        KEYS.iter().any(|(k, _)| *k == key)
    }

    /// Whether a mouse button can be bound.
    pub fn bindable_mouse(button: MouseButton) -> bool {
        MOUSE_BUTTONS.iter().any(|(b, _)| *b == button)
    }
}

/// The Player's bindings: only those changed from the default are kept. Saved as a map from
/// Action to key name; an Action or key the game doesn't know is skipped, so an old or hand-edited
/// file never loses the rest of the Settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "BTreeMap<String, String>", into = "BTreeMap<String, String>")]
pub struct Bindings(BTreeMap<Action, Binding>);

impl Bindings {
    pub fn get(&self, action: Action) -> Binding {
        self.0.get(&action).copied().unwrap_or_else(|| action.default_binding())
    }

    /// Binds `action` to `binding`. Whichever Action had it before takes `action`'s old binding,
    /// so no two Actions ever share one.
    pub fn set(&mut self, action: Action, binding: Binding) {
        let old = self.get(action);
        if let Some(other) = Action::ALL.into_iter().find(|&other| other != action && self.get(other) == binding) {
            self.put(other, old);
        }
        self.put(action, binding);
    }

    fn put(&mut self, action: Action, binding: Binding) {
        if binding == action.default_binding() {
            self.0.remove(&action);
        } else {
            self.0.insert(action, binding);
        }
    }
}

impl From<BTreeMap<String, String>> for Bindings {
    fn from(saved: BTreeMap<String, String>) -> Self {
        let mut bindings = Bindings::default();
        for (action, binding) in saved {
            if let Some(action) = Action::ALL.into_iter().find(|a| a.id() == action)
                && let Some(binding) = Binding::from_name(&binding)
            {
                bindings.set(action, binding);
            }
        }
        bindings
    }
}

impl From<Bindings> for BTreeMap<String, String> {
    fn from(bindings: Bindings) -> Self {
        bindings.0.into_iter().map(|(action, binding)| (action.id().to_string(), binding.name().to_string())).collect()
    }
}

/// A Prompt with the Use key's real name in place of the "F" it's written with.
pub fn with_use_key(prompt: &str, bindings: &Bindings) -> String {
    let key = bindings.get(Action::Use).name();
    if key == "F" {
        return prompt.to_string();
    }
    prompt.replace("Press F ", &format!("Press {key} ")).replace("Hold F ", &format!("Hold {key} "))
}

/// The Player's actions, read through their bindings. With the Pause menu or an Offer open nothing
/// is pressed, which is how a Co-op Player stops while the world runs on: no movement, firing,
/// Knife, Use or Equipment, and any hold lets go.
#[derive(SystemParam)]
pub struct Controls<'w, 's> {
    pause: Option<Res<'w, State<Pause>>>,
    settings: Res<'w, Settings>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    choosing: Query<'w, 's, (), (With<LocalPlayer>, With<Choosing>)>,
}

impl Controls<'_, '_> {
    fn paused(&self) -> bool {
        self.pause.as_ref().is_some_and(|pause| *pause.get() == Pause::Paused) || !self.choosing.is_empty()
    }

    pub fn pressed(&self, action: Action) -> bool {
        if self.paused() {
            return false;
        }
        match self.settings.bindings.get(action) {
            Binding::Key(key) => self.keys.pressed(key),
            Binding::Mouse(button) => self.mouse.pressed(button),
        }
    }

    pub fn just_pressed(&self, action: Action) -> bool {
        if self.paused() {
            return false;
        }
        match self.settings.bindings.get(action) {
            Binding::Key(key) => self.keys.just_pressed(key),
            Binding::Mouse(button) => self.mouse.just_pressed(button),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_binding_is_bindable_and_none_are_shared() {
        for action in Action::ALL {
            let binding = action.default_binding();
            assert_eq!(Binding::from_name(binding.name()), Some(binding), "{action:?}");
            let sharing = Action::ALL.into_iter().filter(|other| other.default_binding() == binding).count();
            assert_eq!(sharing, 1, "{action:?} shares its default");
        }
    }

    #[test]
    fn rebinding_to_a_taken_key_swaps_the_two() {
        let mut bindings = Bindings::default();
        bindings.set(Action::Reload, Binding::Key(KeyCode::KeyF));
        assert_eq!(bindings.get(Action::Reload), Binding::Key(KeyCode::KeyF));
        assert_eq!(bindings.get(Action::Use), Binding::Key(KeyCode::KeyR));
        // Putting them back keeps nothing but defaults.
        bindings.set(Action::Reload, Binding::Key(KeyCode::KeyR));
        assert_eq!(bindings, Bindings::default());
    }

    #[test]
    fn bindings_round_trip_and_skip_what_they_dont_know() {
        let mut bindings = Bindings::default();
        bindings.set(Action::Knife, Binding::Mouse(MouseButton::Back));
        bindings.set(Action::Jump, Binding::Key(KeyCode::ControlLeft));
        let saved: BTreeMap<String, String> = bindings.clone().into();
        assert_eq!(Bindings::from(saved.clone()), bindings);

        let mut odd = saved;
        odd.insert("teleport".into(), "T".into());
        odd.insert("reload".into(), "Hyper".into());
        assert_eq!(Bindings::from(odd), bindings);
    }

    #[test]
    fn prompts_name_the_use_key() {
        let mut bindings = Bindings::default();
        assert_eq!(with_use_key("Press F to open Door", &bindings), "Press F to open Door");
        bindings.set(Action::Use, Binding::Key(KeyCode::KeyE));
        assert_eq!(with_use_key("Press F to open Door", &bindings), "Press E to open Door");
        assert_eq!(with_use_key("Hold F to rebuild Barrier", &bindings), "Hold E to rebuild Barrier");
    }
}
