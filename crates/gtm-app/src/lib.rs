// GTMobi application entry: window, lifecycle, input, third-person player, frame assembly.

use std::sync::Arc;
use std::time::Instant;

use glam::{Mat4, Vec3};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, Touch, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use gtm_render::{character, mesh::{Mesh, Vertex}, DrawItem, GtmRenderer, ModelSlot};
use gtm_touch::TouchInput;
use gtm_world::{Biome, World, CHUNK_SIZE};

/// Humanoid used for the player (from the Defy repo, see assets/third_party/defy/MANIFEST.md).
const CHARACTER_GLB: &[u8] =
    include_bytes!("../../../assets/third_party/defy/characters/template_player.glb");
/// The file holds a male and a female mesh; only the mesh whose name contains this is used.
const CHARACTER_MESH_FILTER: &str = "Male";
const PLAYER_HEIGHT_M: f32 = 1.8;

/// Time of day for the scene. 13:00 matches the bright midday in the reference videos.
const START_HOUR: f32 = 13.0;
/// Start on the beach strip of Vice Dale (see gtm-world biome.rs).
const SPAWN_XZ: (f32, f32) = (0.0, 2300.0);

const RUN_SPEED: f32 = 5.5; // m/s at full stick deflection, uncalibrated
const TURN_RATE: f32 = 12.0; // rad/s toward the movement direction
const CAM_DISTANCE: f32 = 3.6;
const CAM_PIVOT_HEIGHT: f32 = 1.45;
const CAM_SHOULDER: f32 = 0.45; // meters toward the right shoulder
const CAM_MIN_PITCH: f32 = -0.35;
const CAM_MAX_PITCH: f32 = 1.2;
const LOOK_SPEED_YAW: f32 = 2.2; // rad/s at full stick, uncalibrated
const LOOK_SPEED_PITCH: f32 = 1.6;

// ── Platform entry ───────────────────────────────────────────────────────────

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("GTMobi"),
    );
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let event_loop = EventLoop::builder().with_android_app(app).build().expect("event loop");
    run(event_loop);
}

#[cfg(not(target_os = "android"))]
pub fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,gtm_app=debug"))
        .init();
    let event_loop = EventLoop::new().expect("event loop");
    run(event_loop);
}

// ── Pure helpers (unit-tested) ───────────────────────────────────────────────

/// Forward direction on the ground for a heading. Heading 0 looks down +Z; +heading turns toward +X.
pub fn heading_forward(heading: f32) -> Vec3 {
    let (s, c) = heading.sin_cos();
    Vec3::new(s, 0.0, c)
}

/// Right-hand side for a heading in a right-handed, Y-up world (forward × up).
fn heading_right(heading: f32) -> Vec3 {
    let (s, c) = heading.sin_cos();
    Vec3::new(-c, 0.0, s)
}

fn wrap_angle(a: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    let mut x = (a + std::f32::consts::PI).rem_euclid(two_pi) - std::f32::consts::PI;
    if x <= -std::f32::consts::PI {
        x += two_pi;
    }
    x
}

/// Moves `current` toward `target` along the shortest arc by at most `max_step`.
fn turn_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let diff = wrap_angle(target - current);
    if diff.abs() <= max_step { target } else { current + diff.signum() * max_step }
}

/// Camera placement: pivot at the player's upper body, orbiting by `heading` (yaw) and `pitch`
/// (positive = looking down), `distance` behind it and `shoulder` meters to the right.
pub fn orbit_camera(pivot: Vec3, heading: f32, pitch: f32, distance: f32, shoulder: f32) -> (Vec3, Vec3) {
    let (sp, cp) = pitch.sin_cos();
    let f = heading_forward(heading);
    let look_dir = Vec3::new(f.x * cp, -sp, f.z * cp);
    let right = heading_right(heading);
    let target = pivot + right * shoulder;
    (target - look_dir * distance, target)
}

/// Placeholder clothing colors by body region until the model gets real textures.
pub fn tint_vertex(v: &mut Vertex) {
    let [x, y, _] = v.position;
    let skin = [0.72, 0.52, 0.40, 1.0];
    let shirt = [0.86, 0.86, 0.84, 1.0];
    let shorts = [0.36, 0.40, 0.26, 1.0];
    v.color = if y < 0.40 {
        skin
    } else if y < 0.92 {
        shorts
    } else if y < 1.50 && x.abs() < 0.20 {
        shirt
    } else {
        skin
    };
}

// ── App state ────────────────────────────────────────────────────────────────

struct PlayerCharacter {
    meshes: Vec<Mesh>,
    slot: ModelSlot,
}

struct GtmApp {
    // Field order matters: GPU objects drop before the window.
    character: Option<PlayerCharacter>,
    renderer: Option<GtmRenderer>,
    window: Option<Arc<Window>>,
    world: Option<World>,
    touch: TouchInput,

    player_pos: Vec3,
    player_heading: f32,
    cam_heading: f32,
    cam_pitch: f32,

    key_fwd: bool,
    key_back: bool,
    key_left: bool,
    key_right: bool,

