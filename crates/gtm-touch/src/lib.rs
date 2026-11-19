// GTMobi touch input — dual-stick layout
//
// Screen split:
//   Left half  → movement joystick  (WASD equivalent)
//   Right half → camera joystick    (look around)
//
// Each stick tracks one finger ID.
// If the finger lifts, the stick springs back to center.

use glam::Vec2;

const DEAD_ZONE: f32 = 0.08;
const STICK_RADIUS_PX: f32 = 80.0; // in physical pixels

#[derive(Clone, Debug)]
struct Stick {
    /// Anchor point in screen pixels (where the finger first touched)
    anchor: Vec2,
    /// Current finger position
    current: Vec2,
    /// Finger ID tracking this stick (winit touch id)
    finger_id: Option<u64>,
}

impl Stick {
    fn new() -> Self {
        Self { anchor: Vec2::ZERO, current: Vec2::ZERO, finger_id: None }
    }

    /// Normalised offset in [-1, 1]², 0 when no finger.
    fn value(&self) -> Vec2 {
        if self.finger_id.is_none() { return Vec2::ZERO; }
        let raw = (self.current - self.anchor) / STICK_RADIUS_PX;
        let clamped = raw.clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
        if clamped.length() < DEAD_ZONE { Vec2::ZERO } else { clamped }
    }
}

pub struct TouchInput {
    left:  Stick,
    right: Stick,
    screen_width: f32,
}

impl TouchInput {
    pub fn new(screen_width: f32) -> Self {
        Self { left: Stick::new(), right: Stick::new(), screen_width }
    }

    pub fn on_resize(&mut self, width: f32) {
        self.screen_width = width;
    }

    // ── winit touch event plumbing ────────────────────────────────────────

    pub fn on_touch_start(&mut self, id: u64, x: f32, y: f32) {
        let pos = Vec2::new(x, y);
        if x < self.screen_width * 0.5 {
            if self.left.finger_id.is_none() {
                self.left.anchor     = pos;
                self.left.current    = pos;
                self.left.finger_id  = Some(id);
            }
        } else if self.right.finger_id.is_none() {
            self.right.anchor    = pos;
            self.right.current   = pos;
            self.right.finger_id = Some(id);
        }
    }

    pub fn on_touch_move(&mut self, id: u64, x: f32, y: f32) {
        let pos = Vec2::new(x, y);
        if self.left.finger_id  == Some(id) { self.left.current  = pos; }
        if self.right.finger_id == Some(id) { self.right.current = pos; }
    }

    pub fn on_touch_end(&mut self, id: u64) {
        if self.left.finger_id  == Some(id) { self.left.finger_id  = None; }
        if self.right.finger_id == Some(id) { self.right.finger_id = None; }
    }

    // ── Outputs ──────────────────────────────────────────────────────────

    /// Movement direction in XZ plane, normalised, right-hand Y-up.
    /// +x = strafe right, +z = move forward
    pub fn move_input(&self) -> Vec2 { self.left.value() }

    /// Camera delta: x = yaw, y = pitch. Multiply by sensitivity in caller.
    pub fn look_input(&self) -> Vec2 { self.right.value() }

    pub fn is_moving(&self) -> bool { self.left.finger_id.is_some() }
    pub fn is_looking(&self) -> bool { self.right.finger_id.is_some() }
}
