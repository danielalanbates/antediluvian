//! Windowless screenshot mode for the real game.
//!
//! `ANTEDILUVIA_SHOTS=<dir>` runs the full client (world, mobs, HUD) with no
//! OS window: winit is disabled, every camera renders into an off-screen
//! image, and PNGs are written at the seconds listed in
//! `ANTEDILUVIA_SHOT_TIMES` (default `60`), then the app exits. Pair with
//! `ANTEDILUVIA_AUTOCMD` to drive the scene. Nothing appears on screen and no
//! focus is taken, so verification never needs the machine's display.

use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::ui::IsDefaultUiCamera;

pub fn enabled() -> bool {
    std::env::var("ANTEDILUVIA_SHOTS").is_ok()
}

#[derive(Resource)]
struct Shots {
    dir: std::path::PathBuf,
    times: Vec<f32>,
    next: usize,
    target: Handle<Image>,
}

pub struct ShotsPlugin;

impl Plugin for ShotsPlugin {
    fn build(&self, app: &mut App) {
        let Ok(dir) = std::env::var("ANTEDILUVIA_SHOTS") else { return };
        let mut times: Vec<f32> = std::env::var("ANTEDILUVIA_SHOT_TIMES")
            .ok()
            .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_else(|| vec![60.0]);
        times.sort_by(|a, b| a.total_cmp(b));
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        app.add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(std::time::Duration::from_millis(16)))
            .insert_resource(ShotsInit { dir, times })
            .add_systems(PreStartup, init_target)
            .add_systems(Last, (retarget_cameras, take_shots));
    }
}

#[derive(Resource)]
struct ShotsInit {
    dir: std::path::PathBuf,
    times: Vec<f32>,
}

fn init_target(mut commands: Commands, init: Res<ShotsInit>, mut images: ResMut<Assets<Image>>) {
    let size = Extent3d { width: 1600, height: 900, ..default() };
    let mut img = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        bevy::render::render_asset::RenderAssetUsages::default(),
    );
    img.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT;
    commands.insert_resource(Shots {
        dir: init.dir.clone(),
        times: init.times.clone(),
        next: 0,
        target: images.add(img),
    });
}

/// Cameras are spawned by gameplay code targeting the (absent) window; point
/// each at the image as it appears. The first 3D camera also owns the HUD.
fn retarget_cameras(
    mut commands: Commands,
    shots: Res<Shots>,
    mut cams: Query<(Entity, &mut Camera, Has<Camera3d>), Without<ShotTargeted>>,
) {
    for (e, mut cam, is3d) in &mut cams {
        cam.target = RenderTarget::Image(shots.target.clone());
        // HDR into an image target renders geometry semi-transparent in the
        // capture (grass shows through bodies), so shots are LDR unless
        // ANTEDILUVIA_SHOTS_HDR is set. LDR skips tonemapping/bloom: colours
        // differ slightly from the windowed game.
        if std::env::var("ANTEDILUVIA_SHOTS_HDR").is_err() {
            cam.hdr = false;
        }
        let mut ec = commands.entity(e);
        ec.insert(ShotTargeted);
        if is3d {
            ec.insert(IsDefaultUiCamera);
        }
    }
}

#[derive(Component)]
struct ShotTargeted;

fn take_shots(
    mut commands: Commands,
    time: Res<Time>,
    mut shots: ResMut<Shots>,
    mut exit: EventWriter<AppExit>,
) {
    let t = time.elapsed_secs();
    if let Some(&at) = shots.times.get(shots.next) {
        if t >= at {
            let path = shots.dir.join(format!("shot_{:03}s.png", at as u32));
            info!("shots: capturing {}", path.display());
            commands.spawn(Screenshot::image(shots.target.clone())).observe(
                move |trigger: Trigger<ScreenshotCaptured>| {
                    // Drop alpha: the render target's alpha is not coverage,
                    // and saving it made opaque characters look see-through.
                    match trigger.event().0.clone().try_into_dynamic() {
                        Ok(img) => {
                            if let Err(e) = img.to_rgb8().save(&path) {
                                error!("shots: cannot save {}: {e}", path.display());
                            }
                        }
                        Err(e) => error!("shots: cannot convert capture: {e}"),
                    }
                },
            );
            shots.next += 1;
        }
    } else if t >= shots.times.last().copied().unwrap_or(0.0) + 3.0 {
        exit.send(AppExit::Success);
    }
}
