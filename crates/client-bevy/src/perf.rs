//! CHUNK 10 — performance pass.
//! - FPS overlay behind `ANTEDILUVIA_FPS=1` (measure first).
//! - Startup preload of every model/audio asset so first spawn/combat never
//!   hitches on disk IO.
//! - Shadow budget lives at the sun spawn in main.rs (cascade distance cap).

use bevy::asset::UntypedHandle;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::view::VisibilityRange;

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin)
            .add_systems(Startup, preload_assets)
            .add_systems(Update, (fps_overlay, propagate_visibility_range, fps_log));
        if std::env::var("ANTEDILUVIA_FPS").is_ok() {
            app.add_systems(Startup, spawn_fps_text);
        }
    }
}

/// A4: fps to the log every 10s so headless captures still measure perf.
fn fps_log(diag: Res<DiagnosticsStore>, time: Res<Time>, mut acc: Local<f32>) {
    *acc += time.delta_secs();
    if *acc < 10.0 {
        return;
    }
    *acc = 0.0;
    if let Some(fps) = diag
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
    {
        info!("fps: {fps:.1}");
    }
}

/// Keeps every model warm for the whole session — despawning rigs no longer
/// frees (and later re-loads) their GLBs mid-play.
#[derive(Resource)]
struct Preloaded(#[allow(dead_code)] Vec<UntypedHandle>);

/// Walk the assets dir for loadable extensions only — `load_folder` would
/// also hit glTF `.bin` buffer files, which have no loader and error-spam.
fn preload_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    const EXTS: [&str; 4] = ["glb", "gltf", "ogg", "wav"];
    let root = std::path::Path::new("assets");
    let mut handles = Vec::new();
    let mut stack = vec![root.join("models"), root.join("audio")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTS.contains(&e))
            {
                if let Ok(rel) = path.strip_prefix(root) {
                    handles.push(asset_server.load_untyped(rel.to_path_buf()).untyped());
                }
            }
        }
    }
    commands.insert_resource(Preloaded(handles));
}

/// `VisibilityRange` does not inherit in Bevy 0.15, so scene props (glTF
/// children carry the meshes) need the range copied down once loaded. Runs
/// until each ranged root has mesh descendants, then marks it done.
#[derive(Component)]
pub struct RangePropagated;

fn propagate_visibility_range(
    mut commands: Commands,
    roots: Query<(Entity, &VisibilityRange), (With<SceneRoot>, Without<RangePropagated>)>,
    children_q: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
) {
    for (root, range) in &roots {
        let mut found = false;
        let mut stack: Vec<Entity> = vec![root];
        while let Some(e) = stack.pop() {
            if let Ok(kids) = children_q.get(e) {
                stack.extend(kids.iter().copied());
            }
            if e != root && meshes.get(e).is_ok() {
                commands.entity(e).insert(range.clone());
                found = true;
            }
        }
        if found {
            commands.entity(root).insert(RangePropagated);
        }
    }
}

#[derive(Component)]
struct FpsText;

fn spawn_fps_text(mut commands: Commands) {
    commands.spawn((
        Text::new("FPS --"),
        TextFont { font_size: 18.0, ..default() },
        TextColor(Color::srgb(0.6, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(6.0),
            right: Val::Px(10.0),
            ..default()
        },
        FpsText,
    ));
}

fn fps_overlay(diag: Res<DiagnosticsStore>, mut q: Query<&mut Text, With<FpsText>>) {
    let Ok(mut text) = q.get_single_mut() else { return };
    if let Some(fps) = diag
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
    {
        text.0 = format!("FPS {fps:.0}");
    }
}
