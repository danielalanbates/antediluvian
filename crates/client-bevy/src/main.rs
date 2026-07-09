//! Antediluvia — networked Bevy client.
//!
//! This is a *thin* client: it holds no game logic. It connects to the
//! authoritative server, sends input intents (`Move`/`Attack`), and renders
//! whatever entities the server reports in its per-tick snapshots. All movement,
//! AI, combat and progression happen server-side (see `crates/server`).
//!
//! Usage: antediluvia-client-bevy [name] [ws-url]
//!   defaults: name="Adam", url="ws://127.0.0.1:8787"

mod net;

use antediluvia_protocol::{ClientMsg, EntityKind, EntityState, ServerMsg};
use bevy::prelude::*;
use bevy::sprite::{ColorMaterial, MeshMaterial2d};
use net::{start_network, NetRx, NetTx};
use std::collections::{HashMap, HashSet};

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "Adam".into());
    let url = args.next().unwrap_or_else(|| "ws://127.0.0.1:8787".into());

    // Start the network thread before the app so login is already in flight.
    let (tx, rx) = start_network(url, name.clone());

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Antediluvia".into(),
                resolution: (1600.0, 900.0).into(),
                resizable: true,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.10, 0.13, 0.09)))
        .insert_resource(tx)
        .insert_non_send_resource(rx)
        .insert_resource(EntityMap::default())
        .insert_resource(Session {
            name,
            ..default()
        })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (receive_from_server, send_input, camera_follow_player),
        )
        .run();
}

// ─── Components / resources ──────────────────────────────────────────────────

/// A server-owned entity mirrored into the Bevy world. Holds the server id.
#[derive(Component)]
struct ServerEnt(#[allow(dead_code)] u64);

/// The local player's entity (the one whose server id matches our own).
#[derive(Component)]
struct PlayerTag;

#[derive(Component)]
struct MainCamera;

#[derive(Component)]
struct HudText;

#[derive(Resource, Default)]
struct EntityMap(HashMap<u64, Entity>);

#[derive(Resource, Default)]
struct Session {
    name: String,
    my_id: Option<u64>,
    hud: String,
    notice: String,
}

/// Cached meshes + materials so we don't allocate per entity.
#[derive(Resource)]
struct RenderAssets {
    mesh_player: Handle<Mesh>,
    mesh_enemy: Handle<Mesh>,
    mesh_wildlife: Handle<Mesh>,
    mesh_resource: Handle<Mesh>,
    m_me: Handle<ColorMaterial>,
    m_player: Handle<ColorMaterial>,
    m_enemy: Handle<ColorMaterial>,
    m_wildlife: Handle<ColorMaterial>,
    m_tree: Handle<ColorMaterial>,
    m_rock: Handle<ColorMaterial>,
    m_npc: Handle<ColorMaterial>,
}

impl RenderAssets {
    /// Pick (mesh, material) for an entity given its kind, tag, and whether it's us.
    fn pick(&self, e: &EntityState, is_me: bool) -> (Handle<Mesh>, Handle<ColorMaterial>) {
        match e.kind {
            EntityKind::Player => (
                self.mesh_player.clone(),
                if is_me { self.m_me.clone() } else { self.m_player.clone() },
            ),
            EntityKind::Enemy => (self.mesh_enemy.clone(), self.m_enemy.clone()),
            EntityKind::Wildlife => (self.mesh_wildlife.clone(), self.m_wildlife.clone()),
            EntityKind::Resource => {
                let mat = match e.tag.as_deref() {
                    Some("rock") => self.m_rock.clone(),
                    _ => self.m_tree.clone(),
                };
                (self.mesh_resource.clone(), mat)
            }
            EntityKind::Npc => (self.mesh_resource.clone(), self.m_npc.clone()),
        }
    }
}

fn z_for(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Player => 3.0,
        EntityKind::Enemy | EntityKind::Wildlife | EntityKind::Npc => 2.0,
        EntityKind::Resource => 1.0,
    }
}

// ─── Systems ─────────────────────────────────────────────────────────────────

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((Camera2d, MainCamera));

    let assets = RenderAssets {
        mesh_player: meshes.add(Circle::new(17.0)),
        mesh_enemy: meshes.add(Circle::new(15.0)),
        mesh_wildlife: meshes.add(Circle::new(12.0)),
        mesh_resource: meshes.add(Circle::new(13.0)),
        m_me: materials.add(Color::srgb(0.30, 0.65, 1.00)),
        m_player: materials.add(Color::srgb(0.35, 0.85, 0.45)),
        m_enemy: materials.add(Color::srgb(0.85, 0.22, 0.22)),
        m_wildlife: materials.add(Color::srgb(0.80, 0.70, 0.45)),
        m_tree: materials.add(Color::srgb(0.20, 0.50, 0.22)),
        m_rock: materials.add(Color::srgb(0.55, 0.55, 0.58)),
        m_npc: materials.add(Color::srgb(0.90, 0.82, 0.30)),
    };
    commands.insert_resource(assets);

    // HUD line, top-left.
    commands.spawn((
        Text::new("Connecting…"),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::srgb(0.92, 0.92, 0.88)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(14.0),
            ..default()
        },
        HudText,
    ));
}

