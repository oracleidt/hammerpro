#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DiscordStatus {
    #[default]
    Off,
    Disabled,
    NoApplicationId,
    BadApplicationId,
    Connecting,
    NotRunning,
    Connected,
    Refused(String),
}

impl DiscordStatus {
    pub fn id(&self) -> &'static str {
        match self {
            DiscordStatus::Off => "off",
            DiscordStatus::Disabled => "disabled",
            DiscordStatus::NoApplicationId => "no_application_id",
            DiscordStatus::BadApplicationId => "bad_application_id",
            DiscordStatus::Connecting => "connecting",
            DiscordStatus::NotRunning => "discord_not_running",
            DiscordStatus::Connected => "connected",
            DiscordStatus::Refused(_) => "refused",
        }
    }

    pub fn label_key(&self) -> &'static str {
        match self {
            DiscordStatus::Off => "shell.discord.status_off",
            DiscordStatus::Disabled => "shell.discord.status_disabled",
            DiscordStatus::NoApplicationId => "shell.discord.status_no_id",
            DiscordStatus::BadApplicationId => "shell.discord.status_bad_id",
            DiscordStatus::Connecting => "shell.discord.status_connecting",
            DiscordStatus::NotRunning => "shell.discord.status_not_running",
            DiscordStatus::Connected => "shell.discord.status_connected",
            DiscordStatus::Refused(_) => "shell.discord.status_refused",
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            DiscordStatus::Refused(why) => Some(why),
            _ => None,
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, DiscordStatus::Connected)
    }
}

#[derive(Debug, Default)]
pub struct DiscordRuntime {
    pub status: DiscordStatus,
    pub application_id: String,
    pub builtin_id: bool,
    pub activity_details: Option<String>,
    pub activity_state: Option<String>,
}
