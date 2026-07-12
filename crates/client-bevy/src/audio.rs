//! CHUNK C12 — audio: combat one-shots, per-act ambient beds, UI clicks.
//!
//! SFX are Kenney CC0 packs (assets/audio/sfx); ambient loops are synthesized
//! CC0 beds (scripts/gen_ambient.py). One-shots ride the server's combat
//! events (never the 20 Hz snapshots), get a slight deterministic pitch
//! wobble so repeats don't machine-gun, and fall off with distance from the
//! camera. Ambient crossfades over ~2 s on act travel and ducks ~30% at
//! night. `ANTEDILUVIA_MUTE=1` silences everything (CI/screenshots).

use bevy::audio::{AudioSink, AudioSinkPlayback, PlaybackSettings, Volume};
use bevy::prelude::*;

use antediluvia_protocol::Act;

#[derive(Resource)]
pub struct AudioAssets {
    pub attack: Vec<Handle<AudioSource>>,
    pub cast: Vec<Handle<AudioSource>>,
    pub hit: Vec<Handle<AudioSource>>,
    pub die: Vec<Handle<AudioSource>>,
    pub levelup: Handle<AudioSource>,
    pub click: Handle<AudioSource>,
    ambient: Vec<(Act, Handle<AudioSource>)>,
    pub muted: bool,
    /// Monotone counter driving the deterministic pitch wobble.
    pub seq: u32,
}

/// A looping ambient bed for one act; `target` is the volume the fader eases
/// toward (0.0 while fading out, then despawn).
#[derive(Component)]
pub struct AmbientBed {
    pub act: Act,
    pub target: f32,
}

pub fn init_audio_assets(mut commands: Commands, assets: Res<AssetServer>) {
    let load = |p: &str| assets.load::<AudioSource>(p.to_string());
    commands.insert_resource(AudioAssets {
        attack: vec![load("audio/sfx/attack_0.ogg"), load("audio/sfx/attack_1.ogg")],
        cast: vec![load("audio/sfx/cast_0.ogg"), load("audio/sfx/cast_1.ogg")],
        hit: vec![
            load("audio/sfx/hit_0.ogg"),
            load("audio/sfx/hit_1.ogg"),
            load("audio/sfx/hit_2.ogg"),
        ],
        die: vec![load("audio/sfx/die_0.ogg")],
        levelup: load("audio/sfx/levelup.ogg"),
        click: load("audio/sfx/click.ogg"),
        ambient: Act::ALL
            .iter()
            .map(|a| (*a, load(&format!("audio/ambient/{}.ogg", a.as_str()))))
            .collect(),
        muted: std::env::var("ANTEDILUVIA_MUTE").is_ok_and(|v| v == "1"),
        seq: 0,
    });
}

/// Play a one-shot from `pool`, volume-attenuated by distance to the camera.
pub fn one_shot(
    commands: &mut Commands,
    audio: &mut AudioAssets,
    pool: Pool,
    dist: f32,
) {
    if audio.muted {
        return;
    }
    audio.seq = audio.seq.wrapping_add(1);
    let n = audio.seq;
    let handles = match pool {
        Pool::Attack => &audio.attack,
        Pool::Cast => &audio.cast,
        Pool::Hit => &audio.hit,
        Pool::Die => &audio.die,
        Pool::LevelUp => std::slice::from_ref(&audio.levelup),
        Pool::Click => std::slice::from_ref(&audio.click),
    };
    let h = handles[(n as usize) % handles.len()].clone();
    // Deterministic wobble in 0.9..1.1 (no rand dep) and simple falloff.
    let pitch = 0.9 + 0.2 * ((n.wrapping_mul(2654435761) >> 16) & 0xFF) as f32 / 255.0;
    let vol = (1.0 / (1.0 + dist / 400.0)).clamp(0.05, 1.0)
        * if pool == Pool::Click { 0.5 } else { 0.9 };
    commands.spawn((
        AudioPlayer::new(h),
        PlaybackSettings::DESPAWN
            .with_speed(pitch)
            .with_volume(Volume::new(vol)),
    ));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pool {
    Attack,
    Cast,
    Hit,
    Die,
    LevelUp,
    Click,
}

/// Keep exactly one ambient bed per current act, crossfading over ~2 s on
/// travel and ducking to 70% during night hours.
pub fn ambient_system(
    mut commands: Commands,
    audio: Res<AudioAssets>,
    session: Res<crate::Session>,
    time: Res<Time>,
    mut beds: Query<(Entity, &mut AmbientBed, Option<&AudioSink>)>,
) {
    if audio.muted {
        return;
    }
    // Night = sun below the horizon (same 0..1 clock the atmosphere uses).
    let tod = session.time_of_day;
    let night = !(0.25..0.75).contains(&tod);
    let full = if night { 0.28 } else { 0.4 };

    let mut have_current = false;
    for (ent, mut bed, sink) in &mut beds {
        if bed.act == session.act {
            have_current = true;
            bed.target = full;
        } else {
            bed.target = 0.0;
        }
        if let Some(sink) = sink {
            let v = sink.volume();
            let step = time.delta_secs() / 2.0 * full.max(0.01);
            let nv = if v < bed.target {
                (v + step).min(bed.target)
            } else {
                (v - step).max(bed.target)
            };
            sink.set_volume(nv);
            if bed.target == 0.0 && nv <= 0.001 {
                commands.entity(ent).despawn();
            }
        }
    }
    if !have_current {
        if let Some((_, h)) = audio.ambient.iter().find(|(a, _)| *a == session.act) {
            commands.spawn((
                AudioPlayer::new(h.clone()),
                PlaybackSettings::LOOP.with_volume(Volume::new(0.0)),
                AmbientBed { act: session.act, target: full },
            ));
        }
    }
}
