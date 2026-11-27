use pvp_sim::{Action, Config, Observation, StepResult, sim::Snapshot, trace::Replay};
use serde::Serialize;
use std::{
    fs::{File, create_dir_all},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct Recording {
    pub path: PathBuf,
    replay: Replay,
    transitions: BufWriter<File>,
    finished: bool,
}
impl Recording {
    pub fn new(
        root: &Path,
        config: Config,
        seed: u64,
        opponent: pvp_policy::policy::Style,
    ) -> Result<Self> {
        Self::with_players(root, config, seed, opponent, None)
    }
    pub fn new_spectator(
        root: &Path,
        config: Config,
        seed: u64,
        bot_a: pvp_policy::policy::Style,
        bot_b: pvp_policy::policy::Style,
    ) -> Result<Self> {
        Self::with_players(root, config, seed, bot_b, Some(bot_a))
    }
    fn with_players(
        root: &Path,
        config: Config,
        seed: u64,
        opponent: pvp_policy::policy::Style,
        bot_a: Option<pvp_policy::policy::Style>,
    ) -> Result<Self> {
        create_dir_all(root)?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = root.join(format!("duel-{stamp}-{}", std::process::id()));
        create_dir_all(&path)?;
        let mut metadata = File::create(path.join("metadata.json"))?;
        let opponent_id = match opponent {
            pvp_policy::policy::Style::DuelRl
            | pvp_policy::policy::Style::LeagueRl
            | pvp_policy::policy::Style::LeagueG5
            | pvp_policy::policy::Style::LeagueG6
            | pvp_policy::policy::Style::RecurrentPpo
            | pvp_policy::policy::Style::RecurrentG2
            | pvp_policy::policy::Style::RecurrentG2Plus
            | pvp_policy::policy::Style::G3Prototype => opponent.name().to_ascii_lowercase(),
            _ => format!("scripted_{}", opponent.name().to_ascii_lowercase()),
        };
        serde_json::to_writer_pretty(
            &mut metadata,
            &serde_json::json!({"format_version":2,"application":"pvp-play","simulator_version":env!("CARGO_PKG_VERSION"),"human_seat":if bot_a.is_some() { None } else { Some(0) },"mode":if bot_a.is_some() { "spectator" } else { "human" },"bot_a":bot_a.map(|s| s.name()),"opponent":opponent_id,"seed":seed,"config":config,"input_rate_hz":20,"sprint_key":"always_held","notes":"Transition actions are policy decisions. Replay actions include resolved physical keys."}),
        )?;
        Ok(Self {
            transitions: BufWriter::new(File::create(path.join("transitions.ndjson"))?),
            path,
            replay: Replay::new(config, seed),
            finished: false,
        })
    }
    pub fn transition(
        &mut self,
        before: [Observation; 2],
        actions: [Action; 2],
        after: &StepResult,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Transition<'a> {
            before: [Observation; 2],
            actions: [Action; 2],
            after: &'a StepResult,
        }
        serde_json::to_writer(
            &mut self.transitions,
            &Transition {
                before,
                actions,
                after,
            },
        )?;
        writeln!(self.transitions)?;
        self.replay.record(actions);
        if self.replay.actions.len() % 20 == 0 {
            self.transitions.flush()?;
        }
        Ok(())
    }
    pub fn finish(&mut self, snapshot: &Snapshot, reason: &str) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        self.transitions.flush()?;
        let mut file = BufWriter::new(File::create(self.path.join("replay.json"))?);
        serde_json::to_writer(&mut file, &self.replay)?;
        file.flush()?;
        let mut file = BufWriter::new(File::create(self.path.join("result.json"))?);
        serde_json::to_writer_pretty(
            &mut file,
            &serde_json::json!({"reason":reason,"snapshot":snapshot}),
        )?;
        file.flush()?;
        self.finished = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_human_actions_replay_exactly() {
        let root = std::env::temp_dir().join("pvp-play-recording-tests");
        let config = Config::default();
        let mut d = pvp_sim::Duel::new(config.clone(), 42).unwrap();
        let mut r = Recording::new(&root, config, 42, pvp_policy::policy::Style::Pressure).unwrap();
        for _ in 0..3 {
            let before = d.observations();
            let actions = [Action {
                keys: pvp_sim::Keys {
                    forward: true,
                    ..Default::default()
                },
                ..Default::default()
            }; 2];
            let after = d.step(actions).unwrap();
            r.transition(before, actions, &after).unwrap();
        }
        r.finish(&d.snapshot(), "test").unwrap();
        let saved: Replay =
            serde_json::from_reader(File::open(r.path.join("replay.json")).unwrap()).unwrap();
        assert_eq!(saved.run().unwrap().snapshot(), d.snapshot());
        let data = std::fs::read_to_string(r.path.join("transitions.ndjson")).unwrap();
        assert_eq!(data.lines().count(), 3);
        // Keep the temporary diagnostic artifact; never delete a user path here.
    }
}