    last_frame: Instant,
    frame_counter: u64,
}

impl GtmApp {
    fn new() -> Self {
        Self {
            character: None,
            renderer: None,
            window: None,
            world: None,
            touch: TouchInput::new(1280.0),
            player_pos: Vec3::new(SPAWN_XZ.0, 0.0, SPAWN_XZ.1),
            player_heading: 0.0,
            cam_heading: 0.0,
            cam_pitch: 0.25,
            key_fwd: false,
            key_back: false,
            key_left: false,
            key_right: false,
            last_frame: Instant::now(),
            frame_counter: 0,
        }
    }

    /// Movement stick in camera space: x = right, y = forward (both -1..1).
    fn move_axes(&self) -> (f32, f32) {
        let t = self.touch.move_input();
        // Touch stick y grows downward on screen, so forward is -y.
        let mut x = t.x;
        let mut fwd = -t.y;
        if self.key_right { x += 1.0; }
        if self.key_left { x -= 1.0; }
        if self.key_fwd { fwd += 1.0; }
        if self.key_back { fwd -= 1.0; }
        (x.clamp(-1.0, 1.0), fwd.clamp(-1.0, 1.0))
    }

    fn tick(&mut self, dt: f32) {
        let Some(world) = self.world.as_ref() else { return };

        // Look (right stick): dragging right turns the camera right, dragging down looks down.
        let look = self.touch.look_input();
        self.cam_heading = wrap_angle(self.cam_heading - look.x * LOOK_SPEED_YAW * dt);
        self.cam_pitch = (self.cam_pitch + look.y * LOOK_SPEED_PITCH * dt).clamp(CAM_MIN_PITCH, CAM_MAX_PITCH);

        // Move relative to the camera heading.
        let (mx, mf) = self.move_axes();
        let mag = (mx * mx + mf * mf).sqrt().min(1.0);
        if mag > 0.0 {
            let dir = (heading_forward(self.cam_heading) * mf + heading_right(self.cam_heading) * mx)
                .normalize_or_zero();
            if dir != Vec3::ZERO {
                let target_heading = dir.x.atan2(dir.z);
                self.player_heading = turn_toward(self.player_heading, target_heading, TURN_RATE * dt);
                let step = dir * (RUN_SPEED * mag * dt);
                let next = self.player_pos + step;
                // Do not walk into open ocean (there is no water or swimming yet).
                if Biome::at(next.x, next.z) != Biome::Ocean {
                    self.player_pos.x = next.x;
                    self.player_pos.z = next.z;
                }
            }
        }
        self.player_pos.y = world.ground_height(self.player_pos.x, self.player_pos.z);

        // Third-person camera, kept above the ground.
        let pivot = self.player_pos + Vec3::new(0.0, CAM_PIVOT_HEIGHT, 0.0);
        let (mut cam_pos, cam_target) =
            orbit_camera(pivot, self.cam_heading, self.cam_pitch, CAM_DISTANCE, CAM_SHOULDER);
        let min_y = world.ground_height(cam_pos.x, cam_pos.z) + 0.4;
        cam_pos.y = cam_pos.y.max(min_y);
        if let Some(r) = self.renderer.as_mut() {
            r.camera.position = cam_pos;
            r.camera.target = cam_target;
            r.camera.fovy_deg = 65.0;
            r.camera.far = 1500.0;
        }
    }

    fn draw_frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().clamp(0.0, 0.1);
        self.last_frame = now;
        self.tick(dt);

        let (Some(r), Some(world)) = (self.renderer.as_mut(), self.world.as_mut()) else { return };
        world.update(self.player_pos, r.device(), r.queue());
        r.update();

        if let Some(ch) = &self.character {
            let model = Mat4::from_translation(self.player_pos) * Mat4::from_rotation_y(self.player_heading);
            r.write_model_slot(&ch.slot, model);
        }

        // Frustum culling of terrain chunks.
        let vp = gtm_math::Mat4 { cols: r.camera.to_uniform().view_proj };
        let frustum = gtm_cull::Frustum::from_view_proj(&vp);
        let mut items: Vec<DrawItem<'_>> = Vec::with_capacity(128);
        let (mut total, mut drawn) = (0usize, 0usize);
        for (coord, chunk) in world.terrain.chunks.iter() {
            total += 1;
            let o = coord.origin();
            let bounds = gtm_math::Aabb::new(
                gtm_math::Vec3::new(o.x, -12.0, o.z),
                gtm_math::Vec3::new(o.x + CHUNK_SIZE, 40.0, o.z + CHUNK_SIZE),
            );
            if frustum.as_ref().is_some_and(|f| !f.intersects_aabb(&bounds)) {
                continue;
            }
            drawn += 1;
            items.push(DrawItem {
                vertex_buffer: &chunk.vertex_buffer,
                index_buffer: &chunk.index_buffer,
                index_count: chunk.index_count,
                model: None,
            });
        }
        if let Some(ch) = &self.character {
            for m in &ch.meshes {
                items.push(DrawItem {
                    vertex_buffer: &m.vertex_buffer,
                    index_buffer: &m.index_buffer,
                    index_count: m.index_count,
                    model: Some(&ch.slot),
                });
            }
        }

