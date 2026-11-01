use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hp_app::runtime::DiscordStatus;
use hp_app::{AppState, t};
use hp_discord::{Link, Presence, Privacy, Status, Texts, Wanted};

const REFRESH: Duration = Duration::from_millis(500);
const MAX_MAPS: usize = 64;

pub struct DiscordPresence {
    presence: Option<Presence>,
    last_check: Option<Instant>,
    opened: HashMap<String, u64>,
}

impl DiscordPresence {
    pub fn new(active: bool) -> Self {
        Self { presence: active.then(Presence::discord), last_check: None, opened: HashMap::new() }
    }

    pub fn tick(&mut self, state: &mut AppState) {
        let Some(presence) = self.presence.as_mut() else {
            state.runtime.discord.status = DiscordStatus::Off;
            return;
        };
        if self.last_check.is_some_and(|t| t.elapsed() < REFRESH) {
            return;
        }
        self.last_check = Some(Instant::now());
        let s = &state.settings.discord;
        let rt = &mut state.runtime.discord;
        rt.builtin_id = s.application_id.trim().is_empty() && !hp_discord::DEFAULT_APPLICATION_ID.is_empty();
        let id = hp_discord::application_id(&s.application_id);
        rt.application_id = id.unwrap_or_default().to_owned();
        let blocked = if !s.enabled {
            Some(DiscordStatus::Disabled)
        } else if id.is_none() && s.application_id.trim().is_empty() {
            Some(DiscordStatus::NoApplicationId)
        } else if id.is_none() {
            Some(DiscordStatus::BadApplicationId)
        } else {
            None
        };
        if let Some(status) = blocked {
            presence.set(Wanted::default());
            rt.status = status;
            rt.activity_details = None;
            rt.activity_state = None;
            return;
        }
        let id = id.unwrap_or_default().to_owned();
        let doc = &state.doc;
        let map = (doc.path().is_some() || doc.manifest().is_some() || doc.is_dirty())
            .then(|| crate::files::map_display_name(state));
        let opened_at = map.as_ref().map(|name| {
            if self.opened.len() >= MAX_MAPS && !self.opened.contains_key(name) {
                self.opened.clear();
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
            *self.opened.entry(name.clone()).or_insert(now)
        });
        let game_id = state.settings.game.active_game();
        let game_name = hp_fs::known_game(game_id).map_or(game_id, |g| g.name);
        let status = Status {
            map: map.as_deref(),
            opened_at,
            tool: Some(t!(state.tool.label_key())),
            compiling: state.runtime.compile.progress.is_some(),
            game_id,
            game_name,
            version: env!("CARGO_PKG_VERSION"),
            project_url: &s.project_url,
        };
        let privacy = Privacy { show_map: s.show_map, show_elapsed: s.show_elapsed, show_tool: s.show_tool };
        let texts = Texts {
            editing: t!("shell.discord.activity_editing"),
            editing_hidden: t!("shell.discord.activity_editing_hidden"),
            no_map: t!("shell.discord.activity_no_map"),
            compiling: t!("shell.discord.activity_compiling"),
            button: t!("shell.discord.activity_button"),
        };
        let activity = hp_discord::build_activity(&status, privacy, &texts);
        let rt = &mut state.runtime.discord;
        rt.activity_details = Some(activity.details.clone());
        rt.activity_state = Some(activity.state.clone());
        presence.set(Wanted { client_id: id, activity: Some(activity) });
        rt.status = status_of(presence.link());
    }
}

pub fn status_of(link: Link) -> DiscordStatus {
    match link {
        Link::Idle | Link::Connecting => DiscordStatus::Connecting,
        Link::NotRunning => DiscordStatus::NotRunning,
        Link::Connected => DiscordStatus::Connected,
        Link::Refused(why) => DiscordStatus::Refused(why),
    }
}
