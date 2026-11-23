use crate::{particles::CriticalParticles, recording::Recording, storage};
use pvp_policy::policy::{Bot, Style};
use pvp_sim::{
    Action, Config, Duel, Keys, Look, Observation, Outcome,
    config::{ClientTiming, SECOND_NS},
    math::wrap_degrees,
    sim::{ClientView, SoundEvent},
};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};

/// First simulated client frame at or after the sound packet's arrival.
fn audible_at(event_ns: u64, timing: &ClientTiming) -> u64 {
    let arrival = event_ns + timing.downstream_ns;
    let elapsed = arrival.saturating_sub(timing.frame_phase_ns);
    let fps = u64::from(timing.fps);
    let frame = (elapsed * fps).div_ceil(SECOND_NS);
    timing.frame_phase_ns + frame * SECOND_NS / fps
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct Game {
    pub duel: Duel,
    pub previous: ClientView,
    pub current: ClientView,
    pub observation: Observation,
    pub yaw: f32,
    pub pitch: f32,
    pub pending_click: bool,
    pub paused: bool,
    pub started: bool,
    pub pause_reason: String,
    pub status: String,
    pub recording_error: Option<String>,
    pub damage_flash: f32,
    pub opponent_damage_flash: f32,
    pub opponent_swing: f32,
    pub particles: CriticalParticles,
    pub sounds: VecDeque<SoundEvent>,
    pub recording: Option<Recording>,
    pub record_root: PathBuf,
    pub seed: u64,
    pub spectating: bool,
    pub view_seat: usize,
    pub last_actions: [Action; 2],
    pub click_flash: [f32; 2],
    pub human_ping_ms: f64,
    pub sprint_states: [bool; 2],
    pub recurrent_model: Option<Arc<pvp_policy::recurrent::Model>>,
    bot_a: Bot,
    bot: Bot,
}
impl Game {
    pub fn new(config: Config, seed: u64, record_root: PathBuf, style: Style) -> Result<Self> {
        let human_ping_ms = config
            .ping_profile
            .as_ref()
            .and_then(|p| p.fixed_ms[0])
            .unwrap_or(
                (config.clients[0].upstream_ns + config.clients[0].downstream_ns) as f64 / 1e6,
            );
        let duel = Duel::new(config, seed)?;
        let current = duel.client_view(0)?;
        let observation = duel.observe(0)?;
        Ok(Self {
            duel,
            previous: current,
            current,
            observation,
            yaw: current.local.yaw,
            pitch: current.local.pitch,
            pending_click: false,
            paused: true,
            started: false,
            pause_reason: "Ready".into(),
            status: "A new recording starts with the match.".into(),
            recording_error: None,
            damage_flash: 0.0,
            opponent_damage_flash: 0.0,
            opponent_swing: 0.0,
            particles: CriticalParticles::new(),
            sounds: VecDeque::new(),
            recording: None,
            record_root,
            seed,
            spectating: false,
            view_seat: 0,
            last_actions: [Action::default(); 2],
            click_flash: [0.; 2],
            human_ping_ms,
            sprint_states: [false; 2],
            recurrent_model: None,
            bot_a: Bot::new(Style::Critical),
            bot: Bot::new(style),
        })
    }
    pub fn opponent(&self) -> Style {
        self.bot.style()
    }
    pub fn bot_a_style(&self) -> Style {
        self.bot_a.style()
    }
    pub fn toggle_spectating(&mut self) {
        if self.started || !self.paused {
            return;
        }
        self.spectating = !self.spectating;
        let mut config = self.duel.config().clone();
        if let Some(profile) = &mut config.ping_profile {
            profile.fixed_ms[0] = if self.spectating {
                None
            } else {
                Some(self.human_ping_ms)
            };
            self.duel = Duel::new(config, self.seed).expect("valid ping profile");
        }
        self.set_view(0);
    }
    pub fn change_human_ping(&mut self, delta: f64) -> Result<()> {
        if self.started || !self.paused || self.spectating {
            return Ok(());
        }
        self.human_ping_ms = (self.human_ping_ms + delta).clamp(0., 10000.);
        let mut config = self.duel.config().clone();
        if let Some(p) = &mut config.ping_profile {
            p.fixed_ms[0] = Some(self.human_ping_ms);
        } else {
            let total = (self.human_ping_ms * 1e6).round() as u64;
            config.clients[0].upstream_ns = total / 2;
            config.clients[0].downstream_ns = total - total / 2;
        }
        self.duel = Duel::new(config, self.seed)?;
        self.set_view(self.view_seat);
        Ok(())
    }
    pub fn cycle_bot_a(&mut self, direction: isize) {
        self.select_bot_a(self.next_style(self.bot_a.style(), direction));
    }
    fn next_style(&self, current: Style, direction: isize) -> Style {
        let mut styles = Style::ALL.to_vec();
        styles.insert(1, Style::RecurrentPpo);
        styles.insert(2, Style::RecurrentG2);
        styles.insert(3, Style::RecurrentG2Plus);
        let index = styles.iter().position(|s| *s == current).unwrap();
        styles[(index as isize + direction).rem_euclid(styles.len() as isize) as usize]
    }
    pub fn select_bot_a(&mut self, style: Style) {
        if self.spectating && self.paused && !self.started {
            self.bot_a = Bot::new(style);
        }
    }
    pub fn switch_view(&mut self) {
        if self.spectating {
            self.set_view(1 - self.view_seat);
        }
    }
    fn set_view(&mut self, seat: usize) {
        self.view_seat = seat;
        self.current = self.duel.client_view(seat).expect("valid view seat");
        self.previous = self.current;
        self.observation = self.duel.observe(seat).expect("valid view seat");
        self.yaw = self.current.local.yaw;
        self.pitch = self.current.local.pitch;
        self.sounds.clear();
        self.particles = CriticalParticles::new();
        self.damage_flash = 0.;
        self.opponent_damage_flash = 0.;
        self.opponent_swing = 0.;
    }
    pub fn cycle_opponent(&mut self, direction: isize) {
        if self.started || !self.paused {
            return;
        }
        self.bot = Bot::new(self.next_style(self.opponent(), direction));
    }
    pub fn resume(&mut self) {
        if self.duel.is_finished() {
            return;
        }
        // A failed recording must not prevent play. Do not retry halfway through
        // a match: such a replay would be missing its initial actions.
        if !self.started {
            if (self.opponent().is_recurrent()
                || (self.spectating && self.bot_a_style().is_recurrent()))
                && !self.duel.config().expose_opponent_health
            {
                let mut config = self.duel.config().clone();
                config.expose_opponent_health = true;
                self.duel = Duel::new(config, self.seed).expect("valid match configuration");
                self.set_view(self.view_seat);
            }
            let recording = if self.spectating {
                Recording::new_spectator(
                    &self.record_root,
                    self.duel.config().clone(),
                    self.seed,
                    self.bot_a.style(),
                    self.opponent(),
                )
            } else {
                Recording::new(
                    &self.record_root,
                    self.duel.config().clone(),
                    self.seed,
                    self.opponent(),
                )
            };
            match recording {
                Ok(r) => {
                    self.status = format!("Recording: {}", r.path.display());
                    self.recording = Some(r);
                }
                Err(e) => self.disable_recording("start", e.as_ref()),
            }
        }
        self.started = true;
        self.paused = false;
        self.pending_click = false;
        self.previous = self.current;
    }

    fn disable_recording(&mut self, operation: &str, error: &dyn std::error::Error) {
        let path = self
            .recording
            .as_ref()
            .map_or(self.record_root.as_path(), |r| r.path.as_path());
        let message = format!(
            "Recording {operation} failed at {}: {error:?}",
            path.display()
        );
        let log = storage::report_error(&self.record_root, &message);
        self.status = match log {
            Some(path) => format!("Recording OFF. Error log: {}", path.display()),
            None => format!("Recording OFF. {message}"),
        };
        self.recording_error = Some(message);
        self.recording = None;
    }
    pub fn pause(&mut self, reason: &str) {
        self.paused = true;
        self.pending_click = false;
        self.pause_reason = reason.into();
        self.previous = self.current;
    }
    pub fn look(&mut self, dx: f32, dy: f32, sensitivity: f32) {
        self.yaw = wrap_degrees(self.yaw + dx * sensitivity);
        self.pitch = (self.pitch + dy * sensitivity).clamp(-90.0, 90.0);
    }
    pub fn tick(&mut self, keys: Keys, held: bool) -> Result<()> {
        if self.paused || self.duel.is_finished() {
            return Ok(());
        }
        let before = self.duel.observations();
        let human = Action {
            keys,
            look: Look::Angles {
                yaw: self.yaw,
                pitch: self.pitch,
            },
            click: std::mem::take(&mut self.pending_click),
            attack_held: held,
        };
        let actions = [
            if self.spectating {
                self.bot_a.act_in_duel(
                    &self.duel,
                    0,
                    &before[0],
                    self.recurrent_model.as_deref(),
                )?
            } else {
                human
            },
            self.bot
                .act_in_duel(&self.duel, 1, &before[1], self.recurrent_model.as_deref())?,
        ];
        self.last_actions = actions;
        for (seat, action) in actions.iter().enumerate() {
            if action.click {
                self.click_flash[seat] = 0.12;
            }
        }
        let listener = self.duel.config().clients[self.view_seat].clone();
        let after = self.duel.step_with_sounds(actions, |mut event| {
            event.time_ns = audible_at(event.time_ns, &listener);
            self.sounds.push_back(event);
        })?;
        self.previous = self.current;
        self.current = self.duel.client_view(self.view_seat)?;
        if after.observations[self.view_seat].health < self.observation.health {
            self.damage_flash = 0.25;
        }
        self.observation = after.observations[self.view_seat].clone();
        self.sprint_states = after.observations.each_ref().map(|o| o.sprinting);
        if self.spectating {
            self.yaw = self.current.local.yaw;
            self.pitch = self.current.local.pitch;
        }
        if let Some(r) = &mut self.recording {
            if let Err(e) = r.transition(before, actions, &after) {
                self.disable_recording("write", e.as_ref());
            }
        }
        if self.duel.is_finished() {
            self.pause("Match finished");
            self.save("finished");
        }
        Ok(())
    }
    pub fn save(&mut self, reason: &str) {
        if let Some(r) = &mut self.recording {
            match r.finish(&self.duel.snapshot(), reason) {
                Ok(()) => self.status = format!("Saved: {}", r.path.display()),
                Err(e) => self.disable_recording("finish", e.as_ref()),
            }
        }
    }
    pub fn restart(&mut self) -> Result<()> {
        self.save("restart");
        let mut next = Self::new(
            self.duel.config().clone(),
            if self.duel.config().ping_profile.is_some() {
                self.seed.wrapping_add(1)
            } else {
                self.seed
            },
            self.record_root.clone(),
            self.opponent(),
        )?;
        next.spectating = self.spectating;
        next.human_ping_ms = self.human_ping_ms;
        next.bot_a = Bot::new(self.bot_a.style());
        next.recurrent_model = self.recurrent_model.clone();
        if self.spectating {
            next.set_view(self.view_seat);
        }
        *self = next;
        Ok(())
    }
    pub fn outcome_title(&self) -> &str {
        match self.duel.outcome() {
            Outcome::Victory { winner: 0 } if self.spectating => "BOT A WINS",
            Outcome::Victory { .. } if self.spectating => "BOT B WINS",
            Outcome::Victory { winner: 0 } => "YOU WIN",
            Outcome::Victory { .. } => "DEFEAT",
            Outcome::Truncated => "TIME LIMIT",
            Outcome::Ongoing if !self.started && self.spectating => "READY TO WATCH",
            Outcome::Ongoing if !self.started => "READY TO DUEL",
            Outcome::Ongoing => "PAUSED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests::temp_root;

    #[test]
    fn human_ping_settings_and_spectator_rerolls() {
        use pvp_sim::ping::PingProfile;
        let config = Config {
            ping_profile: Some(PingProfile {
                fixed_ms: [Some(20.), None],
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut game =
            Game::new(config, 42, temp_root("ping-settings"), Style::CriticalPlus).unwrap();
        let rtt = |game: &Game, seat: usize| {
            let c = &game.duel.config().clients[seat];
            c.upstream_ns + c.downstream_ns
        };
        assert_eq!(rtt(&game, 0), 20_000_000);
        let opponent = rtt(&game, 1);
        game.change_human_ping(17.).unwrap();
        assert_eq!(rtt(&game, 0), 37_000_000);
        assert_eq!(rtt(&game, 1), opponent);
        game.restart().unwrap();
        assert_eq!(rtt(&game, 0), 37_000_000);
        assert_ne!(rtt(&game, 1), opponent);
        game.toggle_spectating();
        assert_eq!(
            game.duel.config().ping_profile.as_ref().unwrap().fixed_ms[0],
            None
        );
        assert!((5_000_000..=150_000_000).contains(&rtt(&game, 0)));
        game.restart().unwrap();
        game.toggle_spectating();
        assert_eq!(rtt(&game, 0), 37_000_000);
        game.started = true;
        game.change_human_ping(10.).unwrap();
        assert_eq!(rtt(&game, 0), 37_000_000);
    }

    #[test]
    fn spectator_actions_match_bots_and_view_switches_do_not_change_simulation() {
        let config = Config::default();
        let mut game =
            Game::new(config.clone(), 42, temp_root("spectator"), Style::Pressure).unwrap();
        game.toggle_spectating();
        let mut reference = Duel::new(config, 42).unwrap();
        let mut bots = [Bot::new(Style::Critical), Bot::new(Style::Pressure)];
        game.resume();
        for step in 0..6000 {
            if step % 9 == 0 {
                game.switch_view();
            }
            let before = reference.observations();
            let actions = std::array::from_fn(|seat| bots[seat].act(&before[seat]));
            reference.step(actions).unwrap();
            // Deliberately conflicting user keys/clicks must be ignored.
            game.pending_click = true;
            game.tick(
                Keys {
                    backward: true,
                    sneak: true,
                    ..Default::default()
                },
                true,
            )
            .unwrap();
            assert_eq!(game.last_actions, actions);
            assert_eq!(game.duel.snapshot(), reference.snapshot());
            assert_eq!(game.current, game.duel.client_view(game.view_seat).unwrap());
            assert_eq!(game.yaw, game.current.local.yaw);
            assert_eq!(game.observation, game.duel.observe(game.view_seat).unwrap());
            if game.duel.is_finished() {
                break;
            }
        }
        assert!(game.duel.is_finished());
        assert!(matches!(game.outcome_title(), "BOT A WINS" | "BOT B WINS"));
        game.save("spectator_test");
        let path = &game.recording.as_ref().unwrap().path;
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path.join("metadata.json")).unwrap()).unwrap();
        assert!(metadata["human_seat"].is_null());
        assert_eq!(metadata["mode"], "spectator");
        assert_eq!(metadata["bot_a"], "Critical");
        let replay: pvp_sim::trace::Replay =
            serde_json::from_slice(&std::fs::read(path.join("replay.json")).unwrap()).unwrap();
        assert_eq!(replay.run().unwrap().snapshot(), reference.snapshot());
        game.pause("test");
        let before = game.duel.snapshot();
        game.tick(Keys::default(), false).unwrap();
        assert_eq!(game.duel.snapshot(), before);
        let seat = game.view_seat;
        game.restart().unwrap();
        assert!(game.spectating);
        assert_eq!(game.view_seat, seat);
        assert_eq!(game.opponent(), Style::Pressure);
        assert_eq!(game.bot_a_style(), Style::Critical);
        assert_eq!(game.duel.decision_step(), 0);
        assert_eq!(game.click_flash, [0.; 2]);
    }

    #[test]
    fn recurrent_g2_can_be_selected_and_played() {
        let mut game = Game::new(
            Config::default(),
            42,
            temp_root("recurrent_g2"),
            Style::Baseline,
        )
        .unwrap();
        game.cycle_opponent(1);
        game.cycle_opponent(1);
        assert_eq!(game.opponent(), Style::RecurrentG2);
        assert_eq!("recurrent-G2".parse::<Style>().unwrap(), Style::RecurrentG2);
        game.cycle_opponent(1);
        assert_eq!(game.opponent(), Style::RecurrentG2Plus);
        game.resume();
        game.tick(Keys::default(), false).unwrap();
        assert!(game.duel.config().expose_opponent_health);
        assert_eq!(game.duel.decision_step(), 1);
    }

    #[test]
    fn bundled_recurrent_opponent_can_be_selected_and_recorded() {
        let mut game = Game::new(
            Config::default(),
            42,
            temp_root("recurrent_opponent"),
            Style::Baseline,
        )
        .unwrap();
        game.cycle_opponent(1);
        assert_eq!(game.opponent(), Style::RecurrentPpo);
        game.resume();
        game.tick(Keys::default(), false).unwrap();
        assert_eq!(game.duel.decision_step(), 1);
        assert_eq!(game.duel.observe(1).unwrap().opponent_health, Some(20.));
        game.save("recurrent_test");
        let recording = game.recording.as_ref().unwrap();
        let replay: pvp_sim::trace::Replay =
            serde_json::from_slice(&std::fs::read(recording.path.join("replay.json")).unwrap())
                .unwrap();
        assert_eq!(replay.run().unwrap().snapshot(), game.duel.snapshot());
    }

    #[test]
    fn spectator_setup_is_only_editable_before_match_start() {
        let mut game = Game::new(
            Config::default(),
            42,
            temp_root("spectator_setup"),
            Style::Critical,
        )
        .unwrap();
        game.toggle_spectating();
        game.cycle_bot_a(1);
        game.cycle_opponent(1);
        assert_eq!(game.bot_a_style(), Style::Baseline);
        assert_eq!(game.opponent(), Style::Baseline);
        game.resume();
        game.pause("test");
        game.toggle_spectating();
        game.cycle_bot_a(1);
        game.cycle_opponent(1);
        assert!(game.spectating);
        assert_eq!(game.bot_a_style(), Style::Baseline);
        assert_eq!(game.opponent(), Style::Baseline);
        game.switch_view();
        assert_eq!(game.current, game.previous);
        assert_eq!(game.view_seat, 1);
        game.restart().unwrap();
        game.toggle_spectating();
        assert!(!game.spectating);
        assert_eq!(game.view_seat, 0);
        game.switch_view();
        assert_eq!(game.view_seat, 0);
    }

    #[test]
    fn opponent_selection_is_fixed_during_a_recorded_match() {
        let mut game = Game::new(
            Config::default(),
            42,
            temp_root("selection"),
            Style::Baseline,
        )
        .unwrap();
        game.cycle_opponent(-1);
        assert_eq!(game.opponent(), Style::Critical);
        game.resume();
        game.tick(Keys::default(), false).unwrap();
        game.pause("test");
        game.cycle_opponent(1);
        assert_eq!(game.opponent(), Style::Critical);
        let dir = game.recording.as_ref().unwrap().path.clone();
        game.restart().unwrap();
        let meta: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(meta["opponent"], "scripted_critical");
        assert!(dir.join("replay.json").exists());
        assert_eq!(game.opponent(), Style::Critical);
        assert_eq!(game.observation.distance, 20.0);
        game.cycle_opponent(1);
        assert_eq!(game.opponent(), Style::Baseline);
    }

    #[test]
    fn sound_delivery_waits_for_listener_frame_and_downstream() {
        let timing = ClientTiming {
            downstream_ns: 60_000_000,
            ..Default::default()
        };
        assert_eq!(audible_at(50_000_000, &timing), 116_666_666);
        let timing = ClientTiming {
            frame_phase_ns: 3_000_000,
            downstream_ns: 0,
            ..timing
        };
        assert_eq!(audible_at(0, &timing), 3_000_000);
        assert_eq!(audible_at(19_666_666, &timing), 19_666_666);
    }

    #[test]
    fn recording_failure_does_not_prevent_play_or_resume() {
        // A file in place of the recording directory reliably reproduces an
        // inaccessible destination without changing permissions or the CWD.
        let root = temp_root("blocked-recording");
        std::fs::write(&root, "not a directory").unwrap();
        let mut game = Game::new(Config::default(), 42, root.clone(), Style::Pressure).unwrap();
        game.resume();
        assert!(game.started && !game.paused);
        assert!(game.recording.is_none());
        assert!(
            game.recording_error
                .as_ref()
                .unwrap()
                .contains("start failed")
        );
        assert!(game.status.contains("Error log:"));
        game.tick(Keys::default(), false).unwrap();
        assert_eq!(game.duel.decision_step(), 1);

        // Even if another destination becomes available, resuming must not
        // create a partial replay that pretends to start at the initial state.
        let recovered = temp_root("recording-after-failure");
        game.record_root = recovered.clone();
        game.pause("test");
        game.resume();
        game.tick(Keys::default(), false).unwrap();
        game.save("test");
        assert_eq!(game.duel.decision_step(), 2);
        assert!(game.recording.is_none());
        assert!(!recovered.exists());

        // A fresh match can retry recording from the correct initial state.
        game.restart().unwrap();
        game.resume();
        assert!(game.recording_error.is_none());
        assert!(game.recording.is_some());
        game.tick(Keys::default(), false).unwrap();
        game.save("test");
        let path = game.recording.as_ref().unwrap().path.join("replay.json");
        let replay: pvp_sim::trace::Replay =
            serde_json::from_reader(std::fs::File::open(path).unwrap()).unwrap();
        assert_eq!(replay.run().unwrap().snapshot(), game.duel.snapshot());
    }

    #[test]
    fn save_failure_keeps_the_match_playable_and_logs_the_error() {
        let root = temp_root("blocked-save");
        let mut game = Game::new(Config::default(), 42, root.clone(), Style::Pressure).unwrap();
        game.resume();
        game.tick(Keys::default(), false).unwrap();
        let session = game.recording.as_ref().unwrap().path.clone();
        std::fs::create_dir(session.join("replay.json")).unwrap();
        game.save("test");
        assert!(game.recording.is_none());
        assert!(game.recording_error.is_some());
        let log = std::fs::read_to_string(root.join("errors.log")).unwrap();
        assert!(log.contains("Recording finish failed"));
        assert!(!session.join("result.json").exists());
        game.tick(Keys::default(), false).unwrap();
        assert_eq!(game.duel.decision_step(), 2);
    }

    #[test]
    fn view_and_mouse_are_local_and_pause_stops_the_world() {
        let mut game = Game::new(
            Config::default(),
            42,
            std::env::temp_dir().join("pvp-play-tests"),
            Style::Pressure,
        )
        .unwrap();
        game.look(10.0, -10.0, 0.15);
        assert_eq!(game.yaw, 1.5);
        assert_eq!(game.pitch, -1.5);
        game.tick(
            Keys {
                forward: true,
                ..Default::default()
            },
            true,
        )
        .unwrap();
        assert_eq!(game.duel.decision_step(), 0);
        assert_eq!(game.current, game.duel.client_view(0).unwrap());
    }
}