        self.frame_counter += 1;
        if self.frame_counter % 120 == 1 {
            log::info!(
                "frame {}: chunks drawn {drawn}/{total}, player {:?}, dt {:.1} ms",
                self.frame_counter,
                self.player_pos,
                dt * 1000.0
            );
        }

        match r.render_items(&items) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                if let Some(w) = &self.window {
                    let sz = w.inner_size();
                    r.resize(sz.width, sz.height);
                }
            }
            Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
            Err(e) => log::warn!("{e:?}"),
        }
    }
}

fn load_player(r: &GtmRenderer) -> Option<PlayerCharacter> {
    match character::load_bind_pose(CHARACTER_GLB, PLAYER_HEIGHT_M, Some(CHARACTER_MESH_FILTER)) {
        Ok(parts) => {
            let meshes = parts
                .into_iter()
                .map(|mut p| {
                    p.vertices.iter_mut().for_each(tint_vertex);
                    log::info!("character mesh {}: {} verts, {} tris", p.name, p.vertices.len(), p.indices.len() / 3);
                    Mesh::from_cpu(r.device(), &p.vertices, &p.indices)
                })
                .collect();
            Some(PlayerCharacter { meshes, slot: r.create_model_slot() })
        }
        Err(e) => {
            log::error!("character load failed: {e:#}");
            None
        }
    }
}

impl ApplicationHandler for GtmApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let win = match event_loop.create_window(
            Window::default_attributes()
                .with_title("GTMobi")
                .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None))),
        ) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("window creation failed: {e}");
                event_loop.exit();
                return;
            }
        };
        let size = win.inner_size();
        self.touch = TouchInput::new(size.width as f32);
        self.window = Some(win.clone());

        let mut renderer = pollster::block_on(GtmRenderer::new(win));
        renderer.set_time_of_day(START_HOUR);
        self.character = load_player(&renderer);
        self.renderer = Some(renderer);
        self.world = Some(World::new(42));
        self.last_frame = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(sz) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(sz.width, sz.height);
                }
                self.touch.on_resize(sz.width as f32);
            }
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Touch(Touch { id, location, phase, .. }) => {
                let (x, y) = (location.x as f32, location.y as f32);
                match phase {
                    TouchPhase::Started => self.touch.on_touch_start(id, x, y),
                    TouchPhase::Moved => self.touch.on_touch_move(id, x, y),
                    TouchPhase::Ended | TouchPhase::Cancelled => self.touch.on_touch_end(id),
                }
            }

            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(k), state, .. },
                ..
            } => {
                let pressed = state == ElementState::Pressed;
                match k {
                    KeyCode::Escape => event_loop.exit(),
                    KeyCode::KeyW | KeyCode::ArrowUp => self.key_fwd = pressed,
                    KeyCode::KeyS | KeyCode::ArrowDown => self.key_back = pressed,
                    KeyCode::KeyA | KeyCode::ArrowLeft => self.key_left = pressed,
                    KeyCode::KeyD | KeyCode::ArrowRight => self.key_right = pressed,
                    _ => {}
                }
            }

            WindowEvent::RedrawRequested => {
                self.draw_frame(event_loop);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.character = None;
        self.renderer = None;
        self.world = None;
        self.window = None;
    }
}

fn run(event_loop: EventLoop<()>) {
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut GtmApp::new()).expect("run");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_is_forward_cross_up() {
        for h in [0.0f32, 0.7, 2.0, -1.3] {
            let f = heading_forward(h);
            let r = heading_right(h);
            let cross = f.cross(Vec3::Y);
            assert!((cross - r).length() < 1e-5, "heading {h}");
        }
    }

    #[test]
    fn turn_toward_takes_the_short_way() {
        let near_pi = std::f32::consts::PI - 0.1;
        let out = turn_toward(near_pi, -near_pi, 0.05);
        assert!(out > near_pi, "should continue past +PI instead of swinging back, got {out}");
        assert_eq!(turn_toward(0.0, 0.01, 1.0), 0.01);
    }

    #[test]
    fn camera_sits_behind_and_above_when_pitched_down() {
        let pivot = Vec3::new(0.0, 1.5, 0.0);
        let (pos, target) = orbit_camera(pivot, 0.0, 0.3, 4.0, 0.0);
        assert!(pos.z < target.z, "camera behind a +Z-facing player");
        assert!(pos.y > target.y, "camera above target when looking down");
        assert!(((pos - target).length() - 4.0).abs() < 1e-4);
    }

    #[test]
    fn tint_assigns_regions() {
        let mut v = Vertex {
            position: [0.0, 1.2, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
            tangent: [1.0, 0.0, 0.0, 1.0],
            color: [1.0; 4],
        };
        tint_vertex(&mut v);
        assert!(v.color[0] > 0.8, "torso is the light shirt color");
    }
}
