use super::NavScheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavInput {
    RightDrag3d,
    MiddleDrag3d,
    Wasd,
    MouselookKey,
    SpaceLeft3d,
    SpaceRight3d,
    LeftRight3d,
    Arrows3d,
    Pan2d,
    Arrows2d,
    Digits2d,
    FlyClick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavEffect {
    LookFly,
    Look,
    Pan3d,
    Strafe,
    ArrowLook,
    Pan2d,
    Scroll2d,
    ZoomPresets,
    FlyWithRight,
    FlyWithMiddle,
    ToggleMouselook,
    SelectAtCrosshair,
    ToolOrMenu,
}

impl NavInput {
    pub const ALL: [NavInput; 12] = [
        NavInput::RightDrag3d,
        NavInput::MiddleDrag3d,
        NavInput::Wasd,
        NavInput::MouselookKey,
        NavInput::SpaceLeft3d,
        NavInput::SpaceRight3d,
        NavInput::LeftRight3d,
        NavInput::Arrows3d,
        NavInput::Pan2d,
        NavInput::Arrows2d,
        NavInput::Digits2d,
        NavInput::FlyClick,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            NavInput::RightDrag3d => "shell.help.ctl_rmb",
            NavInput::MiddleDrag3d => "shell.view.ctl_mmb",
            NavInput::Wasd => "shell.help.ctl_wasd",
            NavInput::MouselookKey => "shell.nav.z",
            NavInput::SpaceLeft3d => "shell.view.ctl_space_left",
            NavInput::SpaceRight3d => "shell.view.ctl_space_right",
            NavInput::LeftRight3d => "shell.view.ctl_both",
            NavInput::Arrows3d => "shell.view.ctl_arrows_3d",
            NavInput::Pan2d => "shell.help.ctl_mmb",
            NavInput::Arrows2d => "shell.view.ctl_arrows_2d",
            NavInput::Digits2d => "shell.view.ctl_digits",
            NavInput::FlyClick => "shell.help.ctl_fly_click",
        }
    }
}

impl NavEffect {
    pub fn desc_key(self) -> &'static str {
        match self {
            NavEffect::LookFly => "shell.view.ctl_mmb_look",
            NavEffect::Look => "shell.view.ctl_space_left_desc",
            NavEffect::Pan3d => "shell.view.ctl_space_right_desc",
            NavEffect::Strafe => "shell.view.ctl_both_desc",
            NavEffect::ArrowLook => "shell.view.ctl_arrows_3d_desc",
            NavEffect::Pan2d => "shell.nav.pan_2d",
            NavEffect::Scroll2d => "shell.view.ctl_arrows_2d_desc",
            NavEffect::ZoomPresets => "shell.view.ctl_digits_desc",
            NavEffect::FlyWithRight => "shell.help.ctl_wasd_desc",
            NavEffect::FlyWithMiddle => "shell.nav.fly_middle",
            NavEffect::ToggleMouselook => "shell.nav.z_desc",
            NavEffect::SelectAtCrosshair => "shell.help.ctl_fly_click_desc",
            NavEffect::ToolOrMenu => "shell.nav.rmb_free",
        }
    }
}

impl NavScheme {
    pub fn binding(self, input: NavInput) -> Option<NavEffect> {
        let pp = self == NavScheme::HammerPlusPlus;
        let hammer = self != NavScheme::HammerPro;
        match input {
            NavInput::RightDrag3d => Some(if pp { NavEffect::ToolOrMenu } else { NavEffect::LookFly }),
            NavInput::MiddleDrag3d => Some(if pp { NavEffect::LookFly } else { NavEffect::Pan3d }),
            NavInput::Wasd => Some(if pp { NavEffect::FlyWithMiddle } else { NavEffect::FlyWithRight }),
            NavInput::MouselookKey => Some(NavEffect::ToggleMouselook),
            NavInput::SpaceLeft3d => hammer.then_some(NavEffect::Look),
            NavInput::SpaceRight3d => hammer.then_some(NavEffect::Pan3d),
            NavInput::LeftRight3d => hammer.then_some(NavEffect::Strafe),
            NavInput::Arrows3d => hammer.then_some(NavEffect::ArrowLook),
            NavInput::Pan2d => Some(NavEffect::Pan2d),
            NavInput::Arrows2d => Some(NavEffect::Scroll2d),
            NavInput::Digits2d => Some(NavEffect::ZoomPresets),
            NavInput::FlyClick => Some(NavEffect::SelectAtCrosshair),
        }
    }

    pub fn bindings(self) -> Vec<(NavInput, NavEffect)> {
        NavInput::ALL.into_iter().filter_map(|i| self.binding(i).map(|e| (i, e))).collect()
    }

    pub fn right_looks(self) -> bool {
        self.binding(NavInput::RightDrag3d) == Some(NavEffect::LookFly)
    }

    pub fn right_free(self) -> bool {
        self.binding(NavInput::RightDrag3d) == Some(NavEffect::ToolOrMenu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hammer_plus_plus_frees_the_right_button_and_looks_with_the_middle_one() {
        let pp = NavScheme::HammerPlusPlus;
        assert_eq!(pp.binding(NavInput::RightDrag3d), Some(NavEffect::ToolOrMenu));
        assert_eq!(pp.binding(NavInput::MiddleDrag3d), Some(NavEffect::LookFly));
        assert_eq!(pp.binding(NavInput::Wasd), Some(NavEffect::FlyWithMiddle));
        assert!(pp.right_free() && !pp.right_looks() && pp.middle_looks() && pp.hammer_gestures());
        assert!(!pp.bindings().iter().any(|(_, e)| *e == NavEffect::FlyWithRight));
    }

    #[test]
    fn hammer_and_hammer_pro_keep_the_right_button_look() {
        for scheme in [NavScheme::HammerPro, NavScheme::Hammer] {
            assert_eq!(scheme.binding(NavInput::RightDrag3d), Some(NavEffect::LookFly));
            assert_eq!(scheme.binding(NavInput::MiddleDrag3d), Some(NavEffect::Pan3d));
            assert_eq!(scheme.binding(NavInput::Wasd), Some(NavEffect::FlyWithRight));
            assert!(scheme.right_looks() && !scheme.right_free() && !scheme.middle_looks());
        }
        assert!(NavScheme::Hammer.hammer_gestures() && !NavScheme::HammerPro.hammer_gestures());
        assert_eq!(NavScheme::HammerPro.binding(NavInput::SpaceLeft3d), None);
        assert_eq!(NavScheme::Hammer.binding(NavInput::LeftRight3d), Some(NavEffect::Strafe));
    }

    #[test]
    fn every_scheme_lists_each_input_once() {
        for scheme in NavScheme::ALL {
            let b = scheme.bindings();
            for (i, (input, _)) in b.iter().enumerate() {
                assert!(!b[i + 1..].iter().any(|(other, _)| other == input), "{scheme:?} {input:?}");
            }
            assert!(b.iter().any(|(i, _)| *i == NavInput::MouselookKey));
        }
    }
}
