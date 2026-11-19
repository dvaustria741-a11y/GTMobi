// GTMobi — application entry point v2 (Phase 3)
// Integrates: gtm-render, gtm-world, gtm-touch

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, Touch, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use gtm_render::GtmRenderer;
use gtm_touch::TouchInput;
use gtm_world::World;
use glam::Vec3;

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
    let event_loop = EventLoop::builder()
        .with_android_app(app)
        .build()
        .expect("event loop");
    run(event_loop);
}

#[cfg(not(target_os = "android"))]
pub fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,gtm_app=debug"),
    ).init();
    let event_loop = EventLoop::new().expect("event loop");
    run(event_loop);
}

// ── App state ────────────────────────────────────────────────────────────────

struct GtmApp {
    renderer: Option<GtmRenderer>,
    window:   Option<Arc<Window>>,
    world:    Option<World>,
    touch:    TouchInput,

    // Free-fly state
    cam_yaw:   f32,
    cam_pitch: f32,
    cam_pos:   Vec3,

    // Keyboard move flags
    key_fwd:   bool,
    key_back:  bool,
    key_left:  bool,
    key_right: bool,
}

impl GtmApp {
    fn new() -> Self {
        Self {
            renderer: None,
            window:   None,
            world:    None,
            touch:    TouchInput::new(1280.0),
            cam_yaw:   0.0,
            cam_pitch: 0.2,
            cam_pos:   Vec3::new(0.0, 60.0, -400.0), // start above Leonard County
            key_fwd: false, key_back: false, key_left: false, key_right: false,
        }
    }

    fn tick(&mut self, dt: f32) {
        // ── Keyboard move ─────────────────────────────────────────────────
        let speed = 40.0 * dt;
        let (sin_y, cos_y) = self.cam_yaw.sin_cos();
        let forward = Vec3::new(sin_y, 0.0, cos_y);
        let right   = Vec3::new(cos_y, 0.0, -sin_y);

        if self.key_fwd   { self.cam_pos += forward * speed; }
        if self.key_back  { self.cam_pos -= forward * speed; }
        if self.key_right { self.cam_pos += right   * speed; }
        if self.key_left  { self.cam_pos -= right   * speed; }

        // ── Touch move ────────────────────────────────────────────────────
        let mv = self.touch.move_input();
        if mv.length_squared() > 0.0 {
            self.cam_pos += forward * mv.y * speed * 2.0;
            self.cam_pos += right   * mv.x * speed * 2.0;
        }

        // ── Touch look ────────────────────────────────────────────────────
        let lk = self.touch.look_input();
        self.cam_yaw   += lk.x * dt * 1.8;
        self.cam_pitch  = (self.cam_pitch - lk.y * dt * 1.5)
            .clamp(-1.3, 1.3);

        // ── Terrain-follow height (above ground level) ────────────────────
        self.cam_pos.y = self.cam_pos.y.max(5.0);

        // ── Push to renderer camera ───────────────────────────────────────
        if let Some(r) = &mut self.renderer {
            let (sy, cy) = self.cam_yaw.sin_cos();
            let (sp, cp) = self.cam_pitch.sin_cos();
            r.camera.position = self.cam_pos;
            r.camera.target   = self.cam_pos + Vec3::new(sy * cp, sp, cy * cp);
        }
    }
}

impl ApplicationHandler for GtmApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }

        let win = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("GTMobi")
                        .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None))),
                )
                .expect("window"),
        );
        let size = win.inner_size();
        self.touch = TouchInput::new(size.width as f32);

        self.window = Some(win.clone());

        let mut renderer = pollster::block_on(GtmRenderer::new(win));

        // Load player character if present
        #[cfg(not(target_os = "android"))]
        if let Ok(bytes) = std::fs::read("assets/characters/player.glb") {
            let _ = renderer.load_gltf(&bytes);
        }

        self.renderer = Some(renderer);
        self.world    = Some(World::new(42));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(sz) => {
                if let Some(r) = &mut self.renderer { r.resize(sz.width, sz.height); }
                self.touch.on_resize(sz.width as f32);
            }
            WindowEvent::CloseRequested => event_loop.exit(),

            // ── Touch ─────────────────────────────────────────────────────
            WindowEvent::Touch(Touch { id, location, phase, .. }) => {
                let (x, y) = (location.x as f32, location.y as f32);
                match phase {
                    TouchPhase::Started  => self.touch.on_touch_start(id, x, y),
                    TouchPhase::Moved    => self.touch.on_touch_move(id, x, y),
                    TouchPhase::Ended | TouchPhase::Cancelled => self.touch.on_touch_end(id),
                }
            }

            // ── Keyboard (desktop debug) ──────────────────────────────────
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(k), state, .. }, ..
            } => {
                let pressed = state == ElementState::Pressed;
                match k {
                    KeyCode::Escape     => event_loop.exit(),
                    KeyCode::KeyW | KeyCode::ArrowUp    => self.key_fwd   = pressed,
                    KeyCode::KeyS | KeyCode::ArrowDown  => self.key_back  = pressed,
                    KeyCode::KeyA | KeyCode::ArrowLeft  => self.key_left  = pressed,
                    KeyCode::KeyD | KeyCode::ArrowRight => self.key_right = pressed,
                    _ => {}
                }
            }

            WindowEvent::RedrawRequested => {
                self.tick(0.016); // fixed 60fps dt — replace with real timer later

                // Update terrain chunks around camera
                if let (Some(world), Some(r)) = (&mut self.world, &self.renderer) {
                    world.update(self.cam_pos, &r.device(), &r.queue());
                }

                if let Some(r) = &mut self.renderer {
                    r.update();
                    match r.render() {
                        Ok(_) => {}
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
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            _ => {}
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.renderer = None;
        self.window   = None;
    }
}

fn run(event_loop: EventLoop<()>) {
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut GtmApp::new()).expect("run");
}
