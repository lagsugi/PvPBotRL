use pvp_policy::recurrent::{Context, Model, OUTPUTS, WIDTH, encode_sample, greedy_choices};
use pvp_sim::{Action, Observation};
use serde::Deserialize;
use serde_json::json;
use std::{fs::File, io::BufReader, path::Path};

#[derive(Deserialize)]
struct Capture {
    status: String,
    policy_frames: Vec<Frame>,
}

#[derive(Deserialize)]
struct Frame {
    step: usize,
    model: String,
    server_tick: u64,
    observation: Observation,
    recurrent: RecordedContext,
    inference: Inference,
}

#[derive(Deserialize)]
struct RecordedContext {
    opponent_health: f32,
    opponent_health_age_ns: u64,
    health_age_ns: u64,
    #[serde(flatten)]
    context: Context,
}

#[derive(Deserialize)]
struct Inference {
    features: Vec<f32>,
    hidden: Vec<f32>,
    logits: Vec<f32>,
    selected: Action,
}

struct Sensitivity {
    hidden: [f32; WIDTH],
    previous: Action,
    isolated_action_changes: usize,
    isolated_click_changes: usize,
    sequential_action_changes: usize,
    sequential_click_changes: usize,
    max_isolated_logit_delta: f32,
}

struct Decision {
    hidden_before: [f32; WIDTH],
    previous: Action,
    logits: [f32; OUTPUTS],
    action: Action,
}

impl Sensitivity {
    fn new() -> Self {
        Self {
            hidden: [0.; WIDTH],
            previous: Action::default(),
            isolated_action_changes: 0,
            isolated_click_changes: 0,
            sequential_action_changes: 0,
            sequential_click_changes: 0,
            max_isolated_logit_delta: 0.,
        }
    }

    fn measure(
        &mut self,
        model: &Model,
        frame: &Frame,
        modified: &Observation,
        baseline: &Decision,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut isolated_hidden = baseline.hidden_before;
        let isolated = encode_sample(
            modified,
            frame.recurrent.context,
            baseline.previous,
            model.input,
        )?;
        let logits = model.logits(&isolated, &mut isolated_hidden);
        let action = isolated.action(greedy_choices(&logits));
        self.isolated_action_changes += usize::from(action != baseline.action);
        self.isolated_click_changes += usize::from(action.click != baseline.action.click);
        for (&candidate, &reference) in logits.iter().zip(&baseline.logits) {
            self.max_isolated_logit_delta = self
                .max_isolated_logit_delta
                .max((candidate - reference).abs());
        }
        let sequential = encode_sample(
            modified,
            frame.recurrent.context,
            self.previous,
            model.input,
        )?;
        let logits = model.logits(&sequential, &mut self.hidden);
        self.previous = sequential.action(greedy_choices(&logits));
        self.sequential_action_changes += usize::from(self.previous != baseline.action);
        self.sequential_click_changes += usize::from(self.previous.click != baseline.action.click);
        Ok(())
    }

    fn report(&self) -> serde_json::Value {
        json!({"isolated_action_changes":self.isolated_action_changes,"isolated_click_changes":self.isolated_click_changes,
            "sequential_action_changes":self.sequential_action_changes,"sequential_click_changes":self.sequential_click_changes,
            "max_isolated_logit_delta":self.max_isolated_logit_delta})
    }
}

