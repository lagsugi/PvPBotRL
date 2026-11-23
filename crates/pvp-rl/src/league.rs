use pvp_sim::{
    Config, TICK_NS,
    ping::{PingProfile, gamma_rtt_ms},
    rng::derive_seed,
};
use serde::Serialize;

pub struct Random {
    seed: u64,
    index: u64,
}
impl Random {
    pub fn new(seed: u64) -> Self {
        Self { seed, index: 0 }
    }
    pub fn next_u64(&mut self) -> u64 {
        let value = derive_seed(self.seed, self.index);
        self.index += 1;
        value
    }
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn normal(&mut self) -> f64 {
        (-2.0 * (1.0 - self.unit()).ln()).sqrt() * (std::f64::consts::TAU * self.unit()).cos()
    }
}

pub struct Trial {
    pub config: Config,
    pub seed: u64,
    pub seat: usize,
}
pub fn trial(seed: u64, stress: bool) -> Trial {
    let mut rng = Random::new(seed);
    let seat = (rng.next_u64() % 2) as usize;
    let mut profile = PingProfile::default();
    if stress {
        profile.fixed_ms[seat] = Some(120.);
        profile.fixed_ms[1 - seat] = Some(20.);
    }
    let mut config = Config {
        ping_profile: Some(profile),
        first_client: Some((rng.next_u64() % 2) as usize),
        ..Config::default()
    };
    // Include the play window's actual spawn, 60 FPS and zero phases as well as varied starts.
    if rng.unit() >= 0.35 {
        let gap = 3.5 + 28.5 * rng.unit();
        config.spawns[0].position.z = -gap / 2.;
        config.spawns[1].position.z = gap / 2.;
        config.spawns[1].position.x = 4. * rng.unit() - 2.;
        config.server_phase_ns = rng.next_u64() % TICK_NS;
        for c in &mut config.clients {
            c.fps = [30, 60, 144, 240][(rng.next_u64() % 4) as usize];
            c.tick_phase_ns = rng.next_u64() % TICK_NS;
            c.frame_phase_ns = rng.next_u64() % (1_000_000_000 / u64::from(c.fps));
        }
    }
    Trial {
        config,
        seed: rng.next_u64(),
        seat,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum Network {
    Mixed,
    Both0,
    Both2,
    Both20,
    Both60,
    Both120,
    Both150,
    LocalRival,
    LocalActor,
    Adverse,
    Bot20Rival0,
    EarlyBot20Rival0,
    Legacy,
}

pub const NETWORKS: [Network; 10] = [
    Network::Mixed,
    Network::Both0,
    Network::Both2,
    Network::Both20,
    Network::Both60,
    Network::Both120,
    Network::Both150,
    Network::LocalRival,
    Network::LocalActor,
    Network::Adverse,
];
pub const LOCAL_RIVAL: usize = 7;

pub fn scenario(seed: u64, network: Network) -> Trial {
    let mut t = trial(seed, false);
    if network == Network::Legacy {
        return t;
    }
    let mut rng = Random::new(derive_seed(seed, 0x6000));
    let broad = |rng: &mut Random| {
        let unit = rng.unit();
        match rng.next_u64() % 5 {
            0 => unit * 5.,
            1 | 2 => unit * 150.,
            _ => gamma_rtt_ms(unit),
        }
    };
    let local = |rng: &mut Random| {
        if rng.next_u64() % 2 == 0 {
            0.
        } else {
            rng.unit() * 5.
        }
    };
    let (actor, rival, asymmetry) = match network {
        Network::Mixed => (broad(&mut rng), broad(&mut rng), 0.1),
        Network::Both0 => (0., 0., 0.),
        Network::Both2 => (2., 2., 0.),
        Network::Both20 => (20., 20., 0.),
        Network::Both60 => (60., 60., 0.),
        Network::Both120 => (120., 120., 0.),
        Network::Both150 => (150., 150., 0.),
        Network::LocalRival => {
            // Half of the actor's trials use the Paper plugin's default bot RTT.
            let actor = if rng.next_u64() % 2 == 0 {
                20.
            } else {
                broad(&mut rng)
            };
            (actor, local(&mut rng), 0.1)
        }
        Network::LocalActor => (local(&mut rng), broad(&mut rng), 0.1),
        Network::Adverse => (150., 0., 0.),
        Network::Bot20Rival0 | Network::EarlyBot20Rival0 => (20., 0., 0.),
        Network::Legacy => unreachable!(),
    };
    if network == Network::EarlyBot20Rival0 {
        // At 60 FPS these frames run at the decision boundary. The 10 ms
        // bot upstream can reach the server at +11 ms in the same interval.
        t.config.server_phase_ns = 11_000_000;
        for client in &mut t.config.clients {
            client.fps = 60;
            client.frame_phase_ns = 0;
            client.tick_phase_ns = 0;
        }
    }
    let mut fixed_ms = [None; 2];
    fixed_ms[t.seat] = Some(actor);
    fixed_ms[1 - t.seat] = Some(rival);
    t.config.ping_profile = Some(PingProfile {
        fixed_ms,
        asymmetry,
    });
    t
}
