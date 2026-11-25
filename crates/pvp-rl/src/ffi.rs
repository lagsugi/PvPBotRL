use super::*;
use pvp_policy::recurrent::{HEADS, MAX_INPUTS, OUTPUTS, WIDTH};
use rayon::prelude::*;
use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_int},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
};

thread_local! {
    static ERROR: RefCell<CString> = RefCell::new(CString::default());
}

pub struct Batch {
    lanes: Vec<Option<Episode>>,
    pool: rayon::ThreadPool,
    snapshots: Vec<Option<Arc<Model>>>,
    input: Input,
    fixed_equipment: bool,
}

fn checked<T>(f: impl FnOnce() -> Result<T, Box<dyn std::error::Error>>) -> Option<T> {
    let error = match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(value)) => return Some(value),
        Ok(Err(error)) => error.to_string(),
        Err(_) => "panic in Rust training environment".into(),
    };
    ERROR.with(|last| *last.borrow_mut() = CString::new(error.replace('\0', " ")).unwrap());
    None
}

// All calls on a handle are synchronous and exclusive. Array arguments must point
// to initialized, aligned, disjoint storage of the documented lengths. The caller
// owns those arrays; Rust does not keep pointers to them after a call.
#[unsafe(no_mangle)]
pub extern "C" fn rl_error() -> *const c_char {
    ERROR.with(|last| last.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn rl_create(lanes: usize, threads: usize, input: u8) -> *mut Batch {
    create(lanes, threads, input, false)
}

#[unsafe(no_mangle)]
pub extern "C" fn rl_create_fixed_equipment(lanes: usize, threads: usize, input: u8) -> *mut Batch {
    create(lanes, threads, input, true)
}

fn create(lanes: usize, threads: usize, input: u8, fixed_equipment: bool) -> *mut Batch {
    checked(|| {
        if !(1..=4096).contains(&lanes) || !(1..=64).contains(&threads) {
            return Err("invalid batch size or worker count".into());
        }
        let input = match input {
            0 => Input::Full,
            1 => Input::Legacy,
            2 => Input::NoOpponentHp,
            3 => Input::NoAction,
            4 => Input::G3,
            _ => return Err("invalid input variant".into()),
        };
        Ok(Box::into_raw(Box::new(Batch {
            lanes: (0..lanes).map(|_| None).collect(),
            pool: rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()?,
            snapshots: Default::default(),
            input,
            fixed_equipment,
        })))
    })
    .unwrap_or(ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_free(batch: *mut Batch) {
    if !batch.is_null() {
        // SAFETY: The handle was returned by rl_create and ownership is consumed once.
        drop(unsafe { Box::from_raw(batch) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_snapshot(batch: *mut Batch, slot: usize, path: *const c_char) -> c_int {
    checked(|| {
        // SAFETY: The caller supplies the live exclusive handle and a NUL-terminated path.
        let batch = unsafe { &mut *batch };
        let path = unsafe { CStr::from_ptr(path) }.to_str()?;
        if slot >= 4096 {
            return Err("snapshot slot exceeds 4096 model limit".into());
        }
        batch
            .snapshots
            .resize_with(batch.snapshots.len().max(slot + 1), || None);
        let target = batch
            .snapshots
            .get_mut(slot)
            .ok_or("invalid snapshot slot")?;
        *target = Some(Arc::new(Model::load(std::path::Path::new(path))?));
        Ok(0)
    })
    .unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_reset(
    batch: *mut Batch,
    lane: usize,
    seed: u64,
    network: usize,
    opponent: usize,
    max_steps: u64,
    features: *mut f32,
    masks: *mut u8,
) -> c_int {
    checked(|| {
        // SAFETY: The caller supplies the live exclusive handle.
        let batch = unsafe { &mut *batch };
        let target = batch.lanes.get_mut(lane).ok_or("invalid reset lane")?;
        let episode = Episode::with_equipment(
            seed,
            network,
            opponent,
            max_steps,
            &batch.snapshots,
            batch.fixed_equipment,
        )?;
        let obs = episode.features(batch.input)?;
        // SAFETY: Reset output buffers have MAX_INPUTS floats and two mask bytes.
        unsafe {
            ptr::copy_nonoverlapping(obs.values.as_ptr(), features, MAX_INPUTS);
            ptr::copy_nonoverlapping(obs.mask().as_ptr(), masks, 2);
        }
        *target = Some(episode);
        Ok(0)
    })
    .unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_step(
    batch: *mut Batch,
    actions: *const u8,
    beta: f32,
    features: *mut f32,
    masks: *mut u8,
    rewards: *mut f32,
    outcomes: *mut u8,
    hp: *mut f32,
) -> c_int {
    checked(|| {
        // SAFETY: The caller supplies the live exclusive handle.
        let batch = unsafe { &mut *batch };
        let n = batch.lanes.len();
        if !beta.is_finite() || !(0.0..=1.0).contains(&beta) {
            return Err("invalid HP bonus".into());
        }
        // SAFETY: The caller guarantees the lengths and disjointness of these buffers.
        let actions = unsafe { slice::from_raw_parts(actions.cast::<[u8; 4]>(), n) };
        let features =
            unsafe { slice::from_raw_parts_mut(features.cast::<[f32; MAX_INPUTS]>(), n) };
        let masks = unsafe { slice::from_raw_parts_mut(masks.cast::<[u8; 2]>(), n) };
        let rewards = unsafe { slice::from_raw_parts_mut(rewards, n) };
        let outcomes = unsafe { slice::from_raw_parts_mut(outcomes, n) };
        let hp = unsafe { slice::from_raw_parts_mut(hp, n) };
        if actions.iter().any(|row| {
            row.iter()
                .zip(HEADS)
                .any(|(&a, limit)| usize::from(a) >= limit)
        }) {
            return Err("invalid action index".into());
        }
        if batch
            .lanes
            .iter()
            .any(|lane| lane.as_ref().is_none_or(|e| e.duel.is_finished()))
        {
            return Err("all lanes must be initialized and unfinished".into());
        }
        let input = batch.input;
        batch.pool.install(|| {
            (
                batch.lanes.par_iter_mut(),
                actions.par_iter(),
                features.par_iter_mut(),
                masks.par_iter_mut(),
                rewards.par_iter_mut(),
                outcomes.par_iter_mut(),
                hp.par_iter_mut(),
            )
                .into_par_iter()
                .try_for_each(|(lane, &choice, values, mask, reward, outcome, hp)| {
                    let e = lane.as_mut().unwrap();
                    let o = &e.obs[e.seat];
                    let action =
                        recurrent::decode_action(choice, o.on_ground, o.eye_to_target < 3.);
                    let before = e.duel.server_health();
                    let approach =
                        |distance: f64| -0.18 * ((distance - 3.).clamp(0., 16.) as f32 / 16.);
                    let before_approach = if input == Input::G3 {
                        approach(o.eye_to_target)
                    } else {
                        0.
                    };
                    e.advance(action)?;
                    let after = e.duel.server_health();
                    let obs = e.features(input)?;
                    *values = obs.values;
                    *mask = obs.mask();
                    *reward = crate::reward::reward(
                        before,
                        after,
                        e.duel.config().initial_health,
                        e.seat,
                        e.duel.outcome(),
                        beta,
                    );
                    if input == Input::G3 {
                        let overlap = e.record_exchange(before, after, action);
                        let next = if e.duel.is_finished() {
                            0.
                        } else {
                            approach(e.obs[e.seat].eye_to_target)
                        };
                        *reward += next
                            - before_approach
                            - 0.45 * overlap / e.duel.config().initial_health[e.seat];
                    }
                    let result = e.result();
                    *outcome = result.outcome;
                    *hp = result.hp;
                    Ok::<_, SimError>(())
                })
        })?;
        Ok(0)
    })
    .unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_probe(
    batch: *mut Batch,
    slot: usize,
    features: *const f32,
    memory: *mut f32,
    logits: *mut f32,
) -> c_int {
    checked(|| {
        // SAFETY: The caller supplies the handle and disjoint arrays of MAX_INPUTS,
        // WIDTH and OUTPUTS elements respectively.
        let batch = unsafe { &*batch };
        let model = batch
            .snapshots
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or("missing probe model")?;
        let features = Features {
            values: unsafe { *features.cast::<[f32; MAX_INPUTS]>() },
            allow_jump: true,
            allow_click: true,
        };
        let memory = unsafe { &mut *memory.cast::<[f32; WIDTH]>() };
        let out = unsafe { &mut *logits.cast::<[f32; OUTPUTS]>() };
        *out = model.logits(&features, memory);
        Ok(0)
    })
    .unwrap_or(-1)
}
