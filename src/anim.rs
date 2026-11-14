//! Animaciones: interpolación y easing. Filosofía: animar informa y acaricia.

use std::time::Duration;

pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Un valor que se desliza hacia su objetivo.
#[derive(Clone, Copy)]
pub struct Tween {
    pub value: f32,
    pub target: f32,
    /// Unidades por segundo (más alto = más rápido).
    pub speed: f32,
}

impl Tween {
    pub fn new(target: f32, speed: f32) -> Self {
        Tween {
            value: target,
            target,
            speed,
        }
    }

    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    pub fn tick(&mut self, dt: Duration) -> bool {
        let dt = dt.as_secs_f32();
        let diff = self.target - self.value;
        if diff.abs() < 0.001 {
            self.value = self.target;
            return false;
        }
        let step = self.speed * dt;
        if diff.abs() <= step {
            self.value = self.target;
            true
        } else {
            self.value += diff.signum() * step;
            false
        }
    }
}

/// Reloj de animación global: acumula el tiempo y cuenta frames.
pub struct Clock {
    pub elapsed: f32,
    pub tick: u64,
    last: std::time::Instant,
}

impl Clock {
    pub fn new() -> Self {
        Clock {
            elapsed: 0.0,
            tick: 0,
            last: std::time::Instant::now(),
        }
    }

    pub fn advance(&mut self) -> Duration {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last);
        self.last = now;
        self.elapsed += dt.as_secs_f32();
        self.tick += 1;
        dt
    }
}