/// Drain server messages, reconcile the entity set, update the HUD.
fn receive_from_server(
    mut commands: Commands,
    mut rx: NonSendMut<NetRx>,
    mut map: ResMut<EntityMap>,
    mut session: ResMut<Session>,
    assets: Res<RenderAssets>,
    mut transforms: Query<&mut Transform, With<ServerEnt>>,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let mut latest: Option<Vec<EntityState>> = None;
    while let Ok(msg) = rx.0.try_recv() {
        match msg {
            ServerMsg::Welcome { entity_id, character } => {
                session.my_id = Some(entity_id);
                session.hud = format!(
                    "{} — Lv {}  HP {}/{}  MP {}/{}  in {}",
                    character.name, character.level, character.health, character.max_health,
                    character.mana, character.max_mana, character.act.as_str()
                );
            }
            ServerMsg::Stats { character } => {
                session.hud = format!(
                    "{} — Lv {}  HP {}/{}  MP {}/{}  XP {}/{}  in {}",
                    character.name, character.level, character.health, character.max_health,
                    character.mana, character.max_mana, character.xp, character.max_xp,
                    character.act.as_str()
                );
            }
            ServerMsg::LoginRejected { reason } => {
                session.notice = format!("login rejected: {reason}");
            }
            ServerMsg::Notice { text } => session.notice = text,
            ServerMsg::Chat { from, text } => session.notice = format!("{from}: {text}"),
            ServerMsg::Snapshot { entities, .. } => latest = Some(entities),
            ServerMsg::Pong => {}
        }
    }

    if let Some(entities) = latest {
        let my_id = session.my_id;
        let mut seen: HashSet<u64> = HashSet::with_capacity(entities.len());
        for e in &entities {
            seen.insert(e.id);
            let is_me = Some(e.id) == my_id;
            match map.0.get(&e.id) {
                Some(&bevy_ent) => {
                    if let Ok(mut t) = transforms.get_mut(bevy_ent) {
                        t.translation.x = e.x;
                        t.translation.y = e.y;
                    }
                }
                None => {
                    let (mesh, mat) = assets.pick(e, is_me);
                    let mut ec = commands.spawn((
                        Mesh2d(mesh),
                        MeshMaterial2d(mat),
                        Transform::from_xyz(e.x, e.y, z_for(e.kind)),
                        ServerEnt(e.id),
                    ));
                    if is_me {
                        ec.insert(PlayerTag);
                    }
                    map.0.insert(e.id, ec.id());
                }
            }
        }
        // Despawn entities that left the zone / died.
        let gone: Vec<u64> = map.0.keys().copied().filter(|id| !seen.contains(id)).collect();
        for id in gone {
            if let Some(ent) = map.0.remove(&id) {
                commands.entity(ent).despawn();
            }
        }
    }

    if let Ok(mut text) = hud.get_single_mut() {
        let base = if session.hud.is_empty() {
            format!("{} — connecting…", session.name)
        } else {
            session.hud.clone()
        };
        **text = if session.notice.is_empty() {
            format!("{base}\nWASD move · Space attack")
        } else {
            format!("{base}\n{}", session.notice)
        };
    }
}

/// Read the keyboard and send movement intent + attacks. Movement is only sent
/// when the direction changes, to avoid flooding the socket every frame.
fn send_input(
    keys: Res<ButtonInput<KeyCode>>,
    tx: Res<NetTx>,
    mut last_dir: Local<(f32, f32)>,
) {
    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        dy += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        dy -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        dx -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        dx += 1.0;
    }

    if (dx, dy) != *last_dir {
        *last_dir = (dx, dy);
        tx.send(ClientMsg::Move { dx, dy });
    }
    if keys.just_pressed(KeyCode::Space) {
        tx.send(ClientMsg::Attack);
    }
}

fn camera_follow_player(
    mut cam: Query<&mut Transform, (With<MainCamera>, Without<PlayerTag>)>,
    player: Query<&Transform, With<PlayerTag>>,
) {
    let Ok(mut cam_t) = cam.get_single_mut() else { return };
    let Ok(player_t) = player.get_single() else { return };
    cam_t.translation.x = player_t.translation.x;
    cam_t.translation.y = player_t.translation.y;
}
