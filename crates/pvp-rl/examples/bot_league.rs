//! Compare the practice opponents with fixed spawns, swapped seats and both
//! coincident-client orders. Trace data is used only for post-action scoring.
use pvp_policy::policy::{Bot, Style};
use pvp_sim::{Config, Duel, Outcome, trace::TraceEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("bot_a,bot_b,a_wins,b_wins,timeouts,a_sprint_hits,b_sprint_hits,a_crits,b_crits");
    for (i, a) in Style::ALL.into_iter().enumerate() {
        for &b in &Style::ALL[i + 1..] {
            let mut wins = [0; 2];
            let mut timeouts = 0;
            let mut sprint = [0; 2];
            let mut crit = [0; 2];
            for seat_a in 0..2 {
                for first in 0..2 {
                    let mut duel = Duel::new(
                        Config {
                            first_client: Some(first),
                            trace_capacity: 1024,
                            ..Config::default()
                        },
                        42,
                    )?;
                    let mut bots = [
                        Bot::new(if seat_a == 0 { a } else { b }),
                        Bot::new(if seat_a == 1 { a } else { b }),
                    ];
                    while !duel.is_finished() {
                        let obs = duel.observations();
                        duel.step(std::array::from_fn(|p| bots[p].act(&obs[p])))?;
                        for record in duel.drain_trace() {
                            if let TraceEvent::Attack { attacker, result } = record.event {
                                if result.damage.accepted {
                                    let n = attacker ^ seat_a;
                                    sprint[n] += u32::from(result.sprint_hit);
                                    crit[n] += u32::from(result.critical);
                                }
                            }
                        }
                    }
                    assert_eq!(duel.dropped_trace_records(), 0);
                    match duel.outcome() {
                        Outcome::Victory { winner } => wins[winner ^ seat_a] += 1,
                        Outcome::Truncated => timeouts += 1,
                        Outcome::Ongoing => unreachable!(),
                    }
                }
            }
            println!(
                "{},{},{},{},{},{},{},{},{}",
                a.name(),
                b.name(),
                wins[0],
                wins[1],
                timeouts,
                sprint[0],
                sprint[1],
                crit[0],
                crit[1]
            );
        }
    }
    Ok(())
}