fn analyze_impact(mut capture: Capture, model: &Model) -> Result<(), Box<dyn std::error::Error>> {
    let mut hidden = [0.; WIDTH];
    let mut previous = Action::default();
    let mut previous_charge = None;
    let mut fixed_age = Sensitivity::new();
    let mut stale_charge = Sensitivity::new();
    for frame in &mut capture.policy_frames {
        attach_health(frame);
        let baseline_hidden = hidden;
        let features = encode_sample(
            &frame.observation,
            frame.recurrent.context,
            previous,
            model.input,
        )?;
        let logits = model.logits(&features, &mut hidden);
        let action = features.action(greedy_choices(&logits));
        let baseline = Decision {
            hidden_before: baseline_hidden,
            previous,
            logits,
            action,
        };
        let mut modified = frame.observation.clone();
        modified.position_age_ns = 10_000_000;
        modified.velocity_age_ns = 10_000_000;
        modified.metadata_age_ns = 10_000_000;
        fixed_age.measure(model, frame, &modified, &baseline)?;
        let mut modified = frame.observation.clone();
        modified.attack_charge = previous_charge.unwrap_or(modified.attack_charge);
        stale_charge.measure(model, frame, &modified, &baseline)?;
        previous_charge = Some(frame.observation.attack_charge);
        previous = action;
    }
    println!(
        "{}",
        json!({"schema":1,"scope":"fixed-observation-trajectory-sensitivity",
        "capture_policy":capture.policy_frames[0].model,"frames":capture.policy_frames.len(),
        "opponent_packet_ages_fixed_10ms":fixed_age.report(),"attack_charge_previous_decision":stale_charge.report()})
    );
    Ok(())
}

fn attach_health(frame: &mut Frame) {
    frame.observation.opponent_health = Some(frame.recurrent.opponent_health);
    frame.observation.opponent_health_age_ns = Some(frame.recurrent.opponent_health_age_ns);
    frame.observation.health_age_ns = frame.recurrent.health_age_ns;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let capture = args.next().ok_or("expected captured server.json")?;
    let model_file = args.next().ok_or("expected frozen model JSON")?;
    let model = Model::load(Path::new(&model_file))?;
    let capture: Capture = serde_json::from_reader(BufReader::new(File::open(capture)?))?;
    if capture.status != "complete" || capture.policy_frames.is_empty() {
        return Err("Expected a complete policy capture".into());
    }
    match args.next().as_deref() {
        Some(flag) if flag == "--training-impact" => return analyze_impact(capture, &model),
        Some(_) => return Err("Unknown option".into()),
        None => {}
    }
    let mut hidden = [0.; WIDTH];
    let mut previous = Action::default();
    let mut max = [0f32; 3];
    let mut first = None;
    let mut expected_tick = capture.policy_frames[0].server_tick;
    let name = capture.policy_frames[0].model.clone();
    let frame_count = capture.policy_frames.len();
    for (step, mut frame) in capture.policy_frames.into_iter().enumerate() {
        if frame.step != step || frame.model != name || frame.server_tick < expected_tick {
            return Err("Invalid frame order or changed model".into());
        }
        expected_tick = frame.server_tick + 1;
        attach_health(&mut frame);
        let features = encode_sample(
            &frame.observation,
            frame.recurrent.context,
            previous,
            model.input,
        )?;
        let logits = model.logits(&features, &mut hidden);
        previous = features.action(greedy_choices(&logits));
        let fields = [
            (
                "features",
                &features.values[..model.input.size()],
                &frame.inference.features,
                1e-6,
            ),
            ("hidden", hidden.as_slice(), &frame.inference.hidden, 1e-4),
            ("logits", &logits[..OUTPUTS], &frame.inference.logits, 1e-4),
        ];
        for (field, (label, actual, expected, tolerance)) in fields.into_iter().enumerate() {
            if actual.len() != expected.len() {
                return Err(format!("Wrong field length for {label}").into());
            }
            for (index, (&a, &e)) in actual.iter().zip(expected).enumerate() {
                let error = (a - e).abs();
                max[field] = max[field].max(error);
                if error > tolerance && first.is_none() {
                    first =
                        Some(json!({"step":step,"field":label,"index":index,"rust":a,"paper":e}));
                }
            }
        }
        if previous != frame.inference.selected && first.is_none() {
            first = Some(
                json!({"step":step,"field":"action","rust":previous,"paper":frame.inference.selected}),
            );
        }
    }
    println!(
        "{}",
        json!({"schema":1,"scope":"same-live-paper-observation-inference",
        "model":name,"frames":frame_count,"passed":first.is_none(),"first_difference":first,
        "maximum_absolute_error":{"features":max[0],"hidden":max[1],"logits":max[2]}})
    );
    if first.is_some() {
        return Err("Inference differs".into());
    }
    Ok(())
}
